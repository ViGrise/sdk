import { describe, expect, it } from "vitest"
import { accountMenuModel, accountPrimaryLine, accountSecondaryLine } from "../src/index.js"

const base = { session_id: "s", identity_id: "i", picture: null, active: false, expired: false }
const ada = { ...base, session_id: "s1", name: "Ada", email: "ada@example.com", active: true }
const bob = { ...base, session_id: "s2", name: "Bob", email: "bob@example.com" }
const cy = { ...base, session_id: "s3", name: null, email: "cy@example.com", expired: true }

const props = {
  accountsUrl: "https://accounts.vigrise.com",
  returnTo: "https://drive.vigrise.com/",
  clientId: "vigrise-drive",
}

describe("display rule", () => {
  it("uses the name as primary line and the email as secondary line", () => {
    expect(accountPrimaryLine(ada)).toBe("Ada")
    expect(accountSecondaryLine(ada, "Sign in again")).toBe("ada@example.com")
  })

  it("uses the email as primary line when the account has no name, with no secondary line", () => {
    expect(accountPrimaryLine(cy)).toBe("cy@example.com")
    expect(accountSecondaryLine({ ...cy, expired: false }, "Sign in again")).toBeNull()
  })

  it("treats a blank name as missing", () => {
    expect(accountPrimaryLine({ ...bob, name: "  " })).toBe("bob@example.com")
  })

  it("shows the sign-in-again label for an expired account", () => {
    expect(accountSecondaryLine(cy, "Sign in again")).toBe("Sign in again")
    expect(accountSecondaryLine({ ...bob, expired: true }, "Sign in again")).toBe("Sign in again")
  })

  it("falls back to an empty primary line without name and email", () => {
    expect(accountPrimaryLine({ ...base, name: null, email: null })).toBe("")
  })
})

describe("accountMenuModel", () => {
  it("splits the current account from the others and builds every href", () => {
    const model = accountMenuModel({ ...props, accounts: [bob, ada, cy] })
    expect(model.current?.session_id).toBe("s1")
    expect(model.others.map((row) => row.account.session_id)).toEqual(["s2", "s3"])
    expect(model.others[0].href).toContain("https://accounts.vigrise.com/switch?session_id=s2")
    expect(model.manageHref).toBe("https://accounts.vigrise.com/settings")
    expect(model.addAccountHref).toContain("add_account=1")
    expect(model.signOutHref.startsWith("https://accounts.vigrise.com/signout?")).toBe(true)
  })

  it("treats the first account as current when none is active", () => {
    const model = accountMenuModel({ ...props, accounts: [{ ...bob }, { ...cy }] })
    expect(model.current?.session_id).toBe("s2")
    expect(model.others.map((row) => row.account.session_id)).toEqual(["s3"])
  })

  it("has no current account for an empty list", () => {
    const model = accountMenuModel({ ...props, accounts: [] })
    expect(model.current).toBeUndefined()
    expect(model.others).toEqual([])
  })

  it("carries the sso_sid and the post logout redirect into the sign-out href", () => {
    const model = accountMenuModel({ ...props, accounts: [ada], ssoSid: "sid-9", postLogoutRedirectUri: "https://drive.vigrise.com/bye" })
    const url = new URL(model.signOutHref)
    expect(url.searchParams.get("logout_hint")).toBe("sid-9")
    expect(url.searchParams.get("post_logout_redirect_uri")).toBe("https://drive.vigrise.com/bye")
  })
})
