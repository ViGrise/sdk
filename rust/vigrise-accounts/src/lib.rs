//! ViGrise Accounts for Rust services: SSO introspection, back-channel logout
//! verification and OAuth2/OIDC helpers.
//!
//! First-party ViGrise apps read the `vigrise_sso` cookie on the server,
//! introspect it through [`SsoIntrospector`] and issue their own sessions.
//! Accounts calls an app's back-channel logout endpoint when a session ends;
//! [`BackchannelLogout`] verifies that call. Apps on other domains use the
//! [`oauth2`] helpers for the authorization code flow with PKCE.
//!
//! ```no_run
//! use vigrise_accounts::{SsoIntrospector, SsoIntrospectorConfig};
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let config = SsoIntrospectorConfig::new("http://accounts-backend:8080/o/auth", "vigrise-drive", "secret");
//! let introspector = SsoIntrospector::new(config);
//! let answer = introspector.introspect("value of the vigrise_sso cookie").await?;
//! if answer.active {
//!     println!("signed in as {:?}", answer.identity.unwrap().email);
//! }
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

mod backchannel;
mod error;
#[cfg(feature = "axum")]
mod extractor;
mod introspect;
mod jwks;
pub mod oauth2;
#[cfg(feature = "testing")]
#[cfg_attr(docsrs, doc(cfg(feature = "testing")))]
pub mod testing;

pub use backchannel::{BackchannelLogout, BackchannelOutcome, LogoutToken};
pub use error::AccountsError;
#[cfg(feature = "axum")]
#[cfg_attr(docsrs, doc(cfg(feature = "axum")))]
pub use extractor::{AccountsIdentity, AccountsRejection, SSO_HEADER};
pub use introspect::{
    sso_token_hash, Introspection, SsoAccount, SsoIdentity, SsoIntrospector, SsoIntrospectorConfig,
    SsoSession,
};
pub use jwks::Jwks;
