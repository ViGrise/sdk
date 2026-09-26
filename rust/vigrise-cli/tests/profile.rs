use vigrise_cli::profile::{read_session, remove_session, write_session, SessionProfile};

fn session(key: &str) -> SessionProfile {
  SessionProfile {
    access_key_id: key.into(),
    secret_access_key: "secret".into(),
    session_token: "token".into(),
    expiration: "2026-09-26T10:30:00Z".into(),
    role_vrn: "vrn:vigrise:iam:p:role/dev".into(),
  }
}

#[test]
fn a_session_is_written_as_a_profile_the_sdks_read() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("credentials");
  write_session(&path, "dev", &session("VGSTONE")).unwrap();
  let contents = std::fs::read_to_string(&path).unwrap();
  let credentials = vigrise_auth::credentials::parse_profile(&contents, "dev").unwrap();
  assert_eq!(credentials.access_key_id, "VGSTONE");
  assert_eq!(credentials.session_token.as_deref(), Some("token"));
  assert_eq!(
    read_session(&path, "dev").unwrap().unwrap().role_vrn,
    "vrn:vigrise:iam:p:role/dev"
  );
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
      std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
      0o600
    );
  }
}

#[test]
fn other_profiles_comments_and_keys_are_kept() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("credentials");
  std::fs::write(
    &path,
    "# my keys\n[default]\nvigrise_access_key_id = VGAKLONG\nvigrise_secret_access_key = s\n\n[dev]\nregion = hn\nvigrise_access_key_id = VGSTOLD\n",
  )
  .unwrap();
  write_session(&path, "dev", &session("VGSTNEW")).unwrap();
  let contents = std::fs::read_to_string(&path).unwrap();
  assert!(
    contents.starts_with("# my keys\n[default]\nvigrise_access_key_id = VGAKLONG\n"),
    "{contents}"
  );
  assert!(contents.contains("region = hn"), "{contents}");
  assert_eq!(
    contents.matches("vigrise_access_key_id").count(),
    2,
    "{contents}"
  );
  let default = vigrise_auth::credentials::parse_profile(&contents, "default").unwrap();
  assert_eq!(default.access_key_id, "VGAKLONG");
  let dev = vigrise_auth::credentials::parse_profile(&contents, "dev").unwrap();
  assert_eq!(dev.access_key_id, "VGSTNEW");
}

#[test]
fn removing_a_session_leaves_the_rest_of_the_profile() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("credentials");
  std::fs::write(&path, "[dev]\nregion = hn\n").unwrap();
  write_session(&path, "dev", &session("VGSTONE")).unwrap();
  remove_session(&path, "dev").unwrap();
  let contents = std::fs::read_to_string(&path).unwrap();
  assert!(contents.contains("region = hn"), "{contents}");
  assert!(!contents.contains("vigrise_"), "{contents}");
  assert!(read_session(&path, "dev").unwrap().is_none());
}

#[test]
fn whoami_names_the_role_and_how_long_the_session_lasts() {
  use vigrise_cli::profile::describe_session;
  let now = chrono::DateTime::parse_from_rfc3339("2026-09-26T10:00:00Z")
    .unwrap()
    .with_timezone(&chrono::Utc);
  let text = describe_session(&session("VGSTONE"), now);
  assert!(text.contains("vrn:vigrise:iam:p:role/dev"), "{text}");
  assert!(
    text.contains("expires 2026-09-26T10:30:00Z (in 30 minutes)"),
    "{text}"
  );
  let later = now + chrono::Duration::hours(1);
  assert!(describe_session(&session("VGSTONE"), later).contains("expired"));
}
