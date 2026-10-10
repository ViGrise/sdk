//! Library behind the `vigrise` command line: the browser and device sign-in
//! at ViGrise Accounts, and the profiles of the shared credentials file.
//!
//! ```no_run
//! use std::path::Path;
//! use vigrise_cli::profile::{describe_session, read_session};
//!
//! # fn main() -> std::io::Result<()> {
//! let file = Path::new("/home/me/.vigrise/credentials");
//! if let Some(session) = read_session(file, "dev")? {
//!   println!("{}", describe_session(&session, chrono::Utc::now()));
//! }
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

/// Sign-in at ViGrise Accounts and exchange for a role session.
pub mod login;
/// Opening a URL in the default browser.
pub mod opener;
/// Role sessions in the shared credentials file.
pub mod profile;
