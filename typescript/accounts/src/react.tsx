import { useEffect, useRef, useState, type CSSProperties } from "react"
import { addAccountUrl, manageAccountUrl, signOutUrl, switchAccountUrl } from "./urls.js"
import type { SsoAccount } from "./index.js"

export interface AccountMenuLabels {
  addAccount: string
  manageAccount: string
  signOut: string
  signInAgain: string
  accounts: string
}

const DEFAULT_LABELS: AccountMenuLabels = {
  addAccount: "Add account",
  manageAccount: "Manage your ViGrise Account",
  signOut: "Sign out",
  signInAgain: "Sign in again",
  accounts: "Accounts",
}

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
  labels?: Partial<AccountMenuLabels>
  className?: string
  /** Runs when the person chooses sign-out, before the browser leaves for Accounts. */
  onSignOut?: () => void
}

const styles: Record<string, CSSProperties> = {
  list: {
    listStyle: "none",
    margin: 0,
    padding: 4,
    minWidth: 240,
    background: "var(--vg-menu-bg, #fff)",
    color: "var(--vg-menu-fg, #111827)",
    border: "1px solid var(--vg-menu-border, #e5e7eb)",
    borderRadius: 8,
    boxShadow: "0 8px 24px rgba(0,0,0,0.12)",
  },
  row: { display: "flex", alignItems: "center", gap: 8, padding: "8px 10px", borderRadius: 6, color: "inherit", textDecoration: "none" },
  avatar: { width: 28, height: 28, borderRadius: "50%", background: "var(--vg-menu-avatar, #e5e7eb)", display: "inline-flex", alignItems: "center", justifyContent: "center", fontSize: 12, flex: "none", objectFit: "cover" },
  text: { display: "flex", flexDirection: "column", minWidth: 0, lineHeight: 1.25 },
  muted: { fontSize: 12, opacity: 0.7, overflow: "hidden", textOverflow: "ellipsis" },
  divider: { borderTop: "1px solid var(--vg-menu-border, #e5e7eb)", margin: "4px 0" },
}

function Avatar({ account }: { account: SsoAccount }) {
  if (account.picture) return <img src={account.picture} alt="" style={styles.avatar} />
  const initial = (account.name || account.email || "?").slice(0, 1).toUpperCase()
  return <span style={styles.avatar} aria-hidden="true">{initial}</span>
}

/** The list of accounts with the add, manage and sign-out links. */
export function AccountMenuList(props: AccountMenuProps) {
  const labels = { ...DEFAULT_LABELS, ...props.labels }
  const redirect = props.postLogoutRedirectUri ?? props.returnTo
  return (
    <ul role="menu" aria-label={labels.accounts} style={styles.list}>
      {props.accounts.map((account) => (
        <li key={account.session_id} role="none">
          {account.active ? (
            <div role="menuitem" aria-current="true" style={styles.row}>
              <Avatar account={account} />
              <span style={styles.text}>
                <strong>{account.name || account.email}</strong>
                <span style={styles.muted}>{account.email}</span>
              </span>
            </div>
          ) : (
            <a role="menuitem" href={switchAccountUrl(props.accountsUrl, account.session_id, props.returnTo)} style={styles.row}>
              <Avatar account={account} />
              <span style={styles.text}>
                <span>{account.name || account.email}</span>
                <span style={styles.muted}>{account.expired ? labels.signInAgain : account.email}</span>
              </span>
            </a>
          )}
        </li>
      ))}
      <li role="none" style={styles.divider} />
      <li role="none">
        <a role="menuitem" href={addAccountUrl(props.accountsUrl, props.returnTo)} style={styles.row}>{labels.addAccount}</a>
      </li>
      <li role="none">
        <a role="menuitem" href={manageAccountUrl(props.accountsUrl)} style={styles.row}>{labels.manageAccount}</a>
      </li>
      <li role="none">
        <a
          role="menuitem"
          href={signOutUrl({ accountsUrl: props.accountsUrl, clientId: props.clientId, postLogoutRedirectUri: redirect })}
          style={styles.row}
          onClick={props.onSignOut}
        >
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
  const active = props.accounts.find((account) => account.active) ?? props.accounts[0]

  useEffect(() => {
    if (!open) return
    const onPointer = (event: MouseEvent) => {
      if (root.current && !root.current.contains(event.target as Node)) setOpen(false)
    }
    const onKey = (event: KeyboardEvent) => event.key === "Escape" && setOpen(false)
    document.addEventListener("mousedown", onPointer)
    document.addEventListener("keydown", onKey)
    return () => {
      document.removeEventListener("mousedown", onPointer)
      document.removeEventListener("keydown", onKey)
    }
  }, [open])

  if (!active) return null
  return (
    <div ref={root} className={props.className} style={{ position: "relative", display: "inline-block" }}>
      <button
        type="button"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={active.email ?? active.name ?? undefined}
        onClick={() => setOpen((value) => !value)}
        style={{ background: "none", border: 0, padding: 0, cursor: "pointer" }}
      >
        <Avatar account={active} />
      </button>
      {open && (
        <div style={{ position: "absolute", right: 0, top: "calc(100% + 6px)", zIndex: 50 }}>
          <AccountMenuList {...props} />
        </div>
      )}
    </div>
  )
}
