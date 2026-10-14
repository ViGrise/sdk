use serde::{Deserialize, Serialize};

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
