# ClawDB (call:clawdb) — port pending

`call:clawdb` is the ADB-CE (Agent Database, Community Edition) task type backed
by AQL — see the `clawdb-adb-ce-aql` memory. It is a KEPT, intended capability,
distinct from `call:lancedb`.

These two files are the **preserved source** of the ClawDB handler from the now-deleted
`shared/flowr-tasks` crate. They are on the OLD `flowr_core::TaskExecutor` trait
(context: `TaskContext`, result: `FlowResult`) and are NOT compiled.

## To make `call:clawdb` live
Port `call_clawdb.rs.taskexecutor` to a `swf_runtime::TaskHandler` in
`../../src/handlers/call_clawdb.rs` (mirror the sibling handlers, e.g. `call_llm.rs`),
register `ClawDbHandler` in `handlers/mod.rs::register_clawdflow_handlers`, and add
`call_clawdb` to the module list. Cover HTTP mode first (AQL query -> `{url}/query`);
gRPC (`feature=grpc`, tonic/proto) and embedded IPC (`feature=clawdb-ipc`, `clawdb` crate)
are the two additional modes in the original. `call_resource.rs.taskexecutor` is the
resource-config resolver the original depended on — adapt to how `call_llm.rs` reads
its resource in the TaskHandler world.
