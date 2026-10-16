//! ViGrise SDK for Rust: request signing, credentials and clients for ViGrise
//! services in one crate.
//!
//! [`auth`] is always available. The other modules are enabled by Cargo
//! features of the same name.

#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod auth;
mod common;
mod services;

/// ViGrise Accounts: OAuth2/OIDC helpers and back-channel logout.
#[cfg(feature = "accounts")]
#[cfg_attr(docsrs, doc(cfg(feature = "accounts")))]
pub use services::accounts;

/// Client for ViGrise Storage.
#[cfg(feature = "storage")]
#[cfg_attr(docsrs, doc(cfg(feature = "storage")))]
pub use services::storage;

/// Client for the Speech API.
#[cfg(feature = "speech")]
#[cfg_attr(docsrs, doc(cfg(feature = "speech")))]
pub use services::speech;
