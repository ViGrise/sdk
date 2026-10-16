/** Base of the errors of the Accounts server helpers. */
export class AccountsError extends Error {
  constructor(message: string, options?: { cause?: unknown }) {
    super(message, options);
    this.name = new.target.name;
  }
}

/** A logout token or ID token failed verification. */
export class InvalidTokenError extends AccountsError {}

/** The authorization code exchange failed. */
export class TokenExchangeError extends AccountsError {}

/** The signing keys could not be loaded or did not contain the key id. */
export class JwksError extends AccountsError {}
