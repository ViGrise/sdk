//! VGR4 request signing and verification for Vigrise services.
//!
//! VGR4 follows AWS Signature Version 4 without the region component: the
//! credential scope is `<yyyymmdd>/<service>/vgr4_request`.

pub mod authorization;
pub mod canonical;
pub mod credentials;
pub mod error;
pub mod signer;
pub mod signing;

pub use authorization::AuthorizationHeader;
pub use credentials::{Credentials, CredentialsError};
pub use error::AuthError;
pub use signer::{sign_request, SignedRequestHeaders};
