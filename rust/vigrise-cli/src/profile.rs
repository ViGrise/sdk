//! The shared credentials file (`~/.vigrise/credentials`) every ViGrise SDK
//! reads. A session is written into one profile; every other profile, comment
//! and key of the file is kept as it was.

use std::io::Write;
use std::path::Path;

/// Keys a role session owns in its profile.
const SESSION_KEYS: [&str; 5] = [
  "vigrise_access_key_id",
  "vigrise_secret_access_key",
  "vigrise_session_token",
  "vigrise_session_expiration",
  "vigrise_role_vrn",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionProfile {
  pub access_key_id: String,
  pub secret_access_key: String,
  pub session_token: String,
  /// RFC 3339 timestamp.
  pub expiration: String,
  pub role_vrn: String,
}

impl SessionProfile {
  fn values(&self) -> [(&'static str, &str); 5] {
    [
      (SESSION_KEYS[0], &self.access_key_id),
      (SESSION_KEYS[1], &self.secret_access_key),
      (SESSION_KEYS[2], &self.session_token),
      (SESSION_KEYS[3], &self.expiration),
      (SESSION_KEYS[4], &self.role_vrn),
    ]
  }
}

fn section_name(line: &str) -> Option<&str> {
  line
    .trim()
    .strip_prefix('[')
    .and_then(|rest| rest.strip_suffix(']'))
    .map(str::trim)
}

fn key_of(line: &str) -> Option<&str> {
  let line = line.trim();
  if line.starts_with('#') || line.starts_with(';') {
    return None;
  }
  line.split_once('=').map(|(key, _)| key.trim())
}

fn read(path: &Path) -> std::io::Result<String> {
  match std::fs::read_to_string(path) {
    Ok(contents) => Ok(
      contents
        .strip_prefix('\u{feff}')
        .unwrap_or(&contents)
        .to_string(),
    ),
    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
    Err(error) => Err(error),
  }
}

/// Writes `contents` readable only by its owner, replacing the file at once.
pub(crate) fn write_private(path: &Path, contents: &str) -> std::io::Result<()> {
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent)?;
  }
  let temporary = path.with_extension("tmp");
  {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
      use std::os::unix::fs::OpenOptionsExt;
      options.mode(0o600);
    }
    let mut file = options.open(&temporary)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()?;
  }
  std::fs::rename(&temporary, path)
}

/// The file's lines with the session keys of `profile` removed, and the index
/// just after that profile's last line when the profile exists.
fn without_session(contents: &str, profile: &str) -> (Vec<String>, Option<usize>) {
  let mut lines = Vec::new();
  let mut in_profile = false;
  let mut end = None;
  for line in contents.lines() {
    if let Some(name) = section_name(line) {
      in_profile = name == profile;
      lines.push(line.to_string());
      if in_profile {
        end = Some(lines.len());
      }
      continue;
    }
    if in_profile && key_of(line).is_some_and(|key| SESSION_KEYS.contains(&key)) {
      continue;
    }
    lines.push(line.to_string());
    if in_profile && !line.trim().is_empty() {
      end = Some(lines.len());
    }
  }
  (lines, end)
}

fn joined(lines: &[String]) -> String {
  let mut contents = lines.join("\n");
  if !contents.is_empty() {
    contents.push('\n');
  }
  contents
}

pub fn write_session(path: &Path, profile: &str, session: &SessionProfile) -> std::io::Result<()> {
  let (mut lines, end) = without_session(&read(path)?, profile);
  let entries: Vec<String> = session
    .values()
    .iter()
    .map(|(key, value)| format!("{key} = {value}"))
    .collect();
  match end {
    Some(at) => {
      lines.splice(at..at, entries);
    }
    None => {
      if lines.last().is_some_and(|line| !line.trim().is_empty()) {
        lines.push(String::new());
      }
      lines.push(format!("[{profile}]"));
      lines.extend(entries);
    }
  }
  write_private(path, &joined(&lines))
}

pub fn remove_session(path: &Path, profile: &str) -> std::io::Result<()> {
  if !path.exists() {
    return Ok(());
  }
  let (lines, _) = without_session(&read(path)?, profile);
  write_private(path, &joined(&lines))
}

pub fn read_session(path: &Path, profile: &str) -> std::io::Result<Option<SessionProfile>> {
  let contents = read(path)?;
  let mut in_profile = false;
  let mut values = std::collections::HashMap::new();
  for line in contents.lines() {
    if let Some(name) = section_name(line) {
      in_profile = name == profile;
      continue;
    }
    if !in_profile {
      continue;
    }
    if let (Some(key), Some((_, value))) = (key_of(line), line.split_once('=')) {
      values.insert(key.to_string(), value.trim().to_string());
    }
  }
  let mut take = |key: &str| values.remove(key);
  Ok((|| {
    Some(SessionProfile {
      access_key_id: take(SESSION_KEYS[0])?,
      secret_access_key: take(SESSION_KEYS[1])?,
      session_token: take(SESSION_KEYS[2])?,
      expiration: take(SESSION_KEYS[3])?,
      role_vrn: take(SESSION_KEYS[4])?,
    })
  })())
}

/// One line describing a profile's session for `vigrise whoami`.
pub fn describe_session(session: &SessionProfile, now: chrono::DateTime<chrono::Utc>) -> String {
  let remaining = chrono::DateTime::parse_from_rfc3339(&session.expiration)
    .map(|expiration| expiration.with_timezone(&chrono::Utc) - now);
  let state = match remaining {
    Ok(left) if left > chrono::Duration::zero() => format!("in {} minutes", left.num_minutes()),
    Ok(_) => "expired".to_string(),
    Err(_) => "unknown expiry".to_string(),
  };
  format!(
    "{} as {}, expires {} ({state})",
    session.access_key_id, session.role_vrn, session.expiration
  )
}

/// The access key id a profile holds, whatever kind of key it is.
pub fn access_key_id(path: &Path, profile: &str) -> std::io::Result<Option<String>> {
  let contents = read(path)?;
  let mut in_profile = false;
  let mut found = None;
  for line in contents.lines() {
    if let Some(name) = section_name(line) {
      in_profile = name == profile;
      continue;
    }
    if in_profile && key_of(line) == Some(SESSION_KEYS[0]) {
      found = line
        .split_once('=')
        .map(|(_, value)| value.trim().to_string());
    }
  }
  Ok(found)
}
