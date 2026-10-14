//! Client for the Speech API and verification of its webhooks.
//!
//! ```no_run
//! use std::sync::Arc;
//! use vigrise::auth::Credentials;
//! use vigrise::speech::{SpeechClient, SynthesizeOptions};
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let speech = SpeechClient::new(Arc::new(Credentials::resolve()?));
//! let audio = speech
//!   .synthesize("Xin chào", "haley-f1", &SynthesizeOptions::default())
//!   .await?;
//! println!("{} bytes of {}", audio.audio.len(), audio.content_type);
//! # Ok(())
//! # }
//! ```

mod client;
mod error;
mod multipart;
mod options;
mod types;
pub mod webhook;

pub use client::{SpeechClient, SpeechStream};
pub use error::SpeechError;
pub use options::*;
pub use types::*;
pub use webhook::{WebhookError, verify_webhook, verify_webhook_within};
