import pytest

from vigrise.auth import Credentials, resolve_credentials
from vigrise.auth.credentials import parse_profile

FILE = "# c\n[default]\nvigrise_access_key_id = VGAKDEFAULT\nvigrise_secret_access_key = s1\n\n[ci]\nvigrise_access_key_id=VGAKCI\nvigrise_secret_access_key=s2\n"


def test_parse_profiles_crlf_and_bom():
  assert parse_profile(FILE, "ci").access_key_id == "VGAKCI"
  assert parse_profile(FILE.replace("\n", "\r\n"), "default").secret_access_key == "s1"
  assert (
    parse_profile(
      "﻿[default]\r\nvigrise_access_key_id = B\r\nvigrise_secret_access_key = s\r\n", "default"
    ).access_key_id
    == "B"
  )
  assert parse_profile(FILE, "missing") is None


def test_resolution_order(tmp_path):
  explicit = Credentials("X", "x")
  assert resolve_credentials(explicit=explicit, env={}) == explicit
  assert (
    resolve_credentials(
      env={"VIGRISE_ACCESS_KEY_ID": "E", "VIGRISE_SECRET_ACCESS_KEY": "e"}
    ).access_key_id
    == "E"
  )
  (tmp_path / ".vigrise").mkdir()
  (tmp_path / ".vigrise" / "credentials").write_text(FILE)
  assert (
    resolve_credentials(env={"USERPROFILE": str(tmp_path), "VIGRISE_PROFILE": "ci"}).access_key_id
    == "VGAKCI"
  )
  with pytest.raises(LookupError, match="No ViGrise credentials"):
    resolve_credentials(env={"HOME": str(tmp_path / "none")})


def test_secret_is_not_in_repr():
  assert "topsecret" not in repr(Credentials("id", "topsecret"))


def test_resolves_session_token_from_environment_and_profile():
  creds = resolve_credentials(
    env={
      "VIGRISE_ACCESS_KEY_ID": "VGSTENV",
      "VIGRISE_SECRET_ACCESS_KEY": "s",
      "VIGRISE_SESSION_TOKEN": "token-from-env",
    }
  )
  assert creds.session_token == "token-from-env"
  creds = parse_profile(
    "[default]\nvigrise_access_key_id=VGSTFILE\nvigrise_secret_access_key=s\nvigrise_session_token=token-from-file\n",
    "default",
  )
  assert creds.session_token == "token-from-file"
