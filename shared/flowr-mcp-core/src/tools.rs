//! FlowR MCP Tools
//!
//! Tool definitions and handlers for workflow control.
//! Uses StorageBackend trait for backend-agnostic storage.

use crate::protocol::{CallToolResult, Tool};
use flowr_storage::StorageBackend;
use flowr_types::{ExecutionRequest, ExecutionStatus, WorkflowRecord};
use serde_json::{json, Value};
use tracing::{debug, info, Instrument};
use uuid::Uuid;

/// Get all available tools
pub fn get_tools() -> Vec<Tool> {
    vec![
        // Task documentation tools
        Tool {
            name: "list_task_types".to_string(),
            description: "List all available task types with brief descriptions.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "category": {
                        "type": "string",
                        "description": "Optional category filter: 'flow_control', 'io', 'ai', 'script', 'integration'",
                        "enum": ["flow_control", "io", "ai", "script", "integration"]
                    }
                }
            }),
        },
        Tool {
            name: "get_task_type".to_string(),
            description: "Get detailed information about a specific task type.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "task_type": {
                        "type": "string",
                        "description": "The task type name (e.g., 'call:http', 'set', 'for', 'call:llm')"
                    }
                },
                "required": ["task_type"]
            }),
        },
        // Flow CRUD tools
        Tool {
            name: "list_flows".to_string(),
            description: "List all registered workflows.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": {
                        "type": "string",
                        "description": "Optional namespace filter"
                    }
                }
            }),
        },
        Tool {
            name: "list_projects".to_string(),
            description: "List the projects (namespaces) that hold flows, resources or triggers, with their counts. \
                          `platform` marks the platform's own namespaces (a3t, system).".to_string(),
            input_schema: json!({"type": "object", "properties": {}}),
        },
        Tool {
            name: "get_flow".to_string(),
            description: "Get the full definition of a workflow.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": {
                        "type": "string",
                        "default": "default"
                    },
                    "name": {
                        "type": "string",
                        "description": "Workflow name"
                    },
                    "version": {
                        "type": "string",
                        "description": "Optional specific version"
                    }
                },
                "required": ["name"]
            }),
        },
        Tool {
            name: "propose_flow".to_string(),
            description: "Draft a workflow for the user to review WITHOUT saving it. Validates the YAML and renders it in an editable widget; the user saves it themselves. Use this for any new or rewritten workflow — prefer it over `create_flow`, which writes immediately and gives the user nothing to review. Format is CNCF Serverless Workflow DSL 1.0.3: a `document` block (dsl/namespace/name/version) plus a `do` list of single-key taskName -> task entries. It is NOT the 0.8 `states`/`start_at` format.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "yaml": {
                        "type": "string",
                        "description": "Complete CNCF Serverless Workflow YAML (dsl 1.0.3)"
                    }
                },
                "required": ["yaml"]
            }),
        },
        Tool {
            name: "propose_edit".to_string(),
            description: "Propose a change to the document open in the user's designer (a \
template, agent, flow, canvas or resource) WITHOUT saving it. Send the COMPLETE updated \
document, not a fragment or a diff. It is applied to the editor on screen; the user reviews \
it and presses Save to persist. Use this — never create_*/update_* tools — when the user is \
editing in a designer."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "yaml": { "type": "string", "description": "The complete updated document (YAML or JSON)" },
                    "summary": { "type": "string", "description": "One line: what changed" }
                },
                "required": ["yaml"]
            }),
        },
        Tool {
            name: "create_flow".to_string(),
            description: "Create a new workflow from YAML or JSON definition.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "yaml": {
                        "type": "string",
                        "description": "Workflow definition in YAML format"
                    },
                    "json": {
                        "type": "object",
                        "description": "Workflow definition as JSON object"
                    }
                }
            }),
        },
        Tool {
            name: "update_flow".to_string(),
            description: "Update workflow settings (enable/disable, set active version).".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": {
                        "type": "string",
                        "default": "default"
                    },
                    "name": {
                        "type": "string",
                        "description": "Workflow name"
                    },
                    "enabled": {
                        "type": "boolean",
                        "description": "Enable or disable the workflow"
                    },
                    "version": {
                        "type": "string",
                        "description": "Set active version"
                    }
                },
                "required": ["name"]
            }),
        },
        Tool {
            name: "delete_flow".to_string(),
            description: "Delete a workflow.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": {
                        "type": "string",
                        "default": "default"
                    },
                    "name": {
                        "type": "string",
                        "description": "Workflow name"
                    }
                },
                "required": ["name"]
            }),
        },
        // Execution tools
        Tool {
            name: "execute_flow".to_string(),
            description: "Execute a workflow and return the result.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": {
                        "type": "string",
                        "default": "default"
                    },
                    "name": {
                        "type": "string",
                        "description": "Workflow name"
                    },
                    "input": {
                        "type": "object",
                        "description": "Input data for the workflow"
                    }
                },
                "required": ["name"]
            }),
        },
        Tool {
            name: "get_execution_status".to_string(),
            description: "Get the status of a workflow execution.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "instance_id": {
                        "type": "string",
                        "description": "The execution instance ID (UUID)"
                    }
                },
                "required": ["instance_id"]
            }),
        },
        // ── Deployment (canvas) ──
        //
        // A3T's deployment mechanism is the canvas: a flow's placement metadata.
        // A PRESET is a canvas with no flow attached — a reusable shape to pick
        // from. Nothing is placed anywhere until `deploy_flow`.
        Tool {
            name: "list_canvas_presets".to_string(),
            description: "List the deployment presets available to choose from — the shapes a \
flow can be deployed with (shared or isolated runtime, which pool, how far it scales, what \
happens on failure). Start here whenever the user wants to deploy, test or ship something: \
show them the presets and let them pick, rather than inventing placement yourself."
                .to_string(),
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        Tool {
            name: "get_canvas".to_string(),
            description: "Read one canvas or preset in full, to show the user exactly what it \
would do before they commit to it.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "default": "a3t" },
                    "name": { "type": "string", "description": "Canvas or preset name" }
                },
                "required": ["name"]
            }),
        },
        Tool {
            name: "list_runtimes".to_string(),
            description: "List the runtime pools a flow can be placed into, with the labels \
placement selects on (tier, class, zone)."
                .to_string(),
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        Tool {
            name: "preview_placement".to_string(),
            description: "Show WHERE a flow would be deployed — which runtime pool, which \
namespace, shared or isolated, and how far it would scale — WITHOUT deploying anything. \
Always preview before deploying, and show the user the result: placement is the thing they \
are agreeing to. Pass `runtime` to see what would happen on a specific pool."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "default": "a3t" },
                    "name": { "type": "string", "description": "Flow name" },
                    "runtime": { "type": "string", "description": "Optional: a specific runtime pool to place on (from list_runtimes)" }
                },
                "required": ["name"]
            }),
        },
        Tool {
            name: "clone_canvas".to_string(),
            description: "Copy a preset into a new canvas, optionally changing parts of it — \
for example cloning a production preset with a higher minimum replica count. The preset \
itself is never modified, so other flows using it are unaffected. Creates configuration \
only; nothing is deployed."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "preset_namespace": { "type": "string", "default": "a3t" },
                    "preset": { "type": "string", "description": "Preset to copy from" },
                    "name": { "type": "string", "description": "Name for the new canvas" },
                    "namespace": { "type": "string", "default": "a3t" },
                    "patch": {
                        "type": "object",
                        "description": "Changes to apply, shaped like the canvas spec, e.g. {\"lifecycle\":{\"scaling\":{\"min\":4}}}. Arrays replace rather than merge."
                    }
                },
                "required": ["preset", "name"]
            }),
        },
        Tool {
            name: "bind_canvas".to_string(),
            description: "Attach a canvas to a flow, which is what makes it deployable. \
Creates configuration only; nothing is deployed until deploy_flow."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "default": "a3t" },
                    "name": { "type": "string", "description": "Canvas name" },
                    "flow": { "type": "string", "description": "Flow as 'namespace/name'" }
                },
                "required": ["name", "flow"]
            }),
        },
        Tool {
            name: "update_canvas".to_string(),
            description: "Change a canvas that belongs to a flow — for example its scaling or \
target namespace. Shared presets cannot be edited in place; clone them instead. Creates \
configuration only; nothing is redeployed."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "default": "a3t" },
                    "name": { "type": "string", "description": "Canvas name" },
                    "patch": { "type": "object", "description": "Changes shaped like the canvas spec. Arrays replace rather than merge." }
                },
                "required": ["name", "patch"]
            }),
        },
        Tool {
            name: "create_runtime".to_string(),
            description: "Register a new runtime pool that flows can be deployed onto, with the \
labels placement selects on (tier, class, zone). Needs Admin, so the user will be asked to \
approve it. This registers a placement target; it does not start a process or a sandbox."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Runtime name: letters, digits, '-' or '_'" },
                    "namespace": { "type": "string", "default": "default" },
                    "labels": { "type": "object", "description": "e.g. {\"tier\":\"test\",\"class\":\"cpu\"}" },
                    "url": { "type": "string", "description": "Optional endpoint of the pool" }
                },
                "required": ["name", "labels"]
            }),
        },
        Tool {
            name: "deploy_flow".to_string(),
            description: "Deploy a flow — place it into the runtime its canvas selects. This \
is the step that actually acts, so preview_placement first and show the user what they are \
approving. Deploying to a production pool, or taking a dedicated runtime, needs the user's \
approval: call it anyway and the platform will pause and ask them. Pass `runtime` to deploy \
onto a specific pool the user chose; it must still satisfy the canvas's selector."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "default": "a3t" },
                    "name": { "type": "string", "description": "Flow name" },
                    "runtime": { "type": "string", "description": "Optional: the runtime pool to deploy onto (from list_runtimes)" }
                },
                "required": ["name"]
            }),
        },
        Tool {
            name: "list_deployments".to_string(),
            description: "Show every runtime with what is deployed on it (flows and agents), \
each deployment's state (serving, paused, runtime-paused, hibernated), recent activity (runs, \
in flight, error rate, p50/p95) and the host's CPU, memory and disk. Use it to answer \
\"what is deployed / running where\" and to find a deployment's id."
                .to_string(),
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        Tool {
            name: "probe_deployment".to_string(),
            description: "Live look at one deployment: whether it is serving, its last runs \
with errors, the triggers that feed it, and health checks (flow exists, canvas resolves, \
placement still matches)."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "deployment": { "type": "string", "description": "Deployment id, flow 'ns/name', or agent id" }
                },
                "required": ["deployment"]
            }),
        },
        Tool {
            name: "pause_deployment".to_string(),
            description: "Hard-pause a deployed flow or agent. Every new request — triggers, \
manual runs, chat, other agents — is refused until it is resumed; webhooks get the runtime's \
paused response (503 or 404). Runs already in progress finish. Pausing in production needs \
the user's approval: call it anyway and the platform will ask them."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "deployment": { "type": "string", "description": "Deployment id, flow 'ns/name', or agent id" },
                    "reason": { "type": "string", "description": "Shown to whoever finds it paused" }
                },
                "required": ["deployment"]
            }),
        },
        Tool {
            name: "resume_deployment".to_string(),
            description: "Resume a paused deployment so it serves again.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": { "deployment": { "type": "string" } },
                "required": ["deployment"]
            }),
        },
        Tool {
            name: "restart_deployment".to_string(),
            description: "Re-resolve a deployment's placement from its current canvas — use \
after editing the canvas. Refused while paused (resume first)."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": { "deployment": { "type": "string" } },
                "required": ["deployment"]
            }),
        },
        Tool {
            name: "undeploy".to_string(),
            description: "Remove a deployment. The flow and its canvas are kept, so it can \
be deployed again."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": { "deployment": { "type": "string" } },
                "required": ["deployment"]
            }),
        },
        Tool {
            name: "terminate_deployment".to_string(),
            description: "Stop a deployed flow or agent NOW: cancel its runs in progress, then \
remove the deployment. Use undeploy instead to let running work finish. The flow and canvas \
are kept."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": { "deployment": { "type": "string" } },
                "required": ["deployment"]
            }),
        },
        Tool {
            name: "terminate_runtime".to_string(),
            description: "Admin: terminate a runtime — like killing its container. Runs on it \
are cancelled and the runtime is removed; what was deployed there is RESCHEDULED, not lost: \
shared deployments move to another matching runtime, and isolated ones (or ones nothing else \
matches) get a freshly recreated runtime, losing in-memory state. Call WITHOUT confirm first: \
it changes nothing and returns the plan with warnings. Show it to the user, then call again \
with confirm set to the runtime's name."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "runtime": { "type": "string" },
                    "confirm": { "type": "string", "description": "The runtime's name, to confirm" }
                },
                "required": ["runtime"]
            }),
        },
        Tool {
            name: "set_runtime_state".to_string(),
            description: "Admin: change a runtime's state. 'hibernated' sleeps it — the first \
request wakes it. 'paused' refuses everything deployed on it until resumed. 'active' resumes. \
Deployments paused on their own stay paused."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "runtime": { "type": "string" },
                    "state": { "type": "string", "enum": ["active", "hibernated", "paused"] }
                },
                "required": ["runtime", "state"]
            }),
        },
        Tool {
            name: "set_paused_response".to_string(),
            description: "Admin: the HTTP status triggers get while a deployment on this \
runtime is paused — 503 (default) or 404 (hide the endpoint). A canvas can override it with \
lifecycle.pausedResponse."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "runtime": { "type": "string" },
                    "status": { "type": "integer", "enum": [503, 404] }
                },
                "required": ["runtime", "status"]
            }),
        },
        Tool {
            name: "list_permissions".to_string(),
            description: "List what the user has already allowed or declined in this \
conversation. Use it ONLY when the user asks what they have granted or denied, or to \
explain a refusal that already happened.\n\n\
NEVER call this to check whether you are allowed to use another tool, and never ask the \
user to grant a permission before you have tried. Just call the tool you need: if it \
requires approval the platform pauses and asks the user right then, and your call \
continues automatically once they allow it. An empty list here does NOT mean you are \
blocked — it means nothing has been asked yet. Pre-checking only stalls the \
conversation, because the request the user would approve is never raised."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "only": {
                        "type": "string",
                        "enum": ["denied", "granted", "all"],
                        "default": "all",
                        "description": "Filter to just refusals, just grants, or everything"
                    }
                }
            }),
        },
        Tool {
            name: "list_executions".to_string(),
            description: "List recent workflow executions. Pass namespace + name to list runs of a specific workflow, or omit both for recent executions across all workflows.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": {
                        "type": "string",
                        "description": "Workflow namespace (used with name to filter to one workflow)"
                    },
                    "name": {
                        "type": "string",
                        "description": "Workflow name (requires namespace)"
                    },
                    "limit": {
                        "type": "integer",
                        "default": 20,
                        "description": "Maximum number of executions to return"
                    }
                }
            }),
        },
        // State tools
        Tool {
            name: "get_state".to_string(),
            description: "Get a value from the state store.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "key": {
                        "type": "string",
                        "description": "State key to retrieve"
                    }
                },
                "required": ["key"]
            }),
        },
        Tool {
            name: "set_state".to_string(),
            description: "Set a value in the state store.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "key": {
                        "type": "string",
                        "description": "State key"
                    },
                    "value": {
                        "description": "Value to store"
                    },
                    "ttl_secs": {
                        "type": "integer",
                        "description": "Optional TTL in seconds"
                    }
                },
                "required": ["key", "value"]
            }),
        },
        // Resource tools
        Tool {
            name: "list_resources".to_string(),
            description: "List all configured resources (databases, APIs, LLM providers, etc.).".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": {
                        "type": "string",
                        "description": "Optional namespace filter",
                        "default": "default"
                    },
                    "type_ref": {
                        "type": "string",
                        "description": "Optional filter by resource type (e.g., 'openai', 'postgres')"
                    }
                }
            }),
        },
        Tool {
            name: "query_resource".to_string(),
            description: "Run a query against a resource with its driver and return rows. Supports ADB (AQL), e.g. RECALL FROM WORKING WHERE name = \"alice\".".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "resource": { "type": "string", "description": "Resource name" },
                    "namespace": { "type": "string", "default": "default" },
                    "query": { "type": "string", "description": "Query in the resource's language (AQL for ADB)" },
                    "variables": { "type": "object", "description": "Optional query variables" }
                },
                "required": ["resource", "query"]
            }),
        },
        Tool {
            name: "get_resource".to_string(),
            description: "Get details of a specific resource configuration.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": {
                        "type": "string",
                        "default": "default"
                    },
                    "name": {
                        "type": "string",
                        "description": "Resource name"
                    }
                },
                "required": ["name"]
            }),
        },
        Tool {
            name: "list_mcp_tools".to_string(),
            description: "List the tool names an MCP server resource exposes. `name` must be an existing resource of type `mcp` — call with no `name` to list the MCP servers available in the namespace. A server whose URL points back at this process is served in-process; other transports report their tools when discoverable.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "default": "a3t" },
                    "name": { "type": "string", "description": "MCP server resource name; omit to list the available MCP servers" }
                }
            }),
        },
        Tool {
            name: "create_resource".to_string(),
            description: "Create a new resource configuration.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Unique resource name"
                    },
                    "namespace": {
                        "type": "string",
                        "default": "default"
                    },
                    "type_ref": {
                        "type": "string",
                        "description": "Resource type (e.g., 'openai', 'postgres', 'qdrant', 'mcp')"
                    },
                    "config": {
                        "type": "object",
                        "description": "Resource configuration (API keys, connection strings, etc.)"
                    },
                    "description": {
                        "type": "string",
                        "description": "Optional description"
                    }
                },
                "required": ["name", "type_ref", "config"]
            }),
        },
        Tool {
            name: "delete_resource".to_string(),
            description: "Delete a resource configuration.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": {
                        "type": "string",
                        "default": "default"
                    },
                    "name": {
                        "type": "string",
                        "description": "Resource name"
                    }
                },
                "required": ["name"]
            }),
        },
        Tool {
            name: "list_resource_types".to_string(),
            description: "List available resource types (templates for creating resources).".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "category": {
                        "type": "string",
                        "description": "Optional category filter",
                        "enum": ["database", "key_value", "vector", "queue", "api", "storage", "ai", "agent", "mcp", "memory", "channel", "custom"]
                    }
                }
            }),
        },
        // Trigger CRUD tools
        Tool {
            name: "list_triggers".to_string(),
            description: "List all registered triggers (webhooks, cron, events). Can filter by namespace or target flow.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": {
                        "type": "string",
                        "description": "Optional namespace filter"
                    },
                    "target_flow": {
                        "type": "string",
                        "description": "Optional: find triggers targeting this flow name (reverse lookup)"
                    }
                }
            }),
        },
        Tool {
            name: "get_trigger".to_string(),
            description: "Get details of a specific trigger by ID.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Trigger ID (UUID)"
                    }
                },
                "required": ["id"]
            }),
        },
        Tool {
            name: "create_trigger".to_string(),
            description: "Create a new trigger (webhook, cron, event, or adb_topic — run targets once per message published to an ADB topic).".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Trigger name"
                    },
                    "namespace": {
                        "type": "string",
                        "default": "default",
                        "description": "Trigger namespace"
                    },
                    "description": {
                        "type": "string",
                        "description": "Optional description"
                    },
                    "config": {
                        "type": "object",
                        "description": "Trigger config: {type: 'webhook', path: '/...'} or {type: 'cron', expression: '*/5 * * * *'} or {type: 'event', source: '...'} or {type: 'adb_topic', resource: 'ns/adb-resource', topic: 'orders', filter?: {field: value}}"
                    },
                    "targets": {
                        "type": "array",
                        "description": "Array of targets: [{type: 'workflow', namespace: '...', name: '...'}] or [{type: 'agent', agent_id: '...'}]"
                    },
                    "enabled": {
                        "type": "boolean",
                        "default": true
                    }
                },
                "required": ["name", "config", "targets"]
            }),
        },
        Tool {
            name: "update_trigger".to_string(),
            description: "Update an existing trigger by ID.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Trigger ID to update"
                    },
                    "name": {
                        "type": "string"
                    },
                    "namespace": {
                        "type": "string"
                    },
                    "description": {
                        "type": "string"
                    },
                    "config": {
                        "type": "object",
                        "description": "New trigger config"
                    },
                    "targets": {
                        "type": "array",
                        "description": "New target list"
                    },
                    "enabled": {
                        "type": "boolean"
                    }
                },
                "required": ["id"]
            }),
        },
        Tool {
            name: "delete_trigger".to_string(),
            description: "Delete a trigger by ID.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Trigger ID to delete"
                    }
                },
                "required": ["id"]
            }),
        },
        Tool {
            name: "enable_trigger".to_string(),
            description: "Enable a paused trigger.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Trigger ID"
                    }
                },
                "required": ["id"]
            }),
        },
        Tool {
            name: "pause_trigger".to_string(),
            description: "Pause (disable) an active trigger.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Trigger ID"
                    }
                },
                "required": ["id"]
            }),
        },
        Tool {
            name: "attach_trigger".to_string(),
            description: "Attach a target (workflow, agent, or webhook) to a trigger. A trigger can have multiple targets.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Trigger ID"
                    },
                    "target": {
                        "type": "object",
                        "description": "Target to attach: {type: 'workflow', namespace: '...', name: '...'} or {type: 'agent', agent_id: '...'} or {type: 'webhook', url: '...'}"
                    }
                },
                "required": ["id", "target"]
            }),
        },
        Tool {
            name: "detach_trigger".to_string(),
            description: "Detach a target from a trigger by workflow name, agent ID, or webhook URL.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "string",
                        "description": "Trigger ID"
                    },
                    "target_id": {
                        "type": "string",
                        "description": "Target to detach: workflow name, agent ID, or webhook URL"
                    }
                },
                "required": ["id", "target_id"]
            }),
        },

        // ── Agent Template Tools ──────────────────────────────────────
        Tool {
            name: "create_agent_template".to_string(),
            description: "Register a versioned agent template. Templates are reusable CNCF workflow blueprints for agents.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Template name (e.g. 'chat-agent')" },
                    "version": { "type": "string", "description": "Semantic version (e.g. '1.0.0')" },
                    "namespace": { "type": "string", "default": "default" },
                    "description": { "type": "string" },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "spec": { "type": "object", "description": "The CNCF workflow definition (or reference to system/agent-template)" },
                    "defaults": { "type": "object", "description": "Default agent config (system_prompt, llm_resource, memory, tools, etc.)" }
                },
                "required": ["name", "version", "spec"]
            }),
        },
        Tool {
            name: "list_agent_templates".to_string(),
            description: "List all registered agent templates with their versions.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string", "description": "Optional namespace filter" }
                }
            }),
        },
        Tool {
            name: "get_agent_template".to_string(),
            description: "Get an agent template by name (active version) or name:version.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Template name or name:version" },
                    "namespace": { "type": "string", "default": "default" }
                },
                "required": ["name"]
            }),
        },

        // ── Agent Instance Tools ──────────────────────────────────────
        Tool {
            name: "create_agent".to_string(),
            description: "Create an agent instance from a template. Creates the workflow + optional triggers. Returns agent_id.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "template": { "type": "string", "description": "Template ref: 'name' or 'name:version'" },
                    "namespace": { "type": "string", "default": "default" },
                    "agent_id": { "type": "string", "description": "Custom agent ID. Auto-generated if omitted." },
                    "config": {
                        "type": "object",
                        "description": "Agent config overrides: system_prompt, llm_resource, memory_resource, tools, etc.",
                        "properties": {
                            "system_prompt": { "type": "string" },
                            "llm_resource": { "type": "string" },
                            "memory_resource": { "type": "string" },
                            "memory_namespace": { "type": "string" },
                            "max_iterations": { "type": "integer" },
                            "ttl": { "type": "integer" },
                            "tools": { "type": "array" },
                            "mcp_servers": { "type": "array" }
                        }
                    },
                    "triggers": {
                        "type": "array",
                        "description": "Triggers to create: [{kind: 'webhook', path: '/chat'}, {kind: 'cron', expression: '*/5 * * * *'}]",
                        "items": { "type": "object" }
                    }
                },
                "required": ["template"]
            }),
        },
        Tool {
            name: "list_agents".to_string(),
            description: "List all running agent instances.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "namespace": { "type": "string" }
                }
            }),
        },
        Tool {
            name: "get_agent".to_string(),
            description: "Get agent instance details including config, session state, and triggers.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string", "description": "Agent ID" }
                },
                "required": ["agent_id"]
            }),
        },
        Tool {
            name: "delete_agent".to_string(),
            description: "Delete an agent instance, its triggers, and session state.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string", "description": "Agent ID to delete" }
                },
                "required": ["agent_id"]
            }),
        },

        // ── Agent Execution Tools ─────────────────────────────────────
        Tool {
            name: "send_message".to_string(),
            description: "Send a message to an agent. Executes the agent-template workflow with the message and returns the response.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string", "description": "Agent ID (used as session_id)" },
                    "message": { "type": "string", "description": "User message text" },
                    "context": { "type": "object", "description": "Optional additional context" }
                },
                "required": ["agent_id", "message"]
            }),
        },
        Tool {
            name: "get_agent_history".to_string(),
            description: "Get conversation history for an agent session from the state store.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string", "description": "Agent ID (session key)" },
                    "limit": { "type": "integer", "default": 50 }
                },
                "required": ["agent_id"]
            }),
        },
        Tool {
            name: "clear_agent_session".to_string(),
            description: "Clear an agent's conversation history and session state.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string", "description": "Agent ID (session key)" }
                },
                "required": ["agent_id"]
            }),
        },
    ]
}

/// Handle a tool call using the provided storage backend
pub async fn handle_tool_call(
    storage: &dyn StorageBackend,
    principal: &flowr_policy::Principal,
    name: &str,
    arguments: Option<Value>,
) -> CallToolResult {
    handle_tool_call_with_attributes(storage, principal, name, arguments, Default::default()).await
}

/// Site A with caller-supplied policy `attributes`. The agent loop (E0.4c) uses
/// this to carry the acting flow's `template.*` data (E2 #2a/#3) — exposed/enabled
/// tool sets from `metadata.a3t` — so the template ceiling (§6.5) gates the
/// agent's tool calls here, at the point the LLM actually invokes a tool.
/// Tools implemented by the agent runtime (the `flowr-ce` layer), not here.
///
/// Dispatchers must offer these to the CE layer BEFORE falling through to this
/// module — otherwise they land on the "requires the agent runtime" arm below,
/// which is what made the in-process agent unable to list agents or templates
/// even though the very same tools worked over `/message`.
pub const AGENT_RUNTIME_TOOLS: &[&str] = &[
    "create_agent_template", "list_agent_templates", "get_agent_template",
    "create_agent", "list_agents", "get_agent", "delete_agent", "send_message",
    // Deployment (canvas) — implemented in flowr-ce beside the canvas handlers,
    // so they route through the agent-runtime interceptor like the agent tools.
    "list_canvas_presets", "get_canvas", "list_runtimes", "preview_placement",
    "clone_canvas", "bind_canvas", "deploy_flow", "update_canvas", "create_runtime",
    // Runtime operations — the deployment ledger in flowr-ce (`ops`).
    "list_deployments", "probe_deployment", "pause_deployment", "resume_deployment",
    "restart_deployment", "undeploy", "terminate_deployment", "terminate_runtime",
    "set_runtime_state", "set_paused_response",
    // DB console — resource drivers (ADB over HTTP) live in flowr-ce.
    "query_resource",
];

pub fn is_agent_runtime_tool(name: &str) -> bool {
    AGENT_RUNTIME_TOOLS.contains(&name)
}

/// The site-A policy check on its own.
///
/// Returns `Some(denial)` when the call is refused. Split out of
/// `handle_tool_call_with_attributes` so a dispatcher that routes a tool
/// elsewhere (the agent-runtime tools) still applies the identical gate —
/// including the flow/agent capability ceiling carried in `attributes` — rather
/// than quietly skipping authorization.
pub async fn authorize_tool_call(
    principal: &flowr_policy::Principal,
    name: &str,
    args: &Value,
    attributes: serde_json::Map<String, Value>,
) -> Option<CallToolResult> {
    let span = tracing::info_span!(
        "tool_call",
        "aesop.principal.kind" = principal.kind(),
        "aesop.principal.id" = %principal.id(),
        "aesop.on_behalf_of" = principal.on_behalf_of().unwrap_or(""),
        "aesop.action" = flowr_policy::actions::MCP_CALL,
        "aesop.resource.type" = "Tool",
        "aesop.resource.id" = name,
        "aesop.task.source" = flowr_policy::telemetry::TASK_SOURCE_BUILTIN,
        "aesop.decision" = tracing::field::Empty,
    );
    let policy_req = flowr_policy::ExecutionRequest {
        principal: principal.clone(),
        action: flowr_policy::actions::mcp_call(),
        resource: flowr_policy::Resource::tool(name),
        attributes,
        context: flowr_policy::ExecContext::default(),
        params: args.clone(),
    };
    let decision = flowr_policy::policy_hook()
        .check(&policy_req)
        .instrument(span.clone())
        .await;
    span.record("aesop.decision", decision.label());
    flowr_policy::audit::record(&policy_req, &decision);
    match decision {
        flowr_policy::PolicyDecision::Allow => None,
        flowr_policy::PolicyDecision::Deny { reason, policy_ids } => {
            debug!(tool = %name, reason = %reason, "MCP tool call denied by policy");
            Some(CallToolResult::error(format!(
                "denied: {name} — {reason} (policies: {})",
                policy_ids.join(",")
            )))
        }
        // This entry point has no way to reach a human, so it fails closed.
        // A caller that CAN prompt (the in-process chat agent loop) uses
        // `authorize_tool_call_decision` below and handles `Ask` itself.
        flowr_policy::PolicyDecision::Ask { reason, policy_ids, .. } => {
            debug!(tool = %name, reason = %reason,
                   "MCP tool call needs approval, but this path cannot prompt");
            Some(CallToolResult::error(format!(
                "denied: {name} — {reason} (approval required, no interactive approver) (policies: {})",
                policy_ids.join(",")
            )))
        }
    }
}

/// Authorize one MCP tool call, naming the SERVER it is routed to.
///
/// [`authorize_tool_call_decision`] identifies a tool by name alone, which is
/// right for built-ins but not for MCP: two servers can both expose `query`, and
/// they are not the same capability. The server is attached as a resource
/// attribute so policy can gate "this tool on this server", and the caller uses
/// [`mcp_decision_id`] for the matching ledger key.
///
/// `resource.name` stays the bare tool name so existing rules that match on it
/// (the template ceiling, the authoring-tool forbid) keep working unchanged.
pub async fn authorize_mcp_tool_call(
    principal: &flowr_policy::Principal,
    server: Option<&str>,
    name: &str,
    args: &Value,
    attributes: serde_json::Map<String, Value>,
) -> flowr_policy::PolicyDecision {
    let mut resource = flowr_policy::Resource::tool(name);
    if let Some(srv) = server {
        resource = resource.with_attr("server", srv);
    }
    let span = tracing::info_span!(
        "tool_call",
        "aesop.principal.kind" = principal.kind(),
        "aesop.principal.id" = %principal.id(),
        "aesop.on_behalf_of" = principal.on_behalf_of().unwrap_or(""),
        "aesop.action" = flowr_policy::actions::MCP_CALL,
        "aesop.resource.type" = "Tool",
        "aesop.resource.id" = name,
        "aesop.mcp.server" = server.unwrap_or(""),
        "aesop.decision" = tracing::field::Empty,
    );
    let policy_req = flowr_policy::ExecutionRequest {
        principal: principal.clone(),
        action: flowr_policy::actions::mcp_call(),
        resource,
        attributes,
        context: flowr_policy::ExecContext::default(),
        params: args.clone(),
    };
    let decision = flowr_policy::policy_hook()
        .check(&policy_req)
        .instrument(span.clone())
        .await;
    span.record("aesop.decision", decision.label());
    flowr_policy::audit::record(&policy_req, &decision);
    decision
}

/// The id a permission decision is recorded against for an MCP tool.
///
/// Qualified by server, so allowing `query` on the analytics server does not
/// silently allow `query` on a different one. Unqualified for built-ins, whose
/// names are already global.
pub fn mcp_decision_id(server: Option<&str>, tool: &str) -> String {
    match server {
        Some(s) if !s.is_empty() => format!("{s}/{tool}"),
        _ => tool.to_string(),
    }
}

/// Resolve a policy decision against what the user already answered in this
/// session.
///
/// Sits between the policy engine and the prompt, and only ever acts on `Ask` —
/// a `Deny` from policy is not something a session grant may overturn, and an
/// `Allow` needs no ledger. Three outcomes:
///
/// - already granted → `Allow`, without asking again;
/// - already refused → `Deny`, **without re-asking**. A refusal that kept
///   re-prompting would train people to click through;
/// - never asked → the `Ask` passes through and the caller prompts.
///
/// A refusal recorded here stays listed and can be lifted later
/// (`grants::list_denied` / `grants::lift_denial`), so sticky is not final.
pub fn apply_session_decisions(
    session_id: &str,
    decision: flowr_policy::PolicyDecision,
    action: &str,
    resource_type: &str,
    resource_id: &str,
) -> flowr_policy::PolicyDecision {
    use flowr_policy::grants::{consult, decision_key, Verdict};
    if !decision.is_ask() {
        return decision;
    }
    let key = decision_key(action, resource_type, resource_id);
    match consult(session_id, &key) {
        Some(d) if d.verdict == Verdict::Granted => flowr_policy::PolicyDecision::Allow,
        Some(d) => flowr_policy::PolicyDecision::Deny {
            reason: format!(
                "you refused this earlier in this conversation ({}); ask to review denied \
                 permissions to change it",
                d.reason
            ),
            policy_ids: vec!["session-denial".to_string()],
        },
        None => decision,
    }
}

/// What the agent currently holds for a tool, so the LLM can be told rather than
/// discovering it by being refused.
///
/// Knowing "this tool will prompt" or "you already declined this" lets a model
/// choose a different route or explain the blocker, instead of calling and
/// failing. It is told the *state*, never how to change it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolPermission {
    /// Allowed outright by policy — nothing to mention.
    Allowed,
    /// Allowed because the user granted it in this conversation.
    Granted,
    /// Refused in this conversation; calling it will fail.
    Denied,
    /// Permitted only if the user approves when asked.
    NeedsApproval,
}

impl ToolPermission {
    /// One clause appended to the tool description the model sees. `Allowed`
    /// yields nothing: annotating every ordinary tool would be noise.
    pub fn hint(self) -> Option<&'static str> {
        match self {
            ToolPermission::Allowed => None,
            ToolPermission::Granted => Some(
                "(permission NOW: granted by the user in this conversation — call it normally)",
            ),
            ToolPermission::Denied => Some(
                "(permission NOW: declined — a call will fail at this moment. Do not retry it on \
                 your own. This line is recomputed every turn from the live permission list, so \
                 trust it over anything said earlier in this conversation: if it stops saying \
                 declined, the user has re-enabled it and you should just call the tool.)",
            ),
            ToolPermission::NeedsApproval => Some(
                "(permission NOW: needs the user's approval — call it anyway; the platform pauses \
                 and asks them, then your call continues. This line is recomputed every turn, so \
                 trust it over any earlier refusal in this conversation.)",
            ),
        }
    }
}

/// Per-tool permission state for this session, for annotating the tool list.
pub async fn tool_permission(
    principal: &flowr_policy::Principal,
    session_id: &str,
    name: &str,
    attributes: serde_json::Map<String, Value>,
) -> ToolPermission {
    use flowr_policy::grants::{consult, decision_key, Verdict};
    let key = decision_key(flowr_policy::actions::MCP_CALL, "Tool", name);
    // A recorded answer wins: it is what the runtime will act on.
    if let Some(d) = consult_readonly(session_id, &key) {
        return match d {
            Verdict::Granted => ToolPermission::Granted,
            Verdict::Denied => ToolPermission::Denied,
        };
    }
    // Otherwise ask the policy what would happen, WITHOUT auditing it as a real
    // call — this is a listing, not an invocation.
    let req = flowr_policy::ExecutionRequest {
        principal: principal.clone(),
        action: flowr_policy::actions::mcp_call(),
        resource: flowr_policy::Resource::tool(name),
        attributes,
        context: flowr_policy::ExecContext::default(),
        params: json!({}),
    };
    match flowr_policy::policy_hook().check(&req).await {
        flowr_policy::PolicyDecision::Allow => ToolPermission::Allowed,
        flowr_policy::PolicyDecision::Ask { .. } => ToolPermission::NeedsApproval,
        flowr_policy::PolicyDecision::Deny { .. } => ToolPermission::Denied,
    }
}

/// Peek at a recorded answer without consuming a one-time grant. Listing tools
/// must not spend a permission the user has not used yet.
fn consult_readonly(session_id: &str, key: &str) -> Option<flowr_policy::grants::Verdict> {
    flowr_policy::grants::list(session_id)
        .into_iter()
        .find(|d| d.key == key)
        .map(|d| d.verdict)
}

/// The same check as [`authorize_tool_call`], but handing back the raw decision
/// instead of a canned error.
///
/// Callers that can conduct an interactive approval — today the in-process chat
/// agent loop — use this so a `PolicyDecision::Ask` becomes a prompt rather than
/// a refusal. Everything else should keep using `authorize_tool_call`, which
/// fails closed. Both audit identically, so choosing one does not change the
/// trail.
pub async fn authorize_tool_call_decision(
    principal: &flowr_policy::Principal,
    name: &str,
    args: &Value,
    attributes: serde_json::Map<String, Value>,
) -> flowr_policy::PolicyDecision {
    let span = tracing::info_span!(
        "tool_call",
        "aesop.principal.kind" = principal.kind(),
        "aesop.principal.id" = %principal.id(),
        "aesop.on_behalf_of" = principal.on_behalf_of().unwrap_or(""),
        "aesop.action" = flowr_policy::actions::MCP_CALL,
        "aesop.resource.type" = "Tool",
        "aesop.resource.id" = name,
        "aesop.task.source" = flowr_policy::telemetry::TASK_SOURCE_BUILTIN,
        "aesop.decision" = tracing::field::Empty,
    );
    let policy_req = flowr_policy::ExecutionRequest {
        principal: principal.clone(),
        action: flowr_policy::actions::mcp_call(),
        resource: flowr_policy::Resource::tool(name),
        attributes,
        context: flowr_policy::ExecContext::default(),
        params: args.clone(),
    };
    let decision = flowr_policy::policy_hook()
        .check(&policy_req)
        .instrument(span.clone())
        .await;
    span.record("aesop.decision", decision.label());
    flowr_policy::audit::record(&policy_req, &decision);
    decision
}

pub async fn handle_tool_call_with_attributes(
    storage: &dyn StorageBackend,
    principal: &flowr_policy::Principal,
    name: &str,
    arguments: Option<Value>,
    attributes: serde_json::Map<String, Value>,
) -> CallToolResult {
    let args = arguments.unwrap_or(json!({}));

    debug!(tool = %name, args = %args, "Handling tool call");

    // ── E0.2 policy choke point (site A: MCP tool dispatch) ──
    // Every MCP tool call passes here. In CE the hook is AllowAll (one virtual
    // call); EE swaps in CedarHook via `security.policy_hook`.
    if let Some(denied) = authorize_tool_call(principal, name, &args, attributes).await {
        return denied;
    }

    dispatch_tool_call(storage, principal, name, Some(args)).await
}

/// Run a tool that has ALREADY been authorized.
///
/// Split out because the gate must happen exactly once. The agent loop
/// authorizes in `call_mcp_tool` — which is where the interactive `Ask` is
/// resolved, because only that layer knows the session and can put the question
/// to a human — and then dispatched through `handle_tool_call_with_attributes`,
/// which gated a SECOND time. That second check has no session context, so it
/// re-asked the policy, got `Ask` back, and failed closed: the user granted the
/// permission, the grant was recorded, and the call was still refused.
///
/// Callers that have not authorized must use `handle_tool_call_with_attributes`.
pub async fn dispatch_tool_call(
    storage: &dyn StorageBackend,
    principal: &flowr_policy::Principal,
    name: &str,
    arguments: Option<Value>,
) -> CallToolResult {
    let args = arguments.unwrap_or(json!({}));

    match name {
        // Task documentation
        "list_task_types" => list_task_types(&args).await,
        "get_task_type" => get_task_type(&args).await,

        // Flow CRUD
        "list_flows" => list_flows(storage, &args).await,
        "list_projects" => list_projects(storage).await,
        "get_flow" => get_flow(storage, &args).await,
        "propose_flow" => propose_flow(&args),
        "propose_edit" => propose_edit(&args),
        "create_flow" => create_flow(storage, principal, &args).await,
        "update_flow" => update_flow(storage, &args).await,
        "delete_flow" => delete_flow(storage, &args).await,

        // Execution
        "execute_flow" => execute_flow(storage, principal, &args).await,
        "get_execution_status" => get_execution_status(storage, &args).await,
        "list_executions" => list_executions(storage, &args).await,

        // State
        "get_state" => get_state(storage, &args).await,
        "set_state" => set_state(storage, &args).await,

        // Resources
        "list_resources" => list_resources(storage, &args).await,
        "get_resource" => get_resource(storage, &args).await,
        "list_mcp_tools" => list_mcp_tools(storage, &args).await,
        "create_resource" => create_resource(storage, &args).await,
        "delete_resource" => delete_resource(storage, &args).await,
        "list_resource_types" => list_resource_types(storage, &args).await,

        // Triggers
        "list_triggers" => list_triggers(storage, &args).await,
        "get_trigger" => get_trigger(storage, &args).await,
        "create_trigger" => create_trigger(storage, &args).await,
        "update_trigger" => update_trigger(storage, &args).await,
        "delete_trigger" => delete_trigger(storage, &args).await,
        "enable_trigger" => enable_trigger(storage, &args).await,
        "pause_trigger" => pause_trigger(storage, &args).await,
        "attach_trigger" => attach_trigger(storage, &args).await,
        "detach_trigger" => detach_trigger(storage, &args).await,

        // Agent tools — state-based operations (no flowr-ce-agents dependency)
        "get_agent_history" => get_agent_history(storage, &args).await,
        "clear_agent_session" => clear_agent_session(storage, &args).await,

        // Agent tools that need flowr-ce-agents (template manager, execution)
        // are handled in the flowr-ce layer before falling through here
        n if AGENT_RUNTIME_TOOLS.contains(&n) => {
            // Reached only when the caller did not route through the CE layer
            // first. `is_agent_runtime_tool` lets callers check up front.
            CallToolResult::error(format!(
                "Tool '{}' requires the agent runtime. Ensure you're using the ClawdFlow server.", name
            ))
        }

        _ => CallToolResult::error(format!("Unknown tool: {}", name)),
    }
}

// ============================================================================
// Task Documentation Tools
// ============================================================================

async fn list_task_types(args: &Value) -> CallToolResult {
    let category = args.get("category").and_then(|v| v.as_str());

    let all_tasks = vec![
        // Flow Control
        ("set", "flow_control", "Assign data to context variables"),
        ("do", "flow_control", "Execute sequential subtasks"),
        ("switch", "flow_control", "Conditional branching"),
        ("for", "flow_control", "Iterate over a collection"),
        ("fork", "flow_control", "Execute branches in parallel"),
        ("try", "flow_control", "Error handling with retry"),
        ("raise", "flow_control", "Throw an error"),
        ("wait", "flow_control", "Timer-based pause"),
        // I/O
        ("call:http", "io", "Make HTTP requests"),
        ("input:file", "io", "Read from file"),
        ("output:file", "io", "Write to file"),
        ("state:get", "io", "Read flow state"),
        ("state:set", "io", "Write flow state"),
        // AI
        ("call:llm", "ai", "Call LLM provider (OpenAI, Anthropic, etc.)"),
        ("call:agent", "ai", "Execute AI agent with tools"),
        ("call:human", "ai", "Human-in-the-loop approval"),
        // Script
        ("run:shell", "script", "Execute shell command"),
        ("run:script", "script", "Execute script file"),
        ("run:workflow", "script", "Execute nested workflow"),
        ("inline:lua", "script", "Embedded Lua script (~20μs)"),
        ("inline:js", "script", "Embedded JavaScript (~100μs)"),
        // Integration
        ("call:sql", "integration", "SQL database query"),
        ("call:lancedb", "integration", "LanceDB vector database operations"),
        ("call:qdrant", "integration", "Vector database operations"),
        ("call:adb", "integration", "Agent memory on ADB (AQL)"),
        ("call:mcp", "integration", "Call MCP server tool"),
    ];

    let filtered: Vec<_> = all_tasks
        .iter()
        .filter(|(_, cat, _)| category.map_or(true, |c| c == *cat))
        .map(|(name, cat, desc)| {
            json!({
                "name": name,
                "category": cat,
                "description": desc
            })
        })
        .collect();

    CallToolResult::json(json!({
        "task_types": filtered,
        "count": filtered.len()
    }))
}

async fn get_task_type(args: &Value) -> CallToolResult {
    let task_type = match args.get("task_type").and_then(|v| v.as_str()) {
        Some(t) => t,
        None => return CallToolResult::error("task_type is required".to_string()),
    };

    let info = match task_type {
        "call:http" => json!({
            "name": "call:http",
            "description": "Make HTTP requests to external APIs",
            "config": {
                "method": "HTTP method (GET, POST, PUT, DELETE, etc.)",
                "endpoint": { "uri": "Target URL (supports expressions)" },
                "headers": "Optional headers object",
                "body": "Optional request body",
                "query": "Optional query parameters object",
                "timeout": "Optional timeout in seconds (default: 30)"
            },
            "example": {
                "name": "fetch-user",
                "type": "call:http",
                "config": {
                    "method": "GET",
                    "endpoint": { "uri": "https://api.example.com/users/${ .user_id }" }
                }
            }
        }),
        "set" => json!({
            "name": "set",
            "description": "Assign values to context variables",
            "config": {
                "key": "Variable name or path",
                "value": "Value to assign (supports expressions)"
            },
            "example": {
                "name": "set-counter",
                "set": {
                    "counter": "${ .counter + 1 }",
                    "timestamp": "${ now() }"
                }
            }
        }),
        "call:llm" => json!({
            "name": "call:llm",
            "description": "Call LLM provider for text generation",
            "config": {
                "provider": "LLM provider (openai, anthropic, ollama)",
                "model": "Model name",
                "messages": "Array of message objects",
                "temperature": "Optional temperature (0-2)",
                "max_tokens": "Optional max tokens"
            },
            "example": {
                "name": "generate-response",
                "call": "llm",
                "resource": "openai",
                "config": {
                    "model": "gpt-4",
                    "messages": [
                        {"role": "user", "content": "${ .prompt }"}
                    ]
                }
            }
        }),
        "for" => json!({
            "name": "for",
            "description": "Iterate over a collection",
            "config": {
                "each": "Variable name for current item",
                "in": "Collection expression",
                "at": "Optional index variable name",
                "do": "Tasks to execute for each item"
            },
            "example": {
                "name": "process-items",
                "for": {
                    "each": "item",
                    "in": "${ .input.items }",
                    "do": [
                        {"name": "process", "call": "http", "config": {"url": "${ .item.url }"}}
                    ]
                }
            }
        }),
        "switch" => json!({
            "name": "switch",
            "description": "Conditional branching based on expressions",
            "config": {
                "cases": "Array of {when, then} objects",
                "default": "Optional default tasks"
            },
            "example": {
                "name": "route-by-type",
                "switch": [
                    {"when": "${ .type == 'urgent' }", "then": [{"name": "urgent", "set": {"priority": 1}}]},
                    {"default": [{"name": "normal", "set": {"priority": 5}}]}
                ]
            }
        }),
        "call:adb" | "call:clawdb" => json!({
            "name": "call:adb",
            "description": "Execute AQL against ADB (Agent Database) agent memory. call:clawdb is an alias.",
            "resource_config": {
                "url": "ADB server URL, e.g. http://adb:3000 (required)",
                "namespace": "Optional namespace for scoping data",
                "scope": "Optional scope: 'private' or 'shared'"
            },
            "config": {
                "query": "AQL statement (STORE, RECALL, LOOKUP, FORGET, UPDATE, LINK, …)",
                "variables": "Optional query variables object for parameterized queries",
                "_resource": "ADB resource as namespace/name"
            },
            "memory_types": ["WORKING", "EPISODIC", "PROCEDURAL", "SEMANTIC", "TOOLS"],
            "examples": {
                "resource": {
                    "name": "agent-memory",
                    "type": "adb",
                    "config": { "url": "http://adb:3000" }
                },
                "step": {
                    "recall-history": {
                        "call": "adb",
                        "with": {
                            "_resource": "a3t/agent-memory",
                            "query": "RECALL FROM EPISODIC WHERE user = {user} LIMIT 10",
                            "variables": { "user": "${ .user }" }
                        }
                    }
                }
            }
        }),
        "call:mcp" => json!({
            "name": "call:mcp",
            "description": "Call tools on MCP (Model Context Protocol) servers",
            "resource_config": {
                "transport": "Transport type: 'sse' or 'websocket' (required)",
                "url": "MCP server endpoint URL (required)",
                "timeout_ms": "Request timeout in milliseconds (default: 30000)"
            },
            "config": {
                "tool": "Tool name to call (for tool invocations)",
                "arguments": "Tool arguments object",
                "method": "Raw MCP method (e.g., 'tools/list', 'resources/list')",
                "params": "Method params (for raw method calls)"
            },
            "note": "Either 'tool' or 'method' must be specified",
            "example": {
                "name": "search-docs",
                "type": "call:mcp",
                "resource": "docs-mcp-server",
                "config": {
                    "tool": "search_documents",
                    "arguments": {
                        "query": "${ .input.question }",
                        "limit": 5
                    }
                }
            }
        }),
        "state:set" => json!({
            "name": "state:set",
            "description": "Persist a value to the flow state store",
            "config": {
                "key": "State key (required)",
                "value": "Value to store (supports expressions)",
                "ttl": "Optional TTL in seconds"
            },
            "example": {
                "name": "save-result",
                "type": "state:set",
                "config": {
                    "key": "last_processed",
                    "value": "${ .output }"
                }
            }
        }),
        "call:llm" => json!({
            "name": "call:llm",
            "description": "Call a Large Language Model for text generation",
            "resource_config": {
                "provider": "LLM provider: 'anthropic', 'openai', or 'ollama' (required)",
                "api_key": "API key for the provider (required)",
                "model": "Default model to use (e.g., 'claude-sonnet-4-20250514')",
                "api_url": "Custom API endpoint URL (optional)",
                "system_prompt": "Default system prompt (optional)",
                "max_tokens": "Default max tokens (default: 4096)",
                "temperature": "Default temperature (default: 0.7)"
            },
            "config": {
                "prompt": "User prompt / instruction (required)",
                "system_prompt": "Override system prompt",
                "model": "Override model",
                "temperature": "Override temperature",
                "max_tokens": "Override max tokens",
                "output_schema": "Expected JSON schema for structured output",
                "messages": "Additional conversation history"
            },
            "example": {
                "name": "analyze-data",
                "type": "call:llm",
                "resource": "claude-sonnet",
                "config": {
                    "prompt": "Analyze this data: ${ .input | tojson }",
                    "output_schema": {
                        "type": "object",
                        "properties": {
                            "summary": { "type": "string" },
                            "risk_level": { "type": "string" }
                        }
                    }
                }
            }
        }),
        "call:agent" => json!({
            "name": "call:agent",
            "description": "Execute a full AI agent with memory, tools, and learning capabilities",
            "resource_config": {
                "llm": "Name of LLM resource to use for reasoning (required)",
                "memory": "Name of ClawDB resource for agent memory (optional)",
                "namespace": "Default namespace for memory operations (default: 'default')",
                "system_prompt": "Default system prompt for the agent",
                "tools": "Array of tool definitions (name, description, input_schema)",
                "mcp_servers": "Array of MCP server configs to load tools from"
            },
            "config": {
                "prompt": "Task instruction for the agent",
                "system_prompt": "Override system prompt",
                "context_pipeline": "AQL pipeline for context assembly",
                "working_input": "Data to store in WORKING memory",
                "decision_schema": "Expected JSON schema for agent decision",
                "on_complete": "AQL to run after decision (store outcome)",
                "on_success": "AQL to run on success (build ontology)",
                "on_failure": "AQL to run on failure",
                "max_iterations": "Max agent loop iterations (default: 10)",
                "timeout_ms": "Timeout in milliseconds (default: 30000)",
                "allow_tools": "Enable tool calling (default: true)",
                "context": "Additional context to inject"
            },
            "features": [
                "Multi-turn conversation with tool use",
                "Memory via ClawDB (WORKING, EPISODIC, PROCEDURAL, SEMANTIC, TOOLS)",
                "MCP server tool integration",
                "AQL pipelines for context assembly",
                "Ontology building via LINK statements"
            ],
            "example": {
                "resource": {
                    "name": "incident-agent",
                    "type": "agent",
                    "config": {
                        "llm": "claude-sonnet",
                        "memory": "agent-memory",
                        "mcp_servers": [
                            { "url": "http://localhost:3001" }
                        ]
                    }
                },
                "step": {
                    "name": "analyze-incident",
                    "type": "call:agent",
                    "resource": "incident-agent",
                    "config": {
                        "prompt": "Analyze this incident and recommend actions",
                        "decision_schema": {
                            "type": "object",
                            "properties": {
                                "action": { "type": "string" },
                                "confidence": { "type": "number" }
                            }
                        }
                    }
                }
            }
        }),
        "state:get" => json!({
            "name": "state:get",
            "description": "Retrieve a value from the flow state store",
            "config": {
                "key": "State key to retrieve (required)"
            },
            "example": {
                "name": "load-cache",
                "type": "state:get",
                "config": {
                    "key": "cached_data"
                }
            }
        }),
        _ => json!({
            "name": task_type,
            "description": "Task type documentation not available",
            "note": "Use list_task_types to see available task types"
        }),
    };

    CallToolResult::json(info)
}

// ============================================================================
// Flow CRUD Tools
// ============================================================================

async fn list_flows(storage: &dyn StorageBackend, args: &Value) -> CallToolResult {
    let namespace = args.get("namespace").and_then(|v| v.as_str()).unwrap_or("default");

    match storage.workflows().list(namespace).await {
        Ok(workflows) => {
            if workflows.is_empty() {
                return CallToolResult::text("No workflows found.".to_string());
            }

            let summaries: Vec<_> = workflows
                .iter()
                .map(|w| {
                    json!({
                        "namespace": w.namespace,
                        "name": w.name,
                        "version": w.version,
                        "enabled": w.enabled,
                        "step_count": w.step_count
                    })
                })
                .collect();

            CallToolResult::json(json!({
                "workflows": summaries,
                "count": summaries.len()
            }))
        }
        Err(e) => CallToolResult::error(format!("Failed to list workflows: {}", e)),
    }
}

/// Namespaces that belong to the platform rather than to a user's project.
pub const PLATFORM_NAMESPACES: &[&str] = &["a3t", "system"];

/// Every namespace in the registry, as a project: the union of the namespaces
/// holding flows, resources and triggers, plus `default`, with each one's counts.
async fn list_projects(storage: &dyn StorageBackend) -> CallToolResult {
    let mut names: std::collections::BTreeSet<String> = ["default".to_string()].into();
    for ns in PLATFORM_NAMESPACES { names.insert(ns.to_string()); }
    match storage.workflows().namespaces().await {
        Ok(n) => names.extend(n),
        Err(e) => return CallToolResult::error(format!("Failed to list namespaces: {e}")),
    }
    match storage.resources().namespaces().await {
        Ok(n) => names.extend(n),
        Err(e) => return CallToolResult::error(format!("Failed to list namespaces: {e}")),
    }
    let triggers = storage.triggers().list(None).await.unwrap_or_default();
    names.extend(triggers.iter().filter_map(|t| t.get("namespace").and_then(|v| v.as_str())).map(String::from));

    let mut projects = Vec::new();
    for ns in names.iter().filter(|n| !n.is_empty()) {
        let flows = storage.workflows().list(ns).await.map(|f| f.len()).unwrap_or(0);
        let resources = storage.resources().list(ns).await.map(|r| r.len()).unwrap_or(0);
        let trigger_count = triggers.iter().filter(|t| t.get("namespace").and_then(|v| v.as_str()) == Some(ns)).count();
        projects.push(json!({
            "name": ns,
            "platform": PLATFORM_NAMESPACES.contains(&ns.as_str()),
            "flows": flows,
            "resources": resources,
            "triggers": trigger_count,
        }));
    }
    CallToolResult::json(json!({"count": projects.len(), "projects": projects}))
}

async fn get_flow(storage: &dyn StorageBackend, args: &Value) -> CallToolResult {
    let namespace = args.get("namespace").and_then(|v| v.as_str()).unwrap_or("default");
    let name = match args.get("name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => return CallToolResult::error("name is required".to_string()),
    };
    let version = args.get("version").and_then(|v| v.as_str());

    let result = if let Some(v) = version {
        storage.workflows().get_version(namespace, name, v).await
    } else {
        storage.workflows().get(namespace, name).await
    };

    match result {
        Ok(record) => {
            // Return the raw CNCF YAML directly
            CallToolResult::text(format!(
                "# Workflow: {}/{}\n\n```yaml\n{}\n```",
                namespace, name, record.raw_yaml
            ))
        }
        Err(e) => CallToolResult::error(format!("Failed to get workflow: {}", e)),
    }
}


/// Validate a workflow and hand it back for review. Deliberately has no storage
/// access: proposing is a read-only act, so a user who may not write can still
/// draft, and nothing reaches the registry until they press Save (which goes
/// through `create_flow` and its Cedar gate).
/// A designer edit: checked for syntax only (the designer knows its own schema
/// and shows problems as the user reviews), never written anywhere.
fn propose_edit(args: &Value) -> CallToolResult {
    let Some(doc) = args.get("yaml").and_then(|v| v.as_str()) else {
        return CallToolResult::error("yaml is required".to_string());
    };
    match serde_yaml::from_str::<serde_yaml::Value>(doc) {
        Ok(_) => CallToolResult::json(json!({
            "applied_to_editor": true,
            "saved": false,
            "note": "Applied to the editor on screen, NOT saved. The user reviews it and presses Save. Do not claim it is saved.",
        })),
        Err(e) => CallToolResult::json(json!({
            "applied_to_editor": false,
            "error": format!("Not valid YAML/JSON: {e}. Send the complete corrected document."),
        })),
    }
}

fn propose_flow(args: &Value) -> CallToolResult {
    let Some(yaml_str) = args.get("yaml").and_then(|v| v.as_str()) else {
        return CallToolResult::error("yaml is required".to_string());
    };
    match WorkflowRecord::from_yaml(yaml_str) {
        Ok(record) => CallToolResult::json(json!({
            "valid": true,
            "namespace": record.namespace(),
            "name": record.name(),
            "version": record.version(),
            "saved": false,
            "note": "Draft shown to the user for review. It is NOT saved — do not claim otherwise. The user saves it from the editor.",
        })),
        // A validation failure is useful output, not a tool error. It must also
        // be *actionable*: models reliably reach for Serverless Workflow 0.8
        // (`states` / `start_at` / `transition`), and a bare "missing field `do`"
        // sends them round the loop guessing. Returning the target shape turns
        // that into a single correction.
        Err(e) => CallToolResult::json(json!({
            "valid": false,
            "saved": false,
            "error": format!("Invalid CNCF SWF workflow: {}", e),
            "hint": "This is CNCF Serverless Workflow DSL 1.0.3, not 0.8. There is no `states`, `start_at`, `transition` or `next`. A workflow is a `document` block plus a `do` list, where each entry is a single-key map of taskName -> task.",
            "expected_shape": "document:\n  dsl: '1.0.3'\n  namespace: default\n  name: my-flow\n  version: '1.0.0'\ndo:\n  - firstStep:\n      set:\n        greeting: hello\n  - callSomething:\n      call: http\n      with:\n        method: GET\n        endpoint:\n          uri: https://example.com\n",
        })),
    }
}

async fn create_flow(
    storage: &dyn StorageBackend,
    principal: &flowr_policy::Principal,
    args: &Value,
) -> CallToolResult {
    // Accept YAML string or JSON object
    let yaml_str = if let Some(y) = args.get("yaml").and_then(|v| v.as_str()) {
        y.to_string()
    } else if let Some(json_obj) = args.get("json") {
        // Convert JSON to YAML for storage
        match serde_yaml::to_string(json_obj) {
            Ok(y) => y,
            Err(e) => return CallToolResult::error(format!("Invalid JSON: {}", e)),
        }
    } else {
        return CallToolResult::error("Either 'yaml' or 'json' is required".to_string());
    };

    // Parse with swf-parser (CNCF DSL 1.0.3). Ownership is stamped from the
    // authenticated caller, never from the payload — a flow that could name its
    // own owner could name anyone's authority as the bound on its grants.
    let mut record = match WorkflowRecord::from_yaml(&yaml_str) {
        Ok(r) => r,
        Err(e) => return CallToolResult::error(format!("Invalid CNCF SWF workflow: {}", e)),
    };
    // Whoever created the flow stays its owner across re-saves; only a genuinely
    // new flow takes the current caller. Otherwise anyone able to save could take
    // ownership, and with it the authority that bounds the flow's agent grants.
    let existing_owner = storage
        .workflows()
        .get(record.namespace(), record.name())
        .await
        .ok()
        .and_then(|r| r.metadata.owner.clone());
    record.metadata.owner = existing_owner.or_else(|| Some(principal.id().to_string()));

    let namespace = record.namespace().to_string();
    let name = record.name().to_string();
    let version = record.version().to_string();

    match storage.workflows().register(record).await {
        Ok(_) => {
            info!(namespace = %namespace, name = %name, version = %version, "Workflow created");
            CallToolResult::text(format!(
                "Workflow created successfully!\n\n- Namespace: {}\n- Name: {}\n- Version: {}\n\nUse `execute_flow` to run it.",
                namespace, name, version
            ))
        }
        Err(e) => CallToolResult::error(format!("Failed to create workflow: {}", e)),
    }
}

async fn update_flow(storage: &dyn StorageBackend, args: &Value) -> CallToolResult {
    let namespace = args.get("namespace").and_then(|v| v.as_str()).unwrap_or("default");
    let name = match args.get("name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => return CallToolResult::error("name is required".to_string()),
    };

    let enabled = args.get("enabled").and_then(|v| v.as_bool());
    let version = args.get("version").and_then(|v| v.as_str());

    if enabled.is_none() && version.is_none() {
        return CallToolResult::error(
            "At least one of 'enabled' or 'version' is required".to_string(),
        );
    }

    // Update enabled status
    if let Some(en) = enabled {
        if let Err(e) = storage.workflows().set_enabled(namespace, name, en).await {
            return CallToolResult::error(format!("Failed to update enabled: {}", e));
        }
    }

    // Update active version
    if let Some(v) = version {
        if let Err(e) = storage.workflows().set_active_version(namespace, name, v).await {
            return CallToolResult::error(format!("Failed to update version: {}", e));
        }
    }

    CallToolResult::text(format!("Workflow {}/{} updated successfully.", namespace, name))
}

async fn delete_flow(storage: &dyn StorageBackend, args: &Value) -> CallToolResult {
    let namespace = args.get("namespace").and_then(|v| v.as_str()).unwrap_or("default");
    let name = match args.get("name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => return CallToolResult::error("name is required".to_string()),
    };

    if let Err(why) = crate::check_delete("flow", namespace, name).await {
        return CallToolResult::error(why);
    }
    match storage.workflows().delete(namespace, name).await {
        Ok(_) => CallToolResult::text(format!("Deleted workflow {}/{}", namespace, name)),
        Err(e) => CallToolResult::error(format!("Failed to delete: {}", e)),
    }
}

// ============================================================================
// Execution Tools
// ============================================================================

async fn execute_flow(
    storage: &dyn StorageBackend,
    principal: &flowr_policy::Principal,
    args: &Value,
) -> CallToolResult {
    let namespace = args.get("namespace").and_then(|v| v.as_str()).unwrap_or("default");
    let name = match args.get("name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => return CallToolResult::error("name is required".to_string()),
    };
    let input = args.get("input").cloned().unwrap_or(json!({}));

    // Verify workflow exists
    if let Err(e) = storage.workflows().get(namespace, name).await {
        return CallToolResult::error(format!("Workflow not found: {}", e));
    }

    // Keystone: the enqueued flow inherits the caller's identity, so the worker
    // runs its root `ExecutionContext` as this principal (not the service default)
    // and every step is gated as whoever started the flow — an API user, or an
    // agent's LLM calling `execute_flow` in-process as its Agent principal.
    let request = ExecutionRequest::new(namespace, name)
        .with_input(input)
        .with_principal(principal.clone());
    let instance_id = request.instance_id;

    // Enqueue execution request
    if let Err(e) = storage.queue().enqueue(request).await {
        return CallToolResult::error(format!("Failed to start execution: {}", e));
    }
    info!(instance_id = %instance_id, workflow = %format!("{}/{}", namespace, name), "Execution queued");

    // Wait for a terminal state and return the actual result. This tool's
    // contract is "Execute a workflow and return the result" — handing back only
    // an id forced every caller into a poll loop, and an agent's LLM burned a
    // reasoning iteration per poll (or simply never looked). The instance id
    // stays in the text on every path so existing pollers keep working.
    let wait_secs: u64 = std::env::var("FLOWR_MCP_EXEC_WAIT_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(120);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(wait_secs);
    // Short first checks, capped low: list flows finish in milliseconds, and a
    // 1 s cap meant a flow done at 1.6 s was reported at 2.5 s. Each check is one
    // execution-row read, which is cheap once the table is compacted.
    let mut delay = std::time::Duration::from_millis(25);
    loop {
        if let Ok(Some(state)) = storage.executions().get(instance_id).await {
            match state.status {
                ExecutionStatus::Completed => {
                    let output = state.output.unwrap_or(Value::Null);
                    let pretty = serde_json::to_string_pretty(&output)
                        .unwrap_or_else(|_| output.to_string());
                    return CallToolResult::text(format!(
                        "Workflow execution completed.\n\n- Instance ID: `{}`\n- Workflow: {}/{}\n- Status: completed\n\nOutput:\n{}",
                        instance_id, namespace, name, pretty
                    ));
                }
                ExecutionStatus::Failed | ExecutionStatus::Cancelled => {
                    let status = if matches!(state.status, ExecutionStatus::Failed) { "failed" } else { "cancelled" };
                    return CallToolResult::error(format!(
                        "Workflow execution {}.\n\n- Instance ID: `{}`\n- Workflow: {}/{}\n- Error: {}",
                        status, instance_id, namespace, name,
                        state.error.unwrap_or_else(|| "no error detail recorded".to_string())
                    ));
                }
                // Queued / Running / Suspended — keep waiting.
                _ => {}
            }
        }
        if std::time::Instant::now() >= deadline {
            // Not a failure: a long-running or suspended flow. Report where it is
            // so the caller can follow up instead of assuming it broke.
            return CallToolResult::text(format!(
                "Workflow execution started and still running after {}s.\n\n- Instance ID: `{}`\n- Workflow: {}/{}\n- Status: running\n\nUse `get_execution_status` with this instance id to retrieve the result.",
                wait_secs, instance_id, namespace, name
            ));
        }
        tokio::time::sleep(delay).await;
        delay = std::cmp::min(delay * 2, std::time::Duration::from_millis(250));
    }
}

async fn get_execution_status(storage: &dyn StorageBackend, args: &Value) -> CallToolResult {
    let instance_id = match args.get("instance_id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return CallToolResult::error("instance_id is required".to_string()),
    };

    let uuid = match Uuid::parse_str(instance_id) {
        Ok(u) => u,
        Err(_) => return CallToolResult::error("Invalid instance_id format".to_string()),
    };

    match storage.executions().get(uuid).await {
        Ok(Some(state)) => {
            CallToolResult::json(json!({
                "instance_id": instance_id,
                "namespace": state.namespace,
                "name": state.name,
                "status": format!("{:?}", state.status),
                "started_at": state.started_at,
                "completed_at": state.completed_at,
                "current_step": state.current_step,
                "duration_us": state.duration_us,
                // The input was stored with every run but never returned, so no
                // view could show what a run was actually given.
                "input": state.input,
                "output": state.output,
                "error": state.error
            }))
        }
        Ok(None) => {
            CallToolResult::text(format!(
                "Execution `{}` not found or still pending.",
                instance_id
            ))
        }
        Err(e) => CallToolResult::error(format!("Failed to get status: {}", e)),
    }
}

async fn list_executions(storage: &dyn StorageBackend, args: &Value) -> CallToolResult {
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
    let namespace = args.get("namespace").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
    let name = args.get("name").and_then(|v| v.as_str()).filter(|s| !s.is_empty());

    // The storage layer applies `limit` with no ordering (oldest-first), which would
    // hide the most recent runs. Over-fetch, sort newest-first, then truncate.
    let fetch = limit.max(500);
    let result = match (namespace, name) {
        (Some(ns), Some(nm)) => storage.executions().list_by_workflow(ns, nm, fetch).await,
        _ => storage.executions().list_recent(fetch).await,
    };

    match result {
        Ok(mut execs) => {
            execs.sort_by(|a, b| b.started_at.cmp(&a.started_at));
            execs.truncate(limit);
            let rows: Vec<Value> = execs.iter().map(|s| json!({
                "instance_id": s.instance_id,
                "namespace": s.namespace,
                "name": s.name,
                "version": s.version,
                "status": format!("{:?}", s.status),
                "started_at": s.started_at,
                "completed_at": s.completed_at,
                "duration_us": s.duration_us,
                "current_step": s.current_step,
                "input": s.input,
                "output": s.output,
                "error": s.error,
                // Who started the run. Persisted on ExecutionState but previously
                // not surfaced here, which made the execution history unable to
                // answer "who ran this" even once it was being recorded.
                // `null` on any run from before the field existed — absence means
                // "not recorded", never "nobody".
                "principal": s.principal,
                "principal_id": s.principal.as_ref().map(|p| p.id()),
            })).collect();
            CallToolResult::json(json!(rows))
        }
        Err(e) => CallToolResult::error(format!("Failed to list executions: {}", e)),
    }
}

// ============================================================================
// State Tools
// ============================================================================

async fn get_state(storage: &dyn StorageBackend, args: &Value) -> CallToolResult {
    let key = match args.get("key").and_then(|v| v.as_str()) {
        Some(k) => k,
        None => return CallToolResult::error("key is required".to_string()),
    };

    match storage.state().get(key).await {
        Ok(Some(value)) => CallToolResult::json(json!({
            "key": key,
            "value": value
        })),
        Ok(None) => CallToolResult::json(json!({
            "key": key,
            "value": null,
            "found": false
        })),
        Err(e) => CallToolResult::error(format!("Failed to get state: {}", e)),
    }
}

async fn set_state(storage: &dyn StorageBackend, args: &Value) -> CallToolResult {
    let key = match args.get("key").and_then(|v| v.as_str()) {
        Some(k) => k,
        None => return CallToolResult::error("key is required".to_string()),
    };

    let value = match args.get("value") {
        Some(v) => v.clone(),
        None => return CallToolResult::error("value is required".to_string()),
    };

    let ttl = args
        .get("ttl_secs")
        .and_then(|v| v.as_u64())
        .map(|secs| std::time::Duration::from_secs(secs));

    match storage.state().set(key, value.clone(), ttl).await {
        Ok(_) => CallToolResult::json(json!({
            "key": key,
            "status": "set"
        })),
        Err(e) => CallToolResult::error(format!("Failed to set state: {}", e)),
    }
}

// ============================================================================
// Resource Tools
// ============================================================================

async fn list_resources(storage: &dyn StorageBackend, args: &Value) -> CallToolResult {
    let namespace = args.get("namespace").and_then(|v| v.as_str()).unwrap_or("default");
    let type_filter = args.get("type_ref").and_then(|v| v.as_str());

    match storage.resources().list(namespace).await {
        Ok(resources) => {
            let filtered: Vec<_> = resources
                .iter()
                .filter(|r| type_filter.map_or(true, |t| r.type_ref == t))
                .map(|r| {
                    json!({
                        "name": r.name,
                        "namespace": r.namespace,
                        "type": r.type_ref,
                        "description": r.description,
                        "tags": r.tags,
                        "created_at": r.created_at.to_rfc3339()
                    })
                })
                .collect();

            if filtered.is_empty() {
                return CallToolResult::text("No resources found.".to_string());
            }

            CallToolResult::json(json!({
                "resources": filtered,
                "count": filtered.len()
            }))
        }
        Err(e) => CallToolResult::error(format!("Failed to list resources: {}", e)),
    }
}

async fn get_resource(storage: &dyn StorageBackend, args: &Value) -> CallToolResult {
    let namespace = args.get("namespace").and_then(|v| v.as_str()).unwrap_or("default");
    let name = match args.get("name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => return CallToolResult::error("name is required".to_string()),
    };

    match storage.resources().get(namespace, name).await {
        Ok(resource) => {
            // Mask sensitive config fields
            let mut config = resource.config.clone();
            mask_sensitive_fields(&mut config);

            CallToolResult::json(json!({
                "name": resource.name,
                "namespace": resource.namespace,
                "type": resource.type_ref,
                "config": config,
                "description": resource.description,
                "tags": resource.tags,
                "created_at": resource.created_at.to_rfc3339(),
                "updated_at": resource.updated_at.to_rfc3339()
            }))
        }
        Err(e) => CallToolResult::error(format!("Resource not found: {}", e)),
    }
}

/// List the tool names an MCP server resource exposes, so the UI can offer a
/// capability ceiling scoped to that server (not a static catalog). FlowR's own
/// MCP surface (loopback URL) is resolved in-process via `get_tools()`.
static SELF_PORT: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(3002);

/// The port this FlowR serves MCP on (`--port`); servers call this at startup.
pub fn set_self_port(port: u16) {
    SELF_PORT.store(port, std::sync::atomic::Ordering::Relaxed);
}

/// Whether an MCP URL points back at this FlowR, so its tools are served in-process.
///
/// Host must be a loopback name or `flowr` (the service name) **and** the port this
/// FlowR listens on. Matching on the host alone treated every MCP server on the same
/// machine (e.g. ADB on 127.0.0.1:3000 in local dev) as FlowR itself, so its tools
/// were silently replaced by FlowR's. An empty URL also means this FlowR.
pub fn is_self_mcp_url(url: &str) -> bool {
    let url = url.trim();
    if url.is_empty() {
        return true;
    }
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let authority = rest.split(['/', '?']).next().unwrap_or("");
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) if !h.ends_with(']') || authority.starts_with('[') => (h, p.parse::<u16>().ok()),
        _ => (authority, None),
    };
    let port = port.unwrap_or(if url.starts_with("https://") { 443 } else { 80 });
    let host = host.to_ascii_lowercase();
    let selfish = matches!(host.as_str(), "localhost" | "127.0.0.1" | "0.0.0.0" | "[::1]")
        || host == "flowr"
        || host.starts_with("flowr.");
    selfish && port == SELF_PORT.load(std::sync::atomic::Ordering::Relaxed)
}

/// Where an MCP server takes JSON-RPC POSTs. Resources often store the SSE URL
/// (`…/sse`, as A3T's defaults do); its POST endpoint is the sibling `…/message`.
/// Any other URL gets `/message` appended unless it already ends with it — the rule
/// `call:mcp` has always used, now shared by the agent runtime and discovery.
pub fn mcp_message_url(url: &str) -> String {
    let u = url.trim_end_matches('/');
    if let Some(base) = u.strip_suffix("/sse") {
        format!("{base}/message")
    } else if u.ends_with("/message") {
        u.to_string()
    } else {
        format!("{u}/message")
    }
}

/// `Authorization` for a request to `url` when it is an ADB that needs a token (ADB
/// W3): `Bearer $ADB_AUTH_TOKEN`, but only for URLs starting with one of the
/// comma-separated prefixes in `ADB_AUTH_URLS`, so the token never reaches any
/// other server. The token lives in FlowR's environment (a Secret), not in the
/// registry.
pub fn adb_auth_header(url: &str) -> Option<String> {
    let token = std::env::var("ADB_AUTH_TOKEN").ok().filter(|t| !t.is_empty())?;
    let prefixes = std::env::var("ADB_AUTH_URLS").ok()?;
    prefixes
        .split(',')
        .map(|p| p.trim().trim_end_matches('/'))
        .any(|p| !p.is_empty() && (url == p || url.starts_with(&format!("{p}/"))))
        .then(|| format!("Bearer {token}"))
}

/// `req` to `url`, with [`adb_auth_header`] when it applies.
pub fn with_adb_auth(req: reqwest::RequestBuilder, url: &str) -> reqwest::RequestBuilder {
    match adb_auth_header(url) {
        Some(h) => req.header(reqwest::header::AUTHORIZATION, h),
        None => req,
    }
}

/// `tools/list` on a remote MCP server: the tool names, or why they couldn't be read.
async fn remote_tool_names(url: &str) -> Result<Vec<String>, String> {
    let endpoint = mcp_message_url(url);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = with_adb_auth(client.post(&endpoint), &endpoint)
        .json(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}))
        .send()
        .await
        .map_err(|e| format!("{endpoint}: {e}"))?;
    let status = resp.status();
    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("{endpoint}: HTTP {status}, not a JSON-RPC response ({e})"))?;
    if let Some(err) = body.get("error") {
        return Err(format!("{endpoint}: {err}"));
    }
    Ok(body
        .pointer("/result/tools")
        .and_then(|t| t.as_array())
        .map(|tools| {
            tools
                .iter()
                .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default())
}

async fn list_mcp_tools(storage: &dyn StorageBackend, args: &Value) -> CallToolResult {
    let namespace = args.get("namespace").and_then(|v| v.as_str()).unwrap_or("a3t");
    // Discovering what exists is far more useful than an error telling the caller
    // to create something — a model that guesses a server name (or omits one)
    // should be steered to the real resources, not into creating a duplicate.
    async fn mcp_servers(storage: &dyn StorageBackend, namespace: &str) -> Vec<String> {
        storage
            .resources()
            .list(namespace)
            .await
            .map(|rs| {
                rs.into_iter()
                    .filter(|r| r.type_ref == "mcp")
                    .map(|r| format!("{}/{}", r.namespace, r.name))
                    .collect()
            })
            .unwrap_or_default()
    }

    let Some(name) = args.get("name").and_then(|v| v.as_str()) else {
        let servers = mcp_servers(storage, namespace).await;
        return CallToolResult::json(json!({
            "namespace": namespace,
            "mcp_servers": servers,
            "hint": "call list_mcp_tools again with one of these as `name`",
        }));
    };
    let resource = match storage.resources().get(namespace, name).await {
        Ok(r) => r,
        Err(_) => {
            let servers = mcp_servers(storage, namespace).await;
            return CallToolResult::json(json!({
                "error": format!("no MCP server resource '{}/{}'", namespace, name),
                "mcp_servers": servers,
                "hint": "use one of `mcp_servers`; do not create a new resource unless none fit",
            }));
        }
    };
    let url = resource
        .config
        .get("url")
        .or_else(|| resource.config.get("endpoint"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if is_self_mcp_url(url) {
        // FlowR's own MCP surface — the tools this process serves.
        let names: Vec<String> = get_tools().into_iter().map(|t| t.name).collect();
        CallToolResult::json(json!({
            "server": format!("{}/{}", namespace, name),
            "transport": "in-process",
            "tools": names,
        }))
    } else {
        // Remote MCP servers: ask the server itself (tools/list over JSON-RPC POST).
        // An OAuth-protected or unreachable server reports why, instead of an empty
        // list that reads as "this server has no tools".
        let server = format!("{}/{}", namespace, name);
        let transport = resource.config.get("transport").and_then(|v| v.as_str()).unwrap_or("remote");
        match remote_tool_names(url).await {
            Ok(names) => CallToolResult::json(json!({
                "server": server,
                "transport": transport,
                "endpoint": mcp_message_url(url),
                "tools": names,
            })),
            Err(e) => CallToolResult::json(json!({
                "server": server,
                "transport": transport,
                "tools": [],
                "error": format!("could not list tools: {e}"),
            })),
        }
    }
}

async fn create_resource(storage: &dyn StorageBackend, args: &Value) -> CallToolResult {
    use flowr_types::Resource;

    let name = match args.get("name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => return CallToolResult::error("name is required".to_string()),
    };
    let namespace = args.get("namespace").and_then(|v| v.as_str()).unwrap_or("default");
    let type_ref = match args.get("type_ref").and_then(|v| v.as_str()) {
        Some(t) => t,
        None => return CallToolResult::error("type_ref is required".to_string()),
    };
    let config = match args.get("config") {
        Some(c) => c.clone(),
        None => return CallToolResult::error("config is required".to_string()),
    };
    let description = args.get("description").and_then(|v| v.as_str());

    let mut resource = Resource::new(name, type_ref, config)
        .with_namespace(namespace);

    if let Some(desc) = description {
        resource = resource.with_description(desc);
    }

    match storage.resources().create(resource).await {
        Ok(created) => {
            info!(namespace = %namespace, name = %name, type_ref = %type_ref, "Resource created");
            CallToolResult::text(format!(
                "Resource created successfully!\n\n- Namespace: {}\n- Name: {}\n- Type: {}\n\nUse in workflows with `resource: {}/{}`",
                namespace, name, type_ref, namespace, name
            ))
        }
        Err(e) => CallToolResult::error(format!("Failed to create resource: {}", e)),
    }
}

async fn delete_resource(storage: &dyn StorageBackend, args: &Value) -> CallToolResult {
    let namespace = args.get("namespace").and_then(|v| v.as_str()).unwrap_or("default");
    let name = match args.get("name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => return CallToolResult::error("name is required".to_string()),
    };

    if let Err(why) = crate::check_delete("resource", namespace, name).await {
        return CallToolResult::error(why);
    }
    match storage.resources().delete(namespace, name).await {
        Ok(_) => {
            info!(namespace = %namespace, name = %name, "Resource deleted");
            CallToolResult::text(format!("Deleted resource {}/{}", namespace, name))
        }
        Err(e) => CallToolResult::error(format!("Failed to delete: {}", e)),
    }
}

async fn list_resource_types(storage: &dyn StorageBackend, args: &Value) -> CallToolResult {
    let category = args.get("category").and_then(|v| v.as_str());

    match storage.resources().list_types().await {
        Ok(types) => {
            let filtered: Vec<_> = types
                .iter()
                .filter(|t| {
                    category.map_or(true, |c| {
                        format!("{:?}", t.category).to_lowercase() == c.to_lowercase()
                    })
                })
                .map(|t| {
                    json!({
                        "name": t.name,
                        "description": t.description,
                        "category": format!("{:?}", t.category).to_lowercase(),
                        "builtin": t.builtin
                    })
                })
                .collect();

            if filtered.is_empty() {
                // Return built-in types as fallback
                let builtin_types = vec![
                    ("openai", "ai", "OpenAI API (GPT models)"),
                    ("anthropic", "ai", "Anthropic API (Claude models)"),
                    ("groq", "ai", "Groq API (fast inference)"),
                    ("ollama", "ai", "Ollama local LLM server"),
                    ("postgres", "database", "PostgreSQL database"),
                    ("mysql", "database", "MySQL database"),
                    ("redis", "key_value", "Redis key-value store"),
                    ("qdrant", "vector", "Qdrant vector database"),
                    ("adb", "memory", "ADB agent memory (AQL)"),
                    ("mcp", "mcp", "MCP server connection"),
                    ("http", "api", "HTTP/REST API endpoint"),
                ];

                let fallback: Vec<_> = builtin_types
                    .iter()
                    .filter(|(_, cat, _)| category.map_or(true, |c| c == *cat))
                    .map(|(name, cat, desc)| {
                        json!({
                            "name": name,
                            "category": cat,
                            "description": desc,
                            "builtin": true
                        })
                    })
                    .collect();

                return CallToolResult::json(json!({
                    "resource_types": fallback,
                    "count": fallback.len(),
                    "note": "Showing built-in types"
                }));
            }

            CallToolResult::json(json!({
                "resource_types": filtered,
                "count": filtered.len()
            }))
        }
        Err(e) => CallToolResult::error(format!("Failed to list resource types: {}", e)),
    }
}

// ============================================================================
// Trigger Tools
// ============================================================================

async fn list_triggers(storage: &dyn StorageBackend, args: &Value) -> CallToolResult {
    let namespace = args.get("namespace").and_then(|v| v.as_str());
    let target_flow = args.get("target_flow").and_then(|v| v.as_str());

    match storage.triggers().list(namespace).await {
        Ok(triggers) => {
            // Optionally filter by target flow name
            let filtered: Vec<_> = if let Some(flow_name) = target_flow {
                triggers
                    .into_iter()
                    .filter(|t| {
                        t.get("targets")
                            .and_then(|ts| ts.as_array())
                            .map_or(false, |targets| {
                                targets.iter().any(|tgt| {
                                    tgt.get("name")
                                        .and_then(|n| n.as_str())
                                        .map_or(false, |n| n == flow_name)
                                })
                            })
                    })
                    .collect()
            } else {
                triggers
            };

            if filtered.is_empty() {
                return CallToolResult::text("No triggers found.".to_string());
            }
            CallToolResult::json(json!({
                "triggers": filtered,
                "count": filtered.len()
            }))
        }
        Err(e) => CallToolResult::error(format!("Failed to list triggers: {}", e)),
    }
}

async fn get_trigger(storage: &dyn StorageBackend, args: &Value) -> CallToolResult {
    let id = match args.get("id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return CallToolResult::error("id is required".to_string()),
    };

    match storage.triggers().get(id).await {
        Ok(Some(trigger)) => CallToolResult::json(trigger),
        Ok(None) => CallToolResult::error(format!("Trigger '{}' not found", id)),
        Err(e) => CallToolResult::error(format!("Failed to get trigger: {}", e)),
    }
}

async fn create_trigger(storage: &dyn StorageBackend, args: &Value) -> CallToolResult {
    let trigger = args.clone();

    match storage.triggers().create(trigger).await {
        Ok(id) => {
            info!(trigger_id = %id, "Trigger created via MCP");
            CallToolResult::text(format!(
                "Trigger created successfully!\n\n- ID: `{}`\n- Name: {}\n- Status: active",
                id,
                args.get("name").and_then(|v| v.as_str()).unwrap_or("unnamed")
            ))
        }
        Err(e) => CallToolResult::error(format!("Failed to create trigger: {}", e)),
    }
}

async fn update_trigger(storage: &dyn StorageBackend, args: &Value) -> CallToolResult {
    let id = match args.get("id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return CallToolResult::error("id is required".to_string()),
    };

    match storage.triggers().update(id, args.clone()).await {
        Ok(()) => CallToolResult::text(format!("Trigger `{}` updated successfully.", id)),
        Err(e) => CallToolResult::error(format!("Failed to update trigger: {}", e)),
    }
}

async fn delete_trigger(storage: &dyn StorageBackend, args: &Value) -> CallToolResult {
    let id = match args.get("id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return CallToolResult::error("id is required".to_string()),
    };

    match storage.triggers().delete(id).await {
        Ok(()) => {
            info!(trigger_id = %id, "Trigger deleted via MCP");
            CallToolResult::text(format!("Deleted trigger `{}`", id))
        }
        Err(e) => CallToolResult::error(format!("Failed to delete trigger: {}", e)),
    }
}

async fn enable_trigger(storage: &dyn StorageBackend, args: &Value) -> CallToolResult {
    let id = match args.get("id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return CallToolResult::error("id is required".to_string()),
    };

    match storage.triggers().enable(id).await {
        Ok(()) => CallToolResult::text(format!("Trigger `{}` enabled.", id)),
        Err(e) => CallToolResult::error(format!("Failed to enable trigger: {}", e)),
    }
}

async fn pause_trigger(storage: &dyn StorageBackend, args: &Value) -> CallToolResult {
    let id = match args.get("id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return CallToolResult::error("id is required".to_string()),
    };

    match storage.triggers().pause(id).await {
        Ok(()) => CallToolResult::text(format!("Trigger `{}` paused.", id)),
        Err(e) => CallToolResult::error(format!("Failed to pause trigger: {}", e)),
    }
}

async fn attach_trigger(storage: &dyn StorageBackend, args: &Value) -> CallToolResult {
    let id = match args.get("id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return CallToolResult::error("id is required".to_string()),
    };
    let target = match args.get("target") {
        Some(t) => t.clone(),
        None => return CallToolResult::error("target is required".to_string()),
    };

    let target_desc = match target.get("type").and_then(|v| v.as_str()) {
        Some("workflow") => format!("workflow:{}/{}",
            target.get("namespace").and_then(|v| v.as_str()).unwrap_or("default"),
            target.get("name").and_then(|v| v.as_str()).unwrap_or("?")),
        Some("agent") => format!("agent:{}",
            target.get("agent_id").and_then(|v| v.as_str()).unwrap_or("?")),
        Some("webhook") => format!("webhook:{}",
            target.get("url").and_then(|v| v.as_str()).unwrap_or("?")),
        _ => "unknown".to_string(),
    };

    match storage.triggers().attach(id, target).await {
        Ok(()) => {
            info!(trigger_id = %id, target = %target_desc, "Target attached via MCP");
            CallToolResult::text(format!("Attached `{}` to trigger `{}`", target_desc, id))
        }
        Err(e) => CallToolResult::error(format!("Failed to attach target: {}", e)),
    }
}

async fn detach_trigger(storage: &dyn StorageBackend, args: &Value) -> CallToolResult {
    let id = match args.get("id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return CallToolResult::error("id is required".to_string()),
    };
    let target_id = match args.get("target_id").and_then(|v| v.as_str()) {
        Some(t) => t,
        None => return CallToolResult::error("target_id is required".to_string()),
    };

    match storage.triggers().detach(id, target_id).await {
        Ok(()) => {
            info!(trigger_id = %id, target = %target_id, "Target detached via MCP");
            CallToolResult::text(format!("Detached `{}` from trigger `{}`", target_id, id))
        }
        Err(e) => CallToolResult::error(format!("Failed to detach target: {}", e)),
    }
}

/// Mask sensitive fields in config (API keys, passwords, etc.)
// ============================================================================
// Agent Tools (state-based — no flowr-ce-agents dependency)
// ============================================================================

async fn get_agent_history(storage: &dyn StorageBackend, args: &Value) -> CallToolResult {
    let agent_id = match args.get("agent_id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return CallToolResult::error("agent_id is required".to_string()),
    };

    let key = format!("agent:session:{}", agent_id);
    match storage.state().get(&key).await {
        Ok(Some(session)) => {
            let messages = session.get("messages").cloned().unwrap_or(json!([]));
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;

            let trimmed = if let Some(arr) = messages.as_array() {
                let start = arr.len().saturating_sub(limit);
                json!(arr[start..].to_vec())
            } else {
                messages
            };

            CallToolResult::json(json!({
                "agent_id": agent_id,
                "messages": trimmed,
                "last_response": session.get("last_response"),
                "updated_at": session.get("updated_at"),
            }))
        }
        Ok(None) => CallToolResult::text(format!("No session found for agent '{}'", agent_id)),
        Err(e) => CallToolResult::error(format!("Failed to get session: {}", e)),
    }
}

async fn clear_agent_session(storage: &dyn StorageBackend, args: &Value) -> CallToolResult {
    let agent_id = match args.get("agent_id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => return CallToolResult::error("agent_id is required".to_string()),
    };

    let key = format!("agent:session:{}", agent_id);
    match storage.state().delete(&key).await {
        Ok(_) => CallToolResult::text(format!("Session cleared for agent '{}'", agent_id)),
        Err(e) => CallToolResult::error(format!("Failed to clear session: {}", e)),
    }
}

fn mask_sensitive_fields(config: &mut Value) {
    if let Value::Object(map) = config {
        for (key, value) in map.iter_mut() {
            let key_lower = key.to_lowercase();
            if key_lower.contains("key")
                || key_lower.contains("secret")
                || key_lower.contains("password")
                || key_lower.contains("token")
                || key_lower.contains("credential")
            {
                if let Value::String(s) = value {
                    if s.len() > 8 {
                        *value = Value::String(format!("{}...{}", &s[..4], &s[s.len()-4..]));
                    } else {
                        *value = Value::String("****".to_string());
                    }
                }
            } else if let Value::Object(_) = value {
                // Recurse into nested objects
                mask_sensitive_fields(value);
            }
        }
    }
}

#[cfg(test)]
mod mcp_url_tests {
    use super::mcp_message_url;

    #[test]
    fn only_this_flowrs_own_port_is_self() {
        use super::{is_self_mcp_url, set_self_port};
        set_self_port(3002);
        assert!(is_self_mcp_url(""));
        assert!(is_self_mcp_url("http://localhost:3002/message"));
        assert!(is_self_mcp_url("http://127.0.0.1:3002"));
        assert!(is_self_mcp_url("http://flowr:3002/message"));
        assert!(is_self_mcp_url("http://flowr.a3t.svc.cluster.local:3002/message"));
        // Another server on the same machine is not this FlowR.
        assert!(!is_self_mcp_url("http://127.0.0.1:3000/mcp/sse"));
        assert!(!is_self_mcp_url("http://localhost:39001/mcp/message"));
        assert!(!is_self_mcp_url("http://adb.a3t.svc.cluster.local:3000/mcp/message"));
        assert!(!is_self_mcp_url("https://console.dev.a3t.io/message"));
    }

    #[test]
    fn sse_urls_map_to_their_message_endpoint() {
        assert_eq!(mcp_message_url("http://adb:3000/mcp/sse"), "http://adb:3000/mcp/message");
        assert_eq!(mcp_message_url("http://adb:3000/mcp/sse/"), "http://adb:3000/mcp/message");
        assert_eq!(mcp_message_url("http://adb:3000/mcp/message"), "http://adb:3000/mcp/message");
        assert_eq!(mcp_message_url("http://adb:3000/mcp"), "http://adb:3000/mcp/message");
        assert_eq!(mcp_message_url("http://localhost:3002"), "http://localhost:3002/message");
        // Only a trailing /sse is rewritten.
        assert_eq!(mcp_message_url("http://sse.example/x"), "http://sse.example/x/message");
    }
}
