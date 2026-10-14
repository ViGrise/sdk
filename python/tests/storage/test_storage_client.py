import hashlib
import inspect
import io
import json
import re

import httpx
import pytest

from vigrise.auth import Credentials
from vigrise.storage import (
    AsyncStorageClient,
    CompletedPart,
    LifecycleRule,
    PresignRequest,
    PutOptions,
    StorageClient,
    StorageError,
)

CREDENTIALS = Credentials("VGAKTEST", "secret")
BUCKET = {"name": "photos", "project_id": "p1", "created_at": "2026-10-01T12:00:00Z"}


@pytest.fixture(params=[StorageClient, AsyncStorageClient], ids=["sync", "async"])
def make_client(request):
    def build(handler):
        seen = []

        def record(http_request):
            seen.append(http_request)
            return handler(http_request)

        client = request.param(
            endpoint="https://storage.test", credentials=CREDENTIALS, transport=httpx.MockTransport(record)
        )
        return client, seen

    build.is_async = request.param is AsyncStorageClient
    return build


async def call(client, name, *args, **kwargs):
    result = getattr(client, name)(*args, **kwargs)
    return await result if inspect.isawaitable(result) else result


def chunks(make_client, pieces):
    """The pieces as the kind of stream the client under test reads."""
    if make_client.is_async:

        async def generate():
            for piece in pieces:
                yield piece

        return generate()
    return iter(pieces)


def json_response(body, status=200):
    return httpx.Response(status, json=body)


async def test_buckets_are_created_listed_read_and_deleted(make_client):
    def handler(request):
        if request.method == "DELETE":
            return httpx.Response(204)
        if request.method == "GET" and request.url.path == "/v1/buckets":
            return json_response({"buckets": [BUCKET]})
        return json_response(BUCKET, 201 if request.method == "POST" else 200)

    client, seen = make_client(handler)
    bucket = await call(client, "create_bucket", "photos")
    assert (bucket.name, bucket.project_id, bucket.created_at) == ("photos", "p1", "2026-10-01T12:00:00Z")
    assert (seen[-1].method, seen[-1].url.path) == ("POST", "/v1/buckets")
    assert json.loads(seen[-1].content) == {"name": "photos"}
    assert len(await call(client, "list_buckets")) == 1
    assert (await call(client, "get_bucket", "photos")).name == "photos"
    await call(client, "delete_bucket", "photos")
    assert (seen[-1].method, seen[-1].url.path) == ("DELETE", "/v1/buckets/photos")


async def test_every_call_is_signed_vgr4_for_storage(make_client):
    client, seen = make_client(lambda r: json_response({"buckets": []}))
    await call(client, "list_buckets")
    assert re.match(r"^VGR4-HMAC-SHA256 Credential=VGAKTEST/\d{8}/storage/vgr4_request, ", seen[-1].headers["authorization"])


async def test_errors_carry_status_code_and_message(make_client):
    client, _ = make_client(lambda r: json_response({"error": {"code": "NoSuchBucket", "message": "gone"}}, 404))
    with pytest.raises(StorageError) as raised:
        await call(client, "get_bucket", "nope")
    assert (raised.value.status, raised.value.code, raised.value.message) == (404, "NoSuchBucket", "gone")


async def test_an_answer_that_is_not_an_envelope_keeps_its_status_and_body(make_client):
    client, _ = make_client(lambda r: httpx.Response(502, content=b"bad gateway"))
    with pytest.raises(StorageError) as raised:
        await call(client, "get_bucket", "nope")
    assert (raised.value.status, raised.value.code, raised.value.message) == (502, "UnknownError", "bad gateway")


async def test_transport_failures_are_storage_errors_without_status(make_client):
    def refuse(request):
        raise httpx.ConnectError("refused")

    client, _ = make_client(refuse)
    with pytest.raises(StorageError) as raised:
        await call(client, "list_buckets")
    assert (raised.value.status, raised.value.code) == (0, "TransportError")


async def test_put_object_signs_the_payload_and_sends_metadata_and_tags(make_client):
    client, seen = make_client(lambda r: json_response({"etag": "e1", "size": 5}))
    result = await call(
        client, "put_object", "photos", "2026/cat photo.jpg", b"hello",
        content_type="image/jpeg", metadata={"owner": "an"}, tags={"kind": "pet", "a b": "c&d"},
    )
    assert (result.etag, result.size) == ("e1", 5)
    request = seen[-1]
    assert (request.method, request.url.raw_path) == ("PUT", b"/v1/buckets/photos/objects/2026/cat%20photo.jpg")
    assert request.headers["content-type"] == "image/jpeg"
    assert request.headers["x-vgr-meta-owner"] == "an"
    assert request.headers["x-vgr-tagging"] == "a%20b=c%26d&kind=pet"
    assert request.headers["x-vgr-content-sha256"] == hashlib.sha256(b"hello").hexdigest()
    assert request.content == b"hello"


async def test_put_object_stream_sends_the_body_as_unsigned_payload(make_client):
    client, seen = make_client(lambda r: json_response({"etag": "e2", "size": 6}))
    result = await call(client, "put_object_stream", "photos", "big.bin", chunks(make_client, [b"abc", b"def"]), 6)
    assert result.size == 6
    request = seen[-1]
    assert request.headers["x-vgr-content-sha256"] == "UNSIGNED-PAYLOAD"
    assert request.headers["content-length"] == "6"
    assert "transfer-encoding" not in request.headers
    assert request.content == b"abcdef"


async def test_put_object_stream_reads_a_file_like_object(make_client):
    client, seen = make_client(lambda r: json_response({"etag": "e2", "size": 3}))
    await call(client, "put_object_stream", "photos", "f.bin", io.BytesIO(b"xyz"), 3)
    assert seen[-1].content == b"xyz"
    assert seen[-1].headers["x-vgr-content-sha256"] == "UNSIGNED-PAYLOAD"


async def test_get_object_reads_the_headers_and_a_range(make_client):
    client, seen = make_client(
        lambda r: httpx.Response(
            206, content=b"he",
            headers={"content-type": "text/plain", "etag": '"abc"', "content-range": "bytes 0-1/9",
                     "x-vgr-meta-owner": "an", "x-vgr-checksum-sha256": "ff"},
        )
    )
    obj = await call(client, "get_object", "photos", "a.txt", range=(0, 1))
    assert seen[-1].headers["range"] == "bytes=0-1"
    assert obj.body == b"he"
    assert (obj.content_type, obj.etag, obj.size, obj.metadata, obj.checksum_sha256) == ("text/plain", "abc", 9, {"owner": "an"}, "ff")


async def test_get_object_stream_yields_the_body_in_chunks(make_client):
    class Chunks(httpx.SyncByteStream, httpx.AsyncByteStream):
        def __iter__(self):
            yield b"abc"
            yield b"def"

        async def __aiter__(self):
            yield b"abc"
            yield b"def"

    client, _ = make_client(lambda r: httpx.Response(200, stream=Chunks(), headers={"content-length": "6", "content-type": "text/plain"}))
    stream = await call(client, "get_object_stream", "photos", "a.txt")
    assert stream.size == 6
    parts = []
    if make_client.is_async:
        async for part in stream:
            parts.append(part)
    else:
        for part in stream:
            parts.append(part)
    assert b"".join(parts) == b"abcdef"


async def test_head_object_reads_the_headers_without_a_body(make_client):
    client, seen = make_client(lambda r: httpx.Response(200, headers={"content-type": "image/png", "content-length": "12", "etag": '"e"'}))
    obj = await call(client, "head_object", "photos", "a.png")
    assert seen[-1].method == "HEAD"
    assert (obj.content_type, obj.size, obj.etag, obj.body) == ("image/png", 12, "e", b"")


async def test_delete_object(make_client):
    client, seen = make_client(lambda r: httpx.Response(204))
    await call(client, "delete_object", "photos", "a/b.txt")
    assert (seen[-1].method, seen[-1].url.path) == ("DELETE", "/v1/buckets/photos/objects/a/b.txt")


async def test_list_objects_applies_filters(make_client):
    page = {
        "objects": [{"key": "a", "size": 1, "etag": "e", "content_type": "text/plain", "last_modified": "2026-10-01T12:00:00Z"}],
        "common_prefixes": ["x/"], "next_continuation_token": "t2",
    }
    client, seen = make_client(lambda r: json_response(page))
    listed = await call(client, "list_objects", "photos", prefix="a/", delimiter="/", max_keys=10, continuation_token="t1", start_after="a/0")
    assert seen[-1].url.query == b"prefix=a%2F&delimiter=%2F&max_keys=10&continuation_token=t1&start_after=a%2F0"
    assert listed.common_prefixes == ["x/"] and listed.next_continuation_token == "t2"
    assert (listed.objects[0].key, listed.objects[0].content_type, listed.objects[0].last_modified) == ("a", "text/plain", "2026-10-01T12:00:00Z")
    await call(client, "list_objects", "photos")
    assert seen[-1].url.query == b""


async def test_copy_object_keeps_or_replaces_the_description(make_client):
    client, seen = make_client(lambda r: json_response({"etag": "e3", "size": 4}))
    await call(client, "copy_object", "src", "a b.txt", "dst", "c.txt")
    request = seen[-1]
    assert (request.method, request.url.path) == ("PUT", "/v1/buckets/dst/objects/c.txt")
    assert request.headers["x-vgr-copy-source"] == "src/a%20b.txt"
    assert "x-vgr-metadata-directive" not in request.headers
    await call(client, "copy_object", "src", "a.txt", "dst", "c.txt", replace=PutOptions(content_type="text/html", tags={"k": "v"}))
    request = seen[-1]
    assert request.headers["x-vgr-metadata-directive"] == "REPLACE"
    assert request.headers["content-type"] == "text/html" and request.headers["x-vgr-tagging"] == "k=v"


async def test_multipart_calls(make_client):
    def handler(request):
        if request.method == "POST" and request.url.path == "/v1/buckets/photos/uploads":
            return json_response({"upload_id": "u1"}, 201)
        if request.method == "PUT":
            return json_response({"number": 1, "etag": "p1"})
        if request.method == "GET" and request.url.path.endswith("/parts"):
            return json_response({"parts": [{"number": 1, "etag": "p1", "size": 3}]})
        if request.method == "GET":
            return json_response({"uploads": [{"upload_id": "u1", "key": "k", "initiated": "2026-10-01T12:00:00Z"}],
                                  "common_prefixes": [], "is_truncated": True, "next_key_marker": "k", "next_upload_id_marker": "u1"})
        if request.method == "DELETE":
            return httpx.Response(204)
        return json_response({"etag": "done", "size": 3})

    client, seen = make_client(handler)
    upload_id = await call(client, "create_upload", "photos", "big.bin", content_type="video/mp4", metadata={"a": "b"}, tags={"t": "1"})
    assert upload_id == "u1"
    assert json.loads(seen[-1].content) == {"key": "big.bin", "content_type": "video/mp4", "metadata": {"a": "b"}, "tags": {"t": "1"}}
    part = await call(client, "upload_part", "photos", "u1", 1, b"abc")
    assert (part.number, part.etag) == (1, "p1")
    assert (seen[-1].method, seen[-1].url.path) == ("PUT", "/v1/buckets/photos/uploads/u1/parts/1")
    parts = await call(client, "list_parts", "photos", "u1")
    assert (parts[0].number, parts[0].etag, parts[0].size) == (1, "p1", 3)
    done = await call(client, "complete_upload", "photos", "u1", [CompletedPart(1, "p1")])
    assert (done.etag, done.size) == ("done", 3)
    assert json.loads(seen[-1].content) == {"parts": [{"number": 1, "etag": "p1"}]}
    page = await call(client, "list_uploads", "photos", prefix="k", max_uploads=5, key_marker="a", upload_id_marker="b")
    assert seen[-1].url.query == b"prefix=k&max_uploads=5&key_marker=a&upload_id_marker=b"
    assert (page.is_truncated, page.next_key_marker, page.next_upload_id_marker, page.uploads[0].upload_id) == (True, "k", "u1", "u1")
    await call(client, "abort_upload", "photos", "u1")
    assert (seen[-1].method, seen[-1].url.path) == ("DELETE", "/v1/buckets/photos/uploads/u1")


async def test_upload_stream_splits_the_stream_into_parts(make_client):
    parts = []

    def handler(request):
        if request.method == "POST" and request.url.path.endswith("/uploads"):
            return json_response({"upload_id": "u1"}, 201)
        if request.method == "PUT":
            parts.append(request.content)
            return json_response({"number": len(parts), "etag": f"p{len(parts)}"})
        return json_response({"etag": "done", "size": 10})

    client, seen = make_client(handler)
    result = await call(client, "upload_stream", "photos", "big.bin", chunks(make_client, [b"abc", b"defg", b"hi", b"j"]), part_size=4)
    assert (result.etag, result.size) == ("done", 10)
    assert parts == [b"abcd", b"efgh", b"ij"]
    assert json.loads(seen[-1].content) == {"parts": [{"number": 1, "etag": "p1"}, {"number": 2, "etag": "p2"}, {"number": 3, "etag": "p3"}]}


async def test_upload_stream_of_nothing_stores_one_empty_part(make_client):
    parts = []

    def handler(request):
        if request.method == "POST" and request.url.path.endswith("/uploads"):
            return json_response({"upload_id": "u1"}, 201)
        if request.method == "PUT":
            parts.append(request.content)
            return json_response({"number": 1, "etag": "p1"})
        return json_response({"etag": "done", "size": 0})

    client, _ = make_client(handler)
    await call(client, "upload_stream", "photos", "empty", chunks(make_client, []))
    assert parts == [b""]


async def test_upload_stream_aborts_when_a_part_fails(make_client):
    puts = []

    def handler(request):
        if request.method == "POST" and request.url.path.endswith("/uploads"):
            return json_response({"upload_id": "u1"}, 201)
        if request.method == "PUT":
            puts.append(1)
            if len(puts) == 2:
                return json_response({"error": {"code": "InternalError", "message": "disk"}}, 500)
            return json_response({"number": len(puts), "etag": f"p{len(puts)}"})
        return httpx.Response(204)

    client, seen = make_client(handler)
    with pytest.raises(StorageError) as raised:
        await call(client, "upload_stream", "photos", "big.bin", chunks(make_client, [b"aaaabbbbcccc"]), part_size=4)
    assert (raised.value.status, raised.value.code) == (500, "InternalError")
    assert (seen[-1].method, seen[-1].url.path) == ("DELETE", "/v1/buckets/photos/uploads/u1")


async def test_tags(make_client):
    client, seen = make_client(lambda r: json_response({"tags": {"my_tag": "v"}}) if r.method == "GET" else httpx.Response(204))
    await call(client, "put_tags", "photos", "a.txt", {"k": "v"})
    assert (seen[-1].method, seen[-1].url.raw_path) == ("PUT", b"/v1/buckets/photos/objects/a.txt?tagging")
    assert json.loads(seen[-1].content) == {"tags": {"k": "v"}}
    assert await call(client, "get_tags", "photos", "a.txt") == {"my_tag": "v"}
    await call(client, "delete_tags", "photos", "a.txt")
    assert seen[-1].method == "DELETE"


async def test_lifecycle(make_client):
    rules = {"rules": [{"id": "r", "prefix": "tmp/", "expire_after_days": 3, "enabled": True}]}
    client, seen = make_client(lambda r: json_response(rules) if r.method == "GET" else httpx.Response(204))
    await call(
        client, "put_lifecycle", "photos",
        [LifecycleRule("r", prefix="tmp/", tags={"t": "1"}, expire_after_days=3, abort_incomplete_uploads_after_days=1, enabled=False)],
    )
    assert json.loads(seen[-1].content) == {
        "rules": [{"id": "r", "prefix": "tmp/", "tags": {"t": "1"}, "expire_after_days": 3, "abort_incomplete_uploads_after_days": 1, "enabled": False}]
    }
    fetched = await call(client, "get_lifecycle", "photos")
    assert fetched == [LifecycleRule("r", prefix="tmp/", tags={}, expire_after_days=3, enabled=True)]
    await call(client, "delete_lifecycle", "photos")
    assert (seen[-1].method, seen[-1].url.path) == ("DELETE", "/v1/buckets/photos/lifecycle")


async def test_presign(make_client):
    client, seen = make_client(lambda r: json_response({"url": "https://s/x?sig=1", "expires_at": "2026-10-01T13:00:00Z"}))
    presigned = await call(
        client, "presign", "photos",
        PresignRequest("a.txt", "PUT", 600, content_type="text/plain", max_size=100, checksum_sha256="ab", upload_id="u1", part_number=2),
    )
    assert (presigned.url, presigned.expires_at) == ("https://s/x?sig=1", "2026-10-01T13:00:00Z")
    assert (seen[-1].method, seen[-1].url.path) == ("POST", "/v1/buckets/photos/presign")
    assert json.loads(seen[-1].content) == {
        "key": "a.txt", "method": "PUT", "expires_in": 600, "content_type": "text/plain", "max_size": 100,
        "checksum_sha256": "ab", "upload_id": "u1", "part_number": 2,
    }
    await call(client, "presign", "photos", PresignRequest("a.txt", "GET", 60, response_content_disposition="attachment"))
    assert json.loads(seen[-1].content) == {"key": "a.txt", "method": "GET", "expires_in": 60, "response_content_disposition": "attachment"}
