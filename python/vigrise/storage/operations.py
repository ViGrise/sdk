"""What each Storage operation sends and how it reads the answer. The sync and async clients share these."""

import json
from typing import Any, Dict, Iterable, List, Optional

import httpx

from .._common.http import Request, query_pairs, segment
from .errors import transport_error
from .dto import (
    Bucket,
    CompletedPart,
    LifecycleRule,
    ListPage,
    ObjectHead,
    ObjectSummary,
    PresignRequest,
    Presigned,
    PutOptions,
    PutResult,
    Range,
    StorageObject,
    UploadedPart,
    UploadsPage,
    UploadSummary,
)

# Smallest part of a multipart upload, and the default part size of `upload_stream`.
DEFAULT_PART_SIZE = 5 * 1024 * 1024


def _json(method: str, path: str, body: Any, headers: Optional[Dict[str, str]] = None) -> Request:
    merged = {"content-type": "application/json"}
    merged.update(headers or {})
    return Request(method, path, headers=merged, content=json.dumps(body, separators=(",", ":")).encode())


def _without_none(fields: Dict[str, Any]) -> Dict[str, Any]:
    return {name: value for name, value in fields.items() if value is not None}


def read_json(response: httpx.Response) -> Any:
    try:
        return response.json()
    except ValueError as error:
        raise transport_error(f"the answer is not JSON: {error}") from error


def object_headers(options: PutOptions) -> Dict[str, str]:
    headers: Dict[str, str] = {}
    if options.content_type is not None:
        headers["content-type"] = options.content_type
    for name, value in options.metadata.items():
        headers[f"x-vgr-meta-{name}"] = value
    if options.tags:
        headers["x-vgr-tagging"] = "&".join(f"{segment(name)}={segment(value)}" for name, value in sorted(options.tags.items()))
    return headers


def key_path(key: str) -> str:
    return "/".join(segment(part) for part in key.split("/"))


def _object_path(bucket: str, key: str) -> str:
    return f"/v1/buckets/{segment(bucket)}/objects/{key_path(key)}"


def create_bucket_request(name: str) -> Request:
    return _json("POST", "/v1/buckets", {"name": name})


def bucket_of(data: Dict[str, Any]) -> Bucket:
    return Bucket(data["name"], data["project_id"], data["created_at"])


def parse_bucket(response: httpx.Response) -> Bucket:
    return bucket_of(read_json(response))


def list_buckets_request() -> Request:
    return Request("GET", "/v1/buckets")


def parse_buckets(response: httpx.Response) -> List[Bucket]:
    return [bucket_of(item) for item in read_json(response)["buckets"]]


def get_bucket_request(name: str) -> Request:
    return Request("GET", f"/v1/buckets/{segment(name)}")


def delete_bucket_request(name: str) -> Request:
    return Request("DELETE", f"/v1/buckets/{segment(name)}")


def put_object_request(bucket: str, key: str, body: bytes, options: PutOptions) -> Request:
    return Request("PUT", _object_path(bucket, key), headers=object_headers(options), content=body)


def put_object_stream_request(bucket: str, key: str, body: Any, length: int, options: PutOptions) -> Request:
    headers = object_headers(options)
    headers["content-length"] = str(length)
    return Request("PUT", _object_path(bucket, key), headers=headers, stream=body)


def parse_put_result(response: httpx.Response) -> PutResult:
    data = read_json(response)
    return PutResult(data["etag"], data["size"])


def read_object_request(method: str, bucket: str, key: str, range: Optional[Range]) -> Request:
    headers = {"range": f"bytes={range[0]}-{range[1]}"} if range else {}
    return Request(method, _object_path(bucket, key), headers=headers)


def object_head(response: httpx.Response) -> Dict[str, Any]:
    header = response.headers.get
    content_range = header("content-range")
    length = header("content-length")
    total = content_range.rsplit("/", 1)[-1] if content_range else None
    size: Optional[int] = None
    if total is not None and total != "*":
        size = int(total)
    elif length is not None:
        size = int(length)
    etag = header("etag")
    prefix = "x-vgr-meta-"
    return dict(
        content_type=header("content-type"),
        etag=etag.strip('"') if etag else None,
        size=size,
        metadata={name[len(prefix):]: value for name, value in response.headers.items() if name.lower().startswith(prefix)},
        checksum_sha256=header("x-vgr-checksum-sha256"),
    )


def parse_object(response: httpx.Response) -> StorageObject:
    return StorageObject(body=response.content, **object_head(response))


def delete_object_request(bucket: str, key: str) -> Request:
    return Request("DELETE", _object_path(bucket, key))


def list_objects_request(bucket: str, prefix, delimiter, max_keys, continuation_token, start_after) -> Request:
    return Request(
        "GET",
        f"/v1/buckets/{segment(bucket)}/objects",
        query=query_pairs(prefix=prefix, delimiter=delimiter, max_keys=max_keys, continuation_token=continuation_token, start_after=start_after),
    )


def parse_list_page(response: httpx.Response) -> ListPage:
    data = read_json(response)
    objects = [
        ObjectSummary(item["key"], item["size"], item["etag"], item["content_type"], item["last_modified"]) for item in data["objects"]
    ]
    return ListPage(objects, data["common_prefixes"], data.get("next_continuation_token"))


def copy_object_request(source_bucket: str, source_key: str, bucket: str, key: str, replace: Optional[PutOptions]) -> Request:
    headers = object_headers(replace) if replace is not None else {}
    if replace is not None:
        headers["x-vgr-metadata-directive"] = "REPLACE"
    headers["x-vgr-copy-source"] = f"{source_bucket}/{key_path(source_key)}"
    return Request("PUT", _object_path(bucket, key), headers=headers)


def list_uploads_request(bucket: str, prefix, delimiter, max_uploads, key_marker, upload_id_marker) -> Request:
    return Request(
        "GET",
        f"/v1/buckets/{segment(bucket)}/uploads",
        query=query_pairs(prefix=prefix, delimiter=delimiter, max_uploads=max_uploads, key_marker=key_marker, upload_id_marker=upload_id_marker),
    )


def parse_uploads_page(response: httpx.Response) -> UploadsPage:
    data = read_json(response)
    return UploadsPage(
        [UploadSummary(item["upload_id"], item["key"], item["initiated"]) for item in data["uploads"]],
        data["common_prefixes"],
        data["is_truncated"],
        data.get("next_key_marker"),
        data.get("next_upload_id_marker"),
    )


def create_upload_request(bucket: str, key: str, options: PutOptions) -> Request:
    body = {"key": key, "content_type": options.content_type, "metadata": options.metadata, "tags": options.tags}
    return _json("POST", f"/v1/buckets/{segment(bucket)}/uploads", _without_none(body))


def parse_upload_id(response: httpx.Response) -> str:
    return read_json(response)["upload_id"]


def upload_part_request(bucket: str, upload_id: str, number: int, body: bytes) -> Request:
    return Request("PUT", f"/v1/buckets/{segment(bucket)}/uploads/{segment(upload_id)}/parts/{number}", content=body)


def parse_completed_part(response: httpx.Response) -> CompletedPart:
    data = read_json(response)
    return CompletedPart(data["number"], data["etag"])


def list_parts_request(bucket: str, upload_id: str) -> Request:
    return Request("GET", f"/v1/buckets/{segment(bucket)}/uploads/{segment(upload_id)}/parts")


def parse_parts(response: httpx.Response) -> List[UploadedPart]:
    return [UploadedPart(item["number"], item["etag"], item["size"]) for item in read_json(response)["parts"]]


def complete_upload_request(bucket: str, upload_id: str, parts: Iterable[CompletedPart]) -> Request:
    body = {"parts": [{"number": part.number, "etag": part.etag} for part in parts]}
    return _json("POST", f"/v1/buckets/{segment(bucket)}/uploads/{segment(upload_id)}/complete", body)


def abort_upload_request(bucket: str, upload_id: str) -> Request:
    return Request("DELETE", f"/v1/buckets/{segment(bucket)}/uploads/{segment(upload_id)}")


def put_tags_request(bucket: str, key: str, tags: Dict[str, str]) -> Request:
    return _json("PUT", f"{_object_path(bucket, key)}?tagging", {"tags": tags})


def get_tags_request(bucket: str, key: str) -> Request:
    return Request("GET", f"{_object_path(bucket, key)}?tagging")


def parse_tags(response: httpx.Response) -> Dict[str, str]:
    return read_json(response)["tags"]


def delete_tags_request(bucket: str, key: str) -> Request:
    return Request("DELETE", f"{_object_path(bucket, key)}?tagging")


def presign_request(bucket: str, request: PresignRequest) -> Request:
    body = _without_none({
        "key": request.key,
        "method": request.method,
        "expires_in": request.expires_in,
        "content_type": request.content_type,
        "max_size": request.max_size,
        "response_content_disposition": request.response_content_disposition,
        "checksum_sha256": request.checksum_sha256,
        "upload_id": request.upload_id,
        "part_number": request.part_number,
    })
    return _json("POST", f"/v1/buckets/{segment(bucket)}/presign", body)


def parse_presigned(response: httpx.Response) -> Presigned:
    data = read_json(response)
    return Presigned(data["url"], data["expires_at"])


def put_lifecycle_request(bucket: str, rules: List[LifecycleRule]) -> Request:
    body = {
        "rules": [
            _without_none({
                "id": rule.id,
                "prefix": rule.prefix,
                "tags": rule.tags or None,
                "expire_after_days": rule.expire_after_days,
                "abort_incomplete_uploads_after_days": rule.abort_incomplete_uploads_after_days,
                "enabled": rule.enabled,
            })
            for rule in rules
        ]
    }
    return _json("PUT", f"/v1/buckets/{segment(bucket)}/lifecycle", body)


def get_lifecycle_request(bucket: str) -> Request:
    return Request("GET", f"/v1/buckets/{segment(bucket)}/lifecycle")


def parse_lifecycle(response: httpx.Response) -> List[LifecycleRule]:
    return [
        LifecycleRule(
            id=item["id"],
            prefix=item.get("prefix", ""),
            tags=item.get("tags") or {},
            expire_after_days=item.get("expire_after_days"),
            abort_incomplete_uploads_after_days=item.get("abort_incomplete_uploads_after_days"),
            enabled=item.get("enabled", True),
        )
        for item in read_json(response)["rules"]
    ]


def delete_lifecycle_request(bucket: str) -> Request:
    return Request("DELETE", f"/v1/buckets/{segment(bucket)}/lifecycle")
