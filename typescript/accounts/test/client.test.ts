import { describe, expect, it, vi } from "vitest"
import {
  applySsoState,
  createSessionChannel,
  fetchSsoExchange,
  renderSsoStateJson,
  type SessionMessage,
  type SsoExchange,
  type SsoState,
} from "../src/index.js"

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

  it("keeps the held session when the exchange finds no session or Accounts is down", async () => {
    for (const result of [{ status: "anonymous" }, { status: "unavailable" }] as SsoExchange[]) {
      const h = handlers(true, result)
      expect(await applySsoState({ status: "changed" }, h)).toBe("kept")
      expect(h.adopt).not.toHaveBeenCalled()
      expect(h.clear).not.toHaveBeenCalled()
    }
  })
})

describe("fetchSsoExchange", () => {
  it("posts to the session endpoint with the exchange header", async () => {
    const impl = vi.fn(async () => new Response(JSON.stringify({ status: "signed_in", auth }), { status: 200 }))
    const result = await fetchSsoExchange("/sso/session", impl as unknown as typeof fetch)
    expect(result).toEqual({ status: "signed_in", auth })
    const [, init] = impl.mock.calls[0] as unknown as [string, RequestInit]
    expect((init.headers as Record<string, string>)["x-vigrise-sso-exchange"]).toBe("1")
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
