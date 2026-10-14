//! Service-side verification: canonicalizes an incoming request, asks the
//! platform to authorize it, and exposes the authenticated principal.

mod client;
mod middleware;
mod resolver;
mod types;

pub use client::{AuthorizeClient, AuthorizeFuture, Authorizer};
pub use middleware::{
  headers_with_host, hold_leases, request_context, verify, verify_streaming, Verifier,
};
pub use resolver::{ActionResolver, ResolvedAction};
pub use types::{
  AuthorizeRequest, AuthorizeResponse, Authorized, Decision, ErrorBody, Principal, QuotaStatus,
};
