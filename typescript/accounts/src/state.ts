import type { SessionChannel } from "./session-channel.js"

/** What the Astro middleware found out about the SSO cookie of a page request. */
export type SsoState =
  | { status: "changed" }
  | { status: "unchanged" }
  | { status: "signed_out" }
  | { status: "anonymous" }

/** The answer of the session endpoint, which exchanges the SSO cookie at the app backend. */
export type SsoExchange<Auth = unknown> =
  | { status: "signed_in"; auth: Auth }
  | { status: "anonymous" }
  | { status: "unavailable" }

/** Name of the cookie holding the hash of the SSO token the app session was issued for. */
export const SSO_MARKER_COOKIE = "vigrise_app_sso"

/** Id of the script element that carries the state to the browser. */
export const SSO_STATE_ELEMENT_ID = "vigrise-sso-state"

/** Request header the session endpoint requires, so that only the app's own scripts call it. */
export const SSO_EXCHANGE_HEADER = "x-vigrise-sso-exchange"

/** JSON for the state script element. */
export function renderSsoStateJson(state: SsoState): string {
  return JSON.stringify(state).replace(/</g, "\\u003c")
}

/** Reads the state the server rendered; anonymous when the element is missing. */
export function readSsoState(doc: Pick<Document, "getElementById"> = document): SsoState {
  const text = doc.getElementById(SSO_STATE_ELEMENT_ID)?.textContent
  if (!text) return { status: "anonymous" }
  try {
    return JSON.parse(text) as SsoState
  } catch {
    return { status: "anonymous" }
  }
}

/** Asks the app's session endpoint to exchange the SSO cookie for an app session. */
export async function fetchSsoExchange<Auth = unknown>(
  endpoint = "/sso/session",
  fetchImpl: typeof fetch = fetch,
): Promise<SsoExchange<Auth>> {
  try {
    const response = await fetchImpl(endpoint, {
      method: "POST",
      credentials: "same-origin",
      headers: { [SSO_EXCHANGE_HEADER]: "1" },
    })
    if (response.status === 503) return { status: "unavailable" }
    return (await response.json()) as SsoExchange<Auth>
  } catch {
    return { status: "unavailable" }
  }
}

export interface SsoStateHandlers<Auth> {
  /** Whether this browser holds an app session. */
  hasSession(): boolean
  /** Stores the freshly exchanged session. */
  adopt(auth: Auth): void
  /** Removes the app session. */
  clear(): void
  /** Drops client caches keyed to the user. */
  clearCaches?(): void
  /** Exchanges the SSO cookie at the app; defaults to `fetchSsoExchange`. */
  exchange?(): Promise<SsoExchange<Auth>>
  /** Announces account changes to the other tabs. */
  channel?: SessionChannel
}

export type SsoOutcome = "adopted" | "cleared" | "kept"

/**
 * Applies the server-rendered SSO state to the browser's app session. A changed cookie, or an
 * unchanged one without a stored session, is exchanged; a missing cookie ends the session.
 */
export async function applySsoState<Auth>(
  state: SsoState,
  handlers: SsoStateHandlers<Auth>,
): Promise<SsoOutcome> {
  const hadSession = handlers.hasSession()
  if (state.status === "signed_out") {
    handlers.clear()
    handlers.clearCaches?.()
    if (hadSession) handlers.channel?.notify({ type: "session-changed" })
    return "cleared"
  }
  if (state.status === "anonymous" || (state.status === "unchanged" && hadSession)) return "kept"

  const result = await (handlers.exchange ?? (() => fetchSsoExchange<Auth>()))()
  if (result.status !== "signed_in") return "kept"
  handlers.clearCaches?.()
  handlers.adopt(result.auth)
  if (hadSession) handlers.channel?.notify({ type: "session-changed" })
  return "adopted"
}
