/** Base of the errors of the Accounts server helpers. */
export class AccountsError extends Error {
  constructor(message: string, options?: { cause?: unknown }) {
    super(message, options);
    this.name = new.target.name;
  }
}

/** Accounts could not be reached, is rate limiting the client, or answered with a server error. Retry later. */
export class AccountsUnavailableError extends AccountsError {
  /** HTTP status of the answer; undefined when Accounts was not reached. */
  readonly status?: number;

  constructor(message: string, status?: number, options?: { cause?: unknown }) {
    super(message, options);
    this.status = status;
  }
}

/** Accounts rejected the client or answered in an unexpected way: the credentials, the client's permissions or the internal URL need attention. */
export class AccountsMisconfiguredError extends AccountsError {
  readonly status?: number;

  constructor(message: string, status?: number, options?: { cause?: unknown }) {
    super(message, options);
    this.status = status;
  }
}

/** `introspectActive` found a token that is unknown, revoked or ended. */
export class InactiveSessionError extends AccountsError {
  constructor() {
    super("the SSO token is not active");
  }
}

/** A logout token or ID token failed verification. */
export class InvalidTokenError extends AccountsError {}

/** The authorization code exchange failed. */
export class TokenExchangeError extends AccountsError {}

/** The signing keys could not be loaded or did not contain the key id. */
export class JwksError extends AccountsError {}
