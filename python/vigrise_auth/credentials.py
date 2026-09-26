import os
from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable, Mapping, Optional, Union


@dataclass(frozen=True)
class Credentials:
    access_key_id: str
    secret_access_key: str = field(repr=False)
    session_token: Optional[str] = field(default=None, repr=False)


CredentialsProvider = Callable[[], Credentials]
"""Supplies the credentials to sign each request with, such as a refreshed role session."""

CredentialsSource = Union[Credentials, CredentialsProvider]


def credentials_from(source: Optional[CredentialsSource]) -> Credentials:
    """Fixed credentials, a provider's current credentials, or the environment's."""
    if source is None:
        return resolve_credentials()
    if isinstance(source, Credentials):
        return source
    return source()


def parse_profile(contents: str, profile: str) -> Optional[Credentials]:
    """Reads one profile of the INI-style shared credentials file."""
    # Editors on Windows commonly prefix UTF-8 files with a byte order mark.
    text = contents[1:] if contents.startswith("﻿") else contents
    in_profile = False
    values = {}
    for raw_line in text.splitlines():
        line = raw_line.strip()
        if not line or line.startswith(("#", ";")):
            continue
        if line.startswith("[") and line.endswith("]"):
            in_profile = line[1:-1].strip() == profile
            continue
        if in_profile and "=" in line:
            key, _, value = line.partition("=")
            values[key.strip()] = value.strip()
    access_key_id = values.get("vigrise_access_key_id")
    secret = values.get("vigrise_secret_access_key")
    session_token = values.get("vigrise_session_token")
    return Credentials(access_key_id, secret, session_token) if access_key_id and secret else None


def resolve_credentials(explicit: Optional[Credentials] = None, env: Optional[Mapping[str, str]] = None) -> Credentials:
    """Resolution order: explicit credentials, VIGRISE_ACCESS_KEY_ID +
    VIGRISE_SECRET_ACCESS_KEY, then profile VIGRISE_PROFILE (default
    `default`) of VIGRISE_SHARED_CREDENTIALS_FILE (default
    `.vigrise/credentials` under HOME, or USERPROFILE on Windows)."""
    if explicit is not None:
        return explicit
    env = os.environ if env is None else env
    if env.get("VIGRISE_ACCESS_KEY_ID") and env.get("VIGRISE_SECRET_ACCESS_KEY"):
        return Credentials(
            env["VIGRISE_ACCESS_KEY_ID"],
            env["VIGRISE_SECRET_ACCESS_KEY"],
            env.get("VIGRISE_SESSION_TOKEN"),
        )
    home = env.get("HOME") or env.get("USERPROFILE")
    path = env.get("VIGRISE_SHARED_CREDENTIALS_FILE") or (str(Path(home) / ".vigrise" / "credentials") if home else None)
    profile = env.get("VIGRISE_PROFILE", "default")
    found = None
    if path:
        try:
            found = parse_profile(Path(path).read_text(encoding="utf-8"), profile)
        except OSError:
            found = None
    if found is None:
        raise LookupError(f"No ViGrise credentials found in the environment or {path or '~/.vigrise/credentials'} [{profile}]")
    return found
