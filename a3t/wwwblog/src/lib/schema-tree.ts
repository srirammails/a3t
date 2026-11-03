// Renders a language reference (the CNCF Workflow DSL, AQL) as nested <details> HTML for <SchemaTree />.

export type SchemaNode = {
	/** Key or keyword as written. */
	k: string;
	/** Type label shown as a badge; the tree's tone() picks its colour. */
	t: string;
	/** Required. */
	r?: boolean;
	/** What it is for. */
	d: string;
	ex?: string;
	c?: SchemaNode[];
};

export type TreeConfig = {
	/** Prefix of every node id, and of the #<prefix>-<path> deep links. */
	prefix: string;
	/** First crumb of every note's path. */
	root: string;
	slug: (k: string) => string;
	tone: (t: string) => 'ephemeral' | 'signal' | 'checkpoint' | 'ok' | 'plain';
	/** Colours one line of an example; returns HTML. */
	paint: (line: string) => string;
};

export const esc = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

// Light YAML colouring: keys, list dashes, ${ } expressions, quoted strings, comments.
export const paintYaml = (line: string) => {
	if (/^\s*#/.test(line)) return `<span class="cm">${esc(line)}</span>`;
	const m = line.match(/^(\s*)(- )?([\w$.-]+:)?(.*)$/)!;
	const [, ind, dash = '', key = '', rest] = m;
	const val = esc(rest)
		.replace(/(\$\{.*\})/g, '<span class="ex">$1</span>')
		.replace(/('[^'$]*')/g, '<span class="sq">$1</span>');
	return `${ind}${dash ? '<span class="pu">- </span>' : ''}${key ? `<span class="ky">${esc(key)}</span>` : ''}${val}`;
};

// Light AQL colouring: keywords, memory types, strings, variables, -- comments.
const AQL_WORDS = /\b(SCAN|RECALL|LOOKUP|LOAD|STORE|UPDATE|FORGET|LINK|UNLINK|MERGE|ENTITY|REFLECT|PIPELINE|SUBSCRIBE|UNSUBSCRIBE|ACK|PUBLISH|WATCH|UNWATCH|FROM|INTO|TO|WHERE|KEY|LIKE|PATTERN|THRESHOLD|WINDOW|LAST|TOP|BY|SINCE|AND|OR|IN|CONTAINS|STARTS_WITH|ENDS_WITH|ORDER|ASC|DESC|LIMIT|RETURN|MIN_CONFIDENCE|TIMEOUT|AGGREGATE|COUNT|AVG|SUM|MIN|MAX|AS|GROUP|HAVING|WITH|LINKS|FOLLOW|TYPE|WEIGHT|MIN_WEIGHT|DEPTH|OUTBOUND|INBOUND|BOTH|OF|VALID|AT|HISTORY|AGO|TTL|SCOPE|NAMESPACE|FACT|MULTI|THEN|TOPIC|KINDS|NOW|OFFSET|DEBOUNCE)\b/g;
const AQL_MEMORY = /\b(WORKING|EPISODIC|SEMANTIC|PROCEDURAL|TOOLS|ALL)\b/g;
export const paintAql = (line: string) => {
	const at = line.indexOf('--');
	const [code, comment] = at >= 0 ? [line.slice(0, at), line.slice(at)] : [line, ''];
	// Strings first, then colour keywords only outside them.
	const html = code.split(/("(?:[^"\\]|\\.)*")/).map((part, i) => i % 2
		? `<span class="sq">${esc(part)}</span>`
		: esc(part)
			.replace(/(\{\w+\}|\$\w+)/g, '<span class="ex">$1</span>')
			.replace(AQL_MEMORY, '<span class="ky">$1</span>')
			.replace(AQL_WORDS, '<span class="kw">$1</span>')).join('');
	return html + (comment ? `<span class="cm">${esc(comment)}</span>` : '');
};

type Row = SchemaNode & { id: string; trail: string[] };

export function renderTree(nodes: SchemaNode[], cfg: TreeConfig): string {
	const walk = (ns: SchemaNode[], parent: string, trail: string[]): Row[] =>
		ns.map((n) => ({ ...n, id: parent ? `${parent}.${cfg.slug(n.k)}` : cfg.slug(n.k), trail: [...trail, n.k] }));

	const render = (rows: Row[]): string => `<ul>${rows.map((n) => `
	<li data-key="${esc(n.k.toLowerCase())}">
		<details id="${cfg.prefix}-${n.id}">
			<summary>
				<span class="st-plus" aria-hidden="true"></span>
				<code class="st-key">${esc(n.k)}</code>
				<span class="st-type st-${cfg.tone(n.t)}">${esc(n.t)}</span>
				${n.r ? '<span class="st-req">required</span>' : ''}
			</summary>
			<div class="st-note">
				<p class="st-path">${n.trail.map(esc).join(' <span>›</span> ')}</p>
				<p>${esc(n.d)}</p>
				${n.ex ? `<pre><code>${n.ex.split('\n').map(cfg.paint).join('\n')}</code></pre>` : ''}
			</div>
			${n.c ? render(walk(n.c, n.id, n.trail)) : ''}
		</details>
	</li>`).join('')}</ul>`;

	return render(walk(nodes, '', [cfg.root]));
}
