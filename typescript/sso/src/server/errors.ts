import { AccountsError } from "@vigrise/sdk/accounts/server";

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
