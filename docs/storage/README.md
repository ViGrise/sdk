# Storage

Buckets, objects, multipart uploads, tags, presigned URLs and lifecycle rules
over the native `/v1/buckets/...` API. The endpoint is
`https://storage-api.vigrise.com` unless [overridden](../auth/README.md#endpoints);
every call is VGR4-signed for the `storage` service. The S3-compatible API has
its own host and is used with any S3 client.

| | Rust | TypeScript | Python |
| --- | --- | --- | --- |
| Package | `vigrise`, feature `storage` | `@vigrise/sdk/storage` | `vigrise.storage` (extra `storage`) |
| Client | `StorageClient` | `StorageClient` | `StorageClient`, `AsyncStorageClient` |
| Runs in | Tokio | Node 24 (browsers use presigned URLs) | Python 3.9+ |

## Buckets and objects

```rust
use std::sync::Arc;
use vigrise::auth::Credentials;
use vigrise::storage::{PutOptions, StorageClient};

let storage = StorageClient::resolved(Arc::new(Credentials::resolve()?));
storage.create_bucket("photos").await?;
storage.put_object("photos", "2026/cat.jpg", &b"..."[..], &PutOptions::default()).await?;
let object = storage.get_object("photos", "2026/cat.jpg", None).await?;
```

```ts
import { StorageClient } from "@vigrise/sdk/storage";

const storage = new StorageClient();
await storage.createBucket("photos");
await storage.putObject("photos", "2026/cat.jpg", bytes, { contentType: "image/jpeg" });
const object = await storage.getObject("photos", "2026/cat.jpg", { range: [0, 1023] });
```

```python
from vigrise.storage import StorageClient

with StorageClient() as storage:
    storage.create_bucket("photos")
    storage.put_object("photos", "2026/cat.jpg", data, content_type="image/jpeg")
    obj = storage.get_object("photos", "2026/cat.jpg", range=(0, 1023))
```

`put_object` signs the payload hash. Streamed uploads of known length
(`put_object_stream`) sign `UNSIGNED-PAYLOAD`. Also: `head_object`,
`delete_object`, `list_objects` (prefix, delimiter, continuation token),
`copy_object`.

## Multipart uploads

`upload_stream` (`uploadStream`) splits a stream of any length into parts of
5 MiB by default and aborts the upload when a part fails. The lower-level
calls are `create_upload`, `upload_part`, `list_parts`, `complete_upload`,
`abort_upload` and `list_uploads`.

```rust
storage.upload_stream("photos", "big.bin", chunks, 8 * 1024 * 1024, &PutOptions::default()).await?;
```

```ts
await storage.uploadStream("photos", "big.bin", readable, { partSize: 8 * 1024 * 1024 });
```

```python
storage.upload_stream("photos", "big.bin", open("big.bin", "rb"), part_size=8 * 1024 * 1024)
```

## Tags, lifecycle and presigned URLs

`put_tags`, `get_tags` and `delete_tags` act on an object;
`put_lifecycle`, `get_lifecycle` and `delete_lifecycle` on a bucket (rules
expire objects or abort stale uploads). `presign(bucket, request)` returns a URL
for a `GET` or `PUT` that works without credentials until it expires; browsers
upload and download through such URLs.

```ts
const { url } = await storage.presign("photos", { key: "2026/cat.jpg", method: "GET", expiresIn: 600 });
```

## Errors

`StorageError` carries the HTTP `status`, the `code` and the `message` of the
error envelope. In TypeScript and Python a failure before an answer has status
0 and the code `TransportError` or `SigningError`; Rust has separate
`Transport` and `Signing` variants.
