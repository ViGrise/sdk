import { stsUrl } from "../common/endpoint.js";
import { signedFetch } from "./fetch.js";
import type { Credentials, CredentialsProvider } from "./types.js";

export interface AssumeRoleInput {
  roleVrn: string;
  roleSessionName: string;
  durationSeconds?: number;
  sourceIdentity?: string;
  /** Narrows the role's permissions for this session. */
  sessionPolicy?: unknown;
}

export interface AssumeRoleOptions {
  /** The STS endpoint, with or without the `/v1/assume-role` path; `VIGRISE_ENDPOINT_URL_STS`, `VIGRISE_ENDPOINT_URL` or `https://sts-api.vigrise.com` when omitted. */
  endpoint?: string;
  /** Long-term credentials of the caller; resolved from the environment when omitted. */
  credentials?: Credentials | CredentialsProvider;
}

export interface RoleSession {
  credentials: Credentials;
  expiration: Date;
  roleVrn: string;
  sessionVrn: string;
}

/** Calls STS `AssumeRole`. Callers request a new session before `expiration`. */
export async function assumeRole(input: AssumeRoleInput, options: AssumeRoleOptions): Promise<RoleSession> {
  const body = JSON.stringify({
    role_vrn: input.roleVrn,
    role_session_name: input.roleSessionName,
    duration_seconds: input.durationSeconds,
    source_identity: input.sourceIdentity,
    session_policy: input.sessionPolicy,
  });
  const response = await signedFetch(
    stsUrl(options.endpoint),
    { method: "POST", headers: { "content-type": "application/json" }, body },
    { service: "sts", credentials: options.credentials },
  );
  const reply = await response.json();
  if (!response.ok) {
    const error = reply?.error ?? {};
    throw new Error(`${error.code ?? response.status}: ${error.message ?? "AssumeRole failed"}`);
  }
  return {
    credentials: {
      accessKeyId: reply.credentials.access_key_id,
      secretAccessKey: reply.credentials.secret_access_key,
      sessionToken: reply.credentials.session_token,
    },
    expiration: new Date(reply.credentials.expiration),
    roleVrn: reply.assumed_role.role_vrn,
    sessionVrn: reply.assumed_role.session_vrn,
  };
}
