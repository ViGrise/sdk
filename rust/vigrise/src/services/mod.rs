//! The ViGrise services, each with a client, its types and its errors.

#[cfg(feature = "accounts")]
pub mod accounts;
#[cfg(feature = "speech")]
pub mod speech;
#[cfg(feature = "storage")]
pub mod storage;
