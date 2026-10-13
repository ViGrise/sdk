import { renderToStaticMarkup } from "react-dom/server"
import { describe, expect, it } from "vitest"
import { AccountMenuList } from "../src/react.js"

const accounts = [
  { session_id: "s1", identity_id: "i1", name: "Ada", email: "ada@example.com", picture: null, active: true, expired: false },
  { session_id: "s2", identity_id: "i2", name: "Bob", email: "bob@example.com", picture: null, active: false, expired: false },
  { session_id: "s3", identity_id: "i3", name: "Cy", email: "cy@example.com", picture: null, active: false, expired: true },
]

const props = {
  accounts,
  accountsUrl: "https://accounts.vigrise.com",
  returnTo: "https://drive.vigrise.com/",
  clientId: "vigrise-drive",
}

describe("AccountMenuList", () => {
  const html = renderToStaticMarkup(<AccountMenuList {...props} />)

  it("lists every account and marks the active one", () => {
    expect(html).toContain("ada@example.com")
    expect(html).toContain("bob@example.com")
    expect(html).toMatch(/aria-current="true"[^>]*>[\s\S]*Ada/)
  })

  it("links other accounts to the switch URL on Accounts", () => {
    expect(html).toContain("https://accounts.vigrise.com/switch?session_id=s2")
    expect(html).not.toContain("session_id=s1")
  })

  it("offers add account, account settings and sign-out", () => {
    expect(html).toContain("add_account=1")
    expect(html).toContain("https://accounts.vigrise.com/settings")
    expect(html).toContain("/o/oauth2/sessions/logout")
    expect(html).toContain("Add account")
    expect(html).toContain("Manage your ViGrise Account")
  })

  it("labels an expired account so the person signs in again", () => {
    expect(html).toContain("Sign in again")
  })

  it("passes the app session's sso_sid to the sign-out link", () => {
    const html = renderToStaticMarkup(<AccountMenuList {...props} ssoSid="sid-9" />)
    expect(html).toContain("logout_hint=sid-9")
    expect(html).toContain("sid=sid-9")
  })

  it("accepts a sign-out hook", () => {
    const onSignOut = () => {}
    expect(renderToStaticMarkup(<AccountMenuList {...props} onSignOut={onSignOut} />)).toContain("Sign out")
  })

  it("takes labels from the app for translation", () => {
    const vi = renderToStaticMarkup(
      <AccountMenuList {...props} labels={{ addAccount: "Them tai khoan", signOut: "Dang xuat" }} />,
    )
    expect(vi).toContain("Them tai khoan")
    expect(vi).toContain("Dang xuat")
    expect(vi).toContain("Manage your ViGrise Account")
  })
})
