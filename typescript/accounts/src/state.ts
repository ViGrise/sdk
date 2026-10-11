import type { SessionChannel } from "./session-channel.js"

/** What the Astro middleware found out about the SSO session of a page request. */
export type SsoState<Auth = unknown> =
  | { status: "signed_in"; auth: Auth }
  | { status: "unchanged" }
  | { status: "signed_out" }
  | { status: "anonymous" }
  | { status: "unavailable" }

/** Name of the cookie holding the hash of the SSO token the app session was issued for. */
export const SSO_MARKER_COOKIE = "vigrise_app_sso"

/** Id of the script element that carries the state to the browser. */
export const SSO_STATE_ELEMENT_ID = "vigrise-sso-state"

/** JSON for the state script element; `<` is escaped so the payload cannot close the element. */
export function renderSsoStateJson(state: SsoState): string {
  return JSON.stringify(state).replace(/</g, "\\u003c")
}

/** Reads the state the server rendered; anonymous when the element is missing. */
export function readSsoState<Auth = unknown>(doc: Pick<Document, "getElementById"> = document): SsoState<Auth> {
  const text = doc.getElementById(SSO_STATE_ELEMENT_ID)?.textContent
  if (!text) return { status: "anonymous" }
  try {
    return JSON.parse(text) as SsoState<Auth>
  } catch {
    return { status: "anonymous" }
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
  /** Starts a new exchange; used when the cookie is unchanged but the tokens are gone. */
  requestExchange(): void
  /** Announces account changes to the other tabs. */
  channel?: SessionChannel
}

export type SsoOutcome = "adopted" | "cleared" | "kept" | "needs-exchange"

/** Applies the server-rendered SSO state to the browser's app session. */
export function applySsoState<Auth>(state: SsoState<Auth>, handlers: SsoStateHandlers<Auth>): SsoOutcome {
  switch (state.status) {
    case "signed_in": {
      const hadSession = handlers.hasSession()
      handlers.clearCaches?.()
      handlers.adopt(state.auth)
      if (hadSession) handlers.channel?.notify({ type: "session-changed" })
      return "adopted"
    }
    case "signed_out": {
      const hadSession = handlers.hasSession()
      handlers.clear()
      handlers.clearCaches?.()
      if (hadSession) handlers.channel?.notify({ type: "session-changed" })
      return "cleared"
    }
    case "unchanged":
      if (!handlers.hasSession()) {
        handlers.requestExchange()
        return "needs-exchange"
      }
      return "kept"
    default:
      return "kept"
  }
}

/** Removes the marker cookie and reloads, which makes the middleware exchange the SSO cookie again. */
export function requestExchangeByReload(): void {
  document.cookie = `${SSO_MARKER_COOKIE}=; Max-Age=0; Path=/`
  globalThis.location.reload()
}
