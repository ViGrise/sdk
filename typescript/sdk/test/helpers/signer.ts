import { exportJWK, generateKeyPair, SignJWT, type JWK, type KeyLike } from "jose";

/** An RSA key that signs tokens the way ViGrise Accounts does. */
export class TestSigner {
  private constructor(
    readonly kid: string,
    readonly jwk: JWK,
    private readonly privateKey: KeyLike,
  ) {}

  static async generate(kid = `kid-${Math.random().toString(16).slice(2)}`): Promise<TestSigner> {
    const { publicKey, privateKey } = await generateKeyPair("RS256", { modulusLength: 2048, extractable: true });
    return new TestSigner(kid, { ...(await exportJWK(publicKey)), kid, alg: "RS256", use: "sig" }, privateKey);
  }

  get jwkSet() {
    return { keys: [this.jwk] };
  }

  /** Signs claims as an RS256 JWT; `header` overrides header fields. */
  sign(claims: Record<string, unknown>, header: Record<string, unknown> = {}): Promise<string> {
    return new SignJWT(claims as any)
      .setProtectedHeader({ alg: "RS256", kid: this.kid, ...header })
      .sign(this.privateKey);
  }
}
