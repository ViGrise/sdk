from datetime import datetime, timezone

import pytest

from vigrise.auth import Credentials, sign_request


def test_signs_and_returns_session_token_header():
  signed = sign_request(
    "GET",
    "https://example.test/",
    {},
    b"",
    Credentials("VGSTABCDEFGHIJKLMNOP", "secret", "sealed-token"),
    "speech",
    datetime(2026, 9, 26, 10, tzinfo=timezone.utc),
  )
  assert signed["x-vgr-security-token"] == "sealed-token"
  assert "x-vgr-security-token" in signed["authorization"]


def test_temporary_key_requires_session_token():
  with pytest.raises(ValueError, match="session token"):
    sign_request(
      "GET",
      "https://example.test/",
      {},
      b"",
      Credentials("VGSTABCDEFGHIJKLMNOP", "secret"),
      "speech",
    )
