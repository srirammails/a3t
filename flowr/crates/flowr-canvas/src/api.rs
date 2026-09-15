//! In-cluster HTTP API for the controller.
//!
//! This is the seam that lets flowr (EE) deploy a canvas **without a k8s client**:
//! flowr's `POST /canvas/.../deploy` handler forwards the resolved canvas here over
//! ordinary HTTP, and the controller — which already owns the kube client + RBAC —
//! applies the `Canvas` CR, which its reconciler turns into a Sandbox.
//!
//! - `POST   /v1/canvas`            — apply (create/update) a Canvas CR
//! - `GET    /v1/canvas`            — list Canvas CRs + live status
//! - `GET    /v1/canvas/:ns/:name`  — one Canvas + status
//! - `DELETE /v1/canvas/:ns/:name`  — delete a Canvas (GCs its Sandbox)
//! - `GET    /v1/runtimes`          — list Runtime pools + status
//!
//! Auth: a shared bearer token (`A3T_CONTROLLER_TOKEN`); when unset, auth is open
//! (dev). Bind in-cluster only (ClusterIP, no ingress) — flowr reaches it
//! server-to-server.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::{Path, Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use kube::api::{DeleteParams, Patch, PatchParams};
use kube::{Api, ResourceExt};
use serde::Deserialize;
use serde_json::json;
use tracing::{info, warn};

use crate::crd::{Canvas, CanvasSpec, Runtime};

struct ApiState {
    client: kube::Client,
    token: Option<String>,
}

/// Run the HTTP API forever.
pub async fn serve(client: kube::Client, addr: SocketAddr) -> anyhow::Result<()> {
    let token = std::env::var("A3T_CONTROLLER_TOKEN").ok().filter(|s| !s.is_empty());
    if token.is_none() {
        warn!("A3T_CONTROLLER_TOKEN unset — controller API is unauthenticated (dev only)");
    }
    let state = Arc::new(ApiState { client, token });

    let app = Router::new()
        .route("/v1/canvas", get(list_canvas).post(apply_canvas))
        .route("/v1/canvas/:ns/:name", get(get_canvas).delete(delete_canvas))
        .route("/v1/runtimes", get(list_runtimes))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth))
        // healthz stays unauthenticated for probes.
        .route("/healthz", get(|| async { "ok" }))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(%addr, "controller HTTP API listening");
    axum::serve(listener, app).await?;
    Ok(())
}

/// Bearer-token gate (skipped when no token is configured).
async fn auth(State(st): State<Arc<ApiState>>, req: Request, next: Next) -> Response {
    if let Some(expected) = &st.token {
        let ok = req
            .headers()
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(|t| t == expected)
            .unwrap_or(false);
        if !ok {
            return (StatusCode::UNAUTHORIZED, Json(json!({ "error": "unauthorized" }))).into_response();
        }
    }
    next.run(req).await
}

#[derive(Deserialize)]
struct ApplyReq {
    namespace: String,
    name: String,
    /// The canvas spec (flowRef/placement/lifecycle/health). Extra fields ignored.
    spec: CanvasSpec,
}

/// Apply (server-side) a Canvas CR — the reconciler then drives it to a Sandbox.
async fn apply_canvas(State(st): State<Arc<ApiState>>, Json(req): Json<ApplyReq>) -> Response {
    let mut canvas = Canvas::new(&req.name, req.spec);
    canvas.metadata.namespace = Some(req.namespace.clone());
    let api: Api<Canvas> = Api::namespaced(st.client.clone(), &req.namespace);
    match api
        .patch(&req.name, &PatchParams::apply("a3t-controller").force(), &Patch::Apply(&canvas))
        .await
    {
        Ok(o) => {
            info!(namespace = %req.namespace, name = %req.name, "applied Canvas via HTTP");
            (
                StatusCode::OK,
                Json(json!({
                    "applied": true,
                    "namespace": req.namespace,
                    "name": req.name,
                    "status": o.status,
                })),
            )
                .into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, Json(json!({ "applied": false, "error": e.to_string() }))).into_response(),
    }
}

/// List all Canvas CRs (across namespaces) with their live status.
async fn list_canvas(State(st): State<Arc<ApiState>>) -> Response {
    let api: Api<Canvas> = Api::all(st.client.clone());
    match api.list(&Default::default()).await {
        Ok(list) => {
            let items: Vec<_> = list
                .items
                .into_iter()
                .map(|c| {
                    json!({
                        "namespace": c.namespace(),
                        "name": c.name_any(),
                        "spec": c.spec,
                        "status": c.status,
                    })
                })
                .collect();
            Json(json!({ "count": items.len(), "canvases": items })).into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn get_canvas(State(st): State<Arc<ApiState>>, Path((ns, name)): Path<(String, String)>) -> Response {
    let api: Api<Canvas> = Api::namespaced(st.client.clone(), &ns);
    match api.get(&name).await {
        Ok(c) => Json(json!({ "namespace": ns, "name": name, "spec": c.spec, "status": c.status })).into_response(),
        Err(e) => (StatusCode::NOT_FOUND, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}

async fn delete_canvas(State(st): State<Arc<ApiState>>, Path((ns, name)): Path<(String, String)>) -> Response {
    let api: Api<Canvas> = Api::namespaced(st.client.clone(), &ns);
    match api.delete(&name, &DeleteParams::default()).await {
        Ok(_) => {
            info!(namespace = %ns, name = %name, "deleted Canvas via HTTP");
            Json(json!({ "deleted": true, "namespace": ns, "name": name })).into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, Json(json!({ "deleted": false, "error": e.to_string() }))).into_response(),
    }
}

/// List Runtime pools + status (the k8s truth, for the UI runtime picker).
async fn list_runtimes(State(st): State<Arc<ApiState>>) -> Response {
    let api: Api<Runtime> = Api::all(st.client.clone());
    match api.list(&Default::default()).await {
        Ok(list) => {
            let items: Vec<_> = list
                .items
                .into_iter()
                .map(|r| {
                    json!({
                        "namespace": r.namespace(),
                        "name": r.name_any(),
                        "spec": r.spec,
                        "status": r.status,
                    })
                })
                .collect();
            Json(json!({ "count": items.len(), "runtimes": items })).into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, Json(json!({ "error": e.to_string() }))).into_response(),
    }
}
