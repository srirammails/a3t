# FlowR Triggers — Design & Implementation

**Status:** Design spec  
**Author:** Auto-generated  
**Date:** 2026-06-04  

---

## 1. What is a Trigger

A trigger is the bridge between external input and internal execution. It has two parts:

- **Channel** — how input arrives (HTTP, gRPC, cron, file, Kafka, internal event)
- **Routing** — where it goes (which flow/agent, with what input mapping)

Triggers are **independent resources** — they are not owned by any particular flow or agent. Any trigger can be mapped to any target. A single trigger can fan out to multiple targets.

### Core Data Model

```yaml
apiVersion: flowr.io/v1
kind: Trigger
metadata:
  name: agent-http
  namespace: default
  labels:
    tier: application          # or "infrastructure"
spec:
  channel:
    type: http                 # http | grpc | cron | file | kafka | event
    http:                      # type-specific config
      path: /api/agent
      methods: [POST]
      auth: bearer
      cors: true
  targets:
    - flowRef:
        namespace: system
        name: agent-engine
      inputMapping: "${ .body }"       # jq expression: transform channel input → flow input
      weight: 100                       # for weighted routing (optional)
    - flowRef:
        namespace: a3t
        name: audit-log
      inputMapping: "${ {event: \"trigger\", payload: .body} }"
  policy:
    enabled: true
    throttle: 100/min           # rate limit
    timeout: 30s                # max time to wait for target to accept
    retry: 3                    # retries on target failure
    deadLetter:                 # where to send failed deliveries
      flowRef:
        namespace: system
        name: dead-letter-handler
status:
  phase: Active                 # Active | Paused | Error | Provisioning
  lastFired: "2026-06-04T10:00:00Z"
  fireCount: 1523
  error: null
  infraResources:               # created by controller
    - kind: Ingress
      name: trigger-agent-http
    - kind: Service
      name: trigger-agent-http-svc
```

### Operations

| Operation | What happens |
|-----------|-------------|
| **Create** | Store trigger definition. Controller provisions infra. |
| **Update** | Modify channel config, targets, or policy. Controller reconciles. |
| **Delete** | Controller tears down infra. Remove trigger definition. |
| **Assign** | Add a target to `spec.targets[]` |
| **Reassign** | Replace a target's `flowRef` |
| **Deassign** | Remove a target from `spec.targets[]` |
| **Pause** | Set `spec.policy.enabled: false`. Controller scales down infra. |
| **Resume** | Set `spec.policy.enabled: true`. Controller scales up. |
| **Throttle** | Set `spec.policy.throttle`. Controller configures rate limiting. |

### Channel Types

| Type | Input shape | Infra created |
|------|-------------|---------------|
| `http` | `{ headers, body, query, method, path }` | Ingress + Service |
| `grpc` | `{ service, method, message }` | Service + gRPC server |
| `cron` | `{ scheduled_at, iteration, message }` | CronJob |
| `file` | `{ path, event, content?, size }` | PVC + watcher Pod |
| `kafka` | `{ topic, partition, key, value, offset }` | Consumer Deployment |
| `event` | `{ source, type, subject, data }` | Internal subscription (no infra) |

### Input Mapping

Every target has an `inputMapping` — a jq/SWF expression that transforms the channel's raw input into the flow's expected input schema.

```yaml
# HTTP trigger → agent flow
inputMapping: |
  ${ {
    message: .body.message,
    session_id: .body.session_id // .headers["x-session-id"] // "default"
  } }

# File trigger → processing flow
inputMapping: |
  ${ {
    file_path: .path,
    content: .content,
    event_type: .event
  } }

# Cron trigger → maintenance flow
inputMapping: |
  ${ {
    scheduled_at: .scheduled_at,
    action: "cleanup"
  } }
```

---

## 2. Three Levels of Implementation

### Level 1: Application-Only

**Scope:** Everything runs inside the FlowR MCP server process. No K3S involvement.

```
Browser/Client
    ↓
POST /webhook/{path}
    ↓
FlowR MCP Server (single process)
    ├── match trigger by path
    ├── apply inputMapping
    └── enqueue ExecutionRequest
    ↓
Worker loop dequeues and executes
```

**Architecture:**
- Triggers stored in LanceDB/Dragonfly (same as flows, resources)
- Webhook handler is an axum route in the MCP server
- Cron is a tokio::spawn loop checking trigger schedules
- File watching is an inotify/fswatch loop in the same process
- No K3S, no Ingress, no CRDs

**Pros:**
- Zero infrastructure overhead
- Single binary deployment
- Fast to implement and test
- Works locally without K3S

**Cons:**
- Single point of failure
- No TLS termination (unless reverse proxy)
- No horizontal scaling
- Cron/file watchers die with the process
- No rate limiting beyond in-process semaphore

**When to use:** Development, testing, single-tenant deployments, demos.

### Level 2: K3S CRD + Controller

**Scope:** Triggers are K3S Custom Resources. A controller watches them and creates infrastructure.

```
Browser/Client
    ↓
K3S Ingress (auto-created by controller)
    ↓
Trigger Receiver Pod (auto-created by controller)
    ├── validate auth/rate-limit
    ├── apply inputMapping
    └── enqueue ExecutionRequest
    ↓
Worker pool dequeues and executes
```

**Architecture:**
- Trigger CRD registered in K3S (`flowr.io/v1/Trigger`)
- Trigger Controller (Rust or Go) watches CRDs, reconciles:
  - `type: http` → Ingress + ClusterIP Service + receiver Deployment
  - `type: grpc` → same but with gRPC protocol on Ingress
  - `type: cron` → CronJob that posts to internal webhook
  - `type: file` → Deployment with PVC mount + file watcher
  - `type: kafka` → Deployment with Kafka consumer
- MCP API becomes a thin wrapper around `kubectl` / K3S API
- UI unchanged — still talks to MCP API

**CRD Spec:**

```yaml
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: triggers.flowr.io
spec:
  group: flowr.io
  versions:
    - name: v1
      served: true
      storage: true
      schema:
        openAPIV3Schema:
          type: object
          properties:
            spec:
              type: object
              properties:
                channel:
                  type: object
                  properties:
                    type:
                      type: string
                      enum: [http, grpc, cron, file, kafka, event]
                    http:
                      type: object
                      properties:
                        path: { type: string }
                        methods: { type: array, items: { type: string } }
                        auth: { type: string }
                    grpc:
                      type: object
                      properties:
                        service: { type: string }
                        methods: { type: array, items: { type: string } }
                    cron:
                      type: object
                      properties:
                        expression: { type: string }
                        timezone: { type: string }
                        message: { type: string }
                    file:
                      type: object
                      properties:
                        path: { type: string }
                        events: { type: array, items: { type: string, enum: [create, modify, delete] } }
                        pattern: { type: string }
                    kafka:
                      type: object
                      properties:
                        brokers: { type: array, items: { type: string } }
                        topic: { type: string }
                        group: { type: string }
                    event:
                      type: object
                      properties:
                        source: { type: string }
                        filter: { type: string }
                targets:
                  type: array
                  items:
                    type: object
                    properties:
                      flowRef:
                        type: object
                        properties:
                          namespace: { type: string }
                          name: { type: string }
                      inputMapping: { type: string }
                      weight: { type: integer }
                policy:
                  type: object
                  properties:
                    enabled: { type: boolean }
                    throttle: { type: string }
                    timeout: { type: string }
                    retry: { type: integer }
            status:
              type: object
              properties:
                phase: { type: string }
                lastFired: { type: string }
                fireCount: { type: integer }
                error: { type: string }
      subresources:
        status: {}
      additionalPrinterColumns:
        - name: Type
          type: string
          jsonPath: .spec.channel.type
        - name: Phase
          type: string
          jsonPath: .status.phase
        - name: Fires
          type: integer
          jsonPath: .status.fireCount
  scope: Namespaced
  names:
    plural: triggers
    singular: trigger
    kind: Trigger
    shortNames: [trg]
```

**Controller reconciliation logic:**

```
on Trigger create/update:
  1. Read spec.channel.type
  2. Switch:
     http:
       - Ensure Deployment "trigger-{name}-receiver" exists (webhook receiver image)
       - Ensure Service "trigger-{name}-svc" exists (ClusterIP → receiver)
       - Ensure Ingress "trigger-{name}" exists (path → service)
       - If spec.channel.http.auth: add auth annotation to Ingress
     grpc:
       - Same as http but with gRPC backend protocol annotation
     cron:
       - Ensure CronJob "trigger-{name}" exists
       - Schedule = spec.channel.cron.expression
       - Job template: curl internal webhook with message payload
     file:
       - Ensure PVC "trigger-{name}-data" exists
       - Ensure Deployment "trigger-{name}-watcher" exists (file watcher image)
       - Watcher watches spec.channel.file.path for spec.channel.file.events
     kafka:
       - Ensure Deployment "trigger-{name}-consumer" exists
       - Consumer reads from spec.channel.kafka.topic
       - On message: apply inputMapping, enqueue execution
     event:
       - Register internal subscription (no infra)
  3. If spec.policy.enabled == false:
       - Scale all Deployments to 0 replicas
       - Suspend CronJob
  4. Update status.phase

on Trigger delete:
  1. Delete all owned resources (Ingress, Service, Deployment, CronJob, PVC)
```

**Pros:**
- Production-grade reliability (K3S manages lifecycle)
- TLS via cert-manager on Ingress
- Horizontal scaling via HPA
- Rate limiting via Ingress annotations
- Cron survives process restarts
- Audit trail via K3S events

**Cons:**
- More complex to implement
- Requires K3S cluster
- Controller needs to be written and maintained
- Slower iteration cycle

**When to use:** Production, multi-tenant, SaaS deployments.

### Level 3: Hybrid (Recommended)

**Scope:** K3S CRD is the source of truth. MCP API is the access layer. App-level fallback for non-K3S environments.

```
┌─────────────────────────────────────────────────┐
│                     UI                          │
│  Triggers page: CRUD, assign, pause, throttle   │
└─────────────────┬───────────────────────────────┘
                  │ HTTP (MCP API)
┌─────────────────▼───────────────────────────────┐
│              MCP Server                         │
│  if K3S available:                              │
│    → kubectl apply/get/delete Trigger CRD       │
│  else:                                          │
│    → LanceDB/Dragonfly storage (Level 1)        │
└─────────────────┬───────────────────────────────┘
                  │
        ┌─────────┴─────────┐
        ▼                   ▼
   K3S API              LanceDB
   (Level 2)            (Level 1 fallback)
        │
        ▼
   Controller
   reconciles
   Ingress/CronJob/etc
```

**Key design decisions:**
1. **CRD is source of truth** when K3S is available
2. **MCP API auto-detects** K3S availability on startup
3. **Same UI, same API** regardless of backend
4. **Level 1 is always available** as fallback (Docker, local dev)
5. **Level 2 kicks in** when K3S is detected and CRD is registered

**Pros:**
- Works everywhere (local Docker → K3S → full cloud)
- Progressive enhancement (start Level 1, graduate to Level 2)
- Single UI and API surface
- No code changes needed to upgrade

**Cons:**
- Two code paths to maintain (storage vs CRD)
- CRD ↔ storage sync if switching between levels

**When to use:** Always. Start with Level 1, deploy Level 2 when ready.

---

## 3. Implementation TODO

### Phase 1: Level 1 — App-Only (Current Sprint)

- [x] Trigger data model in storage (LanceDB/Dragonfly)
- [x] MCP tools: create_trigger, list_triggers, get_trigger, delete_trigger
- [x] MCP tools: enable_trigger, pause_trigger, update_trigger
- [x] MCP tools: attach_trigger, detach_trigger
- [x] UI: Triggers page with CRUD
- [x] UI: Sidebar entry + PanelStack routing
- [ ] **Webhook handler** — `POST /webhook/{path}` route in MCP server
  - [ ] Match triggers by path
  - [ ] Apply inputMapping (jq/SWF expression)
  - [ ] Enqueue ExecutionRequest for each target
  - [ ] Return delivery receipt `{ delivered, executions: [{ instance_id }] }`
- [ ] **Cron loop** — background task that checks cron triggers
  - [ ] Parse cron expressions (use `cron` crate)
  - [ ] Track last-fired time per trigger
  - [ ] Fire matching triggers on schedule
  - [ ] Create execution with cron input shape
- [ ] **Event subscription** — internal pub/sub
  - [ ] Subscribe to workflow completion/error events
  - [ ] Match event triggers by source/filter
  - [ ] Fire matching triggers
- [ ] **Input mapping** — transform channel input to flow input
  - [ ] Implement jq-style expression evaluation on trigger input
  - [ ] Or reuse SWF runtime expression engine
- [ ] **Throttling** — in-process rate limiter
  - [ ] Token bucket per trigger
  - [ ] Reject over-limit requests with 429
- [ ] **Trigger status tracking**
  - [ ] Track fireCount, lastFired in storage
  - [ ] Update status on error
  - [ ] Surface in UI (last fired, fire count, error)
- [ ] **End-to-end test**
  - [ ] Create HTTP trigger → attach to agent-engine → POST webhook → verify agent executes
  - [ ] Create cron trigger → verify it fires on schedule

### Phase 2: Level 2 — K3S CRD

- [ ] **Register Trigger CRD** in K3S
  - [ ] Write CRD YAML (schema above)
  - [ ] `kubectl apply -f trigger-crd.yaml`
  - [ ] Verify: `kubectl get triggers`
- [ ] **Trigger Controller** (Rust binary or operator)
  - [ ] Watch Trigger CRDs via kube-rs
  - [ ] Reconcile loop:
    - [ ] HTTP triggers → create Ingress + Service + receiver Deployment
    - [ ] gRPC triggers → create Ingress (gRPC backend) + Service
    - [ ] Cron triggers → create CronJob
    - [ ] File triggers → create PVC + watcher Deployment
    - [ ] Kafka triggers → create consumer Deployment
  - [ ] Handle pause/resume (scale to 0 / scale up)
  - [ ] Handle delete (cleanup owned resources)
  - [ ] Update Trigger status subresource
- [ ] **Webhook receiver image**
  - [ ] Minimal Rust binary: receive HTTP, apply inputMapping, enqueue execution
  - [ ] Docker image published to registry
  - [ ] Used by controller for HTTP trigger Deployments
- [ ] **File watcher image**
  - [ ] Rust binary with inotify/fsnotify
  - [ ] Watch configured path, POST to internal webhook on events
- [ ] **Kafka consumer image**
  - [ ] Rust binary with rdkafka
  - [ ] Consume from topic, POST to internal webhook per message
- [ ] **Ingress TLS**
  - [ ] Integrate with cert-manager for automatic HTTPS
  - [ ] Configure via trigger spec.channel.http.tls

### Phase 3: Level 3 — Hybrid

- [ ] **Auto-detect K3S** on MCP server startup
  - [ ] Check for KUBERNETES_SERVICE_HOST env var
  - [ ] Or check for kubeconfig
  - [ ] Set `trigger_backend = "k3s" | "storage"`
- [ ] **MCP trigger tools: K3S backend**
  - [ ] create_trigger → `kubectl apply Trigger`
  - [ ] list_triggers → `kubectl get triggers`
  - [ ] delete_trigger → `kubectl delete trigger`
  - [ ] enable/pause → patch Trigger spec.policy.enabled
  - [ ] throttle → patch Trigger spec.policy.throttle
- [ ] **Sync mechanism** (optional)
  - [ ] On startup: sync LanceDB triggers → CRDs (one-time migration)
  - [ ] Or: keep both in sync bidirectionally
- [ ] **UI: infra status panel**
  - [ ] Show Ingress URL, CronJob status, consumer health
  - [ ] Show fire count, last fired, error from CRD status
- [ ] **Registry: trigger types as function manifests**
  - [ ] Register channel types in registry (like function manifests)
  - [ ] Schema-driven trigger config forms (reuse SchemaConfigForm)

---

## 4. Deployment Matrix

| Environment | Level | Trigger Backend | HTTP Handling | Cron Handling |
|------------|-------|----------------|---------------|---------------|
| Local Docker | 1 | LanceDB | MCP `/webhook` route | tokio cron loop |
| Single server | 1 | Dragonfly | MCP `/webhook` route | tokio cron loop |
| K3S dev | 2 | CRD + controller | Ingress → receiver pod | CronJob |
| K3S prod | 3 | CRD (primary) + Dragonfly (fallback) | Ingress + TLS | CronJob |
| Cloud (EKS/GKE) | 3 | CRD + cloud LB | Cloud Ingress + WAF | CronJob |

---

## 5. API Summary

All operations available via MCP tools and REST:

```
# CRUD
POST   /triggers                    → create
GET    /triggers                    → list (filter by namespace, target)
GET    /triggers/{id}               → get
PUT    /triggers/{id}               → update
DELETE /triggers/{id}               → delete

# Lifecycle
POST   /triggers/{id}/pause         → pause (set enabled=false)
POST   /triggers/{id}/resume        → resume (set enabled=true)
PATCH  /triggers/{id}/policy        → update throttle, timeout, retry

# Routing
POST   /triggers/{id}/targets       → add target (assign)
PUT    /triggers/{id}/targets/{idx} → replace target (reassign)
DELETE /triggers/{id}/targets/{idx} → remove target (deassign)

# Invocation
POST   /webhook/{path}              → fire HTTP trigger by path
POST   /triggers/{id}/test          → test-fire with sample payload

# Status
GET    /triggers/{id}/status        → fire count, last fired, errors
GET    /triggers/{id}/history       → recent fire history
```
