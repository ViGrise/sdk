import { ssoTokenHash } from "./hash.js"
import { SSO_MARKER_COOKIE, type SsoState } from "./state.js"

export type { SsoState } from "./state.js"

/** The parts of an Astro `APIContext` the middleware uses. */
export interface SsoContext {
  request: Request
  cookies: {
    get(name: string): { value: string } | undefined
    set(name: string, value: string, options?: Record<string, unknown>): void
    delete(name: string, options?: Record<string, unknown>): void
  }
  locals: Record<string, any>
}

export interface SsoMiddlewareOptions {
  /** Base URL of the app backend reachable from the Astro server. */
  backendUrl: string
  /** Backend path of the SSO exchange. */
  exchangePath?: string
  /** Name of the shared SSO cookie. */
  cookieName?: string
  /** Name of the cookie that records which SSO token the app session belongs to. */
  markerCookieName?: string
  /** Sets `Secure` on the marker cookie. */
  secureCookies?: boolean
  /** Milliseconds to wait for the backend. */
  timeoutMs?: number
  fetch?: typeof fetch
  log?: (message: string, error?: unknown) => void
}

const MARKER_MAX_AGE_SECS = 60 * 60 * 24 * 30

/**
 * Astro middleware step for first-party apps. It reads the `vigrise_sso` cookie, compares its
 * hash with the marker of the app session and sets `locals.sso`:
 * a new or switched token is exchanged at the backend, a missing cookie ends the app session and
 * an unchanged hash continues it. The SSO value reaches the backend only in `X-ViGrise-SSO`.
 */
export function createSsoMiddleware(options: SsoMiddlewareOptions) {
  const cookieName = options.cookieName ?? "vigrise_sso"
  const markerName = options.markerCookieName ?? SSO_MARKER_COOKIE
  const exchangeUrl = options.backendUrl.replace(/\/+$/, "") + (options.exchangePath ?? "/api/v1/auth/sso")
  const doFetch = options.fetch ?? fetch
  const log = options.log ?? ((message, error) => console.error(message, error))

  const setMarker = (context: SsoContext, hash: string) =>
    context.cookies.set(markerName, hash, {
      path: "/",
      sameSite: "lax",
      secure: options.secureCookies ?? false,
      maxAge: MARKER_MAX_AGE_SECS,
    })

  return async (context: SsoContext): Promise<void> => {
    const accept = context.request.headers.get("accept") ?? ""
    if (!accept.includes("text/html")) return

    const token = context.cookies.get(cookieName)?.value
    const marker = context.cookies.get(markerName)?.value

    if (!token) {
      if (marker) context.cookies.delete(markerName, { path: "/" })
      context.locals.sso = (marker ? { status: "signed_out" } : { status: "anonymous" }) satisfies SsoState
      return
    }

    const hash = await ssoTokenHash(token)
    if (marker === hash) {
      context.locals.sso = { status: "unchanged" } satisfies SsoState
      return
    }

    try {
      const response = await doFetch(exchangeUrl, {
        method: "POST",
        headers: { "x-vigrise-sso": token },
        signal: AbortSignal.timeout(options.timeoutMs ?? 5000),
      })
      if (response.status === 401) {
        if (marker) context.cookies.delete(markerName, { path: "/" })
        context.locals.sso = { status: "anonymous" } satisfies SsoState
        return
      }
      if (!response.ok) throw new Error(`SSO exchange answered ${response.status}`)
      const auth = await response.json()
      setMarker(context, hash)
      context.locals.sso = { status: "signed_in", auth } satisfies SsoState
    } catch (error) {
      log("SSO exchange failed", error)
      context.locals.sso = { status: "unavailable" } satisfies SsoState
    }
  }
}
