const ASSUME_ROLE_PATH = "/v1/assume-role";

type Env = Record<string, string | undefined>;

function processEnv(): Env {
  return (globalThis as { process?: { env?: Env } }).process?.env ?? {};
}

/** The environment variable of one service, such as `VIGRISE_ENDPOINT_URL_VOICE_STUDIO` for `voice-studio`. */
export function envKey(service: string): string {
  return `VIGRISE_ENDPOINT_URL_${service.toUpperCase().replaceAll("-", "_")}`;
}

/**
 * Where a ViGrise service lives: the explicit endpoint, then
 * `VIGRISE_ENDPOINT_URL_<SERVICE>`, then `VIGRISE_ENDPOINT_URL`, then
 * `https://<service>-api.vigrise.com`. Empty values count as unset and a
 * trailing slash is dropped.
 */
export function resolveEndpoint(service: string, explicit?: string, env: Env = processEnv()): string {
  const endpoint = explicit || env[envKey(service)] || env.VIGRISE_ENDPOINT_URL || `https://${service}-api.vigrise.com`;
  return endpoint.replace(/\/+$/, "");
}

/** The URL of the role session call. The STS endpoint is accepted with or without the `/v1/assume-role` path. */
export function stsUrl(explicit?: string, env?: Env): string {
  const endpoint = resolveEndpoint("sts", explicit, env);
  return endpoint.endsWith(ASSUME_ROLE_PATH) ? endpoint : `${endpoint}${ASSUME_ROLE_PATH}`;
}

/** The base URL of the STS endpoint, without the `/v1/assume-role` path. */
export function stsBaseUrl(explicit?: string, env?: Env): string {
  const endpoint = resolveEndpoint("sts", explicit, env);
  return endpoint.endsWith(ASSUME_ROLE_PATH) ? endpoint.slice(0, -ASSUME_ROLE_PATH.length) : endpoint;
}
