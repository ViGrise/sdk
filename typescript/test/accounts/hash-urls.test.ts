import { describe, expect, it } from "vitest"
import { addAccountUrl, manageAccountUrl, signOutUrl, ssoTokenHash, switchAccountUrl } from "../../src/services/accounts/index.js"

describe("ssoTokenHash", () => {
  it("is the lowercase SHA-256 hex digest that Accounts stores", async () => {
    expect(await ssoTokenHash("abc")).toBe(
      "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    )
  })
})

describe("URL builders", () => {
  const accountsUrl = "https://accounts.vigrise.com"

  it("builds the switch URL with the session id and return_to", () => {
    const url = new URL(switchAccountUrl(accountsUrl, "sid-2", "https://drive.vigrise.com/files?x=1"))
    expect(url.origin + url.pathname).toBe("https://accounts.vigrise.com/switch")
    expect(url.searchParams.get("session_id")).toBe("sid-2")
    expect(url.searchParams.get("return_to")).toBe("https://drive.vigrise.com/files?x=1")
  })

  it("builds the add account URL", () => {
    const url = new URL(addAccountUrl(accountsUrl + "/", "https://drive.vigrise.com/"))
    expect(url.pathname).toBe("/auth/login")
    expect(url.searchParams.get("add_account")).toBe("1")
    expect(url.searchParams.get("return_to")).toBe("https://drive.vigrise.com/")
  })

  it("builds the account settings URL", () => {
    expect(manageAccountUrl(accountsUrl)).toBe("https://accounts.vigrise.com/settings")
  })

  it("builds the sign-out URL on the end session endpoint", () => {
    const url = new URL(
      signOutUrl({ accountsUrl, clientId: "vigrise-drive", postLogoutRedirectUri: "https://drive.vigrise.com/" }),
    )
    expect(url.pathname).toBe("/signout")
    expect(url.searchParams.get("client_id")).toBe("vigrise-drive")
    expect(url.searchParams.get("post_logout_redirect_uri")).toBe("https://drive.vigrise.com/")
    expect(url.searchParams.has("id_token_hint")).toBe(false)
  })

  it("honours a discovered end session endpoint and an id token hint", () => {
    const url = new URL(
      signOutUrl({
        accountsUrl,
        endSessionEndpoint: "https://accounts.vigrise.com/o/custom/logout",
        clientId: "c",
        postLogoutRedirectUri: "https://x/",
        idTokenHint: "idt",
      }),
    )
    expect(url.pathname).toBe("/o/custom/logout")
    expect(url.searchParams.get("id_token_hint")).toBe("idt")
  })

  it("passes the app session's sso_sid as logout_hint and sid", () => {
    const url = new URL(
      signOutUrl({ accountsUrl, clientId: "c", postLogoutRedirectUri: "https://x/", ssoSid: "sid-1" }),
    )
    expect(url.searchParams.get("logout_hint")).toBe("sid-1")
    expect(url.searchParams.get("sid")).toBe("sid-1")
  })
})
