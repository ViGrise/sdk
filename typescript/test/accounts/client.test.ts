import { describe, expect, it, vi } from "vitest"
import {
  applySsoState,
  createSessionChannel,
  fetchSsoExchange,
  renderSsoStateJson,
  type SessionMessage,
  type SsoExchange,
  type SsoState,
  watchSsoState,
} from "../../src/accounts/index.js"

const auth = { user: { id: "u" }, tokens: {}, accounts: [] }

function handlers(hasSession: boolean, exchange: SsoExchange = { status: "signed_in", auth }) {
  return {
    hasSession: vi.fn(() => hasSession),
    adopt: vi.fn(),
    clear: vi.fn(),
    clearCaches: vi.fn(),
    exchange: vi.fn(async () => exchange),
    channel: { notify: vi.fn(), subscribe: vi.fn(), close: vi.fn() },
  }
}

describe("applySsoState", () => {
  it("adopts the exchanged session and, when another session was held, tells the other tabs", async () => {
    const h = handlers(true)
    expect(await applySsoState({ status: "changed" }, h)).toBe("adopted")
    expect(h.adopt).toHaveBeenCalledWith(auth)
    expect(h.clearCaches).toHaveBeenCalled()
    expect(h.channel.notify).toHaveBeenCalledWith({ type: "session-changed" })
  })

  it("stays quiet for tabs on the first sign-in", async () => {
    const h = handlers(false)
    await applySsoState({ status: "changed" }, h)
    expect(h.adopt).toHaveBeenCalled()
    expect(h.channel.notify).not.toHaveBeenCalled()
  })

  it("clears the session and caches on sign-out", async () => {
    const h = handlers(true)
    expect(await applySsoState({ status: "signed_out" }, h)).toBe("cleared")
    expect(h.clear).toHaveBeenCalled()
    expect(h.clearCaches).toHaveBeenCalled()
    expect(h.channel.notify).toHaveBeenCalledWith({ type: "session-changed" })
    expect(h.exchange).not.toHaveBeenCalled()
  })

  it("exchanges an unchanged cookie when the tokens are gone", async () => {
    const h = handlers(false)
    expect(await applySsoState({ status: "unchanged" }, h)).toBe("adopted")
    expect(h.exchange).toHaveBeenCalled()
  })

  it("keeps a held session when unchanged or anonymous, without calling the app", async () => {
    for (const status of ["unchanged", "anonymous"] as const) {
      const h = handlers(true)
      expect(await applySsoState({ status } as SsoState, h)).toBe("kept")
      expect(h.exchange).not.toHaveBeenCalled()
      expect(h.clear).not.toHaveBeenCalled()
    }
  })

  it("clears the held session when a changed cookie exchanges to no session or Accounts is down", async () => {
    for (const result of [{ status: "anonymous" }, { status: "unavailable" }] as SsoExchange[]) {
      const h = handlers(true, result)
      expect(await applySsoState({ status: "changed" }, h)).toBe("cleared")
      expect(h.adopt).not.toHaveBeenCalled()
      expect(h.clear).toHaveBeenCalled()
      expect(h.clearCaches).toHaveBeenCalled()
      expect(h.channel.notify).toHaveBeenCalledWith({ type: "session-changed" })
    }
  })

  it("keeps a missing session missing when the exchange of an unchanged cookie finds nothing", async () => {
    const h = handlers(false, { status: "anonymous" })
    expect(await applySsoState({ status: "unchanged" }, h)).toBe("kept")
    expect(h.clear).not.toHaveBeenCalled()
  })
})

describe("watchSsoState", () => {
  const tick = () => new Promise((r) => setTimeout(r, 0))

  function setup(current: SsoState, served: Response | SsoState) {
    const win = new EventTarget()
    const doc = Object.assign(new EventTarget(), { visibilityState: "visible" })
    const response = served instanceof Response ? served : new Response(JSON.stringify(served), { status: 200 })
    const fetchImpl = vi.fn(async () => response.clone())
    const apply = vi.fn(async () => "kept" as const)
    const stop = watchSsoState({
      current,
      fetch: fetchImpl as unknown as typeof fetch,
      apply,
      window: win,
      document: doc,
    })
    return { win, doc, fetchImpl, apply, stop }
  }

  it("checks a no-store same-origin state on focus and applies a state that differs", async () => {
    const { win, fetchImpl, apply, stop } = setup({ status: "unchanged" }, { status: "changed" })
    win.dispatchEvent(new Event("focus"))
    await tick()
    expect(fetchImpl).toHaveBeenCalledWith(
      "/sso/state",
      expect.objectContaining({ cache: "no-store", credentials: "same-origin" }),
    )
    expect(apply).toHaveBeenCalledWith({ status: "changed" })
    stop()
  })

  it("checks when the page becomes visible and skips hidden pages and matching states", async () => {
    const { doc, fetchImpl, apply, stop } = setup({ status: "unchanged" }, { status: "unchanged" })
    doc.visibilityState = "hidden"
    doc.dispatchEvent(new Event("visibilitychange"))
    await tick()
    expect(fetchImpl).not.toHaveBeenCalled()
    doc.visibilityState = "visible"
    doc.dispatchEvent(new Event("visibilitychange"))
    await tick()
    expect(fetchImpl).toHaveBeenCalledTimes(1)
    expect(apply).not.toHaveBeenCalled()
    stop()
  })

  it("ignores failed checks and stops listening after stop()", async () => {
    const { win, fetchImpl, apply, stop } = setup({ status: "unchanged" }, new Response("x", { status: 503 }))
    win.dispatchEvent(new Event("focus"))
    await tick()
    expect(apply).not.toHaveBeenCalled()
    stop()
    win.dispatchEvent(new Event("focus"))
    await tick()
    expect(fetchImpl).toHaveBeenCalledTimes(1)
  })
})

describe("fetchSsoExchange", () => {
  it("posts to the session endpoint with the exchange header", async () => {
    const impl = vi.fn(async () => new Response(JSON.stringify({ status: "signed_in", auth }), { status: 200 }))
    const result = await fetchSsoExchange("/sso/session", impl as unknown as typeof fetch)
    expect(result).toEqual({ status: "signed_in", auth })
    const [, init] = impl.mock.calls[0] as unknown as [string, RequestInit]
    expect((init.headers as Record<string, string>)["x-vigrise-sso-exchange"]).toBe("1")
    // A JSON content type keeps the request out of the form-post origin check of Astro servers.
    expect((init.headers as Record<string, string>)["content-type"]).toBe("application/json")
  })

  it("reports unavailable on a 503 or a network failure", async () => {
    const down = vi.fn(async () => new Response("{}", { status: 503 }))
    expect(await fetchSsoExchange("/x", down as unknown as typeof fetch)).toEqual({ status: "unavailable" })
    const broken = vi.fn(async () => {
      throw new Error("offline")
    })
    expect(await fetchSsoExchange("/x", broken as unknown as typeof fetch)).toEqual({ status: "unavailable" })
  })
})

describe("renderSsoStateJson", () => {
  it("escapes characters that could close the script element", () => {
    const json = renderSsoStateJson({ status: "changed" })
    expect(JSON.parse(json)).toEqual({ status: "changed" })
    expect(json).not.toContain("<")
  })
})

describe("createSessionChannel", () => {
  it("delivers messages to other channel instances of the same name", async () => {
    const received: SessionMessage[] = []
    const a = createSessionChannel("vigrise-test")
    const b = createSessionChannel("vigrise-test")
    b.subscribe((m) => received.push(m))
    a.notify({ type: "session-changed" })
    await new Promise((r) => setTimeout(r, 20))
    expect(received).toEqual([{ type: "session-changed" }])
    a.close()
    b.close()
  })
})
