# vigrise-storage

Client for ViGrise Storage: buckets, objects (signed or streamed with
`UNSIGNED-PAYLOAD`), multipart uploads with a streaming helper, tags, presigned
URLs and lifecycle rules. Every call is signed with VGR4 through
[`vigrise-auth`](https://crates.io/crates/vigrise-auth).

## Installation

```sh
cargo add vigrise-storage vigrise-auth
```

## Example

```rust
use std::sync::Arc;
use vigrise_auth::Credentials;
use vigrise_storage::{PutOptions, StorageClient};

let credentials = Arc::new(Credentials::resolve()?);
let storage = StorageClient::new("https://storage.vigrise.com", credentials);

storage.create_bucket("photos").await?;
storage
    .put_object("photos", "2026/cat.jpg", &b"..."[..], &PutOptions::default())
    .await?;
let object = storage.get_object("photos", "2026/cat.jpg", None).await?;
println!("{} bytes", object.body.len());
```

## Credentials

`StorageClient::new` takes any `CredentialsProvider`. `Credentials::resolve()`
reads, in this order:

1. `VIGRISE_ACCESS_KEY_ID` and `VIGRISE_SECRET_ACCESS_KEY`, with
   `VIGRISE_SESSION_TOKEN` for a role session.
2. Profile `VIGRISE_PROFILE` (default `default`) in
   `VIGRISE_SHARED_CREDENTIALS_FILE` (default `~/.vigrise/credentials`).

For long-running programs, `vigrise_auth::sts::AssumeRoleProvider` (the `sts`
feature of `vigrise-auth`) keeps a role session fresh.

## Features

`vigrise-storage` has no features of its own.

## Documentation

API reference: <https://docs.rs/vigrise-storage>.

## License

Apache-2.0
