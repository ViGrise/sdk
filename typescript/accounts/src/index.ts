export { ssoTokenHash } from "./hash.js"
export {
  addAccountUrl,
  manageAccountUrl,
  signOutUrl,
  switchAccountUrl,
  type SignOutUrlOptions,
} from "./urls.js"
export {
  createSessionChannel,
  reloadOnSessionChange,
  type SessionChannel,
  type SessionMessage,
} from "./session-channel.js"
export {
  applySsoState,
  fetchSsoExchange,
  readSsoState,
  renderSsoStateJson,
  SSO_EXCHANGE_HEADER,
  SSO_MARKER_COOKIE,
  SSO_STATE_ELEMENT_ID,
  type SsoExchange,
  type SsoOutcome,
  type SsoState,
  type SsoStateHandlers,
  watchSsoState,
  type WatchSsoStateOptions,
} from "./state.js"

/** An account of the browser's group, as the backend returns it for the account menu. */
export interface SsoAccount {
  session_id: string
  identity_id: string
  name?: string | null
  email?: string | null
  picture?: string | null
  active: boolean
  expired?: boolean
}
