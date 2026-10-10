//! VGR4 request signing and verification for ViGrise services.
//!
//! VGR4 follows AWS Signature Version 4 without the region component: the
//! credential scope is `<yyyymmdd>/<service>/vgr4_request`. The `sigv4` module
//! implements AWS Signature Version 4 for S3-compatible services.
//!
//! # Signing a request
//!
//! ```
//! use http::{HeaderMap, Method, Uri};
//! use vigrise_auth::{sign_request, Credentials};
//!
//! # fn main() -> Result<(), vigrise_auth::AuthError> {
//! let credentials = Credentials::new("VGAKEXAMPLE", "secret");
//! let uri: Uri = "https://speech.vigrise.com/v1/voices".parse().unwrap();
//! let mut headers = HeaderMap::new();
//! let signed = sign_request(
//!   &Method::GET,
//!   &uri,
//!   &headers,
//!   b"",
//!   &credentials,
//!   "speech",
//!   chrono::Utc::now(),
//! )?;
//! signed.apply(&mut headers)?;
//! assert!(headers.contains_key("authorization"));
//! # Ok(())
//! # }
//! ```
//!
//! # Features
//!
//! | Feature | Adds |
//! | --- | --- |
//! | `reqwest` | `reqwest_signer`: signing of `reqwest` requests |
//! | `sts` | `sts`: role sessions and `AssumeRoleProvider` |
//! | `verify` | `verify`: an `axum` middleware for services that accept VGR4 requests |
//! | `sigv4` | AWS Signature Version 4 verification with aws-chunked bodies and additional checksums |
//! | `metering` | `metering`: background reporting of usage |
//! | `openapi` | `utoipa` schemas for the wire types |

#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

/// The VGR4 `Authorization` header.
pub mod authorization;
pub mod canonical;
#[cfg(feature = "sigv4")]
pub mod checksum;
#[cfg(feature = "sigv4")]
pub mod chunked;
/// Access key and session token credentials, resolved from the environment or
/// the credentials file.
pub mod credentials;
/// Errors of VGR4 authentication and authorization.
pub mod error;
/// Sources of credentials to sign requests with.
pub mod provider;
mod quota;
/// VGR4 signing of requests.
pub mod signer;
pub mod signing;
pub mod sigv4;

#[cfg(feature = "metering")]
pub mod metering;
/// Signing of `reqwest` requests.
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
