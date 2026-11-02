/// Config documents: a resource's, trigger's or canvas's config as plain JSON,
/// edited by key and value. Paths address values inside it; edits are
/// immutable (a new object each time), so views and dirty checks stay simple.

export type Path = (string | number)[];
export const pathKey = (p: Path) => p.map(String).join('.');

export function getAt(v: unknown, p: Path): unknown {
  return p.reduce<any>((o, k) => (o == null ? undefined : o[k]), v);
}

export function setAt<T>(v: T, p: Path, next: unknown): T {
  if (!p.length) return next as T;
  const [k, ...rest] = p;
  const base: any = Array.isArray(v) ? [...v] : { ...(v as any) };
  base[k] = setAt(base[k], rest, next);
  return base;
}

export function removeAt<T>(v: T, p: Path): T {
  if (!p.length) return v;
  const [k, ...rest] = p;
  if (rest.length) return setAt(v, [k], removeAt((v as any)?.[k], rest));
  if (Array.isArray(v)) return v.filter((_, i) => i !== k) as T;
  const { [k as string]: _gone, ...keep } = v as any;
  return keep;
}

/// Which values the server showed masked. Mirrors `masked_config` in
/// flowr-mcp-core: a `secret`'s every string; elsewhere, strings under keys that
/// name a key, secret, password, token or credential — not `*_ref` keys (names of
/// secrets), and not inside arrays. The server keeps the stored value when an
/// update echoes the masked one back, so these may be saved unchanged.
export function maskedPaths(type: string, config: unknown): Set<string> {
  const out = new Set<string>();
  const all = (v: unknown, p: Path) => {
    if (typeof v === 'string') out.add(pathKey(p));
    else if (Array.isArray(v)) v.forEach((x, i) => all(x, [...p, i]));
    else if (v && typeof v === 'object') for (const [k, x] of Object.entries(v)) all(x, [...p, k]);
  };
  const sensitive = (v: unknown, p: Path) => {
    if (!v || typeof v !== 'object' || Array.isArray(v)) return;
    for (const [k, x] of Object.entries(v)) {
      const key = k.toLowerCase();
      if (key.endsWith('_ref')) continue;
      if (/key|secret|password|token|credential/.test(key)) { if (typeof x === 'string') out.add(pathKey([...p, k])); }
      else sensitive(x, [...p, k]);
    }
  };
  if (type === 'secret') all(config, []);
  else sensitive(config, []);
  return out;
}

export const toText = (v: unknown) => JSON.stringify(v ?? {}, null, 2);

export function fromText(text: string | undefined): { value: any; error: string | null } {
  try { return { value: JSON.parse(text ?? '{}'), error: null }; } catch (e) { return { value: null, error: (e as Error).message }; }
}
