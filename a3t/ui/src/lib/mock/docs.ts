// Mock docs content — will be replaced with flow call (call:docs or MCP docs tool)
// Each key matches a page type. Content is an array of sections.

export interface DocSection {
  heading: string;
  body: string;        // HTML string
}

export interface PageDocs {
  title: string;
  sections: DocSection[];
}

export const mockDocs: Record<string, PageDocs> = {
  templates: {
    title: 'Templates',
    sections: [
      {
        heading: 'What is a Template?',
        body: `A template is a <strong>CNCF Serverless Workflow YAML spec</strong> that defines an agent's full capability surface — memory access patterns, knowledge hooks, tool bindings, and procedure skeleton.<br><br>Templates are <strong>immutable once tagged</strong>. Agents reference them by version tag. Think of templates as typed archetypes that encode domain expertise — bid pipelines, incident triage, payment review, trade execution — so you configure rather than engineer.`,
      },
      {
        heading: 'Template lifecycle',
        body: `<ol>
          <li><strong>Author</strong> — Write or import a YAML spec following the CNCF Serverless Workflow 1.0 schema</li>
          <li><strong>Tag</strong> — Version the spec (e.g. v2.1). Tagged versions are immutable.</li>
          <li><strong>Publish</strong> — Push to the template registry as an OCI-signed blob</li>
          <li><strong>Instantiate</strong> — Create live agents from the template, filling in variables</li>
        </ol>`,
      },
      {
        heading: 'Key concepts',
        body: `<dl>
          <dt><strong>AgentTemplate</strong></dt><dd>The Kubernetes-style kind for template specs</dd>
          <dt><strong>Variables</strong></dt><dd>Exposed parameters that agents fill at instantiation</dd>
          <dt><strong>Checkpoints</strong></dt><dd>Durability markers — the agent can resume from these after failure</dd>
          <dt><strong>Resources</strong></dt><dd>Typed bindings the template declares: LLMs, MCP servers, memory stores</dd>
        </dl>`,
      },
    ],
  },

  agents: {
    title: 'Agents',
    sections: [
      {
        heading: 'What is an Agent?',
        body: `An agent is a <strong>running process in the cluster</strong>. Each has its own container, memory, network identity, and lifecycle. Agents are instantiated from templates — they fill in the template's variables and run as long-lived or ephemeral processes depending on their mode.`,
      },
      {
        heading: 'Agent modes',
        body: `<dl>
          <dt><strong>Ephemeral</strong></dt><dd>Create, execute one turn, die. Cheapest mode.</dd>
          <dt><strong>Session</strong></dt><dd>Lives for a TTL (e.g. 30 minutes). Good for chat-like interactions.</dd>
          <dt><strong>Persistent</strong></dt><dd>Long-lived daemon process. Runs continuously.</dd>
          <dt><strong>Reactive</strong></dt><dd>Sleeps until triggered by an event, webhook, or cron.</dd>
        </dl>`,
      },
      {
        heading: 'Lifecycle controls',
        body: `From the agent detail view you can <strong>pause</strong>, <strong>step</strong>, <strong>restart</strong>, or <strong>stop &amp; evict</strong> any running agent. These operate at the process level inside the container.`,
      },
    ],
  },

  canvas: {
    title: 'Canvas',
    sections: [
      {
        heading: 'What is a Canvas?',
        body: `A canvas is a <strong>Kubernetes Custom Resource (CRD)</strong> that declares a multi-agent topology. The controller continuously reconciles the spec against actual cluster state — scaling agents, binding triggers, routing traffic.`,
      },
      {
        heading: 'How it works',
        body: `You declare agents, triggers, and outputs in YAML. The controller creates pods, binds triggers, routes traffic, and reports drift when actual state diverges from desired.`,
      },
      {
        heading: 'Reconciliation pipeline',
        body: `The controller runs: <strong>spec parsed → diff computed → scaling → triggers bound → outputs bound</strong>. Drift is detected automatically and shown in the UI with amber indicators.`,
      },
    ],
  },

  resources: {
    title: 'Resources',
    sections: [
      {
        heading: 'What are Resources?',
        body: `<strong>Resources are typed, reusable bindings</strong> that agents consume. One MCP server can serve many agents. Resources include MCP servers, databases, LLMs, variables, flows, and scripts.`,
      },
      {
        heading: 'Resource types',
        body: `<dl>
          <dt><strong>MCP servers</strong></dt><dd>Tool providers via Model Context Protocol</dd>
          <dt><strong>Databases</strong></dt><dd>ADB, Neo4j, PostgreSQL, Qdrant</dd>
          <dt><strong>LLMs</strong></dt><dd>Anthropic, OpenAI, Ollama — multi-provider support</dd>
          <dt><strong>Variables</strong></dt><dd>Key-value config injected into agent system prompts</dd>
        </dl>`,
      },
    ],
  },

  inputs: {
    title: 'Pending Inputs',
    sections: [
      {
        heading: 'Human-in-the-loop',
        body: `When a workflow reaches a <code>call:human</code> step, it <strong>suspends execution</strong> and creates an input request. The workflow remains paused until you respond here.`,
      },
      {
        heading: 'Request types',
        body: `<strong>Approval</strong> — binary approve/reject decisions with optional defer.<br><strong>Form</strong> — structured data input with typed fields (text, number, select).<br><br>Both support expiration and configurable timeout actions (error, auto-approve, auto-reject, or use default value).`,
      },
    ],
  },

  runtime: {
    title: 'Run Time',
    sections: [
      {
        heading: 'Runtime overview',
        body: `The runtime view shows all running agent processes, resource utilization, execution queue depth, and system-level metrics across the cluster.`,
      },
    ],
  },

  reconciliation: {
    title: 'Controller',
    sections: [
      {
        heading: 'Controller & Reconciliation',
        body: `The A3T controller watches Agent CRDs and continuously reconciles declared state against actual cluster state. This view shows reconciliation events, drift history, and corrective actions taken.`,
      },
    ],
  },

  'flow-editor': {
    title: 'Flow Editor',
    sections: [
      {
        heading: 'Flow Editor',
        body: `Visual editor for CNCF Serverless Workflow definitions. Author agent templates with a drag-and-drop DAG editor, or write YAML directly with live validation and preview.`,
      },
    ],
  },

  'aql-console': {
    title: 'AQL Console',
    sections: [
      {
        heading: 'AQL Console',
        body: `Interactive query editor for ClawDB agent memory. Run AQL queries against working, episodic, procedural, semantic, and tools memory types. Supports RECALL, STORE, FORGET, and aggregate operations.`,
      },
    ],
  },
};
