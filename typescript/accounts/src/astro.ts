import { ssoTokenHash } from "./hash.js"
import { SSO_EXCHANGE_HEADER, SSO_MARKER_COOKIE, type SsoExchange, type SsoState } from "./state.js"

export type { SsoExchange, SsoState } from "./state.js"

/** The parts of an Astro `APIContext` the helpers use. */
export interface SsoContext {
  request: Request
  cookies: {
    get(name: string): { value: string } | undefined
    set(name: string, value: string, options?: Record<string, unknown>): void
    delete(name: string, options?: Record<string, unknown>): void
  }
  locals: Record<string, any>
}

export interface SsoOptions {
  /** Name of the shared SSO cookie. */
  cookieName?: string
  /** Name of the cookie that records which SSO token the app session belongs to. */
  markerCookieName?: string
}

export interface SsoSessionEndpointOptions extends SsoOptions {
  /** Base URL of the app backend reachable from the Astro server. */
  backendUrl: string
  /** Backend path of the SSO exchange. */
  exchangePath?: string
  /** Sets `Secure` on the marker cookie. */
  secureCookies?: boolean
  /** Milliseconds to wait for the backend. */
  timeoutMs?: number
  fetch?: typeof fetch
  log?: (message: string, error?: unknown) => void
}

const MARKER_MAX_AGE_SECS = 60 * 60 * 24 * 30

/**
 * Astro middleware step for first-party apps. It compares the hash of the `vigrise_sso` cookie
 * with the marker of the app session and sets `locals.sso`: `changed` for a new or switched
 * token, `unchanged` for the same one, `signed_out` when the cookie is gone and the app held an
 * SSO session. The page hands the state to the browser, which calls the session endpoint.
 */
export function createSsoMiddleware(options: SsoOptions = {}) {
  const cookieName = options.cookieName ?? "vigrise_sso"
  const markerName = options.markerCookieName ?? SSO_MARKER_COOKIE

  return async (context: SsoContext): Promise<void> => {
    const accept = context.request.headers.get("accept") ?? ""
    if (!accept.includes("text/html")) return

    const token = context.cookies.get(cookieName)?.value
    const marker = context.cookies.get(markerName)?.value

    let state: SsoState
    if (!token) {
      if (marker) context.cookies.delete(markerName, { path: "/" })
      state = marker ? { status: "signed_out" } : { status: "anonymous" }
    } else {
      state = marker === (await ssoTokenHash(token)) ? { status: "unchanged" } : { status: "changed" }
    }
    context.locals.sso = state
  }
}

function json(body: SsoExchange | { error: string }, status: number): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json", "cache-control": "no-store" },
  })
}

/**
 * Astro `POST` endpoint that exchanges the `vigrise_sso` cookie at the app backend (the value
 * travels only in `X-ViGrise-SSO`) and records the cookie's hash in the marker cookie.
 */
export function createSsoSessionEndpoint(options: SsoSessionEndpointOptions) {
  const cookieName = options.cookieName ?? "vigrise_sso"
  const markerName = options.markerCookieName ?? SSO_MARKER_COOKIE
  const exchangeUrl = options.backendUrl.replace(/\/+$/, "") + (options.exchangePath ?? "/api/v1/auth/sso")
  const doFetch = options.fetch ?? fetch
  const log = options.log ?? ((message, error) => console.error(message, error))

  return async (context: Pick<SsoContext, "request" | "cookies">): Promise<Response> => {
    if (context.request.headers.get(SSO_EXCHANGE_HEADER) !== "1") {
      return json({ error: "invalid_request" }, 400)
    }
    const token = context.cookies.get(cookieName)?.value
    if (!token) {
      context.cookies.delete(markerName, { path: "/" })
      return json({ status: "anonymous" }, 200)
    }
    try {
      const response = await doFetch(exchangeUrl, {
        method: "POST",
        headers: { "x-vigrise-sso": token },
        signal: AbortSignal.timeout(options.timeoutMs ?? 5000),
      })
      if (response.status === 401) {
        context.cookies.delete(markerName, { path: "/" })
        return json({ status: "anonymous" }, 200)
      }
      if (!response.ok) throw new Error(`SSO exchange answered ${response.status}`)
      const auth = await response.json()
      context.cookies.set(markerName, await ssoTokenHash(token), {
        path: "/",
        sameSite: "lax",
        secure: options.secureCookies ?? false,
        maxAge: MARKER_MAX_AGE_SECS,
      })
      return json({ status: "signed_in", auth }, 200)
    } catch (error) {
      log("SSO exchange failed", error)
      return json({ status: "unavailable" }, 503)
    }
  }
}
