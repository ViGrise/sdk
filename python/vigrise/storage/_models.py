from dataclasses import dataclass, field
from typing import Dict, List, Optional, Tuple


@dataclass(frozen=True)
class Bucket:
    name: str
    project_id: str
    # Creation time, RFC 3339.
    created_at: str


@dataclass(frozen=True)
class PutOptions:
    """Content type, user metadata (sent as `x-vgr-meta-<name>` headers) and tags of an object."""

    content_type: Optional[str] = None
    metadata: Dict[str, str] = field(default_factory=dict)
    tags: Dict[str, str] = field(default_factory=dict)


@dataclass(frozen=True)
class PutResult:
    etag: str
    size: int


@dataclass(frozen=True)
class ObjectHead:
    content_type: Optional[str] = None
    etag: Optional[str] = None
    # Size of the whole object, also for a range.
    size: Optional[int] = None
    metadata: Dict[str, str] = field(default_factory=dict)
    # Lowercase hex SHA-256 of the object, once the service knows it.
    checksum_sha256: Optional[str] = None


@dataclass(frozen=True)
class StorageObject(ObjectHead):
    """A downloaded object or byte range."""

    body: bytes = b""


@dataclass(frozen=True)
class ObjectSummary:
    key: str
    size: int
    etag: str
    content_type: str
    # Last modification time, RFC 3339.
    last_modified: str


@dataclass(frozen=True)
class ListPage:
    objects: List[ObjectSummary]
    common_prefixes: List[str]
    # Token of the next page; None on the last page.
    next_continuation_token: Optional[str] = None


@dataclass(frozen=True)
class UploadSummary:
    upload_id: str
    key: str
    # Start time, RFC 3339.
    initiated: str


@dataclass(frozen=True)
class UploadsPage:
    uploads: List[UploadSummary]
    common_prefixes: List[str]
    is_truncated: bool
    next_key_marker: Optional[str] = None
    next_upload_id_marker: Optional[str] = None


@dataclass(frozen=True)
class UploadedPart:
    number: int
    etag: str
    size: int


@dataclass(frozen=True)
class CompletedPart:
    number: int
    etag: str


@dataclass(frozen=True)
class PresignRequest:
    key: str
    # `GET` or `PUT`.
    method: str
    # Validity in seconds.
    expires_in: int
    # Content type an upload must declare.
    content_type: Optional[str] = None
    # Largest size in bytes an upload may have.
    max_size: Optional[int] = None
    # The `Content-Disposition` a download's response states; part of the signature.
    response_content_disposition: Optional[str] = None
    # Lowercase hex SHA-256 an upload's content must have.
    checksum_sha256: Optional[str] = None
    # With `part_number`, a `PUT` stores that part of the multipart upload.
    upload_id: Optional[str] = None
    part_number: Optional[int] = None


@dataclass(frozen=True)
class Presigned:
    url: str
    # Expiry time, RFC 3339.
    expires_at: str


@dataclass(frozen=True)
class LifecycleRule:
    # Name of the rule, unique within the bucket.
    id: str
    # Applies to keys that start with this prefix.
    prefix: str = ""
    # Applies to objects that carry all these tags.
    tags: Dict[str, str] = field(default_factory=dict)
    # Deletes objects this many days after creation.
    expire_after_days: Optional[int] = None
    # Aborts multipart uploads still incomplete after this many days.
    abort_incomplete_uploads_after_days: Optional[int] = None
    # A disabled rule is stored and read back but never applied.
    enabled: bool = True


Range = Tuple[int, int]
