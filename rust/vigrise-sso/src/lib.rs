//! First-party ViGrise SSO for apps on `*.vigrise.com`.
//!
//! An app reads the `vigrise_sso` cookie on the server, introspects it through
//! [`SsoIntrospector`] and issues its own session. The Accounts client of the
//! app needs `first_party` and `sso_introspection`, which only operators set.
//!
//! ```no_run
//! use vigrise_sso::{SsoIntrospector, SsoIntrospectorConfig};
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

mod error;
#[cfg(feature = "axum")]
mod extractor;
mod introspect;

pub use error::SsoError;
#[cfg(feature = "axum")]
#[cfg_attr(docsrs, doc(cfg(feature = "axum")))]
pub use extractor::{AccountsIdentity, AccountsRejection, SSO_HEADER};
pub use introspect::{
  Introspection, SsoAccount, SsoIdentity, SsoIntrospector, SsoIntrospectorConfig, SsoSession,
  sso_token_hash,
};
pub use vigrise::accounts::AccountsError;
