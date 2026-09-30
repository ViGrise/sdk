//! VGR4 request signing and verification for ViGrise services.
//!
//! VGR4 follows AWS Signature Version 4 without the region component: the
//! credential scope is `<yyyymmdd>/<service>/vgr4_request`. The `sigv4` module
//! implements AWS Signature Version 4 for S3-compatible services.

pub mod authorization;
pub mod canonical;
#[cfg(feature = "verify")]
pub mod checksum;
#[cfg(feature = "verify")]
pub mod chunked;
pub mod credentials;
pub mod error;
pub mod provider;
mod quota;
pub mod signer;
pub mod signing;
pub mod sigv4;

#[cfg(feature = "metering")]
pub mod metering;
#[cfg(feature = "reqwest")]
pub mod reqwest_signer;
#[cfg(feature = "sts")]
pub mod sts;
#[cfg(feature = "verify")]
pub mod verify;

pub use authorization::AuthorizationHeader;
pub use credentials::{Credentials, CredentialsError};
pub use error::AuthError;
pub use provider::{CredentialsFuture, CredentialsProvider};
pub use quota::QuotaStatus;
pub use signer::{
  sign_request, sign_request_with_payload_hash, SignedRequestHeaders, UNSIGNED_PAYLOAD,
};
