use std::fmt;
use std::path::{Path, PathBuf};

pub const ENV_ACCESS_KEY_ID: &str = "VIGRISE_ACCESS_KEY_ID";
pub const ENV_SECRET_ACCESS_KEY: &str = "VIGRISE_SECRET_ACCESS_KEY";
pub const ENV_PROFILE: &str = "VIGRISE_PROFILE";
pub const ENV_CREDENTIALS_FILE: &str = "VIGRISE_SHARED_CREDENTIALS_FILE";

#[derive(Clone, PartialEq, Eq)]
pub struct Credentials {
  pub access_key_id: String,
  pub secret_access_key: String,
}

impl fmt::Debug for Credentials {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("Credentials")
      .field("access_key_id", &self.access_key_id)
      .field("secret_access_key", &"<redacted>")
      .finish()
  }
}

#[derive(Debug, thiserror::Error)]
pub enum CredentialsError {
  #[error("no Vigrise credentials found in the environment or {0}")]
  NotFound(String),
  #[error("failed to read credentials file {path}: {source}")]
  Io {
    path: String,
    source: std::io::Error,
  },
}

impl Credentials {
  pub fn new(access_key_id: impl Into<String>, secret_access_key: impl Into<String>) -> Self {
    Self {
      access_key_id: access_key_id.into(),
      secret_access_key: secret_access_key.into(),
    }
  }

  /// Resolves credentials from the process environment, then the shared
  /// credentials file.
  pub fn resolve() -> Result<Self, CredentialsError> {
    Self::resolve_with(|name| std::env::var(name).ok())
  }

  /// Resolution order: `VIGRISE_ACCESS_KEY_ID` + `VIGRISE_SECRET_ACCESS_KEY`,
  /// then the `VIGRISE_PROFILE` profile (default `default`) of
  /// `VIGRISE_SHARED_CREDENTIALS_FILE` (default `~/.vigrise/credentials`).
  pub fn resolve_with(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, CredentialsError> {
    if let (Some(id), Some(secret)) = (lookup(ENV_ACCESS_KEY_ID), lookup(ENV_SECRET_ACCESS_KEY)) {
      return Ok(Self::new(id, secret));
    }
    let path = lookup(ENV_CREDENTIALS_FILE)
      .map(PathBuf::from)
      .or_else(|| lookup("HOME").map(|home| Path::new(&home).join(".vigrise").join("credentials")));
    let profile = lookup(ENV_PROFILE).unwrap_or_else(|| "default".to_string());
    let Some(path) = path else {
      return Err(CredentialsError::NotFound("~/.vigrise/credentials".into()));
    };
    match std::fs::read_to_string(&path) {
      Ok(contents) => parse_profile(&contents, &profile)
        .ok_or_else(|| CredentialsError::NotFound(format!("{} [{profile}]", path.display()))),
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
        Err(CredentialsError::NotFound(path.display().to_string()))
      }
      Err(source) => Err(CredentialsError::Io {
        path: path.display().to_string(),
        source,
      }),
    }
  }
}

/// Parses the INI-style shared credentials file and returns one profile.
pub fn parse_profile(contents: &str, profile: &str) -> Option<Credentials> {
  let mut in_profile = false;
  let (mut id, mut secret) = (None, None);
  for line in contents.lines() {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
      continue;
    }
    if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
      in_profile = name.trim() == profile;
      continue;
    }
    if !in_profile {
      continue;
    }
    if let Some((key, value)) = line.split_once('=') {
      match key.trim() {
        "vigrise_access_key_id" => id = Some(value.trim().to_string()),
        "vigrise_secret_access_key" => secret = Some(value.trim().to_string()),
        _ => {}
      }
    }
  }
  Some(Credentials::new(id?, secret?))
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::collections::HashMap;

  const FILE: &str = "# comment\n[default]\nvigrise_access_key_id = VGAKDEFAULT\nvigrise_secret_access_key = s1\n\n[ci]\nvigrise_access_key_id=VGAKCI\nvigrise_secret_access_key=s2\n";

  #[test]
  fn parses_named_profiles() {
    assert_eq!(
      parse_profile(FILE, "default").unwrap().access_key_id,
      "VGAKDEFAULT"
    );
    assert_eq!(parse_profile(FILE, "ci").unwrap().secret_access_key, "s2");
    assert!(parse_profile(FILE, "missing").is_none());
  }

  #[test]
  fn parses_crlf_line_endings() {
    let crlf = FILE.replace('\n', "\r\n");
    assert_eq!(parse_profile(&crlf, "ci").unwrap().access_key_id, "VGAKCI");
  }

  #[test]
  fn environment_wins_over_file() {
    let env: HashMap<&str, &str> =
      [(ENV_ACCESS_KEY_ID, "VGAKENV"), (ENV_SECRET_ACCESS_KEY, "s")].into();
    let creds = Credentials::resolve_with(|k| env.get(k).map(|v| v.to_string())).unwrap();
    assert_eq!(creds.access_key_id, "VGAKENV");
  }

  #[test]
  fn reads_profile_from_file() {
    let path = std::env::temp_dir().join(format!("vigrise-creds-{}", std::process::id()));
    std::fs::write(&path, FILE).unwrap();
    let env: HashMap<&str, String> = [
      (ENV_CREDENTIALS_FILE, path.display().to_string()),
      (ENV_PROFILE, "ci".to_string()),
    ]
    .into();
    let creds = Credentials::resolve_with(|k| env.get(k).cloned()).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(creds.access_key_id, "VGAKCI");
  }

  #[test]
  fn debug_redacts_secret() {
    let rendered = format!("{:?}", Credentials::new("id", "topsecret"));
    assert!(!rendered.contains("topsecret"));
  }
}
