import { describe, expect, it, vi } from "vitest"
import {
  applySsoState,
  createSessionChannel,
  renderSsoStateJson,
  type SessionMessage,
  type SsoState,
} from "../src/index.js"

const auth = { user: { id: "u" }, tokens: {}, accounts: [] }

function handlers(hasSession: boolean) {
  return {
    hasSession: vi.fn(() => hasSession),
    adopt: vi.fn(),
    clear: vi.fn(),
    clearCaches: vi.fn(),
    requestExchange: vi.fn(),
    channel: { notify: vi.fn(), subscribe: vi.fn(), close: vi.fn() },
  }
}

describe("applySsoState", () => {
  it("adopts a new session and, when another session was held, tells the other tabs", () => {
    const h = handlers(true)
    expect(applySsoState({ status: "signed_in", auth }, h)).toBe("adopted")
    expect(h.adopt).toHaveBeenCalledWith(auth)
    expect(h.clearCaches).toHaveBeenCalled()
    expect(h.channel.notify).toHaveBeenCalledWith({ type: "session-changed" })
  })

  it("stays quiet for tabs on the first sign-in", () => {
    const h = handlers(false)
    applySsoState({ status: "signed_in", auth }, h)
    expect(h.adopt).toHaveBeenCalled()
    expect(h.channel.notify).not.toHaveBeenCalled()
  })

  it("clears the session and caches on sign-out", () => {
    const h = handlers(true)
    expect(applySsoState({ status: "signed_out" }, h)).toBe("cleared")
    expect(h.clear).toHaveBeenCalled()
    expect(h.clearCaches).toHaveBeenCalled()
    expect(h.channel.notify).toHaveBeenCalledWith({ type: "session-changed" })
  })

  it("asks for a new exchange when the cookie is unchanged but the tokens are gone", () => {
    const h = handlers(false)
    expect(applySsoState({ status: "unchanged" }, h)).toBe("needs-exchange")
    expect(h.requestExchange).toHaveBeenCalled()
  })

  it("keeps a held session when unchanged, anonymous or unavailable", () => {
    for (const status of ["unchanged", "unavailable"] as const) {
      const h = handlers(true)
      expect(applySsoState({ status } as SsoState, h)).toBe("kept")
      expect(h.clear).not.toHaveBeenCalled()
    }
    const h = handlers(true)
    expect(applySsoState({ status: "anonymous" }, h)).toBe("kept")
    expect(h.clear).not.toHaveBeenCalled()
  })
})

describe("renderSsoStateJson", () => {
  it("escapes characters that could close the script element", () => {
    const json = renderSsoStateJson({ status: "signed_in", auth: { name: "</script><b>" } })
    expect(json).not.toContain("<")
    expect(JSON.parse(json).auth.name).toBe("</script><b>")
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
