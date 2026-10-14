import type { SsoAccount } from "./index.js"
import { addAccountUrl, manageAccountUrl, signOutUrl, switchAccountUrl } from "./urls.js"

/** Primary line of an account: the name when it has one, else the email. */
export function displayName(account: Pick<SsoAccount, "name" | "email">): string {
  return account.name?.trim() || account.email?.trim() || ""
}

/**
 * Secondary line of an account: "sign in again" for an expired account, else the
 * email when the primary line is the name, else nothing.
 */
export function secondaryLine(
  account: Pick<SsoAccount, "name" | "email" | "expired">,
  signInAgainLabel: string,
): string | null {
  if (account.expired) return signInAgainLabel
  return account.name?.trim() ? account.email?.trim() || null : null
}

export interface AccountMenuModelInput {
  accounts: SsoAccount[]
  /** Public URL of ViGrise Accounts. */
  accountsUrl: string
  /** URL of the current app page Accounts returns to. */
  returnTo: string
  /** OAuth2 client id of the app, for sign-out. */
  clientId: string
  /** Where sign-out returns; defaults to `returnTo`. */
  postLogoutRedirectUri?: string
  /** The `sso_sid` of the app session, so sign-out ends that Accounts session. */
  ssoSid?: string
  /** Text of the secondary line of an expired account. */
  signInAgainLabel?: string
}

export interface AccountMenuRow {
  account: SsoAccount
  primary: string
  secondary: string | null
  /** Switch link on Accounts. */
  href: string
}

export interface AccountMenuModel {
  /** The active account; the first account when none is active. */
  current: SsoAccount | undefined
  currentPrimary: string
  currentSecondary: string | null
  others: AccountMenuRow[]
  manageHref: string
  addAccountHref: string
  signOutHref: string
}

const DEFAULT_SIGN_IN_AGAIN = "Sign in again"

/** Everything the account menu renders, without any markup. */
export function accountMenuModel(input: AccountMenuModelInput): AccountMenuModel {
  const signInAgain = input.signInAgainLabel ?? DEFAULT_SIGN_IN_AGAIN
  const current = input.accounts.find((account) => account.active) ?? input.accounts[0]
  const others = input.accounts
    .filter((account) => account !== current)
    .map((account) => ({
      account,
      primary: displayName(account),
      secondary: secondaryLine(account, signInAgain),
      href: switchAccountUrl(input.accountsUrl, account.session_id, input.returnTo),
    }))
  return {
    current,
    currentPrimary: current ? displayName(current) : "",
    currentSecondary: current ? secondaryLine(current, signInAgain) : null,
    others,
    manageHref: manageAccountUrl(input.accountsUrl),
    addAccountHref: addAccountUrl(input.accountsUrl, input.returnTo),
    signOutHref: signOutUrl({
      accountsUrl: input.accountsUrl,
      clientId: input.clientId,
      postLogoutRedirectUri: input.postLogoutRedirectUri ?? input.returnTo,
      ssoSid: input.ssoSid,
    }),
  }
}
