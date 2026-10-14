import type { SessionChannel } from "./session-channel.js";

/** What the Astro middleware found out about the SSO cookie of a page request. */
export type SsoState =
  { status: "changed" } | { status: "unchanged" } | { status: "signed_out" } | { status: "anonymous" };

/** The answer of the session endpoint, which exchanges the SSO cookie at the app backend. */
export type SsoExchange<Auth = unknown> =
  { status: "signed_in"; auth: Auth } | { status: "anonymous" } | { status: "unavailable" };

/** Name of the cookie holding the hash of the SSO token the app session was issued for. */
export const SSO_MARKER_COOKIE = "vigrise_app_sso";

/** Id of the script element that carries the state to the browser. */
export const SSO_STATE_ELEMENT_ID = "vigrise-sso-state";

/** Request header the session endpoint requires, so that only the app's own scripts call it. */
export const SSO_EXCHANGE_HEADER = "x-vigrise-sso-exchange";

/** JSON for the state script element. */
export function renderSsoStateJson(state: SsoState): string {
  return JSON.stringify(state).replace(/</g, "\\u003c");
}

/** Reads the state the server rendered; anonymous when the element is missing. */
export function readSsoState(doc: Pick<Document, "getElementById"> = document): SsoState {
  const text = doc.getElementById(SSO_STATE_ELEMENT_ID)?.textContent;
  if (!text) return { status: "anonymous" };
  try {
    return JSON.parse(text) as SsoState;
  } catch {
    return { status: "anonymous" };
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
      headers: { [SSO_EXCHANGE_HEADER]: "1", "content-type": "application/json" },
      body: "{}",
    });
    if (response.status === 503) return { status: "unavailable" };
    return (await response.json()) as SsoExchange<Auth>;
  } catch {
    return { status: "unavailable" };
  }
}

export interface SsoStateHandlers<Auth> {
  /** Whether this browser holds an app session. */
  hasSession(): boolean;
  /** Stores the freshly exchanged session. */
  adopt(auth: Auth): void;
  /** Removes the app session. */
  clear(): void;
  /** Drops client caches keyed to the user. */
  clearCaches?(): void;
  /** Exchanges the SSO cookie at the app; defaults to `fetchSsoExchange`. */
  exchange?(): Promise<SsoExchange<Auth>>;
  /** Announces account changes to the other tabs. */
  channel?: SessionChannel;
}

export type SsoOutcome = "adopted" | "cleared" | "kept";

/**
 * Applies the server-rendered SSO state to the browser's app session. A changed cookie, or an
 * unchanged one without a stored session, is exchanged; a missing cookie, or a changed cookie that
 * no longer exchanges to a session, ends the session.
 */
export async function applySsoState<Auth>(state: SsoState, handlers: SsoStateHandlers<Auth>): Promise<SsoOutcome> {
  const hadSession = handlers.hasSession();
  if (state.status === "signed_out") {
    handlers.clear();
    handlers.clearCaches?.();
    if (hadSession) handlers.channel?.notify({ type: "session-changed" });
    return "cleared";
  }
  if (state.status === "anonymous" || (state.status === "unchanged" && hadSession)) return "kept";

  const result = await (handlers.exchange ?? (() => fetchSsoExchange<Auth>()))();
  if (result.status !== "signed_in") {
    if (state.status !== "changed") return "kept";
    handlers.clear();
    handlers.clearCaches?.();
    if (hadSession) handlers.channel?.notify({ type: "session-changed" });
    return "cleared";
  }
  handlers.clearCaches?.();
  handlers.adopt(result.auth);
  if (hadSession) handlers.channel?.notify({ type: "session-changed" });
  return "adopted";
}

export interface WatchSsoStateOptions {
  /** The state the page holds, usually `readSsoState()`. */
  current: SsoState;
  /** Applies a state that differs from the one held, usually `applySsoState` with the app handlers. */
  apply(state: SsoState): Promise<SsoOutcome>;
  /** Same-origin endpoint serving `createSsoStateEndpoint`. */
  endpoint?: string;
  fetch?: typeof fetch;
  window?: Pick<EventTarget, "addEventListener" | "removeEventListener">;
  document?: Pick<EventTarget, "addEventListener" | "removeEventListener"> & { visibilityState: string };
}

/**
 * Re-checks the SSO state whenever the page becomes visible or gains focus and applies it when it
 * differs from the state the page holds. Returns a function that stops watching.
 */
export function watchSsoState(options: WatchSsoStateOptions): () => void {
  const win = options.window ?? window;
  const doc = options.document ?? document;
  const doFetch = options.fetch ?? fetch;
  const endpoint = options.endpoint ?? "/sso/state";
  let current = options.current;
  let checking = false;

  const check = async () => {
    if (checking || doc.visibilityState !== "visible") return;
    checking = true;
    try {
      const response = await doFetch(endpoint, {
        cache: "no-store",
        credentials: "same-origin",
        headers: { accept: "application/json" },
      });
      if (!response.ok) return;
      const next = (await response.json()) as SsoState;
      if (next.status === current.status) return;
      current = next;
      await options.apply(next);
    } catch {
      // A failed check leaves the held session untouched; the next focus retries.
    } finally {
      checking = false;
    }
  };

  win.addEventListener("focus", check);
  doc.addEventListener("visibilitychange", check);
  return () => {
    win.removeEventListener("focus", check);
    doc.removeEventListener("visibilitychange", check);
  };
}
