/// Running a statement on a database resource — the one place the UI does so.
import { adbSql, mcpCall } from '$lib/services/api';
import { fail, ok, type Result } from '../items';
import { safely } from './util';

export type QueryResult = {
  /// Rows as objects when the answer is tabular; otherwise empty and `raw` holds it.
  columns: string[];
  rows: Record<string, unknown>[];
  raw: string;
  ms: number;
};

export type QueryTarget = { project: string; name: string; type: string };

/// `lang` picks the endpoint where a database speaks more than one (ADB: AQL,
/// or read-only SQL over its tables for superadmins — the server decides).
export const run = (db: QueryTarget, text: string, lang: string): Promise<Result<QueryResult>> =>
  safely('query', async () => {
    const t0 = performance.now();
    const ms = () => Math.round(performance.now() - t0);
    const q = text.trim();
    if (!q) return fail('Nothing to run');

    if (lang === 'sql' && (db.type === 'adb' || db.type === 'clawdb')) {
      const r = await adbSql(`${db.project}/${db.name}`, q);
      if (!r.ok || !r.data?.success) return fail(r.error ?? r.data?.error ?? 'SQL failed');
      const body = r.data.data as any;
      return ok({ columns: body.columns ?? [], rows: body.rows ?? [], raw: JSON.stringify(body.rows ?? [], null, 2), ms: ms() });
    }

    let r;
    if (db.type === 'postgres' || db.type === 'mysql') {
      r = await mcpCall('call_sql', { query: q, resource: db.name, namespace: db.project });
    } else if (db.type === 'redis') {
      const [cmd, ...args] = q.split(/\s+/);
      r = await mcpCall('call_redis', { operation: cmd.toLowerCase(), key: args[0] ?? '', value: args[1] ?? '', args, resource: db.name, namespace: db.project });
    } else {
      r = await mcpCall('query_resource', { query: q, resource: db.name, namespace: db.project });
    }
    if (!r.ok) return fail(r.error ?? 'Query failed');
    return ok({ ...shape(r.data), ms: ms() });
  });

/// A table when the answer is a list of objects (as is, or as JSON text).
function shape(data: unknown): Omit<QueryResult, 'ms'> {
  let v = data;
  if (typeof v === 'string') { try { v = JSON.parse(v); } catch { return { columns: [], rows: [], raw: data as string }; } }
  if (Array.isArray(v) && v.length && v.every((x) => x && typeof x === 'object' && !Array.isArray(x))) {
    const rows = v as Record<string, unknown>[];
    return { columns: [...new Set(rows.flatMap((x) => Object.keys(x)))], rows, raw: JSON.stringify(rows, null, 2) };
  }
  return { columns: [], rows: [], raw: typeof v === 'string' ? v : JSON.stringify(v, null, 2) };
}
