// The CNCF Serverless Workflow (Open Workflow) DSL 1.0 as a tree, for <SchemaTree of="swf" />.
// Checked against dsl-reference.md in serverlessworkflow/specification. Each example must be valid YAML;
// the component parses them at build time and fails the build if one is not.

import type { SchemaNode } from './schema-tree';

const duration = 'A duration: an object such as { minutes: 5, seconds: 30 } (days, hours, minutes, seconds, milliseconds), or an ISO 8601 string such as PT5M30S.';

export const schema: SchemaNode[] = [
	{
		k: 'document', t: 'object', r: true,
		d: 'The identity card of the workflow: who owns it, what it is called and which version this is. namespace + name + version identify a workflow in a runtime.',
		ex: `document:
  dsl: '1.0.0'
  namespace: shop
  name: process-order
  version: '1.0.0'
  title: Process an order`,
		c: [
			{ k: 'dsl', t: 'string', r: true, d: 'Version of the DSL the file is written in. Quote it: unquoted, 1.0 would be read as a number.', ex: `dsl: '1.0.0'` },
			{ k: 'namespace', t: 'string', r: true, d: 'The group the workflow belongs to, like a folder or a team.', ex: `namespace: shop` },
			{ k: 'name', t: 'string', r: true, d: 'Name of the workflow, unique within its namespace. Lowercase with hyphens by convention.', ex: `name: process-order` },
			{ k: 'version', t: 'string', r: true, d: 'Semantic version (major.minor.patch) of this definition. Bump it when the workflow changes.', ex: `version: '1.2.0'` },
			{ k: 'title', t: 'string', d: 'A short, human-readable title for catalogs and dashboards.', ex: `title: Process an order` },
			{ k: 'summary', t: 'string', d: 'A longer description, in Markdown. A block scalar (|) keeps the line breaks.', ex: `summary: |
  Validates an order, charges the card
  and waits for the shipment.` },
			{ k: 'tags', t: 'map · string: string', d: 'Free key/value labels for search and grouping. You choose the keys; keys and values are strings.', ex: `tags:
  team: payments
  tier: gold` },
			{ k: 'metadata', t: 'map', d: 'Anything else you want to attach. The runtime keeps it but does not act on it.', ex: `metadata:
  owner: payments-team
  runbook: https://wiki.example.com/orders` },
		],
	},
	{
		k: 'input', t: 'object',
		d: 'Checks and shapes the data the workflow is started with. Bad input fails the run before any task starts.',
		ex: `input:
  schema:
    format: json
    document:
      type: object
      required: [order]
  from: \${ .order }`,
		c: [
			{
				k: 'schema', t: 'object', d: 'A JSON Schema that the raw input must match. Give it inline (document) or load it from a URL (resource).',
				ex: `schema:
  format: json
  resource:
    endpoint: https://schemas.example.com/order.json`,
				c: [
					{ k: 'format', t: 'string', r: true, d: 'The schema language. The spec defines json, for JSON Schema.', ex: `format: json` },
					{ k: 'document', t: 'object', d: 'The schema itself, written inline. Use this or resource.', ex: `document:
  type: object
  required: [order]
  properties:
    order:
      type: object` },
					{ k: 'resource', t: 'object', d: 'Where to fetch the schema from: an object with an endpoint (a URI, or { uri, authentication }).', ex: `resource:
  endpoint: https://schemas.example.com/order.json` },
				],
			},
			{ k: 'from', t: 'expression', d: 'A jq expression that picks, or reshapes, what the first task sees as ".". Without it, tasks see the whole input.', ex: `from: \${ .payload.order }` },
		],
	},
	{
		k: 'use', t: 'object',
		d: 'Reusable building blocks. Declare a policy, function, error or secret here once, then refer to it by name from any task.',
		ex: `use:
  secrets: [paymentKey]
  retries:
    threeTimes:
      delay: { seconds: 2 }
      limit:
        attempt: { count: 3 }`,
		c: [
			{
				k: 'authentications', t: 'map · name: auth', d: 'Named authentication policies. A call refers to one with authentication: { use: <name> }. Each policy uses exactly one scheme.',
				ex: `authentications:
  shopApi:
    bearer:
      token: \${ $secrets.shopToken }`,
				c: [
					{ k: 'basic', t: 'object', d: 'HTTP basic auth: username and password.', ex: `basic:
  username: admin
  password: \${ $secrets.adminPassword }` },
					{ k: 'bearer', t: 'object', d: 'A bearer token sent in the Authorization header.', ex: `bearer:
  token: \${ $secrets.apiToken }` },
					{ k: 'digest', t: 'object', d: 'HTTP digest auth: username and password, sent as a digest.', ex: `digest:
  username: admin
  password: \${ $secrets.adminPassword }` },
					{ k: 'oauth2', t: 'object', d: 'OAuth 2.0: the authority to get a token from, the grant type, the client and the scopes.', ex: `oauth2:
  authority: https://auth.example.com
  grant: client_credentials
  client:
    id: workflow-runtime
    secret: \${ $secrets.clientSecret }
  scopes: [orders.write]` },
					{ k: 'oidc', t: 'object', d: 'OpenID Connect: the same fields as oauth2, with endpoints discovered from the authority.', ex: `oidc:
  authority: https://auth.example.com
  grant: client_credentials
  client:
    id: workflow-runtime
    secret: \${ $secrets.clientSecret }` },
				],
			},
			{
				k: 'catalogs', t: 'map · name: catalog', d: 'Named function catalogs: shared libraries of functions. A task calls one as call: <function>:<version>@<catalog>.',
				ex: `catalogs:
  shared:
    endpoint: https://github.com/acme/functions
# used as:  call: sendEmail:1.0.0@shared`,
				c: [{ k: 'endpoint', t: 'string | object', r: true, d: 'Where the catalog lives: a URI, or { uri, authentication }.', ex: `endpoint: https://github.com/acme/functions` }],
			},
			{
				k: 'errors', t: 'map · name: error', d: 'Named error definitions. raise: { error: <name> } raises one without repeating its fields.',
				ex: `errors:
  outOfStock:
    type: https://example.com/errors/out-of-stock
    status: 409
    title: Item is out of stock`,
			},
			{
				k: 'extensions', t: 'array · name: extension', d: 'Hooks that run tasks before or after every task of a given type: logging, policy checks, mocks. Order matters, so it is a list.',
				ex: `extensions:
  - logCalls:
      extend: call
      before:
        - log:
            call: http
            with:
              method: post
              endpoint: https://log.example.com
              body: \${ $task }`,
				c: [
					{ k: 'extend', t: 'string', r: true, d: 'The task type to extend: call, composite, emit, for, listen, raise, run, set, switch, try, wait, or all.', ex: `extend: call` },
					{ k: 'when', t: 'expression', d: 'Apply the extension only when this is true.', ex: `when: '\${ $task.reference | startswith("/do/0") }'` },
					{ k: 'before', t: 'array · name: task', d: 'Tasks to run before the extended task.', ex: `before:
  - audit:
      set: { started: true }` },
					{ k: 'after', t: 'array · name: task', d: 'Tasks to run after the extended task.', ex: `after:
  - audit:
      set: { finished: true }` },
				],
			},
			{
				k: 'functions', t: 'map · name: task', d: 'Named reusable tasks, usually calls. Any task runs one with call: <name> and passes arguments in with.',
				ex: `functions:
  getWeather:
    call: http
    with:
      method: get
      endpoint: https://api.example.com/weather
# used as:  call: getWeather`,
			},
			{
				k: 'retries', t: 'map · name: retryPolicy', d: 'Named retry policies. A try refers to one with retry: <name>.',
				ex: `retries:
  threeTimes:
    delay: { seconds: 2 }
    backoff:
      exponential: {}
    limit:
      attempt: { count: 3 }`,
				c: [
					{ k: 'delay', t: 'duration', d: `How long to wait between attempts. ${duration}`, ex: `delay: { seconds: 2 }` },
					{ k: 'backoff', t: 'object', d: 'How the delay grows: constant, exponential or linear. Set exactly one; the value is an object, often empty.', ex: `backoff:
  exponential: {}` },
					{ k: 'limit', t: 'object', d: 'When to give up: attempt.count (maximum attempts), attempt.duration (per attempt) or duration (in total).', ex: `limit:
  attempt:
    count: 5
  duration: { minutes: 2 }` },
					{ k: 'jitter', t: 'object', d: 'A random extra delay between from and to, so many retries do not all hit a service at once.', ex: `jitter:
  from: { milliseconds: 100 }
  to: { seconds: 1 }` },
					{ k: 'when', t: 'expression', d: 'Retry only when this is true.', ex: `when: \${ $error.status >= 500 }` },
					{ k: 'exceptWhen', t: 'expression', d: 'Do not retry when this is true.', ex: `exceptWhen: \${ $error.status == 400 }` },
				],
			},
			{ k: 'secrets', t: 'array · string', d: 'The names of secrets the workflow may read. Values come from the runtime, never from the file; read them as $secrets.<name>.', ex: `secrets:
  - paymentKey
  - shopToken` },
			{
				k: 'timeouts', t: 'map · name: timeout', d: 'Named timeouts. A workflow or task refers to one with timeout: <name>.',
				ex: `timeouts:
  short:
    after: { seconds: 30 }`,
			},
		],
	},
	{
		k: 'do', t: 'array · name: task', r: true,
		d: 'The tasks, run top to bottom. Each list item is a map with one key: the key is the task name, the value is the task. The task type is the key inside it (call, set, switch and so on), next to the fields every task can have.',
		ex: `do:
  - greet:
      set:
        message: Hello
  - send:
      call: http
      with:
        method: post
        endpoint: https://hooks.example.com/hello
        body: \${ . }`,
		c: [
			{
				k: '(any task)', t: 'object', d: 'Fields every task can have, whatever its type. They sit next to the type key, at the same indentation.',
				ex: `- charge:
    if: \${ .total > 0 }
    call: http
    with:
      method: post
      endpoint: https://pay.example.com/charges
    timeout:
      after: { seconds: 30 }
    then: ship`,
				c: [
					{ k: 'if', t: 'expression', d: 'Run the task only when this is true. Otherwise it is skipped and its input passes through unchanged.', ex: `if: \${ .amount > 1000 }` },
					{ k: 'input', t: 'object', d: 'Same as the workflow input, for one task: schema to check, and from to choose what the task sees as ".".', ex: `input:
  from: \${ .order }` },
					{ k: 'output', t: 'object', d: 'Reshapes what the task passes to the next task. as is a jq expression; schema checks the result.', ex: `output:
  as: \${ .body.id }` },
					{ k: 'export', t: 'object', d: 'Saves data in $context, which later tasks can read wherever they are. as builds the new context.', ex: `export:
  as: '\${ $context + { orderId: .id } }'` },
					{ k: 'timeout', t: 'string | object', d: 'Fail the task if it runs longer than after. A string names a timeout from use.timeouts.', ex: `timeout:
  after: { seconds: 30 }` },
					{ k: 'then', t: 'flow directive', d: 'Where to go next: continue (the next task, the default), exit (leave the current task list), end (finish the workflow), or the name of a task.', ex: `then: notifyCustomer` },
					{ k: 'metadata', t: 'map', d: 'Free information about the task. The runtime keeps it but does not act on it.', ex: `metadata:
  owner: payments-team` },
				],
			},
			{
				k: 'call', t: 'string', d: 'Calls something outside the workflow and returns its result. call says what to call; with holds the arguments, and their shape depends on the kind of call.',
				ex: `- getUser:
    call: http
    with:
      method: get
      endpoint: https://api.example.com/users/42`,
				c: [
					{ k: 'call', t: 'string', r: true, d: 'What to call: http, grpc, openapi, asyncapi, a2a, mcp, or the name of a function from use.functions or a catalog. A3T adds adb (agent memory) and a3t:agent.', ex: `call: openapi` },
					{ k: 'with', t: 'map', d: 'The arguments. The children below show what each built-in call expects.', ex: `with:
  method: get
  endpoint: https://api.example.com/users/42` },
					{
						k: 'call: http', t: 'object', d: 'An HTTP request. Returns the response body by default.',
						ex: `call: http
with:
  method: post
  endpoint: https://api.example.com/orders
  headers:
    content-type: application/json
  body:
    id: \${ .id }`,
						c: [
							{ k: 'method', t: 'string', r: true, d: 'The HTTP method: get, post, put, patch, delete…', ex: `method: post` },
							{ k: 'endpoint', t: 'string | object', r: true, d: 'The URL, or { uri, authentication } when the call needs credentials.', ex: `endpoint:
  uri: https://api.example.com/orders
  authentication:
    use: shopApi` },
							{ k: 'headers', t: 'map', d: 'Request headers, name: value.', ex: `headers:
  accept: application/json` },
							{ k: 'body', t: 'any', d: 'The request body: an object, a string or an expression.', ex: `body: \${ .order }` },
							{ k: 'query', t: 'map', d: 'Query parameters added to the URL.', ex: `query:
  page: 2
  size: 50` },
							{ k: 'output', t: 'string', d: 'What the task returns: content (the body, the default), response (status, headers and body) or raw (base64).', ex: `output: response` },
							{ k: 'redirect', t: 'boolean', d: 'Accept 3xx redirects instead of treating them as errors. Defaults to false.', ex: `redirect: true` },
						],
					},
					{
						k: 'call: grpc', t: 'object', d: 'Calls a gRPC method, described by a .proto file.',
						ex: `call: grpc
with:
  proto:
    endpoint: https://example.com/greeter.proto
  service:
    name: GreeterApi.Greeter
    host: localhost
    port: 5011
  method: SayHello
  arguments:
    name: \${ .user.name }`,
						c: [
							{ k: 'proto', t: 'object', r: true, d: 'Where to fetch the .proto file: an object with an endpoint.', ex: `proto:
  endpoint: https://example.com/greeter.proto` },
							{ k: 'service', t: 'object', r: true, d: 'The service to call: name and host are required; port and authentication are optional.', ex: `service:
  name: GreeterApi.Greeter
  host: localhost
  port: 5011` },
							{ k: 'method', t: 'string', r: true, d: 'The gRPC method name.', ex: `method: SayHello` },
							{ k: 'arguments', t: 'map', d: 'The method arguments, name: value.', ex: `arguments:
  name: Ada` },
						],
					},
					{
						k: 'call: openapi', t: 'object', d: 'Calls an operation from an OpenAPI document, by its operationId. The runtime builds the HTTP request.',
						ex: `call: openapi
with:
  document:
    endpoint: https://petstore.swagger.io/v2/swagger.json
  operationId: findPetsByStatus
  parameters:
    status: available`,
						c: [
							{ k: 'document', t: 'object', r: true, d: 'Where to fetch the OpenAPI document: an object with an endpoint.', ex: `document:
  endpoint: https://api.example.com/openapi.json` },
							{ k: 'operationId', t: 'string', r: true, d: 'The operationId in that document.', ex: `operationId: findPetsByStatus` },
							{ k: 'parameters', t: 'map', d: 'Parameters of the operation, path, query or body, by name.', ex: `parameters:
  status: available` },
							{ k: 'authentication', t: 'object', d: 'Credentials for the call, inline or by name.', ex: `authentication:
  use: shopApi` },
							{ k: 'output', t: 'string', d: 'content (default), response or raw, as with http.', ex: `output: content` },
						],
					},
					{
						k: 'call: asyncapi', t: 'object', d: 'Publishes or subscribes to messages through an AsyncAPI operation: Kafka, MQTT, AMQP, NATS and others.',
						ex: `call: asyncapi
with:
  document:
    endpoint: https://example.com/orders.asyncapi.yaml
  operation: publishOrder
  protocol: kafka
  message:
    payload: \${ .order }`,
						c: [
							{ k: 'document', t: 'object', r: true, d: 'Where to fetch the AsyncAPI document.', ex: `document:
  endpoint: https://example.com/orders.asyncapi.yaml` },
							{ k: 'operation', t: 'string', d: 'The operation to run (AsyncAPI 3). For AsyncAPI 2.6, use channel.', ex: `operation: publishOrder` },
							{ k: 'protocol', t: 'string', d: 'Picks the server by protocol: kafka, mqtt, amqp, nats, ws…', ex: `protocol: kafka` },
							{ k: 'message', t: 'object', d: 'The message to publish: payload, headers. Use this or subscription.', ex: `message:
  payload: \${ .order }` },
							{ k: 'subscription', t: 'object', d: 'Consume messages instead: a filter, how many to consume, and what to do with each.', ex: `subscription:
  consume:
    amount: 5` },
						],
					},
					{
						k: 'call: a2a', t: 'object', d: 'Talks to another agent over the A2A (agent-to-agent) protocol: send a message, check or cancel a task.',
						ex: `call: a2a
with:
  method: message/send
  agentCard:
    endpoint: https://agents.example.com/.well-known/agent.json
  parameters:
    message:
      parts:
        - kind: text
          text: Summarise order 42`,
						c: [
							{ k: 'method', t: 'string', r: true, d: 'The A2A JSON-RPC method: message/send, message/stream, tasks/get, tasks/cancel…', ex: `method: message/send` },
							{ k: 'agentCard', t: 'object', d: 'Where to fetch the agent card that describes the agent. Use this or server.', ex: `agentCard:
  endpoint: https://agents.example.com/.well-known/agent.json` },
							{ k: 'server', t: 'string | object', d: 'The A2A server URL, when there is no agent card.', ex: `server: https://agents.example.com/a2a` },
							{ k: 'parameters', t: 'map', d: 'Parameters of the method. For message/send, the runtime fills in messageId and role.', ex: `parameters:
  message:
    parts:
      - kind: text
        text: Hello` },
						],
					},
					{
						k: 'call: mcp', t: 'object', d: 'Calls a Model Context Protocol server: list or call tools, read resources, get prompts.',
						ex: `call: mcp
with:
  protocolVersion: '2025-06-18'
  method: tools/call
  parameters:
    name: search
    arguments:
      query: \${ .question }
  transport:
    http:
      endpoint: https://mcp.example.com`,
						c: [
							{ k: 'method', t: 'string', r: true, d: 'The MCP method: tools/list, tools/call, resources/read, prompts/get…', ex: `method: tools/call` },
							{ k: 'transport', t: 'object', r: true, d: 'How to reach the server: http (an endpoint) or stdio (a command to start).', ex: `transport:
  stdio:
    command: npx
    arguments: [my-mcp-server]` },
							{ k: 'parameters', t: 'map', d: 'Method parameters, such as the tool name and its arguments.', ex: `parameters:
  name: search
  arguments:
    query: workflows` },
							{ k: 'protocolVersion', t: 'string', r: true, d: 'The MCP protocol version. Defaults to 2025-06-18.', ex: `protocolVersion: '2025-06-18'` },
						],
					},
				],
			},
			{
				k: 'set', t: 'map | expression', d: 'Builds data. The task output is exactly what you write, with ${ } expressions evaluated against the task input.',
				ex: `- price:
    set:
      orderId: \${ .order.id }
      total: \${ [.order.items[] | .price * .qty] | add }`,
			},
			{
				k: 'switch', t: 'array · name: case', d: 'Branches. Cases are checked in list order; the first whose when is true decides where to go. A case without when is the default.',
				ex: `- route:
    switch:
      - urgent:
          when: \${ .priority == "P1" }
          then: pageOnCall
      - default:
          then: fileTicket`,
				c: [
					{ k: 'when', t: 'expression', d: 'The condition for this case. Leave it out on one case, the default.', ex: `when: \${ .total > 100 }` },
					{ k: 'then', t: 'flow directive', r: true, d: 'Where to go when the case matches: a task name, continue, exit or end.', ex: `then: applyDiscount` },
				],
			},
			{
				k: 'for', t: 'object', d: 'Loops over a list and runs its do once per item. The item and its index are variables.',
				ex: `- notifyAll:
    for:
      each: user
      in: \${ .users }
    do:
      - send:
          call: http
          with:
            method: post
            endpoint: https://hooks.example.com/notify
            body: \${ $user }`,
				c: [
					{ k: 'for.in', t: 'expression | array', r: true, d: 'The list to loop over.', ex: `for:
  in: \${ .order.items }` },
					{ k: 'for.each', t: 'string', d: 'Variable name for the current item. Defaults to item, read as $item.', ex: `for:
  each: line
  in: \${ .lines }` },
					{ k: 'for.at', t: 'string', d: 'Variable name for the index. Defaults to index.', ex: `for:
  in: \${ .lines }
  at: i` },
					{ k: 'while', t: 'expression', d: 'Keep looping only while this is true.', ex: `while: \${ $index < 10 }` },
					{ k: 'do', t: 'array · name: task', r: true, d: 'The tasks to run for each item.', ex: `do:
  - reserve:
      set:
        sku: \${ $item.sku }` },
				],
			},
			{
				k: 'fork', t: 'object', d: 'Runs branches in parallel. It waits for all of them and returns a list of their outputs, unless compete makes it a race.',
				ex: `- lookup:
    fork:
      compete: true
      branches:
        - fromCache:
            call: http
            with:
              method: get
              endpoint: https://cache.example.com/item/1
        - fromDb:
            call: http
            with:
              method: get
              endpoint: https://db.example.com/item/1`,
				c: [
					{ k: 'branches', t: 'array · name: task', d: 'The tasks to run at the same time, one per list item.', ex: `branches:
  - reserveStock:
      set: { reserved: true }
  - sendEmail:
      set: { sent: true }` },
					{ k: 'compete', t: 'boolean', d: 'true: the first branch to finish wins and its output is the result. Defaults to false.', ex: `compete: true` },
				],
			},
			{
				k: 'try', t: 'array · name: task', d: 'Runs tasks and handles their errors. try holds the tasks; catch says which errors to handle and how: retry, run recovery tasks, or both.',
				ex: `- charge:
    try:
      - pay:
          call: http
          with:
            method: post
            endpoint: https://pay.example.com/charges
    catch:
      errors:
        with:
          status: 503
      retry: threeTimes`,
				c: [
					{
						k: 'catch', t: 'object', r: true, d: 'Which errors to handle, and how.',
						ex: `catch:
  errors:
    with:
      type: https://serverlessworkflow.io/spec/1.0.0/errors/communication
  as: failure
  do:
    - logIt:
        set:
          failed: \${ $failure.title }`,
						c: [
							{ k: 'errors.with', t: 'object', d: 'Catch only errors with these properties: type, status, instance, title, detail.', ex: `errors:
  with:
    status: 503` },
							{ k: 'as', t: 'string', d: 'Variable name for the caught error. Defaults to error, read as $error.', ex: `as: failure` },
							{ k: 'when', t: 'expression', d: 'Catch only when this is true.', ex: `when: \${ $error.status >= 500 }` },
							{ k: 'exceptWhen', t: 'expression', d: 'Do not catch when this is true.', ex: `exceptWhen: \${ $error.status == 404 }` },
							{ k: 'retry', t: 'string | object', d: 'Retry the try tasks: a policy inline, or the name of one in use.retries.', ex: `retry: threeTimes` },
							{ k: 'do', t: 'array · name: task', d: 'Recovery tasks to run once the error is caught (and retries, if any, are used up).', ex: `do:
  - refund:
      call: http
      with:
        method: post
        endpoint: https://pay.example.com/refunds` },
							{ k: 'then', t: 'flow directive', d: 'Where to go after handling the error.', ex: `then: end` },
						],
					},
				],
			},
			{
				k: 'emit', t: 'object', d: 'Publishes an event in CloudEvents format, so other systems and workflows can react.',
				ex: `- announce:
    emit:
      event:
        with:
          source: https://shop.example.com
          type: com.example.order.paid.v1
          data:
            orderId: \${ .id }`,
				c: [
					{
						k: 'event.with', t: 'object', r: true, d: 'The CloudEvent attributes. source and type are required when emitting; the runtime fills id and time.',
						ex: `event:
  with:
    source: https://shop.example.com
    type: com.example.order.paid.v1
    data: \${ . }`,
						c: [
							{ k: 'source', t: 'string', r: true, d: 'A URI for where the event happened.', ex: `source: https://shop.example.com` },
							{ k: 'type', t: 'string', r: true, d: 'What happened, usually reverse-DNS with a version.', ex: `type: com.example.order.paid.v1` },
							{ k: 'data', t: 'any', d: 'The event payload.', ex: `data:
  orderId: \${ .id }` },
							{ k: 'subject', t: 'string', d: 'What the event is about, within the source.', ex: `subject: order-42` },
						],
					},
				],
			},
			{
				k: 'listen', t: 'object', d: 'Pauses until matching events arrive, then continues with them as output. Use it for approvals, callbacks and anything that happens later.',
				ex: `- awaitApproval:
    listen:
      to:
        one:
          with:
            type: com.example.approval.v1`,
				c: [
					{
						k: 'to', t: 'object', r: true, d: 'Which events to wait for. Set one of one, all or any.',
						ex: `to:
  all:
    - with:
        type: com.example.payment.done.v1
    - with:
        type: com.example.stock.reserved.v1`,
						c: [
							{ k: 'one', t: 'object', d: 'Wait for one event that matches the filter. with lists the attributes to match.', ex: `one:
  with:
    type: com.example.approval.v1` },
							{ k: 'all', t: 'array · filter', d: 'Wait until every filter in the list has matched an event.', ex: `all:
  - with: { type: com.example.a.v1 }
  - with: { type: com.example.b.v1 }` },
							{ k: 'any', t: 'array · filter', d: 'Continue as soon as any filter matches. An empty list accepts any event.', ex: `any:
  - with: { type: com.example.a.v1 }
  - with: { type: com.example.b.v1 }` },
							{ k: 'until', t: 'expression | object', d: 'With any only: keep listening until this condition holds, or until these events arrive.', ex: `until: \${ length >= 3 }` },
						],
					},
					{ k: 'read', t: 'string', d: 'What each event contributes: data (the payload, default), envelope (with its attributes) or raw.', ex: `read: envelope` },
				],
			},
			{
				k: 'wait', t: 'duration', d: `Pauses for a fixed time, for back-off, rate limits or giving another system time. ${duration}`,
				ex: `- coolDown:
    wait:
      seconds: 30`,
			},
			{
				k: 'raise', t: 'object', d: 'Fails on purpose with a structured error. A surrounding try can catch it; otherwise the workflow ends with that error.',
				ex: `- reject:
    raise:
      error:
        type: https://serverlessworkflow.io/spec/1.0.0/errors/validation
        status: 400
        title: Order has no items`,
				c: [
					{
						k: 'error', t: 'string | object', r: true, d: 'The error to raise, inline or by the name of one in use.errors.',
						ex: `error: outOfStock`,
						c: [
							{ k: 'type', t: 'string', r: true, d: 'A URI naming the kind of error. The spec defines standard ones: validation, communication, timeout, authentication, authorization, configuration, expression, runtime.', ex: `type: https://serverlessworkflow.io/spec/1.0.0/errors/timeout` },
							{ k: 'status', t: 'integer', r: true, d: 'A status code, HTTP style.', ex: `status: 409` },
							{ k: 'title', t: 'string', d: 'A short summary for people.', ex: `title: Item is out of stock` },
							{ k: 'detail', t: 'string', d: 'An explanation of this occurrence. Can be an expression.', ex: `detail: \${ "SKU " + .sku + " is sold out" }` },
						],
					},
				],
			},
			{
				k: 'run', t: 'object', d: 'Runs a process: a container, a script, a shell command or another workflow. Set exactly one of the four.',
				ex: `- resize:
    run:
      container:
        image: ghcr.io/example/resizer:1.0
        command: resize --width 800`,
				c: [
					{ k: 'container', t: 'object', d: 'Runs a container image. image is required; command, arguments, environment, ports, volumes and pullPolicy are optional.', ex: `container:
  image: ghcr.io/example/label-printer:1.2
  environment:
    PRINTER: dock-3` },
					{ k: 'script', t: 'object', d: 'Runs code in js or python, inline (code) or from a URL (source).', ex: `script:
  language: python
  code: |
    import sys
    print(sys.argv[1].upper())
  arguments: [hello]` },
					{ k: 'shell', t: 'object', d: 'Runs a shell command with optional arguments, environment and working directory.', ex: `shell:
  command: ls
  arguments: [-la, /data]` },
					{ k: 'workflow', t: 'object', d: 'Runs another workflow as a sub-workflow, by namespace, name and version, with an input.', ex: `workflow:
  namespace: shop
  name: send-invoice
  version: '1.0.0'
  input: \${ . }` },
					{ k: 'await', t: 'boolean', d: 'Wait for the process to finish. false starts it and moves on. Defaults to true.', ex: `await: false` },
					{ k: 'return', t: 'string', d: 'What the task outputs: stdout (default), stderr, code, all or none.', ex: `return: all` },
				],
			},
			{
				k: 'do', t: 'array · name: task', d: 'do is also a task: a group of tasks run in order, inside another list. Useful for structure and for then: exit.',
				ex: `- prepare:
    do:
      - load:
          set: { loaded: true }
      - check:
          set: { checked: true }`,
			},
		],
	},
	{
		k: 'timeout', t: 'string | object', d: 'A limit on the whole run. If it takes longer than after, the workflow fails with a timeout error. A string names a timeout from use.timeouts.',
		ex: `timeout:
  after: { minutes: 30 }`,
		c: [{ k: 'after', t: 'duration', r: true, d: `How long the run may take. ${duration}`, ex: `after: { hours: 1, minutes: 30 }` }],
	},
	{
		k: 'output', t: 'object', d: 'Shapes what the workflow returns. Without it, the workflow returns the output of its last task.',
		ex: `output:
  as: '\${ { orderId: $context.orderId, total: $context.total } }'`,
		c: [
			{ k: 'as', t: 'expression', d: 'A jq expression that builds the result. Quote it when it contains ": ", or YAML reads it as a map.', ex: `as: '\${ { id: .orderId } }'` },
			{ k: 'schema', t: 'object', d: 'A JSON Schema the result must match, the same shape as input.schema.', ex: `schema:
  format: json
  document:
    type: object
    required: [orderId]` },
		],
	},
	{
		k: 'schedule', t: 'object', d: 'Starts the workflow on its own instead of on request. Set one of every, cron, after or on.',
		ex: `schedule:
  cron: '0 6 * * *'`,
		c: [
			{ k: 'every', t: 'duration', d: 'Start a run at this interval, even if the last one is still running.', ex: `every: { minutes: 15 }` },
			{ k: 'cron', t: 'string', d: 'A cron expression. Quote it: * has a meaning in YAML.', ex: `cron: '0 * * * *'` },
			{ k: 'after', t: 'duration', d: 'Start again this long after the previous run completes.', ex: `after: { hours: 1 }` },
			{ k: 'on', t: 'object', d: 'Start when events arrive, with the same one, all or any as listen.to.', ex: `on:
  one:
    with:
      type: com.example.order.created.v1` },
		],
	},
	{
		k: 'evaluate', t: 'object', d: 'How runtime expressions are evaluated. Rarely needed: the defaults are jq, strict.',
		ex: `evaluate:
  language: jq
  mode: strict`,
		c: [
			{ k: 'language', t: 'string', d: 'The expression language. Defaults to jq.', ex: `language: jq` },
			{ k: 'mode', t: 'string', d: 'strict (default): only values inside ${ } are expressions. loose: every string is tried as an expression.', ex: `mode: strict` },
		],
	},
];
