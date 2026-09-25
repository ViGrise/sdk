//! VGR4 request signing and verification for Vigrise services.
//!
//! VGR4 follows AWS Signature Version 4 without the region component: the
//! credential scope is `<yyyymmdd>/<service>/vgr4_request`.

pub mod canonical;
pub mod error;
pub mod signing;

pub use error::AuthError;
