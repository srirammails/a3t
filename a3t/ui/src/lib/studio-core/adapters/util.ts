import { fail, ok, type Result } from '../items';

/// The client's answer as a Result. The client (`services/api`) returns
/// `{ ok, data, error, status }`; adapters speak `Result`.
export function result<T = any>(r: { ok: boolean; data?: any; error?: string; status?: number }, what: string): Result<T> {
  return r.ok ? ok(r.data as T) : fail(r.error ?? (r.status ? `${what}: HTTP ${r.status}` : `${what} failed`));
}

/// A call that never throws.
export async function safely<T>(what: string, call: () => Promise<Result<T>>): Promise<Result<T>> {
  try { return await call(); } catch (e) { return fail(`${what}: ${(e as Error).message}`); }
}

/// The rows of a listing, whichever key the server put them under.
export const rows = (d: any, ...keys: string[]): any[] =>
  Array.isArray(d) ? d : keys.map((k) => d?.[k]).find(Array.isArray) ?? [];
