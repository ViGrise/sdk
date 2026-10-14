import { describe, expect, it, vi } from "vitest"
import { ssoTokenHash } from "../../src/accounts/index.js"
import {
  createSsoMiddleware,
  createSsoStateEndpoint,
  createSsoSessionEndpoint,
  type SsoState,
} from "../../src/accounts/astro.js"

class FakeCookies {
  jar = new Map<string, string>()
  setCalls: Array<[string, string, Record<string, unknown>]> = []
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
    this.setCalls.push([name, value, options])
  }
  delete(name: string) {
    this.jar.delete(name)
    this.deleted.push(name)
  }
}

function pageContext(cookies: Record<string, string>, accept = "text/html") {
  return {
    request: new Request("https://drive.vigrise.com/", { headers: { accept } }),
    cookies: new FakeCookies(cookies),
    locals: {} as Record<string, unknown>,
  }
}

describe("createSsoMiddleware", () => {
  const run = async (cookies: Record<string, string>, accept?: string) => {
    const ctx = pageContext(cookies, accept)
    await createSsoMiddleware()(ctx)
    return ctx
  }

  it("reports a new SSO cookie as changed", async () => {
    const ctx = await run({ vigrise_sso: "token-1" })
    expect((ctx.locals.sso as SsoState).status).toBe("changed")
  })

  it("reports a different hash (account switch) as changed", async () => {
    const ctx = await run({ vigrise_sso: "token-2", vigrise_app_sso: await ssoTokenHash("token-1") })
    expect((ctx.locals.sso as SsoState).status).toBe("changed")
  })

  it("reports the same hash as unchanged", async () => {
    const ctx = await run({ vigrise_sso: "token-1", vigrise_app_sso: await ssoTokenHash("token-1") })
    expect((ctx.locals.sso as SsoState).status).toBe("unchanged")
  })

  it("ends the app session when the SSO cookie is missing", async () => {
    const ctx = await run({ vigrise_app_sso: await ssoTokenHash("token-1") })
    expect((ctx.locals.sso as SsoState).status).toBe("signed_out")
    expect(ctx.cookies.deleted).toContain("vigrise_app_sso")
  })

  it("is anonymous without either cookie", async () => {
    const ctx = await run({})
    expect((ctx.locals.sso as SsoState).status).toBe("anonymous")
  })

  it("leaves non-HTML requests alone", async () => {
    const ctx = await run({ vigrise_sso: "token-1" }, "application/json")
    expect(ctx.locals.sso).toBeUndefined()
  })
})

describe("createSsoSessionEndpoint", () => {
  const auth = { user: { id: "u1" }, tokens: { access: { token: "a", expires: 1 } }, accounts: [] }
  const endpoint = (impl: typeof fetch) =>
    createSsoSessionEndpoint({
      backendUrl: "http://drive-backend:8080",
      fetch: impl,
      secureCookies: true,
      log: () => {},
    })
  const call = (cookies: Record<string, string>, header = "1") => ({
    request: new Request("https://drive.vigrise.com/sso/session", {
      method: "POST",
      headers: header ? { "x-vigrise-sso-exchange": header } : {},
    }),
    cookies: new FakeCookies(cookies),
  })
  const okFetch = () => vi.fn(async () => new Response(JSON.stringify(auth), { status: 200 }))

  it("exchanges the cookie, sends it only in X-ViGrise-SSO and records its hash", async () => {
    const fetchImpl = okFetch()
    const ctx = call({ vigrise_sso: "token-1" })
    const response = await endpoint(fetchImpl as unknown as typeof fetch)(ctx)

    const [url, init] = fetchImpl.mock.calls[0] as unknown as [string, RequestInit]
    expect(url).toBe("http://drive-backend:8080/api/v1/auth/sso")
    expect(init.method).toBe("POST")
    const headers = new Headers(init.headers)
    expect(headers.get("x-vigrise-sso")).toBe("token-1")
    expect(headers.get("cookie")).toBeNull()

    expect(response.headers.get("cache-control")).toBe("no-store")
    expect(await response.json()).toEqual({ status: "signed_in", auth })
    expect(ctx.cookies.get("vigrise_app_sso")?.value).toBe(await ssoTokenHash("token-1"))
    expect(ctx.cookies.setCalls[0][2]).toMatchObject({ path: "/", secure: true, sameSite: "lax" })
  })

  it("requires the exchange header", async () => {
    const fetchImpl = okFetch()
    const response = await endpoint(fetchImpl as unknown as typeof fetch)(call({ vigrise_sso: "t" }, ""))
    expect(response.status).toBe(400)
    expect(fetchImpl).not.toHaveBeenCalled()
  })

  it("answers anonymous without a cookie and clears the marker", async () => {
    const fetchImpl = okFetch()
    const ctx = call({ vigrise_app_sso: "old" })
    const response = await endpoint(fetchImpl as unknown as typeof fetch)(ctx)
    expect(await response.json()).toEqual({ status: "anonymous" })
    expect(ctx.cookies.deleted).toContain("vigrise_app_sso")
    expect(fetchImpl).not.toHaveBeenCalled()
  })

  it("treats an inactive session (401) as anonymous and clears the marker", async () => {
    const ctx = call({ vigrise_sso: "stale", vigrise_app_sso: "old" })
    const response = await endpoint(vi.fn(async () => new Response("{}", { status: 401 })) as unknown as typeof fetch)(ctx)
    expect(await response.json()).toEqual({ status: "anonymous" })
    expect(ctx.cookies.deleted).toContain("vigrise_app_sso")
  })

  it("answers 503 and keeps the marker when Accounts is unavailable", async () => {
    for (const impl of [
      vi.fn(async () => new Response("{}", { status: 503 })),
      vi.fn(async () => {
        throw new Error("connect refused")
      }),
    ]) {
      const ctx = call({ vigrise_sso: "token-2", vigrise_app_sso: "old" })
      const response = await endpoint(impl as unknown as typeof fetch)(ctx)
      expect(response.status).toBe(503)
      expect(await response.json()).toEqual({ status: "unavailable" })
      expect(ctx.cookies.deleted).toEqual([])
      expect(ctx.cookies.get("vigrise_app_sso")?.value).toBe("old")
    }
  })
})

describe("createSsoStateEndpoint", () => {
  it("answers the state of the cookie as no-store JSON for any accept header", async () => {
    const hash = await ssoTokenHash("token-1")
    const handler = createSsoStateEndpoint()
    const call = async (cookies: Record<string, string>) => {
      const ctx = pageContext(cookies, "application/json")
      const response = await handler(ctx)
      expect(response.headers.get("cache-control")).toBe("no-store")
      return response.json()
    }
    expect(await call({ vigrise_sso: "token-1", vigrise_app_sso: hash })).toEqual({ status: "unchanged" })
    expect(await call({ vigrise_sso: "token-2", vigrise_app_sso: hash })).toEqual({ status: "changed" })
    expect(await call({ vigrise_app_sso: hash })).toEqual({ status: "signed_out" })
    expect(await call({})).toEqual({ status: "anonymous" })
  })
})
