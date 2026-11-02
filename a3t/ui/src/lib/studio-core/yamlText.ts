/// Writing YAML back in the style it was read in, so an edit changes only what
/// it changes: lists indented under their key or not, as the text has them, and
/// long text not re-wrapped. Plain TypeScript (studio-core).
import * as YAML from 'yaml';

/// Does the text indent a list under its key (`do:\n  - a`) or not (`do:\n- a`)?
export function indentsLists(text: string): boolean {
  const m = text.match(/^([ \t]*)[^\s#-][^\n]*:[ \t]*\n(\1[ \t]*)- /m);
  return m ? m[2].length > m[1].length : true;
}

export function write(doc: YAML.Document, original: string): string {
  return doc.toString({ indentSeq: indentsLists(original), lineWidth: 0 });
}
