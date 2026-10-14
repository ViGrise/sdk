import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { AccountMenu, AccountMenuList } from "../../src/services/accounts/react.js";

const accounts = [
  {
    session_id: "s1",
    identity_id: "i1",
    name: "Ada",
    email: "ada@example.com",
    picture: null,
    active: true,
    expired: false,
  },
  {
    session_id: "s2",
    identity_id: "i2",
    name: "Bob",
    email: "bob@example.com",
    picture: null,
    active: false,
    expired: false,
  },
  {
    session_id: "s3",
    identity_id: "i3",
    name: "Cy",
    email: "cy@example.com",
    picture: null,
    active: false,
    expired: true,
  },
];

const props = {
  accounts,
  accountsUrl: "https://accounts.vigrise.com",
  returnTo: "https://drive.vigrise.com/",
  clientId: "vigrise-drive",
};

describe("AccountMenuList", () => {
  const html = renderToStaticMarkup(<AccountMenuList {...props} />);

  it("lists every account and marks the active one", () => {
    expect(html).toContain("ada@example.com");
    expect(html).toContain("bob@example.com");
    expect(html).toMatch(/aria-current="true"[^>]*>[\s\S]*Ada/);
  });

  it("links other accounts to the switch URL on Accounts", () => {
    expect(html).toContain("https://accounts.vigrise.com/switch?session_id=s2");
    expect(html).not.toContain("session_id=s1");
  });

  it("offers add account, account settings and sign-out", () => {
    expect(html).toContain("add_account=1");
    expect(html).toContain("https://accounts.vigrise.com/settings");
    expect(html).toContain("https://accounts.vigrise.com/signout?");
    expect(html).toContain("Add another account");
    expect(html).toContain("Manage your ViGrise Account");
  });

  it("labels an expired account so the person signs in again", () => {
    expect(html).toContain("Sign in again");
  });

  it("passes the app session's sso_sid to the sign-out link", () => {
    const html = renderToStaticMarkup(<AccountMenuList {...props} ssoSid="sid-9" />);
    expect(html).toContain("logout_hint=sid-9");
    expect(html).toContain("sid=sid-9");
  });

  it("accepts a sign-out hook", () => {
    const onSignOut = () => {};
    expect(renderToStaticMarkup(<AccountMenuList {...props} onSignOut={onSignOut} />)).toContain("Sign out");
  });

  it("takes labels from the app for translation", () => {
    const vi = renderToStaticMarkup(
      <AccountMenuList {...props} labels={{ addAccount: "Them tai khoan", signOut: "Dang xuat" }} />,
    );
    expect(vi).toContain("Them tai khoan");
    expect(vi).toContain("Dang xuat");
    expect(vi).toContain("Manage your ViGrise Account");
  });

  it("keeps the addAccount label working", () => {
    const html = renderToStaticMarkup(<AccountMenuList {...props} labels={{ addAccount: "Add account" }} />);
    expect(html).toContain("Add account");
  });

  it("prefers addAnotherAccount over addAccount", () => {
    const html = renderToStaticMarkup(
      <AccountMenuList {...props} labels={{ addAccount: "A", addAnotherAccount: "B" }} />,
    );
    expect(html).toContain(">B<");
  });

  it("shows the current account once, with the name and the email on separate lines", () => {
    expect(html.match(/ada@example\.com/g)).toHaveLength(1);
    expect(html).toContain('data-slot="primary"');
    expect(html).toContain('data-slot="secondary"');
  });

  it("renders dividers as separators and describes the manage item with the current account", () => {
    expect(html).toContain('role="separator"');
    expect(html).not.toMatch(/role="none"[^>]*aria-hidden/);
    const describedBy = html.match(/role="menuitem"[^>]*aria-describedby="([^"]+)"/)?.[1];
    expect(describedBy).toBeTruthy();
    expect(html).toMatch(new RegExp(`id="${describedBy}"[^>]*>[\\s\\S]*Ada`));
  });

  it("carries a data-slot on every part and no inline colours", () => {
    for (const slot of [
      "list",
      "header",
      "avatar",
      "primary",
      "secondary",
      "manage",
      "accountRow",
      "addAccount",
      "signOut",
      "divider",
    ]) {
      expect(html).toContain(`data-slot="${slot}"`);
    }
    expect(html).not.toMatch(/background|color:/);
  });

  it("applies classNames per slot", () => {
    const styled = renderToStaticMarkup(
      <AccountMenuList {...props} classNames={{ list: "menu-list", signOut: "menu-out" }} />,
    );
    expect(styled).toMatch(/data-slot="list"[^>]*class="menu-list"/);
    expect(styled).toContain("menu-out");
  });

  it("renders the email as primary line for an account without a name", () => {
    const html = renderToStaticMarkup(<AccountMenuList {...props} accounts={[{ ...accounts[0], name: null }]} />);
    expect(html).not.toContain("undefined");
    expect(html.match(/ada@example\.com/g)).toHaveLength(1);
  });
});

describe("AccountMenu", () => {
  it("renders the root and the trigger slots with the avatar of the current account", () => {
    const html = renderToStaticMarkup(
      <AccountMenu {...props} className="legacy" classNames={{ root: "menu-root", trigger: "menu-trigger" }} />,
    );
    expect(html).toContain('data-slot="root"');
    expect(html).toContain("legacy menu-root");
    expect(html).toContain("menu-trigger");
    expect(html).toContain('aria-label="ada@example.com"');
    expect(html).not.toContain('role="menu"');
  });
});
