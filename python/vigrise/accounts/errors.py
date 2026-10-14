from typing import Optional


class AccountsError(Exception):
  """Base of the errors of the Accounts helpers."""


class AccountsUnavailableError(AccountsError):
  """Accounts could not be reached, is rate limiting the client, or answered with a server error. Retry later."""

  def __init__(self, message: str, status: Optional[int] = None):
    super().__init__(message)
    self.status = status


class AccountsMisconfiguredError(AccountsError):
  """Accounts rejected the client or answered in an unexpected way: the credentials, the client's permissions or the internal URL need attention."""

  def __init__(self, message: str, status: Optional[int] = None):
    super().__init__(message)
    self.status = status


class InactiveSessionError(AccountsError):
  """`introspect_active` found a token that is unknown, revoked or ended."""

  def __init__(self):
    super().__init__("the SSO token is not active")


class InvalidTokenError(AccountsError):
  """A logout token or ID token failed verification."""


class TokenExchangeError(AccountsError):
  """The authorization code exchange failed."""


class JwksError(AccountsError):
  """The signing keys could not be loaded or did not contain the key id."""
