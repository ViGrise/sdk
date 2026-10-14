import { useEffect, useId, useRef, useState, type KeyboardEvent } from "react"
import { accountMenuModel } from "./account-menu-model.js"
import type { SsoAccount } from "./index.js"

export interface AccountMenuLabels {
  /** Label of the add-account entry; `addAnotherAccount` takes precedence. */
  addAccount: string
  addAnotherAccount: string
  manageAccount: string
  signOut: string
  signInAgain: string
  accounts: string
  /** Accessible name of the trigger when the current account has no email or name. */
  openMenu: string
}

const DEFAULT_LABELS: AccountMenuLabels = {
  addAccount: "Add another account",
  addAnotherAccount: "Add another account",
  manageAccount: "Manage your ViGrise Account",
  signOut: "Sign out",
  signInAgain: "Sign in again",
  accounts: "Accounts",
  openMenu: "Account menu",
}

/** The parts of the account menu; each carries `data-slot` with its name and the class from `classNames`. */
export const ACCOUNT_MENU_SLOTS = [
  "root",
  "trigger",
  "popover",
  "list",
  "header",
  "avatar",
  "text",
  "primary",
  "secondary",
  "manage",
  "accountRow",
  "addAccount",
  "signOut",
  "divider",
] as const

export type AccountMenuSlot = (typeof ACCOUNT_MENU_SLOTS)[number]
export type AccountMenuClassNames = Partial<Record<AccountMenuSlot, string>>

export interface AccountMenuProps {
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
  labels?: Partial<AccountMenuLabels>
  /** Class of the `root` slot of `AccountMenu`. */
  className?: string
  /** Classes per slot; the component carries no styling of its own. */
  classNames?: AccountMenuClassNames
  /** Runs when the person chooses sign-out, before the browser leaves for Accounts. */
  onSignOut?: () => void
}

function resolveLabels(labels: Partial<AccountMenuLabels> | undefined): AccountMenuLabels {
  const merged = { ...DEFAULT_LABELS, ...labels }
  // A custom `addAccount` label applies unless `addAnotherAccount` is also given.
  if (labels?.addAccount && !labels.addAnotherAccount) merged.addAnotherAccount = labels.addAccount
  return merged
}

function slot(classNames: AccountMenuClassNames | undefined, name: AccountMenuSlot) {
  return { "data-slot": name, className: classNames?.[name] }
}

function Avatar({ account, classNames }: { account: SsoAccount; classNames?: AccountMenuClassNames }) {
  if (account.picture) return <img src={account.picture} alt="" {...slot(classNames, "avatar")} />
  const initial = (account.name?.trim() || account.email?.trim() || "?").slice(0, 1).toUpperCase()
  return (
    <span aria-hidden="true" {...slot(classNames, "avatar")}>
      {initial}
    </span>
  )
}

function moveFocus(event: KeyboardEvent<HTMLElement>) {
  const keys = ["ArrowDown", "ArrowUp", "Home", "End"]
  if (!keys.includes(event.key)) return
  const items = Array.from(event.currentTarget.querySelectorAll<HTMLElement>('[role="menuitem"]'))
  if (items.length === 0) return
  event.preventDefault()
  const index = items.indexOf(document.activeElement as HTMLElement)
  const next =
    event.key === "Home" ? 0
    : event.key === "End" ? items.length - 1
    : event.key === "ArrowDown" ? (index + 1) % items.length
    : (index <= 0 ? items.length : index) - 1
  items[next]?.focus()
}

/** The Google style account list: current account, other accounts, add account and sign-out. */
export function AccountMenuList(props: AccountMenuProps) {
  const labels = resolveLabels(props.labels)
  const classNames = props.classNames
  const model = accountMenuModel({ ...props, signInAgainLabel: labels.signInAgain })
  const headerTextId = useId()
  return (
    <ul role="menu" aria-label={labels.accounts} onKeyDown={moveFocus} {...slot(classNames, "list")}>
      {model.current && (
        <li role="none">
          <div aria-current="true" {...slot(classNames, "header")}>
            <Avatar account={model.current} classNames={classNames} />
            <span id={headerTextId} {...slot(classNames, "text")}>
              <span {...slot(classNames, "primary")}>{model.currentPrimary}</span>
              {model.currentSecondary && <span {...slot(classNames, "secondary")}>{model.currentSecondary}</span>}
            </span>
            <a role="menuitem" href={model.manageHref} aria-describedby={headerTextId} {...slot(classNames, "manage")}>
              {labels.manageAccount}
            </a>
          </div>
        </li>
      )}
      {model.others.length > 0 && <li role="separator" {...slot(classNames, "divider")} />}
      {model.others.map((row) => (
        <li key={row.account.session_id} role="none">
          <a role="menuitem" href={row.href} {...slot(classNames, "accountRow")}>
            <Avatar account={row.account} classNames={classNames} />
            <span {...slot(classNames, "text")}>
              <span {...slot(classNames, "primary")}>{row.primary}</span>
              {row.secondary && <span {...slot(classNames, "secondary")}>{row.secondary}</span>}
            </span>
          </a>
        </li>
      ))}
      <li role="none">
        <a role="menuitem" href={model.addAccountHref} {...slot(classNames, "addAccount")}>
          {labels.addAnotherAccount}
        </a>
      </li>
      <li role="separator" {...slot(classNames, "divider")} />
      <li role="none">
        <a role="menuitem" href={model.signOutHref} onClick={props.onSignOut} {...slot(classNames, "signOut")}>
          {labels.signOut}
        </a>
      </li>
    </ul>
  )
}

/** Header control that opens the account list. */
export function AccountMenu(props: AccountMenuProps) {
  const [open, setOpen] = useState(false)
  const root = useRef<HTMLDivElement>(null)
  const trigger = useRef<HTMLButtonElement>(null)
  const labels = resolveLabels(props.labels)
  const classNames = props.classNames
  const active = props.accounts.find((account) => account.active) ?? props.accounts[0]

  useEffect(() => {
    if (!open) return
    root.current?.querySelector<HTMLElement>('[role="menuitem"]')?.focus()
    const onPointer = (event: MouseEvent) => {
      if (root.current && !root.current.contains(event.target as Node)) setOpen(false)
    }
    const onKey = (event: globalThis.KeyboardEvent) => {
      if (event.key !== "Escape") return
      setOpen(false)
      trigger.current?.focus()
    }
    document.addEventListener("mousedown", onPointer)
    document.addEventListener("keydown", onKey)
    return () => {
      document.removeEventListener("mousedown", onPointer)
      document.removeEventListener("keydown", onKey)
    }
  }, [open])

  if (!active) return null
  return (
    <div ref={root} data-slot="root" className={[props.className, classNames?.root].filter(Boolean).join(" ") || undefined}>
      <button
        ref={trigger}
        type="button"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={active.email ?? active.name ?? labels.openMenu}
        onClick={() => setOpen((value) => !value)}
        {...slot(classNames, "trigger")}
      >
        <Avatar account={active} classNames={classNames} />
      </button>
      {open && (
        <div onKeyDown={(event) => event.key === "Tab" && setOpen(false)} {...slot(classNames, "popover")}>
          <AccountMenuList {...props} />
        </div>
      )}
    </div>
  )
}
