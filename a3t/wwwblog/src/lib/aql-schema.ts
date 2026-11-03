// AQL (Agent Query Language) as a tree of verbs, for <SchemaTree of="aql" />.
// Follows grammar/aql.pest and the v0.5 spec plus the v0.6–v0.10 additions in srirammails/AQL.
// Every example is a complete statement; check them against the real parser before publishing
// (each one must parse with aql_parser::parse).
import type { SchemaNode } from './schema-tree';

// Building blocks shared by several verbs.

const where = (verb: string): SchemaNode => ({
	k: 'WHERE', t: 'predicate',
	d: 'Conditions on fields: field, operator, value. Join them with AND and OR, group them with brackets. Fields can be dotted, like metadata.pod.',
	ex: `${verb} WHERE pod = "payments-api" AND resolved = true`,
	c: [
		{ k: '=  !=  <>  <  >  <=  >=', t: 'operator', d: 'Compare a field with a value. Strings in double quotes; numbers, true, false and null as they are.', ex: `RECALL FROM EPISODIC WHERE resolution_time > 300` },
		{ k: 'CONTAINS  STARTS_WITH  ENDS_WITH', t: 'operator', d: 'Match part of a text field.', ex: `RECALL FROM SEMANTIC WHERE concept STARTS_WITH "k8s_"` },
		{ k: 'IN [ … ]', t: 'operator', d: 'The field equals any value in the list.', ex: `RECALL FROM EPISODIC WHERE pod IN ["payments-api", "checkout"]` },
		{ k: 'AND  OR  ( )', t: 'logic', d: 'Combine conditions. Brackets make the grouping explicit.', ex: `RECALL FROM EPISODIC WHERE (pod = "a" OR pod = "b") AND resolved = true` },
		{ k: '{name}  $name', t: 'value', d: 'A variable instead of a literal: bound by the request, or by an earlier stage of a PIPELINE.', ex: `RECALL FROM EPISODIC WHERE url = {url} LIMIT 10` },
	],
});

const shape: SchemaNode = {
	k: 'shape the result', t: 'modifiers',
	d: 'Order, cut and trim what comes back, so the LLM gets a few useful rows instead of everything.',
	ex: `RECALL FROM EPISODIC WHERE outcome = "success"
  ORDER BY confidence DESC LIMIT 5
  RETURN incident_id, action, confidence`,
	c: [
		{ k: 'ORDER BY field [ASC | DESC]', t: 'modifier', d: 'Sort by one field. Ascending unless you say DESC.', ex: `RECALL FROM EPISODIC WHERE pod = "payments-api" ORDER BY time DESC` },
		{ k: 'LIMIT n', t: 'modifier', d: 'Return at most n records.', ex: `RECALL FROM EPISODIC WHERE pod = "payments-api" LIMIT 10` },
		{ k: 'RETURN fields', t: 'modifier', d: 'Return only these fields, or * for all of them. Keeps the context small.', ex: `LOOKUP FROM SEMANTIC KEY concept = "payments-api" RETURN owner, memory_limit` },
		{ k: 'MIN_CONFIDENCE c', t: 'modifier', d: 'Drop records whose confidence is below c (0 to 1).', ex: `RECALL FROM SEMANTIC LIKE $page_embedding MIN_CONFIDENCE 0.8` },
		{ k: 'TIMEOUT d', t: 'modifier', d: 'A time budget for this statement. Durations are a number and a unit: ms, s, m, h or d.', ex: `RECALL FROM EPISODIC WHERE pod = "payments-api" TIMEOUT 50ms` },
	],
};

const summarise: SchemaNode = {
	k: 'summarise', t: 'modifiers',
	d: 'Turn many records into a few numbers. Often the LLM needs a signal, not fifty rows.',
	ex: `RECALL FROM EPISODIC WHERE strategy = "tech_news_premium"
  AGGREGATE COUNT(*) AS uses, AVG(ctr) AS avg_ctr
  HAVING uses > 10 AND avg_ctr < 0.01
  RETURN uses, avg_ctr`,
	c: [
		{ k: 'AGGREGATE f(field) AS name', t: 'modifier', d: 'Compute COUNT, AVG, SUM, MIN or MAX over the matched records. COUNT(*) counts them. AS names the result.', ex: `RECALL FROM EPISODIC WHERE root_cause = "OOMKilled"
  AGGREGATE COUNT(*) AS incidents, AVG(resolution_time) AS avg_minutes` },
		{ k: 'GROUP BY fields', t: 'modifier', d: 'With AGGREGATE: one result row per group instead of one overall.', ex: `RECALL FROM EPISODIC WHERE outcome = "won"
  AGGREGATE COUNT(*) AS wins GROUP BY url` },
		{ k: 'HAVING condition', t: 'modifier', d: 'Filter on the aggregated values. Needs AGGREGATE.', ex: `RECALL FROM EPISODIC WHERE outcome = "won"
  AGGREGATE COUNT(*) AS wins GROUP BY url
  HAVING wins > 5` },
	],
};

const linkSelect: SchemaNode[] = [
	{ k: 'ALL', t: 'option', d: 'Every link type.', ex: `LOOKUP FROM PROCEDURAL KEY pattern_id = "oom-fix" WITH LINKS ALL` },
	{ k: 'TYPE "t"', t: 'option', d: 'One link type, by name.', ex: `RECALL FROM SEMANTIC WHERE concept = "kubernetes_oom" FOLLOW LINKS TYPE "triggers"` },
	{ k: 'TYPE IN ("a", "b")', t: 'option', d: 'Several link types.', ex: `LOOKUP FROM SEMANTIC KEY id = "person:ada" FOLLOW LINKS TYPE IN ("works_at", "based_in")` },
	{ k: 'OUTBOUND  INBOUND  BOTH', t: 'option', d: 'Which way to read links: from this record (the default), into it, or both.', ex: `LOOKUP FROM SEMANTIC KEY id = "org:acme" FOLLOW LINKS TYPE "works_at" INBOUND` },
	{ k: 'MIN_WEIGHT w', t: 'option', d: 'Only links with at least this weight.', ex: `LOOKUP FROM PROCEDURAL KEY pattern_id = "oom-fix" WITH LINKS TYPE "applied_to" MIN_WEIGHT 0.8` },
];

const links: SchemaNode = {
	k: 'read the graph', t: 'modifiers',
	d: 'Records are connected by LINKs. These modifiers show the links behind a record, or walk along them to other records, even in another memory type.',
	ex: `RECALL FROM SEMANTIC WHERE concept = "kubernetes_oom"
  FOLLOW LINKS TYPE "triggers"
  RETURN pattern_id, steps, confidence`,
	c: [
		{ k: 'WITH LINKS', t: 'modifier', d: 'Return each record with its evidence: how many links of each type it has, and their weights.', ex: `LOOKUP FROM PROCEDURAL KEY pattern_id = "oom-fix" WITH LINKS ALL`, c: linkSelect },
		{
			k: 'FOLLOW LINKS', t: 'modifier', d: 'Walk along links and return the records at the other end. Starting in SEMANTIC and following "triggers" can return PROCEDURAL records.',
			ex: `LOOKUP FROM SEMANTIC KEY id = "person:ada" FOLLOW LINKS TYPE "knows" DEPTH 2`,
			c: [...linkSelect, { k: 'DEPTH n', t: 'option', d: 'How many hops to walk. One by default.', ex: `LOOKUP FROM SEMANTIC KEY id = "person:ada" FOLLOW LINKS TYPE "knows" DEPTH 2` }],
		},
	],
};

const time: SchemaNode = {
	k: 'read the past', t: 'modifiers',
	d: 'SEMANTIC and PROCEDURAL keep old versions when a record is updated. These modifiers read memory as it was, not just as it is.',
	ex: `RECALL FROM SEMANTIC WHERE subject = "person:ada" AS OF "2026-10-01"`,
	c: [
		{ k: 'AS OF t', t: 'modifier', d: 'What memory held at time t: a date, a variable, or a duration with AGO.', ex: `RECALL FROM SEMANTIC WHERE predicate = "price" AS OF 7d AGO` },
		{ k: 'VALID AT t', t: 'modifier', d: 'SEMANTIC facts that were true at t, as memory knows them now. Facts carry valid_from and valid_to.', ex: `RECALL FROM SEMANTIC WHERE subject = "person:ada" VALID AT "2015-06-01"` },
		{ k: 'WITH HISTORY', t: 'modifier', d: 'Every version, current and replaced, oldest first.', ex: `LOOKUP FROM PROCEDURAL KEY pattern_id = "deploy" WITH HISTORY` },
	],
};

const share: SchemaNode = {
	k: 'lifetime and sharing', t: 'modifiers',
	d: 'How long a memory lives and who else can see it. Forgetting is built into the write.',
	ex: `STORE INTO SEMANTIC (concept = "k8s_oom_pattern", knowledge = "payments-api OOMs on Fridays")
  SCOPE shared NAMESPACE "platform-agents" TTL 30d`,
	c: [
		{ k: 'TTL d', t: 'modifier', d: 'Forget this memory automatically after a duration: 30s, 15m, 12h, 7d.', ex: `STORE INTO EPISODIC (event = "login", user_id = "u123") TTL 7d` },
		{ k: 'SCOPE private | shared | cluster', t: 'modifier', d: 'Who can see the memory: this agent only, agents sharing the NAMESPACE, or the whole cluster.', ex: `STORE INTO SEMANTIC (concept = "auth", owner = "identity-team") SCOPE shared NAMESPACE "platform-agents"` },
		{ k: 'NAMESPACE "name"', t: 'modifier', d: 'The group of agents a shared memory belongs to.', ex: `RECALL FROM SEMANTIC WHERE concept = "k8s_oom_pattern" SCOPE shared NAMESPACE "platform-agents"` },
		{ k: 'IN SCOPE "scope"', t: 'modifier', d: 'A named memory boundary, such as agent:42 or team:billing. It decides where the memory is placed.', ex: `STORE INTO SEMANTIC (fact = "renewal due") IN SCOPE "team:billing" TTL 30d` },
	],
};

const memories = (types: [string, string][]): SchemaNode[] =>
	types.map(([k, d]) => ({ k, t: 'memory', d, ex: '' }));

const payload: SchemaNode = {
	k: '( field = value, … )', t: 'payload', r: true,
	d: 'The record, as field = value pairs in brackets. Fields are free: the agent decides what to keep.',
	ex: `STORE INTO EPISODIC (incident_id = "inc-001", pod = "payments-api", resolved = true, resolution_time = 420)`,
};

const ends = (verb: string): SchemaNode[] => [
	{ k: 'FROM memory WHERE …', t: 'clause', r: true, d: 'The records the links start at. Any memory type except ALL.', ex: `${verb} FROM PROCEDURAL WHERE pattern_id = "oom-fix" TO EPISODIC WHERE incident_id = "inc-001" TYPE "applied_to"` },
	{ k: 'TO memory WHERE …', t: 'clause', r: true, d: 'The records the links end at. It can be a different memory type.', ex: `${verb} FROM SEMANTIC WHERE concept = "auth" TO PROCEDURAL WHERE pattern_id = "rotate-keys" TYPE "triggers"` },
];

const handle = (verb: string, example: string): SchemaNode => ({ k: 'handle', t: 'clause', r: true, d: `The id returned when the ${verb === 'UNWATCH' ? 'continuous query' : 'subscription'} was created: a string, a variable or a name.`, ex: example });

const start: SchemaNode = {
	k: 'FROM NOW | FROM OFFSET n', t: 'clause',
	d: 'Where to start: new changes only (the default), or replay from an offset you saved.',
	ex: `SUBSCRIBE TOPIC "orders.events" WHERE status = "paid" FROM OFFSET 1042`,
};

export const aqlSchema: SchemaNode[] = [
	{
		k: 'SCAN', t: 'read verb',
		d: 'Look at what the agent is holding right now. SCAN reads working memory only, usually through a WINDOW.',
		ex: `SCAN FROM WORKING WINDOW LAST 10`,
		c: [
			{ k: 'FROM WORKING', t: 'clause', r: true, d: 'SCAN only reads WORKING, the current task state. For other memory, use RECALL or LOOKUP.', ex: `SCAN FROM WORKING` },
			{
				k: 'WINDOW', t: 'predicate', d: 'Which part of working memory: the latest items, a time span, the most important, or everything since an event.',
				ex: `SCAN FROM WORKING WINDOW TOP 3 BY attention_weight`,
				c: [
					{ k: 'LAST n', t: 'option', d: 'The n most recent items.', ex: `SCAN FROM WORKING WINDOW LAST 10` },
					{ k: 'LAST duration', t: 'option', d: 'Everything from the last 30s, 5m, 1h…', ex: `SCAN FROM WORKING WINDOW LAST 30s` },
					{ k: 'TOP n BY field', t: 'option', d: 'The n items with the highest value of a field, such as attention_weight.', ex: `SCAN FROM WORKING WINDOW TOP 3 BY attention_weight` },
					{ k: 'SINCE field = value', t: 'option', d: 'Everything since a given item arrived.', ex: `SCAN FROM WORKING WINDOW SINCE event_id = "inc-001"` },
				],
			},
			{ ...shape, ex: `SCAN FROM WORKING WINDOW LAST 60s RETURN event_id, source, alert, pod` },
		],
	},
	{
		k: 'RECALL', t: 'read verb',
		d: 'Search memory by condition or by similarity: what happened before, what do I know. The most used read.',
		ex: `RECALL FROM EPISODIC WHERE outcome = "success"
  ORDER BY confidence DESC LIMIT 5
  RETURN *`,
		c: [
			{
				k: 'FROM memory', t: 'clause', r: true, d: 'Where to search: EPISODIC, SEMANTIC, or ALL for everything at once.',
				ex: `RECALL FROM ALL WHERE url = "example.com/news"`,
				c: memories([['EPISODIC', 'Events: what happened, when, with what outcome.'], ['SEMANTIC', 'Facts and concepts: what the agent knows.'], ['ALL', 'Every memory type in one search: "tell me everything about this URL".']]),
			},
			where('RECALL FROM EPISODIC'),
			{ k: 'LIKE $embedding', t: 'predicate', d: 'Similarity search: the records closest to an embedding, passed in as a variable.', ex: `RECALL FROM SEMANTIC LIKE $page_embedding MIN_CONFIDENCE 0.8 LIMIT 5` },
			{ k: 'WINDOW', t: 'predicate', d: 'On EPISODIC: the latest events by count or time, as SCAN does for working memory.', ex: `RECALL FROM EPISODIC WINDOW LAST 1h` },
			{ k: 'IN SCOPE "scope"', t: 'modifier', d: 'Search only one named boundary, such as one agent or team.', ex: `RECALL FROM SEMANTIC IN SCOPE "agent:42" WHERE predicate = "owns"` },
			shape, summarise, links, time,
		],
	},
	{
		k: 'LOOKUP', t: 'read verb',
		d: 'Exact access: a record by its key, or the procedure whose pattern best matches. Used for runbooks, tools and known concepts.',
		ex: `LOOKUP FROM SEMANTIC KEY concept = "auth"`,
		c: [
			{
				k: 'FROM memory', t: 'clause', r: true, d: 'PROCEDURAL, TOOLS or SEMANTIC. Not ALL: an exact key means different things in each.',
				ex: `LOOKUP FROM PROCEDURAL KEY pattern_id = "oom-fix"`,
				c: memories([['PROCEDURAL', 'How to do things: runbooks and strategies, with a confidence.'], ['TOOLS', 'The tools the agent can use, with their rankings.'], ['SEMANTIC', 'Facts, concepts and entities.']]),
			},
			{ k: 'KEY field = value', t: 'predicate', d: 'One record by its key.', ex: `LOOKUP FROM SEMANTIC KEY concept = "payments-api" RETURN owner, memory_limit, dependencies` },
			{ k: 'PATTERN … THRESHOLD t', t: 'predicate', d: 'The procedures that match a situation, given as text or a variable. THRESHOLD sets how close a match must be.', ex: `LOOKUP FROM PROCEDURAL PATTERN "pod OOMKilled restart loop" THRESHOLD 0.6` },
			where('LOOKUP FROM TOOLS'),
			shape, links, time,
		],
	},
	{
		k: 'LOAD', t: 'read verb',
		d: 'Pick the best tools for the task, ranked, so only those go into the context.',
		ex: `LOAD FROM TOOLS WHERE task = "bidding" ORDER BY ranking DESC LIMIT 3`,
		c: [
			{ k: 'FROM TOOLS', t: 'clause', r: true, d: 'LOAD only reads the tool registry.', ex: `LOAD FROM TOOLS WHERE task = "triage"` },
			{ ...where('LOAD FROM TOOLS'), r: true },
			{ ...shape, ex: `LOAD FROM TOOLS WHERE task = "bidding" ORDER BY ranking DESC LIMIT 3` },
		],
	},
	{
		k: 'STORE', t: 'write verb',
		d: 'Write a new memory. A write always names one memory type: the agent must decide whether this is an event, a fact or a procedure.',
		ex: `STORE INTO EPISODIC (event = "login", user_id = "u123") TTL 7d`,
		c: [
			{
				k: 'INTO memory', t: 'clause', r: true, d: 'Exactly one memory type. STORE INTO ALL is an error.',
				ex: `STORE INTO PROCEDURAL (pattern_id = "oom-fix", steps = "raise limit, restart", confidence = 0.6)`,
				c: memories([['WORKING', 'Scratch state for the current task.'], ['EPISODIC', 'Something that happened.'], ['SEMANTIC', 'A fact or concept.'], ['PROCEDURAL', 'A way of doing something.'], ['TOOLS', 'A tool the agent can use.']]),
			},
			{
				k: 'FACT [MULTI]', t: 'option', d: 'On SEMANTIC: a subject–predicate–object fact. A fact with an object is also a link. Storing the same subject and predicate again replaces the old fact; MULTI keeps several.',
				ex: `STORE INTO SEMANTIC FACT (subject = "person:ada", predicate = "works_at", object = "org:acme", confidence = 0.9)`,
				c: [
					{ k: 'subject, predicate', t: 'field', r: true, d: 'The entity the fact is about, and the relation.', ex: `STORE INTO SEMANTIC FACT (subject = "sku:42", predicate = "price", value = 10)` },
					{ k: 'object | value', t: 'field', r: true, d: 'Exactly one: another entity (makes a link) or a plain value.', ex: `STORE INTO SEMANTIC FACT MULTI (subject = "person:ada", predicate = "knows", object = "person:bob")` },
					{ k: 'valid_from, valid_to', t: 'field', d: 'When the fact holds. Outside that range, reads skip it without any write.', ex: `STORE INTO SEMANTIC FACT (subject = "person:ada", predicate = "works_at", object = "org:acme", valid_from = "2020-01-01")` },
				],
			},
			payload, share,
		],
	},
	{
		k: 'UPDATE', t: 'write verb',
		d: 'Change records that already exist. This is how a procedure gets tuned: its confidence and parameters move with experience.',
		ex: `UPDATE INTO PROCEDURAL WHERE pattern_id = "oom-fix"
  (scale_to = "768Mi", confidence = 0.94, success_count = 8)`,
		c: [
			{ k: 'INTO memory', t: 'clause', r: true, d: 'The memory type to change. INTO, because data flows into memory.', ex: `UPDATE INTO WORKING WHERE task_id = "t-9" (status = "done")` },
			{ ...where('UPDATE INTO PROCEDURAL'), r: true, ex: `UPDATE INTO PROCEDURAL WHERE pattern = "checkout" (confidence = 0.95)` },
			{ ...payload, d: 'The fields to set. Setting a field to null removes it.', ex: `UPDATE INTO SEMANTIC WHERE concept = "auth" (owner = "identity-team", deprecated = null)` },
		],
	},
	{
		k: 'FORGET', t: 'write verb',
		d: 'Remove memories. Forgetting is a feature: without it the important gets buried under the trivial.',
		ex: `FORGET FROM WORKING WHERE stale = true`,
		c: [
			{ k: 'FROM memory', t: 'clause', r: true, d: 'Any memory type, or ALL. Forgetting a SEMANTIC entity also forgets the facts about it, and links to forgotten records go with them.', ex: `FORGET FROM ALL WHERE user_id = "u123"` },
			{ ...where('FORGET FROM WORKING'), r: true, d: 'Required, always: FORGET without WHERE is a syntax error, so an agent cannot wipe its memory by accident.' },
		],
	},
	{
		k: 'LINK', t: 'graph verb',
		d: 'Connect records, across memory types, with a type and a weight. Links are where learning lives: a runbook linked to the incidents it fixed has evidence.',
		ex: `LINK FROM PROCEDURAL WHERE pattern_id = "oom-fix"
  TO EPISODIC WHERE incident_id = "inc-001"
  TYPE "applied_to" WEIGHT 0.97`,
		c: [
			...ends('LINK'),
			{ k: 'TYPE "name"', t: 'clause', r: true, d: 'Any string. The engine stores and indexes it but never interprets it: the agent invents its own vocabulary.', ex: `LINK FROM EPISODIC WHERE incident_id = "inc-001" TO EPISODIC WHERE incident_id = "inc-005" TYPE "recurring_pattern_friday"` },
			{ k: 'WEIGHT w', t: 'clause', d: 'How strong the link is, 0 to 1. LINK again with the same type sets a new weight instead of adding a second link.', ex: `LINK FROM SEMANTIC WHERE concept = "auth" TO PROCEDURAL WHERE pattern_id = "rotate-keys" TYPE "triggers" WEIGHT 0.8` },
		],
	},
	{
		k: 'UPDATE LINK', t: 'graph verb',
		d: 'Change existing links in place: a new weight, a new type, or both.',
		ex: `UPDATE LINK FROM PROCEDURAL WHERE pattern_id = "oom-fix"
  TO EPISODIC WHERE incident_id = "inc-001"
  TYPE "applied_to" WEIGHT 0.99`,
		c: [
			...ends('UPDATE LINK').map((n) => ({ ...n, ex: n.ex!.replace(/^UPDATE LINK (.*) TYPE "(\w+)"$/, 'UPDATE LINK $1 TYPE "$2" WEIGHT 0.9') })),
			{ k: 'TYPE "name"', t: 'clause', d: 'Which links to change. Without it, every link between the matched records.', ex: `UPDATE LINK FROM SEMANTIC WHERE key = "a" TO SEMANTIC WHERE key = "b" WEIGHT 0.5` },
			{ k: 'WEIGHT w | ( type = …, weight = … )', t: 'clause', r: true, d: 'The new weight, or an assignment list that can also rename the type.', ex: `UPDATE LINK FROM SEMANTIC WHERE key = "acme-owner" TO SEMANTIC WHERE key = "acme-renewal" TYPE "related" (type = "owns", weight = 0.9)` },
		],
	},
	{
		k: 'UNLINK', t: 'graph verb',
		d: 'Remove links and keep the records at both ends.',
		ex: `UNLINK FROM SEMANTIC WHERE key = "acme-owner" TO SEMANTIC WHERE key = "acme-renewal" TYPE "related"`,
		c: [
			...ends('UNLINK'),
			{ k: 'TYPE "name"', t: 'clause', d: 'Remove only links of this type. Without it, every link between the matched records.', ex: `UNLINK FROM SEMANTIC WHERE key = "acme-owner" TO SEMANTIC WHERE key = "acme-renewal"` },
		],
	},
	{
		k: 'MERGE ENTITY', t: 'graph verb',
		d: 'Two keys turned out to name one thing. Facts about the duplicate move to the survivor, and the duplicate key becomes an alias.',
		ex: `MERGE ENTITY "person:ada-lovelace" INTO "person:ada"`,
		c: [
			{ k: '"duplicate"', t: 'clause', r: true, d: 'The SEMANTIC entity key that goes away.', ex: `MERGE ENTITY "org:acme-inc" INTO "org:acme"` },
			{ k: 'INTO "survivor"', t: 'clause', r: true, d: 'The entity key that stays and receives the facts.', ex: `MERGE ENTITY "org:acme-inc" INTO "org:acme"` },
		],
	},
	{
		k: 'REFLECT', t: 'compose verb',
		d: 'Assemble one context package from several memory types for the LLM\'s next step. With THEN it also writes back what was learned: consolidation, as sleep does for brains.',
		ex: `REFLECT FROM WORKING, FROM SEMANTIC, FROM EPISODIC`,
		c: [
			{ k: 'FROM memory [predicate], …', t: 'clause', r: true, d: 'One or more sources, separated by commas, each with its own optional predicate. FROM ALL takes everything.', ex: `REFLECT FROM EPISODIC WHERE incident_id = {current}, FROM PROCEDURAL WHERE pattern_id = {matched}` },
			{ k: 'WITH LINKS · FOLLOW LINKS', t: 'modifier', d: 'Include the links behind the records, or the records they lead to.', ex: `REFLECT FROM EPISODIC WHERE incident_id = {current} WITH LINKS TYPE "applied_to"` },
			{ k: 'THEN STORE | UPDATE', t: 'clause', d: 'Write what the agent learned from reading, in the same statement.', ex: `REFLECT FROM EPISODIC WHERE incident_id = {current},
        FROM PROCEDURAL WHERE pattern_id = {matched}
  THEN STORE INTO PROCEDURAL (
    pattern    = "learned_pattern",
    steps      = "step1, step2",
    confidence = 0.6
  )` },
		],
	},
	{
		k: 'PIPELINE', t: 'compose verb',
		d: 'Chain reads into one round trip with a hard time budget. Each stage can use what earlier stages found. A stage that runs out of time returns what it has.',
		ex: `PIPELINE bid_decision TIMEOUT 80ms
  LOAD FROM TOOLS WHERE task = "bidding" LIMIT 3
  | LOOKUP FROM SEMANTIC KEY url = {url}
  | RECALL FROM EPISODIC WHERE url = {url} LIMIT 10
  | REFLECT FROM SEMANTIC, FROM EPISODIC, FROM PROCEDURAL`,
		c: [
			{ k: 'name', t: 'clause', d: 'Optional name, for logs and metrics.', ex: `PIPELINE TIMEOUT 20ms SCAN FROM WORKING | RECALL FROM EPISODIC WHERE pod = "test"` },
			{ k: 'TIMEOUT d', t: 'clause', r: true, d: 'The budget for the whole pipeline: 80ms for a bid, 16ms for a game frame.', ex: `PIPELINE npc_turn TIMEOUT 16ms SCAN FROM WORKING WINDOW LAST 5 | LOOKUP FROM PROCEDURAL KEY behaviour = "patrol"` },
			{ k: 'stage | stage | …', t: 'clause', r: true, d: 'SCAN, RECALL, LOOKUP, LOAD or REFLECT, joined by |. Writes are not stages.', ex: `PIPELINE incident_triage TIMEOUT 200ms
  SCAN FROM WORKING WINDOW LAST 60s
  | RECALL FROM EPISODIC WHERE pod = "payments-api" AND root_cause = "OOMKilled"
  | REFLECT FROM WORKING, FROM EPISODIC` },
			{ k: '{variable}', t: 'value', d: 'A value found by an earlier stage, used in a later one.', ex: `PIPELINE TIMEOUT 50ms LOOKUP FROM SEMANTIC KEY url = {url} | RECALL FROM EPISODIC WHERE url = {url}` },
		],
	},
	{
		k: 'SUBSCRIBE', t: 'stream verb',
		d: 'Follow every committed change in a scope or a topic, in order, instead of polling.',
		ex: `SUBSCRIBE SCOPE "team:billing" KINDS fact, edge FROM NOW`,
		c: [
			{ k: 'SCOPE "s" | TOPIC "t"', t: 'clause', r: true, d: 'What to follow: a memory scope, or a topic that agents PUBLISH to.', ex: `SUBSCRIBE TOPIC "orders.events"` },
			{ k: 'KINDS …', t: 'clause', d: 'Only these kinds of change: entity, fact, edge, episode, tool, event.', ex: `SUBSCRIBE SCOPE "agent:42" KINDS fact, edge` },
			{ k: 'WHERE …', t: 'predicate', d: 'Only changes that match.', ex: `SUBSCRIBE TOPIC "orders.events" WHERE status = "paid"` },
			start,
		],
	},
	{ k: 'UNSUBSCRIBE', t: 'stream verb', d: 'Stop a subscription.', ex: `UNSUBSCRIBE "sub-7f3a"`, c: [handle('UNSUBSCRIBE', `UNSUBSCRIBE $sub`)] },
	{
		k: 'ACK', t: 'stream verb', d: 'Tell the engine you have handled changes up to an offset, so they are not delivered again.',
		ex: `ACK "sub-7f3a" OFFSET 1050`,
		c: [handle('ACK', `ACK $sub OFFSET 1050`), { k: 'OFFSET n', t: 'clause', r: true, d: 'The last offset handled.', ex: `ACK "sub-7f3a" OFFSET 1050` }],
	},
	{
		k: 'PUBLISH', t: 'stream verb', d: 'Write a message to a topic, for other agents subscribed to it.',
		ex: `PUBLISH TOPIC "orders.events" (order_id = "o-1", status = "paid") TTL 7d`,
		c: [
			{ k: 'TOPIC "t"', t: 'clause', r: true, d: 'The topic to write to.', ex: `PUBLISH TOPIC "alerts" (pod = "payments-api", level = "high")` },
			{ ...payload, ex: `PUBLISH TOPIC "orders.events" (order_id = "o-1", status = "paid")` },
			{ k: 'TTL d', t: 'modifier', d: 'How long the message is kept.', ex: `PUBLISH TOPIC "orders.events" (order_id = "o-1", status = "paid") TTL 7d` },
		],
	},
	{
		k: 'WATCH', t: 'stream verb',
		d: 'A continuous query: the current result now, then only what changes, as it changes.',
		ex: `WATCH RECALL FROM SEMANTIC IN SCOPE "agent:42" WHERE predicate = "owns" LIMIT 20
      DEBOUNCE 100ms FROM NOW`,
		c: [
			{ k: 'read statement', t: 'clause', r: true, d: 'A RECALL, LOOKUP, SCAN or LOAD. Writes, REFLECT and PIPELINE cannot be watched.', ex: `WATCH SCAN FROM WORKING WINDOW LAST 10` },
			{ k: 'DEBOUNCE d', t: 'clause', d: 'Wait this long for changes to settle before sending an update.', ex: `WATCH LOAD FROM TOOLS WHERE task = "triage" DEBOUNCE 200ms` },
			{ ...start, ex: `WATCH RECALL FROM EPISODIC WHERE pod = "payments-api" FROM NOW` },
		],
	},
	{ k: 'UNWATCH', t: 'stream verb', d: 'Stop a continuous query.', ex: `UNWATCH "cq-91"`, c: [handle('UNWATCH', `UNWATCH $cq`)] },
];
