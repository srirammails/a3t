//! FlowR EE Server
//!
//! Extends ClawdFlow CE with:
//! - JWT authentication
//! - Casbin RBAC (pluggable: internal/external/flow)
//! - Config file (flowr.yaml)
//! - Settings table in system DB
//! - Auth routes: /auth/login, /auth/signup, /auth/me
//! - CORS
//! - Web UI dashboard (--ui flag)

mod auth;
mod auth_routes;
mod config;

use flowr_ce::flowr_storage::{SettingsStore, StorageBackend};

use axum::{
    middleware,
    routing::{get, post},
    Extension, Router,
    response::{IntoResponse, Html},
};
use flowr_ce::{
    build_ce_router, init_storage, restore_triggers, run_stdio, serve, spawn_worker, ServerConfig,
};
use std::sync::Arc;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use auth::{jwt_auth_middleware, AuthConfig};
use auth_routes::{login_handler, me_handler, signup_handler};
use config::FlowrConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let args: Vec<String> = std::env::args().collect();
    let mode = get_arg(&args, "--mode").unwrap_or_else(|| "stdio".to_string());
    let port: u16 = get_arg(&args, "--port")
        .and_then(|p| p.parse().ok())
        .unwrap_or(3002);
    let ui_enabled = args.contains(&"--ui".to_string());
    let config_path = get_arg(&args, "--config").unwrap_or_else(|| "config/flowr.yaml".to_string());

    // Load config
    let mut flowr_config = FlowrConfig::load(&config_path);
    flowr_config.apply_env_overrides();

    // Authentication is unconditional. There is deliberately no switch to turn
    // it off: `bootstrap::seed_platform` provisions the first credential, so no
    // unauthenticated window is ever needed to install the platform.
    info!(mode = %mode, ui = ui_enabled, auth = true, "Starting FlowR server");

    let server_config = ServerConfig {
        port,
        mode: mode.clone(),
        data_dir: std::env::var("CLAWDFLOW_DATA_DIR").unwrap_or_else(|_| {
            dirs::data_local_dir()
                .map(|p| p.join("clawdflow").to_string_lossy().to_string())
                .unwrap_or_else(|| ".clawdflow".to_string())
        }),
        ui_enabled,
    };

    // Storage backend selection. CE always uses embedded LanceDB. EE may request an
    // external backend (Dragonfly/Iggy/…) via config/env; those connectors are wired
    // in later phases, so anything other than lancedb logs and falls back for now.
    #[cfg(feature = "ee")]
    let requested_backend = flowr_config.storage.backend.to_lowercase();
    #[cfg(not(feature = "ee"))]
    let requested_backend = "lancedb".to_string();
    if !requested_backend.starts_with("lancedb") {
        warn!(backend = %requested_backend, "external storage backend requested; connector not yet wired in this build — using embedded LanceDB");
    }
    info!(active = "lancedb", requested = %requested_backend, "storage backend selected");
    let storage: Arc<dyn StorageBackend> = init_storage(&server_config.data_dir).await?;

    // Seed default settings
    let settings = storage.settings();
    let defaults = vec![
        ("a3t.db_path", serde_json::json!(flowr_config.a3t.db_path), "config"),
        ("auth.token_expiry", serde_json::json!(flowr_config.auth.token_expiry), "config"),
        ("security.policy_hook", serde_json::json!(flowr_config.security.policy_hook), "config"),
        ("security.policies_dir", serde_json::json!(flowr_config.security.policies_dir), "config"),
        ("security.audit_exporter", serde_json::json!(flowr_config.security.audit_exporter), "config"),
        ("security.authn", serde_json::json!(flowr_config.security.authn), "config"),
        ("security.oidc_issuer", serde_json::json!(flowr_config.security.oidc_issuer), "config"),
        ("security.oidc_audience", serde_json::json!(flowr_config.security.oidc_audience), "config"),
        ("security.oidc_admin_user", serde_json::json!(flowr_config.security.oidc_admin_user), "config"),
    ];
    let seeded = settings.seed_defaults(&defaults).await
        .unwrap_or_else(|e| { warn!(error = %e, "Failed to seed settings"); 0 });
    if seeded > 0 { info!(count = seeded, "Settings seeded"); }

    // E0.6: install the AESOP policy hook named by the effective `security.policy_hook`
    // SETTING (config seeds it; the settings CRUD is the source of truth, so a
    // change takes effect on restart). This one setting drives the CE→EE
    // enforcement upgrade. CE ships allow_all / deny_all; EE adds the "cedar" arm.
    let hook_setting = settings.get_string("security.policy_hook").await
        .ok().flatten()
        .unwrap_or_else(|| flowr_config.security.policy_hook.clone());
    let policies_dir = settings.get_string("security.policies_dir").await
        .ok().flatten()
        .unwrap_or_else(|| flowr_config.security.policies_dir.clone());
    // E2 #4: resolve the AuthN scheme + OIDC params up front (before `storage`
    // is moved into build_ce_router). Precedence: env → settings DB → config, so
    // a deployment can flip AuthN via env even after the setting was seeded (the
    // settings-seed footgun otherwise pins a DB value across restarts).
    let env_or = |var: &str, setting: Option<String>, cfg: &str| -> String {
        std::env::var(var).ok().filter(|s| !s.is_empty())
            .or(setting.filter(|s| !s.is_empty()))
            .unwrap_or_else(|| cfg.to_string())
    };
    let authn = env_or("FLOWR_SECURITY_AUTHN",
        settings.get_string("security.authn").await.ok().flatten(), &flowr_config.security.authn);
    let oidc_issuer = env_or("FLOWR_SECURITY_OIDC_ISSUER",
        settings.get_string("security.oidc_issuer").await.ok().flatten(), &flowr_config.security.oidc_issuer);
    let oidc_audience = env_or("FLOWR_SECURITY_OIDC_AUDIENCE",
        settings.get_string("security.oidc_audience").await.ok().flatten(), &flowr_config.security.oidc_audience);
    // Keycloak Admin API creds for IdP management (create users from the UI).
    // User is env→settings→config; the PASSWORD is env-only (a secret, never seeded).
    let oidc_admin_user = env_or("FLOWR_SECURITY_OIDC_ADMIN_USER",
        settings.get_string("security.oidc_admin_user").await.ok().flatten(), &flowr_config.security.oidc_admin_user);
    let oidc_admin_password = std::env::var("FLOWR_SECURITY_OIDC_ADMIN_PASSWORD").unwrap_or_default();
    let oidc_client_id = env_or("FLOWR_SECURITY_OIDC_CLIENT_ID",
        settings.get_string("security.oidc_client_id").await.ok().flatten(), &flowr_config.security.oidc_client_id);
    // E2 #2c: gather InfoSec-owned `type_ref: cedar` policy resources from the
    // registry to layer onto the baked baseline. `config.cedar` holds the source
    // (see §6.3). Empty → the hook degrades to exactly the baseline pack.
    let registry_policies = collect_registry_cedar_policies(&storage).await;
    if !registry_policies.is_empty() {
        info!(count = registry_policies.len(), "loaded registry cedar policy resources");
    }
    let (schema_base, schema_fragments) = collect_registry_schema(&storage).await;
    if schema_base.is_some() || !schema_fragments.is_empty() {
        info!(base_override = schema_base.is_some(), fragments = schema_fragments.len(), "loaded registry cedar schema (base + fragments)");
    }
    let effective_hook = install_policy_hook(&hook_setting, &policies_dir, &registry_policies, schema_base.as_deref(), &schema_fragments);
    let edition = if effective_hook == "cedar" { "ee" } else { "ce" };
    // `enforcing` = a non-permissive hook is active (cedar or deny_all).
    let enforcing = effective_hook != "allow_all";
    let _ = EDITION_INFO.set(serde_json::json!({
        "edition": edition,
        "enforcing": enforcing,
        "policy_hook": effective_hook,
        "configured_policy_hook": hook_setting,
        "features": if cfg!(feature = "ee") { serde_json::json!(["cedar"]) } else { serde_json::json!([]) },
    }));
    info!(policy_hook = %effective_hook, edition = %edition, "AESOP policy hook installed");

    info!("FlowR initialized successfully");

    // Task handlers (lancedb, LLM, agent, etc.) now live in flowr-ce and are
    // registered natively — no EE-side injection.

    spawn_worker(storage.clone());

    match mode.as_str() {
        "http" | "sse" => {
            restore_triggers(&storage).await;

            // Clone storage for the EE policy-admin endpoints (reload re-reads the
            // registry) before `build_ce_router` moves it.
            let sec_storage = storage.clone();
            let ce_router = build_ce_router(storage);

            // Superadmin / platform seeding is handled by the a3t install flow
            // (a3t-install → a3t_schema_install), not in-process here.

            let mut router = {
                let auth_config = Arc::new(AuthConfig {
                    jwt_secret: flowr_config.auth.jwt_secret.clone(),
                    token_expiry_secs: flowr_config.auth.token_expiry,
                    refresh_expiry_secs: flowr_config.auth.refresh_expiry,
                    a3t_db_path: flowr_config.a3t.db_path.clone(),
                    // Canonical external URL, used as the OAuth resource identifier.
                    resource_uri: std::env::var("MCP_RESOURCE_URI")
                        .unwrap_or_else(|_| "http://localhost:3002".to_string()),
                    // OIDC login fallback (EE): lets Keycloak users sign in via the
                    // same email/password form. Empty issuer disables it.
                    oidc_issuer: oidc_issuer.clone(),
                    oidc_client_id: oidc_client_id.clone(),
                });

                info!(
                    a3t_db = %auth_config.a3t_db_path,
                    token_expiry = auth_config.token_expiry_secs,
                    "JWT auth enabled (authorization via the AESOP policy hook)"
                );

                // Auth + OAuth discovery routes (public). Merged after the auth
                // middleware layer below, so they bypass it.
                let auth_routes = Router::new()
                    .route("/auth/login", post(login_handler))
                    .route("/auth/signup", post(signup_handler))
                    .route("/auth/me", get(me_handler))
                    // E0.6: report active edition + policy hook (public).
                    .route("/edition", get(edition_handler))
                    .route(
                        "/.well-known/oauth-protected-resource",
                        get(oauth_protected_resource),
                    )
                    .route(
                        "/.well-known/oauth-authorization-server",
                        get(oauth_authorization_server),
                    )
                    .layer(Extension(auth_config.clone()));

                // E2 #4: choose the AuthN layer by `security.authn`. "oidc" (EE)
                // installs aesop-tower's RS256/JWKS layer; otherwise the HS256 dev
                // layer. Both bypass the public routes (merged after). The audit
                // endpoint sits behind the layer.
                // EE policy-admin surface: list / hot-reload / dry-run test. All
                // Admin-gated (see `is_policy_admin`), all behind the AuthN layer.
                let policy_admin = PolicyAdminState {
                    storage: sec_storage,
                    policies_dir: policies_dir.clone(),
                    hook_setting: hook_setting.clone(),
                };
                // EE IdP-admin surface: connect to Keycloak + create/list users.
                let idp_admin = IdpAdminState {
                    issuer: oidc_issuer.clone(),
                    admin_user: oidc_admin_user.clone(),
                    admin_password: oidc_admin_password.clone(),
                    client: reqwest::Client::new(),
                };
                let base = ce_router
                    .merge(Router::new().route("/security/audit", get(audit_handler)))
                    .merge(
                        Router::new()
                            .route("/security/policies", get(policies_list_handler))
                            .route("/security/policies/reload", post(policies_reload_handler))
                            .route("/security/policies/test", post(policies_test_handler))
                            .layer(Extension(policy_admin)),
                    )
                    .merge(
                        Router::new()
                            .route("/security/idp/status", get(idp_status_handler))
                            .route("/security/idp/users", get(idp_list_users_handler).post(idp_create_user_handler))
                            .route("/security/idp/users/:id",
                                get(idp_get_user_handler).put(idp_update_user_handler).delete(idp_delete_user_handler))
                            .layer(Extension(idp_admin)),
                    );

                let secured;
                #[cfg(feature = "ee")]
                {
                    if authn == "oidc" {
                        let audience = if oidc_audience.is_empty() { None } else { Some(oidc_audience.clone()) };
                        let cfg = aesop_tower::oidc::AuthnConfig::from_issuer(&oidc_issuer, audience);
                        info!(issuer = %oidc_issuer, jwks = %cfg.jwks_uri, "AuthN: OIDC (RS256/JWKS via aesop-tower)");
                        let state = Arc::new(auth::OidcState {
                            cache: aesop_tower::oidc::JwksCache::new(cfg.jwks_uri.clone()),
                            config: cfg,
                        });
                        secured = base
                            .layer(middleware::from_fn(auth::oidc_auth_middleware))
                            .layer(Extension(state));
                    } else if authn == "dual" {
                        // Console keeps its HS256 token; external MCP clients (Claude
                        // Desktop) present Keycloak OIDC tokens. Both are accepted.
                        let audience = if oidc_audience.is_empty() { None } else { Some(oidc_audience.clone()) };
                        let cfg = aesop_tower::oidc::AuthnConfig::from_issuer(&oidc_issuer, audience);
                        info!(issuer = %oidc_issuer, jwks = %cfg.jwks_uri, "AuthN: dual (HS256 + OIDC RS256/JWKS)");
                        let state = Arc::new(auth::OidcState {
                            cache: aesop_tower::oidc::JwksCache::new(cfg.jwks_uri.clone()),
                            config: cfg,
                        });
                        secured = base
                            .layer(middleware::from_fn(auth::dual_auth_middleware))
                            .layer(Extension(state))
                            .layer(Extension(auth_config));
                    } else {
                        info!("AuthN: HS256 shared-secret");
                        secured = base
                            .layer(middleware::from_fn(jwt_auth_middleware))
                            .layer(Extension(auth_config));
                    }
                }
                #[cfg(not(feature = "ee"))]
                {
                    let _ = &authn;
                    info!("AuthN: HS256 shared-secret");
                    secured = base
                        .layer(middleware::from_fn(jwt_auth_middleware))
                        .layer(Extension(auth_config));
                }
                secured.merge(auth_routes)
            };

            // Add UI routes
            if ui_enabled {
                info!("UI enabled at /ui/");
                router = router.merge(build_ui_router());
            }

            // CORS
            let cors_origins: Vec<axum::http::HeaderValue> = flowr_config.cors.origins.iter()
                .filter_map(|s| s.parse().ok())
                .collect();
            info!(origins = ?flowr_config.cors.origins, "CORS enabled");

            let cors = tower_http::cors::CorsLayer::new()
                .allow_origin(cors_origins)
                .allow_methods([
                    axum::http::Method::GET,
                    axum::http::Method::POST,
                    axum::http::Method::PUT,
                    axum::http::Method::DELETE,
                    axum::http::Method::OPTIONS,
                ])
                .allow_headers([
                    axum::http::header::CONTENT_TYPE,
                    axum::http::header::AUTHORIZATION,
                ])
                .allow_credentials(true);

            router = router.layer(cors);

            serve(router, port).await?;
        }
        _ => {
            run_stdio(storage).await?;
        }
    }

    info!("FlowR shutting down");
    Ok(())
}

fn get_arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

// =============================================================================
// OAuth 2.0 discovery metadata (MCP authorization)
// =============================================================================

fn resource_uri() -> String {
    std::env::var("MCP_RESOURCE_URI").unwrap_or_else(|_| "http://localhost:3002".to_string())
}

/// Cached edition/policy-hook info, populated once at startup for `GET /edition`.
static EDITION_INFO: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();

/// E0.6 — install the AESOP policy hook named by `security.policy_hook` and
/// return the *effective* hook name (may differ from the request if the named
/// hook isn't compiled into this build). AllowAll is the default when unset, so
/// "allow_all" needs no install; `policy_hook()` already returns it.
/// E2 #2c: read InfoSec-owned Cedar policy resources (`type_ref: cedar`) from
/// the registry, returning each resource's `config.cedar` source. These layer
/// onto the baked baseline pack at hook install. Malformed/typeless resources
/// are skipped here; a syntactically bad policy is caught when the engine loads.
async fn collect_registry_cedar_policies(
    storage: &Arc<dyn flowr_ce::flowr_storage::StorageBackend>,
) -> Vec<String> {
    let mut out = Vec::new();
    for ns in &["system", "a3t", "default"] {
        if let Ok(resources) = storage.resources().list(ns).await {
            for r in resources {
                if r.type_ref == "cedar" {
                    if let Some(src) = r.config.get("cedar").and_then(|v| v.as_str()) {
                        out.push(src.to_string());
                    }
                }
            }
        }
    }
    out
}

/// Collect the registry-supplied Cedar **schema**: an optional base override
/// (`type_ref: cedarschema`, name `base-schema`) plus per-resource fragments
/// (a `type_ref: cedarschema` resource's `config.schema`, or any resource's
/// inline `config.cedar_schema`). Values may be a JSON object or a JSON string.
async fn collect_registry_schema(
    storage: &Arc<dyn flowr_ce::flowr_storage::StorageBackend>,
) -> (Option<String>, Vec<String>) {
    let as_str = |v: &serde_json::Value| -> String {
        v.as_str().map(|s| s.to_string()).unwrap_or_else(|| v.to_string())
    };
    let mut base_override: Option<String> = None;
    let mut fragments = Vec::new();
    for ns in &["system", "a3t", "default"] {
        if let Ok(resources) = storage.resources().list(ns).await {
            for r in resources {
                if r.type_ref == "cedarschema" {
                    if let Some(s) = r.config.get("schema") {
                        if r.name == "base-schema" {
                            base_override = Some(as_str(s));
                        } else {
                            fragments.push(as_str(s));
                        }
                    }
                    continue;
                }
                if let Some(s) = r.config.get("cedar_schema") {
                    fragments.push(as_str(s));
                }
            }
        }
    }
    (base_override, fragments)
}

fn install_policy_hook(
    name: &str,
    policies_dir: &str,
    registry_policies: &[String],
    schema_base: Option<&str>,
    schema_fragments: &[String],
) -> &'static str {
    let _ = policies_dir; // used only by the ee build
    let _ = registry_policies; // used only by the ee build
    let _ = schema_base; // used only by the ee build
    let _ = schema_fragments; // used only by the ee build
    match name {
        "deny_all" => {
            let _ = flowr_policy::set_policy_hook(Arc::new(flowr_policy::DenyAll));
            "deny_all"
        }
        "cedar" => {
            #[cfg(feature = "ee")]
            {
                // EE build: install the Cedar hook. Load order (E2 #2c, §6.3):
                //   • security.policies_dir set  → file-based pack (deploy override)
                //   • otherwise                  → baked baseline + attached registry
                //     `type_ref: cedar` policy resources owned by InfoSec.
                let loaded = if !policies_dir.is_empty() {
                    flowr_aesop::CedarHook::from_dir(policies_dir)
                } else {
                    flowr_aesop::CedarHook::with_registry_policies(
                        registry_policies,
                        schema_base,
                        schema_fragments,
                    )
                };
                match loaded {
                    Ok(hook) => {
                        let _ = flowr_policy::set_policy_hook(Arc::new(hook));
                        info!(policies_dir = %policies_dir, registry_policies = registry_policies.len(), "AESOP Cedar policy hook installed (enforcing)");
                        return "cedar";
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "cedar hook failed to load; failing closed to deny_all");
                        let _ = flowr_policy::set_policy_hook(Arc::new(flowr_policy::DenyAll));
                        return "deny_all";
                    }
                }
            }
            #[cfg(not(feature = "ee"))]
            {
                // CE build has no Cedar hook — fall back to the permissive default.
                warn!(
                    "security.policy_hook='cedar' requested, but the Cedar hook is \
                     EE-only and not compiled into this build; falling back to allow_all"
                );
                "allow_all"
            }
        }
        _ => "allow_all",
    }
}

/// `GET /security/audit` (E2 #1) — the decision audit log (JWT-protected).
/// `?limit=N` (default 100), `?denials_only=true` for the security-relevant subset.
async fn audit_handler(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let limit = params.get("limit").and_then(|s| s.parse().ok()).unwrap_or(100);
    let denials_only = params.get("denials_only").map(|s| s == "true").unwrap_or(false);
    let records = if denials_only {
        flowr_policy::audit::recent_denials(limit)
    } else {
        flowr_policy::audit::recent(limit)
    };
    axum::Json(serde_json::json!({ "count": records.len(), "records": records }))
}

/// `GET /edition` — reports the active edition and policy hook (public).
async fn edition_handler() -> impl IntoResponse {
    axum::Json(EDITION_INFO.get().cloned().unwrap_or_else(|| {
        serde_json::json!({ "edition": "ce", "policy_hook": "allow_all" })
    }))
}

// =============================================================================
// EE policy administration — list / hot-reload / dry-run test
//   POST /security/policies/reload  → limitation #1 (no more pod restart)
//   POST /security/policies/test    → limitation #4 (test before you commit)
//   GET  /security/policies         → limitation #2/#3 (a real Policies surface)
// The write/apply paths (create/update/delete a cedar resource) reuse the
// existing `/registry/resources` CRUD; these add the *operate* surface on top.
// =============================================================================

/// State for the policy-admin endpoints: storage (to re-read `type_ref: cedar`
/// registry resources) plus the hook config needed to rebuild on reload.
#[derive(Clone)]
struct PolicyAdminState {
    storage: Arc<dyn flowr_ce::flowr_storage::StorageBackend>,
    policies_dir: String,
    hook_setting: String,
}

/// Policy administration is restricted to InfoSec/admin roles. Gate on the
/// canonical roles carried on the authenticated Principal (built by the AuthN
/// layer via `roles::canonicalize`). Deliberately does NOT route through the
/// Cedar hook: reload must stay reachable even if a bad policy set turned the
/// hook deny-all, so an admin can always recover.
fn is_policy_admin(principal: Option<&flowr_policy::Principal>) -> bool {
    use flowr_policy::{roles, Principal};
    matches!(principal, Some(Principal::Human { roles: r, .. })
        if r.iter().any(|role|
            role == roles::SUPER_ADMIN || role == roles::SECURITY_ADMIN || role == roles::ADMIN))
}

/// `GET /security/policies` — list InfoSec-owned Cedar policy resources
/// (`type_ref == "cedar"`) with their source, so the UI has a first-class
/// Policies view instead of the generic resource form. Admin-gated.
async fn policies_list_handler(
    Extension(state): Extension<PolicyAdminState>,
    principal: Option<Extension<flowr_policy::Principal>>,
) -> impl IntoResponse {
    if !is_policy_admin(principal.as_ref().map(|e| &e.0)) {
        return (axum::http::StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({ "error": "policy admin role required" }))).into_response();
    }
    let mut policies = Vec::new();
    for ns in &["system", "a3t", "default"] {
        if let Ok(resources) = state.storage.resources().list(ns).await {
            for r in resources {
                if r.type_ref == "cedar" {
                    policies.push(serde_json::json!({
                        "namespace": r.namespace,
                        "name": r.name,
                        "cedar": r.config.get("cedar").and_then(|v| v.as_str()).unwrap_or(""),
                        "description": r.config.get("description").and_then(|v| v.as_str()),
                    }));
                }
            }
        }
    }
    axum::Json(serde_json::json!({ "count": policies.len(), "policies": policies })).into_response()
}

/// `POST /security/policies/reload` — hot-reload the Cedar policy set with no
/// process restart (limitation #1). Re-reads the registry, rebuilds the hook,
/// and **only swaps it in if it loads cleanly** — a malformed draft leaves the
/// currently-enforcing hook untouched rather than bricking enforcement.
async fn policies_reload_handler(
    Extension(state): Extension<PolicyAdminState>,
    principal: Option<Extension<flowr_policy::Principal>>,
) -> impl IntoResponse {
    if !is_policy_admin(principal.as_ref().map(|e| &e.0)) {
        return (axum::http::StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({ "error": "policy admin role required" }))).into_response();
    }
    if state.hook_setting != "cedar" {
        return axum::Json(serde_json::json!({
            "reloaded": false,
            "reason": format!("policy_hook is '{}', not cedar — nothing to reload", state.hook_setting),
        })).into_response();
    }
    #[cfg(feature = "ee")]
    {
        let policies = collect_registry_cedar_policies(&state.storage).await;
        let count = policies.len();
        let (schema_base, schema_fragments) = collect_registry_schema(&state.storage).await;
        let frag_count = schema_fragments.len();
        let loaded = if !state.policies_dir.is_empty() {
            flowr_aesop::CedarHook::from_dir(&state.policies_dir)
        } else {
            flowr_aesop::CedarHook::with_registry_policies(&policies, schema_base.as_deref(), &schema_fragments)
        };
        match loaded {
            Ok(hook) => {
                let _ = flowr_policy::set_policy_hook(Arc::new(hook));
                info!(registry_policies = count, schema_fragments = frag_count, "policy hot-reload applied");
                axum::Json(serde_json::json!({ "reloaded": true, "registry_policies": count, "schema_fragments": frag_count })).into_response()
            }
            Err(e) => {
                // Do NOT swap — keep the working hook so a bad policy can't brick
                // enforcement. Surface the load error for the UI to display.
                warn!(error = %e, "policy hot-reload rejected; keeping current hook");
                (axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                    axum::Json(serde_json::json!({ "reloaded": false, "error": e }))).into_response()
            }
        }
    }
    #[cfg(not(feature = "ee"))]
    {
        let _ = &state;
        (axum::http::StatusCode::NOT_IMPLEMENTED,
            axum::Json(serde_json::json!({ "error": "policy reload is an EE feature" }))).into_response()
    }
}

/// Body for `POST /security/policies/test`.
#[derive(serde::Deserialize)]
struct PolicyTestRequest {
    /// Who is acting (full `flowr_policy::Principal`, incl. roles).
    principal: flowr_policy::Principal,
    /// `domain:verb`, e.g. `tool:invoke`, `resource:create`.
    action: String,
    /// The target entity.
    resource: PolicyTestResource,
    /// Ambient principal attributes the policy may read — notably
    /// `{"template": {"exposedTools": [...]}}` to exercise the flow ceiling.
    #[serde(default)]
    attributes: serde_json::Map<String, serde_json::Value>,
    /// Uncommitted Cedar to evaluate *before* saving it (the "test before you
    /// commit" path). Layered on top of the baseline pack.
    #[serde(default)]
    draft_policies: Vec<String>,
    /// Whether to also include the currently-saved registry policies (default
    /// true). Set false to test a draft against the baseline in isolation.
    #[serde(default = "default_true")]
    include_registry: bool,
}

#[derive(serde::Deserialize)]
struct PolicyTestResource {
    #[serde(rename = "type")]
    type_name: String,
    id: String,
    #[serde(default)]
    attributes: serde_json::Map<String, serde_json::Value>,
}

fn default_true() -> bool { true }

/// `POST /security/policies/test` — evaluate a hypothetical request against the
/// baseline + (optionally) the live registry + an uncommitted draft, and return
/// the decision with matched policy ids — **without mutating the live hook**
/// (limitation #4). This is the "would this be allowed, and why?" simulator.
async fn policies_test_handler(
    Extension(state): Extension<PolicyAdminState>,
    principal: Option<Extension<flowr_policy::Principal>>,
    axum::Json(body): axum::Json<PolicyTestRequest>,
) -> impl IntoResponse {
    if !is_policy_admin(principal.as_ref().map(|e| &e.0)) {
        return (axum::http::StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({ "error": "policy admin role required" }))).into_response();
    }
    #[cfg(feature = "ee")]
    {
        use flowr_policy::PolicyHook;
        let action = match flowr_policy::Action::new(&body.action) {
            Ok(a) => a,
            Err(e) => return (axum::http::StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({ "error": format!("invalid action: {e}") }))).into_response(),
        };
        // Assemble the policy set: baseline (always, inside with_registry_policies)
        // + live registry (optional) + the caller's draft.
        let mut policies = if body.include_registry {
            collect_registry_cedar_policies(&state.storage).await
        } else {
            Vec::new()
        };
        policies.extend(body.draft_policies.iter().cloned());
        // Validate/evaluate the draft against the same merged schema (base + fragments).
        let (schema_base, schema_fragments) = collect_registry_schema(&state.storage).await;
        let hook = match flowr_aesop::CedarHook::with_registry_policies(&policies, schema_base.as_deref(), &schema_fragments) {
            Ok(h) => h,
            Err(e) => return (axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                axum::Json(serde_json::json!({ "error": format!("policy set failed to load: {e}") }))).into_response(),
        };
        // Faithful simulation: for a `Tool` resource, auto-attach the same
        // security metadata the real choke point does via `Resource::tool()` —
        // `name` (from the id) and `destructive` — unless the caller already set
        // them. So testing "delete_flow" fires the destructive guardrail without
        // the caller needing to know that attribute exists.
        let mut resource = flowr_policy::Resource {
            type_name: body.resource.type_name,
            id: body.resource.id,
            attributes: body.resource.attributes,
        };
        if resource.type_name == "Tool" {
            resource.attributes.entry("name".to_string())
                .or_insert_with(|| serde_json::Value::String(resource.id.clone()));
            resource.attributes.entry("destructive".to_string())
                .or_insert_with(|| serde_json::Value::Bool(flowr_policy::is_destructive(&resource.id)));
        }
        let req = flowr_policy::ExecutionRequest {
            principal: body.principal,
            action,
            resource,
            context: Default::default(),
            attributes: body.attributes,
            params: serde_json::Value::Null,
        };
        let decision = hook.check(&req).await;
        // This endpoint exists to show what the policy WOULD do, so `Ask` is
        // reported as itself rather than folded into a deny. Collapsing it here
        // would make the Policies page claim a request is refused when in fact
        // the user would be prompted and could allow it — the opposite of what a
        // dry run is for. `allowed` stays false (an unanswered question is not
        // permission) and `scopes` says what the prompt would offer.
        let (allowed, reason, policy_ids, scopes) = match &decision {
            flowr_policy::PolicyDecision::Allow => (true, None, Vec::new(), Vec::new()),
            flowr_policy::PolicyDecision::Deny { reason, policy_ids } =>
                (false, Some(reason.clone()), policy_ids.clone(), Vec::new()),
            flowr_policy::PolicyDecision::Ask { reason, policy_ids, scopes } => (
                false,
                Some(reason.clone()),
                policy_ids.clone(),
                scopes.iter().map(|s| s.as_str().to_string()).collect(),
            ),
        };
        axum::Json(serde_json::json!({
            "decision": decision.label(),
            "allowed": allowed,
            "reason": reason,
            "policy_ids": policy_ids,
            "scopes": scopes,
            "tested_policies": policies.len(),
        })).into_response()
    }
    #[cfg(not(feature = "ee"))]
    {
        let _ = (&state, &body);
        (axum::http::StatusCode::NOT_IMPLEMENTED,
            axum::Json(serde_json::json!({ "error": "policy test is an EE feature" }))).into_response()
    }
}

// =============================================================================
// EE IdP administration — connect to Keycloak + create/list users (limitation #5)
//   GET  /security/idp/status  → is the admin connection configured + reachable?
//   GET  /security/idp/users   → list realm users
//   POST /security/idp/users   → create a user (+ assign realm roles)
// Auth to Keycloak uses the master-realm admin (admin-cli password grant); the
// password is env-only (FLOWR_SECURITY_OIDC_ADMIN_PASSWORD). Prod should swap in
// a scoped service-account client — only aesop_tower::oidc_admin::admin_token changes.
// =============================================================================

/// State for the IdP-admin endpoints. Plain strings so it compiles in CE too;
/// the ee handlers build `aesop_tower::oidc_admin::IdpAdminConfig` from them.
#[derive(Clone)]
struct IdpAdminState {
    issuer: String,
    admin_user: String,
    admin_password: String,
    client: reqwest::Client,
}

/// Body for `POST /security/idp/users` (kept local so it exists in CE builds).
#[derive(serde::Deserialize)]
struct IdpNewUser {
    username: String,
    #[serde(default)]
    email: Option<String>,
    password: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    temporary: bool,
    #[serde(default)]
    roles: Vec<String>,
}

/// `GET /security/idp/status` — is Keycloak admin configured, and can we reach it?
async fn idp_status_handler(
    Extension(state): Extension<IdpAdminState>,
    principal: Option<Extension<flowr_policy::Principal>>,
) -> impl IntoResponse {
    if !is_policy_admin(principal.as_ref().map(|e| &e.0)) {
        return (axum::http::StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({ "error": "policy admin role required" }))).into_response();
    }
    #[cfg(feature = "ee")]
    {
        match aesop_tower::oidc_admin::IdpAdminConfig::from_issuer(
            &state.issuer, state.admin_user.clone(), state.admin_password.clone())
        {
            None => axum::Json(serde_json::json!({
                "configured": false,
                "reason": "set security.oidc_issuer to a Keycloak realm URL and FLOWR_SECURITY_OIDC_ADMIN_PASSWORD",
            })).into_response(),
            Some(cfg) => match aesop_tower::oidc_admin::admin_token(&state.client, &cfg).await {
                Ok(_) => axum::Json(serde_json::json!({
                    "configured": true, "connected": true,
                    "base_url": cfg.base_url, "realm": cfg.realm,
                })).into_response(),
                Err(e) => axum::Json(serde_json::json!({
                    "configured": true, "connected": false,
                    "base_url": cfg.base_url, "realm": cfg.realm, "error": e.to_string(),
                })).into_response(),
            },
        }
    }
    #[cfg(not(feature = "ee"))]
    {
        let _ = &state;
        (axum::http::StatusCode::NOT_IMPLEMENTED,
            axum::Json(serde_json::json!({ "error": "IdP management is an EE feature" }))).into_response()
    }
}

/// `GET /security/idp/users` — list realm users (projected to id/username/email/roles).
async fn idp_list_users_handler(
    Extension(state): Extension<IdpAdminState>,
    principal: Option<Extension<flowr_policy::Principal>>,
) -> impl IntoResponse {
    if !is_policy_admin(principal.as_ref().map(|e| &e.0)) {
        return (axum::http::StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({ "error": "policy admin role required" }))).into_response();
    }
    #[cfg(feature = "ee")]
    {
        use aesop_tower::oidc_admin as kc;
        let cfg = match kc::IdpAdminConfig::from_issuer(&state.issuer, state.admin_user.clone(), state.admin_password.clone()) {
            Some(c) => c,
            None => return (axum::http::StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({ "error": "IdP not configured" }))).into_response(),
        };
        let token = match kc::admin_token(&state.client, &cfg).await {
            Ok(t) => t,
            Err(e) => return (axum::http::StatusCode::BAD_GATEWAY,
                axum::Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
        };
        match kc::list_users(&state.client, &cfg, &token).await {
            Ok(users) => {
                let projected: Vec<_> = users.iter().map(|u| serde_json::json!({
                    "id": u.get("id").and_then(|v| v.as_str()),
                    "username": u.get("username").and_then(|v| v.as_str()),
                    "email": u.get("email").and_then(|v| v.as_str()),
                    "enabled": u.get("enabled").and_then(|v| v.as_bool()),
                })).collect();
                axum::Json(serde_json::json!({ "count": projected.len(), "users": projected })).into_response()
            }
            Err(e) => (axum::http::StatusCode::BAD_GATEWAY,
                axum::Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
        }
    }
    #[cfg(not(feature = "ee"))]
    {
        let _ = &state;
        (axum::http::StatusCode::NOT_IMPLEMENTED,
            axum::Json(serde_json::json!({ "error": "IdP management is an EE feature" }))).into_response()
    }
}

/// `POST /security/idp/users` — create a Keycloak user and assign realm roles.
async fn idp_create_user_handler(
    Extension(state): Extension<IdpAdminState>,
    principal: Option<Extension<flowr_policy::Principal>>,
    axum::Json(body): axum::Json<IdpNewUser>,
) -> impl IntoResponse {
    if !is_policy_admin(principal.as_ref().map(|e| &e.0)) {
        return (axum::http::StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({ "error": "policy admin role required" }))).into_response();
    }
    #[cfg(feature = "ee")]
    {
        use aesop_tower::oidc_admin as kc;
        if body.username.trim().is_empty() || body.password.is_empty() {
            return (axum::http::StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({ "error": "username and password are required" }))).into_response();
        }
        let cfg = match kc::IdpAdminConfig::from_issuer(&state.issuer, state.admin_user.clone(), state.admin_password.clone()) {
            Some(c) => c,
            None => return (axum::http::StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({ "error": "IdP not configured" }))).into_response(),
        };
        let token = match kc::admin_token(&state.client, &cfg).await {
            Ok(t) => t,
            Err(e) => return (axum::http::StatusCode::BAD_GATEWAY,
                axum::Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
        };
        let new_user = kc::NewUser {
            username: body.username.clone(),
            email: body.email.clone(),
            password: body.password.clone(),
            name: body.name.clone(),
            temporary: body.temporary,
            roles: body.roles.clone(),
        };
        match kc::create_user(&state.client, &cfg, &token, &new_user).await {
            Ok(id) => {
                info!(username = %body.username, roles = ?body.roles, "created Keycloak user");
                (axum::http::StatusCode::CREATED,
                    axum::Json(serde_json::json!({ "created": true, "id": id, "username": body.username, "roles": body.roles }))).into_response()
            }
            Err(kc::IdpAdminError::Api { status, body: b }) if status == 409 => {
                let _ = b;
                (axum::http::StatusCode::CONFLICT,
                    axum::Json(serde_json::json!({ "error": format!("user '{}' already exists", body.username) }))).into_response()
            }
            Err(e) => (axum::http::StatusCode::BAD_GATEWAY,
                axum::Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
        }
    }
    #[cfg(not(feature = "ee"))]
    {
        let _ = (&state, &body);
        (axum::http::StatusCode::NOT_IMPLEMENTED,
            axum::Json(serde_json::json!({ "error": "IdP management is an EE feature" }))).into_response()
    }
}

/// Body for `PUT /security/idp/users/:id` — all fields optional (partial update).
#[derive(serde::Deserialize)]
struct IdpUpdateUser {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    temporary: bool,
    /// When present, replaces the user's manageable realm roles.
    #[serde(default)]
    roles: Option<Vec<String>>,
}

/// `GET /security/idp/users/:id` — one user's detail + its realm roles (for edit).
async fn idp_get_user_handler(
    Extension(state): Extension<IdpAdminState>,
    principal: Option<Extension<flowr_policy::Principal>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    if !is_policy_admin(principal.as_ref().map(|e| &e.0)) {
        return (axum::http::StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({ "error": "policy admin role required" }))).into_response();
    }
    #[cfg(feature = "ee")]
    {
        use aesop_tower::oidc_admin as kc;
        let (cfg, token) = match idp_cfg_token(&state).await {
            Ok(v) => v,
            Err(resp) => return resp,
        };
        let user = match kc::get_user(&state.client, &cfg, &token, &id).await {
            Ok(u) => u,
            Err(e) => return (axum::http::StatusCode::BAD_GATEWAY,
                axum::Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
        };
        let roles = kc::user_realm_roles(&state.client, &cfg, &token, &id).await.unwrap_or_default();
        let first = user.get("firstName").and_then(|v| v.as_str()).unwrap_or("");
        let last = user.get("lastName").and_then(|v| v.as_str()).unwrap_or("");
        let name = format!("{first} {last}").trim().to_string();
        axum::Json(serde_json::json!({
            "id": user.get("id").and_then(|v| v.as_str()),
            "username": user.get("username").and_then(|v| v.as_str()),
            "email": user.get("email").and_then(|v| v.as_str()),
            "name": name,
            "enabled": user.get("enabled").and_then(|v| v.as_bool()),
            // Manageable roles only — hide Keycloak's default composite.
            "roles": roles.into_iter().filter(|r| !r.starts_with("default-roles")).collect::<Vec<_>>(),
        })).into_response()
    }
    #[cfg(not(feature = "ee"))]
    { let _ = (&state, &id); idp_ee_only() }
}

/// `PUT /security/idp/users/:id` — update profile / password / roles.
async fn idp_update_user_handler(
    Extension(state): Extension<IdpAdminState>,
    principal: Option<Extension<flowr_policy::Principal>>,
    axum::extract::Path(id): axum::extract::Path<String>,
    axum::Json(body): axum::Json<IdpUpdateUser>,
) -> impl IntoResponse {
    if !is_policy_admin(principal.as_ref().map(|e| &e.0)) {
        return (axum::http::StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({ "error": "policy admin role required" }))).into_response();
    }
    #[cfg(feature = "ee")]
    {
        use aesop_tower::oidc_admin as kc;
        let (cfg, token) = match idp_cfg_token(&state).await { Ok(v) => v, Err(resp) => return resp };
        let upd = kc::UpdateUser {
            name: body.name.clone(),
            email: body.email.clone(),
            enabled: body.enabled,
            password: body.password.clone(),
            temporary: body.temporary,
            roles: body.roles.clone(),
        };
        match kc::update_user(&state.client, &cfg, &token, &id, &upd).await {
            Ok(()) => { info!(user_id = %id, "updated Keycloak user");
                axum::Json(serde_json::json!({ "updated": true, "id": id })).into_response() }
            Err(e) => (axum::http::StatusCode::BAD_GATEWAY,
                axum::Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
        }
    }
    #[cfg(not(feature = "ee"))]
    { let _ = (&state, &id, &body); idp_ee_only() }
}

/// `DELETE /security/idp/users/:id` — delete a user.
async fn idp_delete_user_handler(
    Extension(state): Extension<IdpAdminState>,
    principal: Option<Extension<flowr_policy::Principal>>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> impl IntoResponse {
    if !is_policy_admin(principal.as_ref().map(|e| &e.0)) {
        return (axum::http::StatusCode::FORBIDDEN,
            axum::Json(serde_json::json!({ "error": "policy admin role required" }))).into_response();
    }
    #[cfg(feature = "ee")]
    {
        use aesop_tower::oidc_admin as kc;
        let (cfg, token) = match idp_cfg_token(&state).await { Ok(v) => v, Err(resp) => return resp };
        match kc::delete_user(&state.client, &cfg, &token, &id).await {
            Ok(()) => { info!(user_id = %id, "deleted Keycloak user");
                (axum::http::StatusCode::NO_CONTENT, axum::Json(serde_json::json!({ "deleted": true }))).into_response() }
            Err(e) => (axum::http::StatusCode::BAD_GATEWAY,
                axum::Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
        }
    }
    #[cfg(not(feature = "ee"))]
    { let _ = (&state, &id); idp_ee_only() }
}

/// Shared: build the IdP config + fetch an admin token, or return an error response.
#[cfg(feature = "ee")]
async fn idp_cfg_token(state: &IdpAdminState)
    -> Result<(aesop_tower::oidc_admin::IdpAdminConfig, String), axum::response::Response>
{
    use aesop_tower::oidc_admin as kc;
    let cfg = kc::IdpAdminConfig::from_issuer(&state.issuer, state.admin_user.clone(), state.admin_password.clone())
        .ok_or_else(|| (axum::http::StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({ "error": "IdP not configured" }))).into_response())?;
    let token = kc::admin_token(&state.client, &cfg).await
        .map_err(|e| (axum::http::StatusCode::BAD_GATEWAY,
            axum::Json(serde_json::json!({ "error": e.to_string() }))).into_response())?;
    Ok((cfg, token))
}

#[cfg(not(feature = "ee"))]
fn idp_ee_only() -> axum::response::Response {
    (axum::http::StatusCode::NOT_IMPLEMENTED,
        axum::Json(serde_json::json!({ "error": "IdP management is an EE feature" }))).into_response()
}

/// RFC 9728 — OAuth 2.0 Protected Resource Metadata. Tells an MCP client what
/// this resource is and which authorization server issues tokens for it.
async fn oauth_protected_resource() -> impl IntoResponse {
    let r = resource_uri();
    // When an external OIDC provider (Keycloak) is the token authority, point MCP
    // clients at IT — so Claude Desktop runs the browser OAuth (auth-code + PKCE)
    // flow against Keycloak, which has the authorization_endpoint + DCR. In HS256
    // mode we advertise ourselves (password-grant token endpoint).
    let authn = std::env::var("FLOWR_SECURITY_AUTHN").unwrap_or_default();
    let issuer = std::env::var("FLOWR_SECURITY_OIDC_ISSUER").unwrap_or_default();
    let auth_server = if matches!(authn.as_str(), "oidc" | "dual") && !issuer.is_empty() {
        issuer
    } else {
        r.clone()
    };
    axum::Json(serde_json::json!({
        "resource": r,
        "authorization_servers": [auth_server],
        "bearer_methods_supported": ["header"],
        "resource_documentation": r,
    }))
}

/// RFC 8414 — OAuth 2.0 Authorization Server Metadata.
///
/// flowr issues bearer JWTs from a password login rather than a full OAuth
/// authorization-code flow, so the interactive `authorization_endpoint` /
/// PKCE / dynamic-registration machinery is not yet present. This advertises
/// what does exist (the token-issuing login endpoint and the audience-bound
/// bearer scheme); the authorization-code flow is intentionally future work.
async fn oauth_authorization_server() -> impl IntoResponse {
    let r = resource_uri();
    axum::Json(serde_json::json!({
        "issuer": r,
        "token_endpoint": format!("{}/auth/login", r),
        "token_endpoint_auth_methods_supported": ["none"],
        "grant_types_supported": ["password"],
        "response_types_supported": ["token"],
        "code_challenge_methods_supported": ["S256"],
    }))
}

// =============================================================================
// UI Router (EE feature)
// =============================================================================

fn build_ui_router() -> Router {
    Router::new()
        .route("/ui", get(ui_redirect))
        .route("/ui/", get(ui_index))
        .route("/ui/app.js", get(ui_js))
        .route("/ui/style.css", get(ui_css))
}

async fn ui_redirect() -> impl IntoResponse {
    axum::response::Redirect::permanent("/ui/")
}

async fn ui_index() -> impl IntoResponse {
    Html(include_str!("../ui/index.html"))
}

async fn ui_js() -> impl IntoResponse {
    (
        [("content-type", "application/javascript")],
        include_str!("../ui/app.js"),
    )
}

async fn ui_css() -> impl IntoResponse {
    (
        [("content-type", "text/css")],
        include_str!("../ui/style.css"),
    )
}
