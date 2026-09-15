// Markdown rendering for chat replies.
//
// Output is injected with {@html}, and the source is LLM-generated text that can
// contain arbitrary HTML, so parsing MUST be followed by sanitising — otherwise a
// model (or anything it echoes back from a tool result) could inject script or
// event-handler attributes into the console. DOMPurify strips those while
// leaving the structural markup tables and headings need.
import { marked } from 'marked';
import DOMPurify from 'dompurify';

marked.setOptions({
  gfm: true,      // tables, strikethrough, autolinks
  breaks: true,   // single newline → <br>, matching how chat models write
});

/** Render markdown to sanitised HTML. Falls back to escaped text on any error. */
export function renderMarkdown(src: string): string {
  try {
    const html = marked.parse(src ?? '', { async: false }) as string;
    return DOMPurify.sanitize(html, {
      USE_PROFILES: { html: true },
      // Links open elsewhere; forbid anything that can execute or embed.
      FORBID_TAGS: ['script', 'style', 'iframe', 'object', 'embed', 'form', 'input'],
      FORBID_ATTR: ['onerror', 'onload', 'onclick', 'onmouseover', 'style'],
    });
  } catch {
    const div = document.createElement('div');
    div.textContent = src ?? '';
    return div.innerHTML;
  }
}
