//! Code the services share: endpoint resolution, signed requests, the error
//! envelope, multipart bodies and pagination.

pub mod endpoint;
#[cfg(any(feature = "speech", feature = "storage"))]
pub(crate) mod envelope;
#[cfg(feature = "speech")]
pub(crate) mod multipart;
#[cfg(feature = "speech")]
pub mod pagination;
#[cfg(any(feature = "speech", feature = "storage"))]
pub(crate) mod signed;
