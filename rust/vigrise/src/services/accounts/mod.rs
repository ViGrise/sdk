//! ViGrise Accounts for Rust services: OAuth2/OIDC helpers and back-channel
//! logout verification.
//!
//! Apps use the [`oauth2`] helpers for the authorization code flow with PKCE
//! and verify ID tokens with [`oauth2::IdTokenVerifier`]. Accounts calls an
//! app's back-channel logout endpoint when a session ends;
//! [`BackchannelLogout`] verifies that call.
//!
//! ```no_run
//! use vigrise::accounts::oauth2::Pkce;
//!
//! let pkce = Pkce::generate();
//! println!("{}", pkce.challenge);
//! ```

mod backchannel;
mod error;
mod http;
mod jwks;
pub mod oauth2;
#[cfg(feature = "accounts-testing")]
#[cfg_attr(docsrs, doc(cfg(feature = "accounts-testing")))]
pub mod testing;

pub use backchannel::{BackchannelLogout, BackchannelOutcome, LogoutToken};
pub use error::AccountsError;
pub use jwks::Jwks;
