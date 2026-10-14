use std::time::Duration;

use crate::speech::types::{AudioFormat, GenerationSource, JobStatus};

/// Options of a synthesis.
#[derive(Debug, Clone, Default)]
pub struct SynthesizeOptions {
  /// Model to synthesize with; the service default when `None`.
  pub model: Option<String>,
  /// Audio format; the service default when `None`.
  pub format: Option<AudioFormat>,
  /// Whether the service keeps the audio as a generation; the service default when `None`.
  pub store_audio: Option<bool>,
  /// Makes the call safe to repeat: the service runs one request per key.
  pub idempotency_key: Option<String>,
}

/// Options of cloning a voice.
#[derive(Debug, Clone, Default)]
pub struct CloneOptions {
  /// Makes the call safe to repeat: the service runs one request per key.
  pub idempotency_key: Option<String>,
}

/// Options of a job.
#[derive(Debug, Clone, Default)]
pub struct JobOptions {
  /// Model to synthesize with; the service default when `None`.
  pub model: Option<String>,
  /// Audio format; the service default when `None`.
  pub format: Option<AudioFormat>,
  /// Where the outcome is sent; verify it with [`verify_webhook`](crate::speech::verify_webhook).
  pub webhook_url: Option<String>,
  /// Whether the service keeps the audio as a generation; the service default when `None`.
  pub store_audio: Option<bool>,
  /// Makes the call safe to repeat: the service creates one job per key.
  pub idempotency_key: Option<String>,
}

/// Filters and paging of a list of generations.
#[derive(Debug, Clone, Default)]
pub struct ListGenerationsOptions {
  /// The `next_cursor` of the previous page.
  pub cursor: Option<String>,
  /// Most items per page.
  pub limit: Option<u64>,
  /// Only generations requested this way.
  pub source: Option<GenerationSource>,
  /// Only generations spoken by this voice.
  pub voice: Option<String>,
}

/// Filters and paging of a list of jobs.
#[derive(Debug, Clone, Default)]
pub struct ListJobsOptions {
  /// The `next_cursor` of the previous page.
  pub cursor: Option<String>,
  /// Most items per page.
  pub limit: Option<u64>,
  /// Only jobs in this status.
  pub status: Option<JobStatus>,
}

/// How `wait_for_job` polls.
#[derive(Debug, Clone)]
pub struct WaitOptions {
  /// Pause between polls.
  pub poll_interval: Duration,
  /// Gives up after this long; waits without end when `None`.
  pub timeout: Option<Duration>,
}

impl Default for WaitOptions {
  fn default() -> Self {
    Self {
      poll_interval: Duration::from_secs(2),
      timeout: None,
    }
  }
}
