class AccountsError(Exception):
  """Base of the errors of the Accounts helpers."""


class InvalidTokenError(AccountsError):
  """A logout token or ID token failed verification."""


class TokenExchangeError(AccountsError):
  """The authorization code exchange failed."""


class JwksError(AccountsError):
  """The signing keys could not be loaded or did not contain the key id."""
