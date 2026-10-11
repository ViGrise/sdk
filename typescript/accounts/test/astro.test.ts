import { describe, expect, it, vi } from "vitest"
import { ssoTokenHash } from "../src/index.js"
import { createSsoMiddleware, type SsoState } from "../src/astro.js"

class FakeCookies {
  jar = new Map<string, string>()
  set_calls: Array<[string, string, Record<string, unknown>]> = []
  deleted: string[] = []
  constructor(initial: Record<string, string> = {}) {
    for (const [k, v] of Object.entries(initial)) this.jar.set(k, v)
  }
  get(name: string) {
    const value = this.jar.get(name)
    return value === undefined ? undefined : { value }
  }
  set(name: string, value: string, options: Record<string, unknown> = {}) {
    this.jar.set(name, value)
    this.set_calls.push([name, value, options])
  }
  delete(name: string) {
    this.jar.delete(name)
    this.deleted.push(name)
  }
}

const auth = { user: { id: "u1" }, tokens: { access: { token: "a", expires: 1 } }, accounts: [] }

function context(cookies: Record<string, string>, accept = "text/html") {
  return {
    request: new Request("https://drive.vigrise.com/", { headers: { accept } }),
    cookies: new FakeCookies(cookies),
    locals: {} as Record<string, unknown>,
  }
}

function middleware(fetchImpl: typeof fetch) {
  return createSsoMiddleware({ backendUrl: "http://drive-backend:8080", fetch: fetchImpl, secureCookies: true, log: () => {} })
}

const ok = () => vi.fn(async () => new Response(JSON.stringify(auth), { status: 200 }))

describe("createSsoMiddleware", () => {
  it("exchanges a new SSO cookie, sends it in X-ViGrise-SSO and records the hash", async () => {
    const fetchImpl = ok()
    const ctx = context({ vigrise_sso: "token-1" })
    await middleware(fetchImpl as unknown as typeof fetch)(ctx)

    const [url, init] = fetchImpl.mock.calls[0] as unknown as [string, RequestInit]
    expect(url).toBe("http://drive-backend:8080/api/v1/auth/sso")
    expect(init.method).toBe("POST")
    const headers = new Headers(init.headers)
    expect(headers.get("x-vigrise-sso")).toBe("token-1")
    expect(headers.get("cookie")).toBeNull()

    const state = ctx.locals.sso as SsoState
    expect(state.status).toBe("signed_in")
    expect(state.status === "signed_in" && state.auth).toEqual(auth)
    expect(ctx.cookies.get("vigrise_app_sso")?.value).toBe(await ssoTokenHash("token-1"))
    expect(ctx.cookies.set_calls[0][2]).toMatchObject({ path: "/", secure: true, sameSite: "lax" })
  })

  it("continues when the hash is unchanged", async () => {
    const fetchImpl = ok()
    const hash = await ssoTokenHash("token-1")
    const ctx = context({ vigrise_sso: "token-1", vigrise_app_sso: hash })
    await middleware(fetchImpl as unknown as typeof fetch)(ctx)
    expect(fetchImpl).not.toHaveBeenCalled()
    expect((ctx.locals.sso as SsoState).status).toBe("unchanged")
  })

  it("exchanges again when the hash differs (account switch)", async () => {
    const fetchImpl = ok()
    const ctx = context({ vigrise_sso: "token-2", vigrise_app_sso: await ssoTokenHash("token-1") })
    await middleware(fetchImpl as unknown as typeof fetch)(ctx)
    expect(fetchImpl).toHaveBeenCalledTimes(1)
    expect((ctx.locals.sso as SsoState).status).toBe("signed_in")
    expect(ctx.cookies.get("vigrise_app_sso")?.value).toBe(await ssoTokenHash("token-2"))
  })

  it("ends the app session when the SSO cookie is missing", async () => {
    const fetchImpl = ok()
    const ctx = context({ vigrise_app_sso: await ssoTokenHash("token-1") })
    await middleware(fetchImpl as unknown as typeof fetch)(ctx)
    expect(fetchImpl).not.toHaveBeenCalled()
    expect((ctx.locals.sso as SsoState).status).toBe("signed_out")
    expect(ctx.cookies.deleted).toContain("vigrise_app_sso")
  })

  it("is anonymous without either cookie", async () => {
    const fetchImpl = ok()
    const ctx = context({})
    await middleware(fetchImpl as unknown as typeof fetch)(ctx)
    expect((ctx.locals.sso as SsoState).status).toBe("anonymous")
    expect(fetchImpl).not.toHaveBeenCalled()
  })

  it("treats an inactive SSO session (401) as anonymous and clears the marker", async () => {
    const fetchImpl = vi.fn(async () => new Response("{}", { status: 401 }))
    const ctx = context({ vigrise_sso: "stale", vigrise_app_sso: "old" })
    await middleware(fetchImpl as unknown as typeof fetch)(ctx)
    expect((ctx.locals.sso as SsoState).status).toBe("anonymous")
    expect(ctx.cookies.deleted).toContain("vigrise_app_sso")
  })

  it("keeps the app session when Accounts is unavailable (503 or network error)", async () => {
    for (const impl of [
      vi.fn(async () => new Response("{}", { status: 503 })),
      vi.fn(async () => {
        throw new Error("connect refused")
      }),
    ]) {
      const ctx = context({ vigrise_sso: "token-2", vigrise_app_sso: "old" })
      await middleware(impl as unknown as typeof fetch)(ctx)
      expect((ctx.locals.sso as SsoState).status).toBe("unavailable")
      expect(ctx.cookies.deleted).toEqual([])
      expect(ctx.cookies.get("vigrise_app_sso")?.value).toBe("old")
    }
  })

  it("leaves non-HTML requests alone", async () => {
    const fetchImpl = ok()
    const ctx = context({ vigrise_sso: "token-1" }, "application/json")
    await middleware(fetchImpl as unknown as typeof fetch)(ctx)
    expect(ctx.locals.sso).toBeUndefined()
    expect(fetchImpl).not.toHaveBeenCalled()
  })
})
