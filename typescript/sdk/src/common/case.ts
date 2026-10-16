const camel = (name: string) => name.replace(/_([a-z0-9])/g, (_, letter: string) => letter.toUpperCase());

/** Converts the keys of parsed JSON from snake_case to camelCase, ISO timestamps named `*_at` to dates, and drops nulls. */
export function camelize(value: unknown): any {
  if (Array.isArray(value)) return value.map(camelize);
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value)
        .filter(([, entry]) => entry !== null)
        .map(([key, entry]) => [
          camel(key),
          key.endsWith("_at") && typeof entry === "string" ? new Date(entry) : camelize(entry),
        ]),
    );
  }
  return value;
}
