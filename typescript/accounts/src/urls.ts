function base(accountsUrl: string): string {
  return accountsUrl.replace(/\/+$/, "")
}

/** Sends the browser to Accounts to make another account of the browser the active one. */
export function switchAccountUrl(accountsUrl: string, sessionId: string, returnTo: string): string {
  const query = new URLSearchParams({ session_id: sessionId, return_to: returnTo })
  return `${base(accountsUrl)}/switch?${query}`
}

/** Sends the browser to the Accounts login to add another account to the browser. */
export function addAccountUrl(accountsUrl: string, returnTo: string): string {
  const query = new URLSearchParams({ add_account: "1", return_to: returnTo })
  return `${base(accountsUrl)}/auth/login?${query}`
}

/** The ViGrise Account settings page. */
export function manageAccountUrl(accountsUrl: string): string {
  return `${base(accountsUrl)}/settings`
}

export interface SignOutUrlOptions {
  accountsUrl: string
  /** The `end_session_endpoint` of the discovery document, when the app has it. */
  endSessionEndpoint?: string
  clientId: string
  /** Registered post logout redirect URI of the client. */
  postLogoutRedirectUri: string
  /** ID token of an OAuth2 fallback session; selects that session at Accounts. */
  idTokenHint?: string
  /** The `sso_sid` of the app session; selects the Accounts session to end. */
  ssoSid?: string
}

/** RP-initiated logout URL: signs the active account out of ViGrise and returns to the app. */
export function signOutUrl(options: SignOutUrlOptions): string {
  const endpoint = options.endSessionEndpoint ?? `${base(options.accountsUrl)}/o/oauth2/sessions/logout`
  const query = new URLSearchParams({
    client_id: options.clientId,
    post_logout_redirect_uri: options.postLogoutRedirectUri,
  })
  if (options.ssoSid) {
    query.set("logout_hint", options.ssoSid)
    query.set("sid", options.ssoSid)
  }
  if (options.idTokenHint) query.set("id_token_hint", options.idTokenHint)
  return `${endpoint}?${query}`
}
