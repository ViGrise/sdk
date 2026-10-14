export interface Credentials {
  accessKeyId: string;
  secretAccessKey: string;
  sessionToken?: string;
}

/** Supplies the credentials to sign each request with, such as a refreshed role session. */
export type CredentialsProvider = () => Credentials | Promise<Credentials>;
