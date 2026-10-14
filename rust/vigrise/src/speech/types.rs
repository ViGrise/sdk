use bytes::Bytes;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Defines an enumeration of the string values a service sends, with a
/// variant that keeps values added later.
macro_rules! open_enum {
  ($(#[$meta:meta])* $name:ident { $($(#[$vmeta:meta])* $variant:ident => $text:literal),+ $(,)? }) => {
    $(#[$meta])*
    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub enum $name {
      $($(#[$vmeta])* $variant,)+
      /// A value this version of the SDK does not know.
      Other(String),
    }

    impl $name {
      /// The value as the service spells it.
      pub fn as_str(&self) -> &str {
        match self {
          $(Self::$variant => $text,)+
          Self::Other(value) => value,
        }
      }
    }

    impl From<&str> for $name {
      fn from(value: &str) -> Self {
        match value {
          $($text => Self::$variant,)+
          other => Self::Other(other.to_string()),
        }
      }
    }

    impl std::fmt::Display for $name {
      fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
      }
    }

    impl Serialize for $name {
      fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
      }
    }

    impl<'de> Deserialize<'de> for $name {
      fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::from(String::deserialize(deserializer)?.as_str()))
      }
    }
  };
}

open_enum! {
  /// Audio formats a synthesis can be returned in.
  AudioFormat {
    /// WAV container.
    Wav => "wav",
    /// Raw signed 16-bit little-endian mono PCM.
    Pcm => "pcm",
    /// MP3.
    Mp3 => "mp3",
    /// Opus in an Ogg container.
    Opus => "opus",
  }
}

open_enum! {
  /// How a generation was requested.
  GenerationSource {
    /// `synthesize`.
    Sync => "sync",
    /// `synthesize_stream`.
    Stream => "stream",
    /// A job.
    Job => "job",
  }
}

open_enum! {
  /// Whether a generation holds all of its audio.
  GenerationStatus {
    /// The audio is complete.
    Complete => "complete",
    /// The audio stopped before the end.
    Incomplete => "incomplete",
  }
}

open_enum! {
  /// Progress of a job.
  JobStatus {
    /// Waiting for a worker.
    Queued => "queued",
    /// Being synthesized.
    Running => "running",
    /// Finished with audio.
    Succeeded => "succeeded",
    /// Finished with an error.
    Failed => "failed",
    /// Cancelled before it finished.
    Cancelled => "cancelled",
  }
}

impl JobStatus {
  /// Whether the job will not change any more.
  pub fn is_finished(&self) -> bool {
    matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
  }
}

open_enum! {
  /// Where a voice comes from.
  VoiceKind {
    /// A built-in voice.
    System => "system",
    /// A voice cloned in the project.
    Cloned => "cloned",
  }
}

open_enum! {
  /// How the speaker of a reference recording relates to the person cloning the voice.
  Relationship {
    /// The speaker is the person cloning the voice.
    Own => "self",
    /// The speaker authorized the cloning.
    Authorized => "authorized",
  }
}

/// A synthesized audio file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Speech {
  /// The audio.
  pub audio: Bytes,
  /// Content type of the audio, such as `audio/wav`.
  pub content_type: String,
  /// Id of the generation that holds the audio.
  pub generation_id: String,
  /// Samples per second, when the service reports it.
  pub sample_rate: Option<u32>,
}

/// A voice, built-in or cloned.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Voice {
  /// Name of a built-in voice, or id of a cloned one.
  pub id: String,
  /// Whether the voice is built-in or cloned.
  #[serde(rename = "type")]
  pub kind: VoiceKind,
  /// Name given when the voice was cloned.
  #[serde(default)]
  pub name: Option<String>,
  /// Length of the reference recording in milliseconds.
  #[serde(default)]
  pub duration_ms: Option<i64>,
  /// Who cloned the voice.
  #[serde(default)]
  pub created_by: Option<String>,
  /// When the voice was cloned.
  #[serde(default)]
  pub created_at: Option<DateTime<Utc>>,
  /// Id of the consent record of a cloned voice.
  #[serde(default)]
  pub consent_id: Option<String>,
}

/// The consent statement a person agrees to before cloning a voice.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ConsentStatement {
  /// Version to send back in [`Consent::statement_version`].
  pub version: i32,
  /// The text the speaker agrees to.
  pub statement: String,
}

/// The speaker's consent to clone a voice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Consent {
  /// Version of the [`ConsentStatement`] that was shown.
  pub statement_version: i32,
  /// Name of the speaker.
  pub speaker_name: String,
  /// How the speaker relates to the person cloning the voice.
  pub relationship: Relationship,
  /// Whether the speaker agreed to the statement.
  pub agreed: bool,
}

/// A recording of the voice to clone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
  /// The recording.
  pub bytes: Bytes,
  /// Content type of the recording, such as `audio/wav`.
  pub content_type: String,
  /// File name sent with the recording.
  pub file_name: String,
}

impl Reference {
  /// A recording with the content type and file name to send it with.
  pub fn new(
    bytes: impl Into<Bytes>,
    content_type: impl Into<String>,
    file_name: impl Into<String>,
  ) -> Self {
    Self {
      bytes: bytes.into(),
      content_type: content_type.into(),
      file_name: file_name.into(),
    }
  }
}

/// Whether the audio of a generation can still be downloaded.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AudioInfo {
  /// Whether the audio can be downloaded now.
  pub available: bool,
  /// Size of the audio in bytes.
  pub bytes: i64,
  /// When the audio is deleted.
  #[serde(default)]
  pub expires_at: Option<DateTime<Utc>>,
}

/// One synthesis and the audio it produced.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Generation {
  /// Id of the generation.
  pub id: String,
  /// How the generation was requested.
  pub source: GenerationSource,
  /// The job the generation belongs to.
  #[serde(default)]
  pub job_id: Option<String>,
  /// Voice that spoke.
  pub voice: String,
  /// Model that synthesized.
  pub model: String,
  /// Audio format.
  pub format: String,
  /// Text that was spoken.
  pub text: String,
  /// Characters in the text.
  pub characters: i64,
  /// Characters metered for the generation.
  pub characters_used: i64,
  /// Length of the audio in milliseconds.
  pub duration_ms: i64,
  /// Whether the audio is complete.
  pub status: GenerationStatus,
  /// Whether the audio can be downloaded.
  pub audio: AudioInfo,
  /// Who requested the generation.
  pub created_by: String,
  /// When the generation was requested.
  pub created_at: DateTime<Utc>,
}

/// A temporary URL to download the audio of a generation from.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AudioUrl {
  /// The URL, signed for a limited time.
  pub url: String,
  /// When the URL stops working.
  pub expires_at: DateTime<Utc>,
}

/// One turn of a script.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn {
  /// Voice that speaks the turn.
  pub voice: String,
  /// Text of the turn.
  pub text: String,
}

/// What a job synthesizes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobInput {
  /// One text in one voice.
  Text {
    /// The text.
    text: String,
    /// The voice.
    voice: String,
  },
  /// Turns spoken in order.
  Script {
    /// The turns.
    turns: Vec<Turn>,
    /// Silence between turns in milliseconds; the service default when `None`.
    gap_ms: Option<u32>,
  },
}

/// How far a job has come.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Progress {
  /// Pieces synthesized.
  pub pieces_done: i32,
  /// Pieces in the job.
  pub pieces_total: i32,
  /// Characters synthesized.
  pub characters_done: i64,
  /// Characters in the job.
  pub characters: i64,
}

/// A synthesis that runs in the background.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Job {
  /// Id of the job.
  pub id: String,
  /// Progress of the job.
  pub status: JobStatus,
  /// How far the job has come.
  pub progress: Progress,
  /// Characters metered for the job so far.
  pub characters_used: i64,
  /// Audio format.
  pub format: String,
  /// Where the outcome is sent.
  #[serde(default)]
  pub webhook_url: Option<String>,
  /// Why the job failed.
  #[serde(default)]
  pub error: Option<String>,
  /// The generation that holds the audio of a finished job.
  #[serde(default)]
  pub generation_id: Option<String>,
  /// Who created the job.
  pub created_by: String,
  /// When the job was created.
  pub created_at: DateTime<Utc>,
  /// When the job finished.
  #[serde(default)]
  pub finished_at: Option<DateTime<Utc>>,
}

/// A page of a list.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Page<T> {
  /// The items.
  pub data: Vec<T>,
  /// Pass to the next call to read the next page; `None` on the last page.
  #[serde(default)]
  pub next_cursor: Option<String>,
}

/// A speech model, as the engine describes it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Model {
  /// Id of the model.
  pub id: String,
  /// The other fields of the description.
  #[serde(flatten)]
  pub extra: Map<String, Value>,
}

/// The secret that signs the project's webhooks.
#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct WebhookSecret {
  /// The secret, `whsec_` followed by base64.
  pub secret: String,
  /// When the secret was created.
  pub created_at: DateTime<Utc>,
}

impl std::fmt::Debug for WebhookSecret {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("WebhookSecret")
      .field("secret", &"<redacted>")
      .field("created_at", &self.created_at)
      .finish()
  }
}

/// A verified webhook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebhookEvent {
  /// Id of the delivery, from the `webhook-id` header.
  pub id: String,
  /// `job.succeeded`, `job.failed` or `job.cancelled`.
  pub kind: String,
  /// When the event happened.
  pub timestamp: DateTime<Utc>,
  /// The job as it was at the event.
  pub data: Job,
}
