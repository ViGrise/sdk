"""Verification of signed tokens. Fetching the key is the only step that differs between sync and async."""

from typing import Any, Dict

import jwt

from ._errors import InvalidTokenError

LEEWAY_SECONDS = 30


def unverified_header(token: str, label: str) -> Dict[str, Any]:
    """The header of a token that is signed with RS256; anything else is refused before a key is looked up."""
    try:
        header = jwt.get_unverified_header(token)
    except jwt.PyJWTError as error:
        raise InvalidTokenError(f"invalid {label}: {error}") from error
    if header.get("alg") != "RS256":
        raise InvalidTokenError(f"invalid {label}: unsupported signing algorithm")
    return header


def decode(token: str, key: Any, label: str, issuer: str, audience: str, required: list, leeway: int = 0) -> Dict[str, Any]:
    """The claims of a token whose signature, issuer, audience and expiry hold."""
    try:
        return jwt.decode(
            token,
            key,
            algorithms=["RS256"],
            issuer=issuer,
            audience=audience,
            leeway=leeway,
            options={"require": required},
        )
    except jwt.PyJWTError as error:
        raise InvalidTokenError(f"invalid {label}: {error}") from error
