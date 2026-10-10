//! Client for ViGrise Storage. Every call is VGR4-signed with the given
//! credentials; streamed uploads sign `UNSIGNED-PAYLOAD`.
//!
//! ```no_run
//! use std::sync::Arc;
//! use vigrise_auth::Credentials;
//! use vigrise_storage::{PutOptions, StorageClient};
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let credentials = Arc::new(Credentials::resolve()?);
//! let storage = StorageClient::new("https://storage.vigrise.com", credentials);
//! storage.create_bucket("photos").await?;
//! storage
//!     .put_object("photos", "2026/cat.jpg", &b"..."[..], &PutOptions::default())
//!     .await?;
//! let object = storage.get_object("photos", "2026/cat.jpg", None).await?;
//! println!("{} bytes", object.body.len());
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

mod client;
mod error;
mod types;

pub use client::StorageClient;
pub use error::StorageError;
pub use types::*;
