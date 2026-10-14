import hashlib

# Request header that carries the `vigrise_sso` cookie value from an app's frontend to its backend.
SSO_HEADER = "x-vigrise-sso"


def sso_token_hash(token: str) -> str:
    """Hash of a `vigrise_sso` cookie value: lowercase hex SHA-256. Apps store it to notice when the cookie changes."""
    return hashlib.sha256(token.encode()).hexdigest()
