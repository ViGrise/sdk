//! Client for ViGrise Storage. Every call is VGR4-signed with the given
//! credentials; streamed uploads sign `UNSIGNED-PAYLOAD`.

mod client;
mod error;
mod types;

pub use client::StorageClient;
pub use error::StorageError;
pub use types::*;
