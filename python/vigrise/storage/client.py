from typing import Any, AsyncIterable, AsyncIterator, Dict, Iterable, Iterator, List, Optional, Union

import httpx

from .. import _http
from .._client import AsyncSender, Failures, SyncSender
from ..auth.credentials import CredentialsSource
from ..auth.endpoint import resolve_endpoint
from . import _operations as ops
from ._errors import StorageError, error_from_response, signing_error, transport_error
from ._models import (
    Bucket,
    CompletedPart,
    LifecycleRule,
    ListPage,
    PresignRequest,
    Presigned,
    PutOptions,
    PutResult,
    Range,
    StorageObject,
    UploadedPart,
    UploadsPage,
)

DEFAULT_PART_SIZE = ops.DEFAULT_PART_SIZE

_FAILURES = Failures(signing_error, transport_error, error_from_response)
_READ_SIZE = 64 * 1024

SyncBody = Union[Iterable[bytes], Any]
"""An iterable of byte chunks, or a file-like object with `read`."""

AsyncBody = Union[AsyncIterable[bytes], Iterable[bytes], Any]
"""An async iterable of byte chunks, an iterable of byte chunks, or a file-like object with `read`."""


def _options(content_type, metadata, tags) -> PutOptions:
    return PutOptions(content_type, dict(metadata or {}), dict(tags or {}))


def _sync_chunks(body: SyncBody) -> Iterator[bytes]:
    if hasattr(body, "read"):
        while True:
            chunk = body.read(_READ_SIZE)
            if not chunk:
                return
            yield chunk
    else:
        yield from body


async def _async_chunks(body: AsyncBody) -> AsyncIterator[bytes]:
    if hasattr(body, "__aiter__"):
        async for chunk in body:
            yield chunk
    else:
        for chunk in _sync_chunks(body):
            yield chunk


class _Stream:
    def __init__(self, response: httpx.Response):
        self._response = response
        head = ops.object_head(response)
        self.content_type = head["content_type"]
        self.etag = head["etag"]
        self.size = head["size"]
        self.metadata = head["metadata"]
        self.checksum_sha256 = head["checksum_sha256"]


class StorageObjectStream(_Stream):
    """A download whose body is read as it arrives. Iterate it for the chunks and close it when done."""

    def __iter__(self) -> Iterator[bytes]:
        try:
            yield from self._response.iter_bytes()
        except httpx.HTTPError as error:
            raise transport_error(str(error)) from error

    def close(self) -> None:
        self._response.close()

    def __enter__(self) -> "StorageObjectStream":
        return self

    def __exit__(self, *exc_info) -> None:
        self.close()


class AsyncStorageObjectStream(_Stream):
    """A download whose body is read as it arrives. Iterate it for the chunks and close it when done."""

    async def __aiter__(self) -> AsyncIterator[bytes]:
        try:
            async for chunk in self._response.aiter_bytes():
                yield chunk
        except httpx.HTTPError as error:
            raise transport_error(str(error)) from error

    async def aclose(self) -> None:
        await self._response.aclose()

    async def __aenter__(self) -> "AsyncStorageObjectStream":
        return self

    async def __aexit__(self, *exc_info) -> None:
        await self.aclose()


class StorageClient:
    """A client for ViGrise Storage. Every call is VGR4-signed for the `storage` service."""

    def __init__(
        self,
        endpoint: Optional[str] = None,
        credentials: Optional[CredentialsSource] = None,
        *,
        timeout: Optional[float] = 60.0,
        transport: Optional[httpx.BaseTransport] = None,
    ):
        self._sender = SyncSender("storage", resolve_endpoint("storage", endpoint), credentials, timeout, _FAILURES, transport)

    def close(self) -> None:
        self._sender.close()

    def __enter__(self) -> "StorageClient":
        return self

    def __exit__(self, *exc_info) -> None:
        self.close()

    def _send(self, request: _http.Request, stream: bool = False) -> httpx.Response:
        return self._sender.send(request, stream)

    def create_bucket(self, name: str) -> Bucket:
        """Creates a bucket in the caller's project."""
        return ops.parse_bucket(self._send(ops.create_bucket_request(name)))

    def list_buckets(self) -> List[Bucket]:
        return ops.parse_buckets(self._send(ops.list_buckets_request()))

    def get_bucket(self, name: str) -> Bucket:
        return ops.parse_bucket(self._send(ops.get_bucket_request(name)))

    def delete_bucket(self, name: str) -> None:
        self._send(ops.delete_bucket_request(name))

    def put_object(self, bucket: str, key: str, body: bytes, *, content_type: Optional[str] = None,
                   metadata: Optional[Dict[str, str]] = None, tags: Optional[Dict[str, str]] = None) -> PutResult:
        """Uploads `body` in one request, signed with its hash."""
        options = _options(content_type, metadata, tags)
        return ops.parse_put_result(self._send(ops.put_object_request(bucket, key, bytes(body), options)))

    def put_object_stream(self, bucket: str, key: str, body: SyncBody, length: int, *, content_type: Optional[str] = None,
                          metadata: Optional[Dict[str, str]] = None, tags: Optional[Dict[str, str]] = None) -> PutResult:
        """Uploads `length` bytes from `body` in one streamed request, signed as UNSIGNED-PAYLOAD."""
        options = _options(content_type, metadata, tags)
        request = ops.put_object_stream_request(bucket, key, _sync_chunks(body), length, options)
        return ops.parse_put_result(self._send(request))

    def get_object(self, bucket: str, key: str, *, range: Optional[Range] = None) -> StorageObject:
        """Downloads an object, or the inclusive byte range `(first, last)` of it."""
        return ops.parse_object(self._send(ops.read_object_request("GET", bucket, key, range)))

    def get_object_stream(self, bucket: str, key: str, *, range: Optional[Range] = None) -> StorageObjectStream:
        """Downloads an object, or the inclusive byte range of it, as a stream."""
        return StorageObjectStream(self._send(ops.read_object_request("GET", bucket, key, range), stream=True))

    def head_object(self, bucket: str, key: str) -> StorageObject:
        """An object's headers without its body."""
        return ops.parse_object(self._send(ops.read_object_request("HEAD", bucket, key, None)))

    def delete_object(self, bucket: str, key: str) -> None:
        self._send(ops.delete_object_request(bucket, key))

    def list_objects(self, bucket: str, *, prefix: Optional[str] = None, delimiter: Optional[str] = None,
                     max_keys: Optional[int] = None, continuation_token: Optional[str] = None,
                     start_after: Optional[str] = None) -> ListPage:
        """Lists the objects of a bucket, one page at a time."""
        request = ops.list_objects_request(bucket, prefix, delimiter, max_keys, continuation_token, start_after)
        return ops.parse_list_page(self._send(request))

    def copy_object(self, source_bucket: str, source_key: str, bucket: str, key: str, *,
                    replace: Optional[PutOptions] = None) -> PutResult:
        """Copies an object, in this or another bucket, without moving its bytes. With `replace`, the new
        object takes its content type, metadata and tags from it; otherwise it keeps the source's."""
        return ops.parse_put_result(self._send(ops.copy_object_request(source_bucket, source_key, bucket, key, replace)))

    def list_uploads(self, bucket: str, *, prefix: Optional[str] = None, delimiter: Optional[str] = None,
                     max_uploads: Optional[int] = None, key_marker: Optional[str] = None,
                     upload_id_marker: Optional[str] = None) -> UploadsPage:
        """Lists the multipart uploads in progress."""
        request = ops.list_uploads_request(bucket, prefix, delimiter, max_uploads, key_marker, upload_id_marker)
        return ops.parse_uploads_page(self._send(request))

    def create_upload(self, bucket: str, key: str, *, content_type: Optional[str] = None,
                      metadata: Optional[Dict[str, str]] = None, tags: Optional[Dict[str, str]] = None) -> str:
        """Starts a multipart upload and returns its id."""
        options = _options(content_type, metadata, tags)
        return ops.parse_upload_id(self._send(ops.create_upload_request(bucket, key, options)))

    def upload_part(self, bucket: str, upload_id: str, number: int, body: bytes) -> CompletedPart:
        """Uploads part `number` of a multipart upload in one request."""
        return ops.parse_completed_part(self._send(ops.upload_part_request(bucket, upload_id, number, bytes(body))))

    def list_parts(self, bucket: str, upload_id: str) -> List[UploadedPart]:
        """The parts stored so far in a multipart upload, by part number."""
        return ops.parse_parts(self._send(ops.list_parts_request(bucket, upload_id)))

    def complete_upload(self, bucket: str, upload_id: str, parts: Iterable[CompletedPart]) -> PutResult:
        """Completes a multipart upload from its parts and returns the stored object."""
        return ops.parse_put_result(self._send(ops.complete_upload_request(bucket, upload_id, parts)))

    def abort_upload(self, bucket: str, upload_id: str) -> None:
        """Aborts a multipart upload and discards its parts."""
        self._send(ops.abort_upload_request(bucket, upload_id))

    def upload_stream(self, bucket: str, key: str, stream: SyncBody, *, part_size: int = DEFAULT_PART_SIZE,
                      content_type: Optional[str] = None, metadata: Optional[Dict[str, str]] = None,
                      tags: Optional[Dict[str, str]] = None) -> PutResult:
        """Uploads a stream of any length in parts of `part_size` bytes, aborting the upload when a part fails."""
        upload_id = self.create_upload(bucket, key, content_type=content_type, metadata=metadata, tags=tags)
        try:
            parts: List[CompletedPart] = []
            buffer = bytearray()
            for chunk in _sync_chunks(stream):
                buffer += chunk
                while len(buffer) >= part_size:
                    parts.append(self.upload_part(bucket, upload_id, len(parts) + 1, bytes(buffer[:part_size])))
                    del buffer[:part_size]
            if buffer or not parts:
                parts.append(self.upload_part(bucket, upload_id, len(parts) + 1, bytes(buffer)))
            return self.complete_upload(bucket, upload_id, parts)
        except BaseException:
            try:
                self.abort_upload(bucket, upload_id)
            except StorageError:
                pass
            raise

    def put_tags(self, bucket: str, key: str, tags: Dict[str, str]) -> None:
        """Replaces the tags of an object."""
        self._send(ops.put_tags_request(bucket, key, tags))

    def get_tags(self, bucket: str, key: str) -> Dict[str, str]:
        return ops.parse_tags(self._send(ops.get_tags_request(bucket, key)))

    def delete_tags(self, bucket: str, key: str) -> None:
        self._send(ops.delete_tags_request(bucket, key))

    def presign(self, bucket: str, request: PresignRequest) -> Presigned:
        """Creates a presigned URL for downloading or uploading an object without credentials."""
        return ops.parse_presigned(self._send(ops.presign_request(bucket, request)))

    def put_lifecycle(self, bucket: str, rules: List[LifecycleRule]) -> None:
        """Replaces the lifecycle rules of a bucket."""
        self._send(ops.put_lifecycle_request(bucket, rules))

    def get_lifecycle(self, bucket: str) -> List[LifecycleRule]:
        return ops.parse_lifecycle(self._send(ops.get_lifecycle_request(bucket)))

    def delete_lifecycle(self, bucket: str) -> None:
        self._send(ops.delete_lifecycle_request(bucket))


class AsyncStorageClient:
    """The async counterpart of `StorageClient`, with the same operations."""

    def __init__(
        self,
        endpoint: Optional[str] = None,
        credentials: Optional[CredentialsSource] = None,
        *,
        timeout: Optional[float] = 60.0,
        transport: Optional[httpx.AsyncBaseTransport] = None,
    ):
        self._sender = AsyncSender("storage", resolve_endpoint("storage", endpoint), credentials, timeout, _FAILURES, transport)

    async def aclose(self) -> None:
        await self._sender.aclose()

    async def __aenter__(self) -> "AsyncStorageClient":
        return self

    async def __aexit__(self, *exc_info) -> None:
        await self.aclose()

    async def _send(self, request: _http.Request, stream: bool = False) -> httpx.Response:
        return await self._sender.send(request, stream)

    async def create_bucket(self, name: str) -> Bucket:
        """Creates a bucket in the caller's project."""
        return ops.parse_bucket(await self._send(ops.create_bucket_request(name)))

    async def list_buckets(self) -> List[Bucket]:
        return ops.parse_buckets(await self._send(ops.list_buckets_request()))

    async def get_bucket(self, name: str) -> Bucket:
        return ops.parse_bucket(await self._send(ops.get_bucket_request(name)))

    async def delete_bucket(self, name: str) -> None:
        await self._send(ops.delete_bucket_request(name))

    async def put_object(self, bucket: str, key: str, body: bytes, *, content_type: Optional[str] = None,
                         metadata: Optional[Dict[str, str]] = None, tags: Optional[Dict[str, str]] = None) -> PutResult:
        """Uploads `body` in one request, signed with its hash."""
        options = _options(content_type, metadata, tags)
        return ops.parse_put_result(await self._send(ops.put_object_request(bucket, key, bytes(body), options)))

    async def put_object_stream(self, bucket: str, key: str, body: AsyncBody, length: int, *, content_type: Optional[str] = None,
                                metadata: Optional[Dict[str, str]] = None, tags: Optional[Dict[str, str]] = None) -> PutResult:
        """Uploads `length` bytes from `body` in one streamed request, signed as UNSIGNED-PAYLOAD."""
        options = _options(content_type, metadata, tags)
        request = ops.put_object_stream_request(bucket, key, _async_chunks(body), length, options)
        return ops.parse_put_result(await self._send(request))

    async def get_object(self, bucket: str, key: str, *, range: Optional[Range] = None) -> StorageObject:
        """Downloads an object, or the inclusive byte range `(first, last)` of it."""
        return ops.parse_object(await self._send(ops.read_object_request("GET", bucket, key, range)))

    async def get_object_stream(self, bucket: str, key: str, *, range: Optional[Range] = None) -> AsyncStorageObjectStream:
        """Downloads an object, or the inclusive byte range of it, as a stream."""
        return AsyncStorageObjectStream(await self._send(ops.read_object_request("GET", bucket, key, range), stream=True))

    async def head_object(self, bucket: str, key: str) -> StorageObject:
        """An object's headers without its body."""
        return ops.parse_object(await self._send(ops.read_object_request("HEAD", bucket, key, None)))

    async def delete_object(self, bucket: str, key: str) -> None:
        await self._send(ops.delete_object_request(bucket, key))

    async def list_objects(self, bucket: str, *, prefix: Optional[str] = None, delimiter: Optional[str] = None,
                           max_keys: Optional[int] = None, continuation_token: Optional[str] = None,
                           start_after: Optional[str] = None) -> ListPage:
        """Lists the objects of a bucket, one page at a time."""
        request = ops.list_objects_request(bucket, prefix, delimiter, max_keys, continuation_token, start_after)
        return ops.parse_list_page(await self._send(request))

    async def copy_object(self, source_bucket: str, source_key: str, bucket: str, key: str, *,
                          replace: Optional[PutOptions] = None) -> PutResult:
        """Copies an object, in this or another bucket, without moving its bytes. With `replace`, the new
        object takes its content type, metadata and tags from it; otherwise it keeps the source's."""
        request = ops.copy_object_request(source_bucket, source_key, bucket, key, replace)
        return ops.parse_put_result(await self._send(request))

    async def list_uploads(self, bucket: str, *, prefix: Optional[str] = None, delimiter: Optional[str] = None,
                           max_uploads: Optional[int] = None, key_marker: Optional[str] = None,
                           upload_id_marker: Optional[str] = None) -> UploadsPage:
        """Lists the multipart uploads in progress."""
        request = ops.list_uploads_request(bucket, prefix, delimiter, max_uploads, key_marker, upload_id_marker)
        return ops.parse_uploads_page(await self._send(request))

    async def create_upload(self, bucket: str, key: str, *, content_type: Optional[str] = None,
                            metadata: Optional[Dict[str, str]] = None, tags: Optional[Dict[str, str]] = None) -> str:
        """Starts a multipart upload and returns its id."""
        options = _options(content_type, metadata, tags)
        return ops.parse_upload_id(await self._send(ops.create_upload_request(bucket, key, options)))

    async def upload_part(self, bucket: str, upload_id: str, number: int, body: bytes) -> CompletedPart:
        """Uploads part `number` of a multipart upload in one request."""
        return ops.parse_completed_part(await self._send(ops.upload_part_request(bucket, upload_id, number, bytes(body))))

    async def list_parts(self, bucket: str, upload_id: str) -> List[UploadedPart]:
        """The parts stored so far in a multipart upload, by part number."""
        return ops.parse_parts(await self._send(ops.list_parts_request(bucket, upload_id)))

    async def complete_upload(self, bucket: str, upload_id: str, parts: Iterable[CompletedPart]) -> PutResult:
        """Completes a multipart upload from its parts and returns the stored object."""
        return ops.parse_put_result(await self._send(ops.complete_upload_request(bucket, upload_id, parts)))

    async def abort_upload(self, bucket: str, upload_id: str) -> None:
        """Aborts a multipart upload and discards its parts."""
        await self._send(ops.abort_upload_request(bucket, upload_id))

    async def upload_stream(self, bucket: str, key: str, stream: AsyncBody, *, part_size: int = DEFAULT_PART_SIZE,
                            content_type: Optional[str] = None, metadata: Optional[Dict[str, str]] = None,
                            tags: Optional[Dict[str, str]] = None) -> PutResult:
        """Uploads a stream of any length in parts of `part_size` bytes, aborting the upload when a part fails."""
        upload_id = await self.create_upload(bucket, key, content_type=content_type, metadata=metadata, tags=tags)
        try:
            parts: List[CompletedPart] = []
            buffer = bytearray()
            async for chunk in _async_chunks(stream):
                buffer += chunk
                while len(buffer) >= part_size:
                    parts.append(await self.upload_part(bucket, upload_id, len(parts) + 1, bytes(buffer[:part_size])))
                    del buffer[:part_size]
            if buffer or not parts:
                parts.append(await self.upload_part(bucket, upload_id, len(parts) + 1, bytes(buffer)))
            return await self.complete_upload(bucket, upload_id, parts)
        except BaseException:
            try:
                await self.abort_upload(bucket, upload_id)
            except StorageError:
                pass
            raise

    async def put_tags(self, bucket: str, key: str, tags: Dict[str, str]) -> None:
        """Replaces the tags of an object."""
        await self._send(ops.put_tags_request(bucket, key, tags))

    async def get_tags(self, bucket: str, key: str) -> Dict[str, str]:
        return ops.parse_tags(await self._send(ops.get_tags_request(bucket, key)))

    async def delete_tags(self, bucket: str, key: str) -> None:
        await self._send(ops.delete_tags_request(bucket, key))

    async def presign(self, bucket: str, request: PresignRequest) -> Presigned:
        """Creates a presigned URL for downloading or uploading an object without credentials."""
        return ops.parse_presigned(await self._send(ops.presign_request(bucket, request)))

    async def put_lifecycle(self, bucket: str, rules: List[LifecycleRule]) -> None:
        """Replaces the lifecycle rules of a bucket."""
        await self._send(ops.put_lifecycle_request(bucket, rules))

    async def get_lifecycle(self, bucket: str) -> List[LifecycleRule]:
        return ops.parse_lifecycle(await self._send(ops.get_lifecycle_request(bucket)))

    async def delete_lifecycle(self, bucket: str) -> None:
        await self._send(ops.delete_lifecycle_request(bucket))
