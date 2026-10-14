/** An account of the browser's group, as the backend returns it for the account menu. */
export interface SsoAccount {
  session_id: string;
  identity_id: string;
  name?: string | null;
  email?: string | null;
  picture?: string | null;
  active: boolean;
  expired?: boolean;
}
