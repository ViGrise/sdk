"""Client for ViGrise Storage: buckets, objects, multipart uploads, tags, presigned URLs and lifecycle rules."""

from ._errors import StorageError
from ._models import (
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
    StorageObject,
    UploadedPart,
    UploadsPage,
    UploadSummary,
)
from .client import (
    DEFAULT_PART_SIZE,
    AsyncStorageClient,
    AsyncStorageObjectStream,
    StorageClient,
    StorageObjectStream,
)

__all__ = [name for name in dir() if name[:1].isupper()]
