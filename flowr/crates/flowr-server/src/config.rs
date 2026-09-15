//! FlowR configuration loader
//!
//! Loads flowr.yaml config file and seeds defaults into settings table.

use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{info, warn};

/// Top-level FlowR configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowrConfig {
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub a3t: A3tConfig,
    #[serde(default)]
    pub cors: CorsConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub storage: StorageConfig,
}

/// Which store backs flow state, the event log, and the dispatch queue.
/// `lancedb` is embedded (zero-install) and is the ONLY backend honored in CE.
/// EE may point at an external store (`dragonfly`, `redis`, `iggy`, `adb`) via
/// `url`. Selected at boot; the CE/EE gate is enforced in the factory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    #[serde(default = "default_storage_backend")]
    pub backend: String,
    /// Connection string for an external backend (ignored for `lancedb`).
    #[serde(default)]
    pub url: String,
    /// Optionally override just the event-log/queue backend (blank = same as `backend`).
    #[serde(default)]
    pub event_backend: String,
    #[serde(default)]
    pub event_url: String,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self { backend: default_storage_backend(), url: String::new(), event_backend: String::new(), event_url: String::new() }
    }
}

fn default_storage_backend() -> String { "lancedb".to_string() }

/// `security.*` — the AESOP policy seam (E0.6).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Which policy hook the runtime installs at startup:
    /// - `"allow_all"` — CE default (permissive; the `AllowAll` hook)
    /// - `"deny_all"`  — lockdown / test (the `DenyAll` hook)
    /// - `"cedar"`     — EE (Cedar-decided; requires the aesop crates, not in CE)
    ///
    /// This is the single setting that drives the CE→EE enforcement upgrade:
    /// `EE = CE + aesop crates + security.policy_hook = "cedar"`.
    #[serde(default = "default_policy_hook")]
    pub policy_hook: String,

    /// Directory the EE Cedar hook loads its policy pack from. Empty → the
    /// embedded A3T baseline. Reserved in CE (unused); consumed by the `ee` build.
    #[serde(default)]
    pub policies_dir: String,

    /// OTel/audit exporter target for `aesop.*` decision events. Reserved in CE;
    /// consumed by EE's audit layer (E2).
    #[serde(default)]
    pub audit_exporter: String,

    /// AuthN scheme (E2 #4): `"hs256"` (default — shared-secret JWT, the CE dev
    /// layer) or `"oidc"` (EE — RS256/JWKS via `aesop-tower::AuthnLayer`).
    /// Behavior-neutral until flipped; `"oidc"` in a non-`ee` build falls back to
    /// `hs256`, mirroring `policy_hook="cedar"`.
    #[serde(default = "default_authn")]
    pub authn: String,
    /// OIDC issuer (when `authn="oidc"`), e.g.
    /// `http://keycloak.a3t.svc:8080/realms/a3t`. JWKS is derived from it.
    #[serde(default)]
    pub oidc_issuer: String,
    /// OIDC audience to require (empty → audience not validated).
    #[serde(default)]
    pub oidc_audience: String,
    /// Keycloak Admin API user for IdP management (create users from the A3T UI).
    /// The master-realm admin username; default `admin`. The **password** is never
    /// stored in config — it comes from `FLOWR_SECURITY_OIDC_ADMIN_PASSWORD` (env
    /// / k8s secret). Empty password → IdP management is "not configured".
    #[serde(default = "default_oidc_admin_user")]
    pub oidc_admin_user: String,
    /// Public OIDC client id used for the console's password-grant login fallback
    /// (direct access grants). Default `a3t-ee`.
    #[serde(default = "default_oidc_client_id")]
    pub oidc_client_id: String,
}

fn default_oidc_admin_user() -> String { "admin".to_string() }
fn default_oidc_client_id() -> String { "a3t-ee".to_string() }

fn default_authn() -> String { "hs256".to_string() }

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            policy_hook: default_policy_hook(),
            policies_dir: String::new(),
            audit_exporter: String::new(),
            authn: default_authn(),
            oidc_issuer: String::new(),
            oidc_audience: String::new(),
            oidc_admin_user: default_oidc_admin_user(),
            oidc_client_id: default_oidc_client_id(),
        }
    }
}

fn default_policy_hook() -> String { "allow_all".to_string() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    #[serde(default = "default_jwt_secret")]
    pub jwt_secret: String,
    #[serde(default = "default_token_expiry")]
    pub token_expiry: u64,
    #[serde(default = "default_refresh_expiry")]
    pub refresh_expiry: u64,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            jwt_secret: default_jwt_secret(),
            token_expiry: default_token_expiry(),
            refresh_expiry: default_refresh_expiry(),
        }
    }
}

fn default_jwt_secret() -> String { "flowr-dev-secret-change-in-production".to_string() }
fn default_token_expiry() -> u64 { 86400 }
fn default_refresh_expiry() -> u64 { 604800 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A3tConfig {
    #[serde(default = "default_a3t_db_path")]
    pub db_path: String,
}

impl Default for A3tConfig {
    fn default() -> Self {
        Self { db_path: default_a3t_db_path() }
    }
}

fn default_a3t_db_path() -> String { "/data/a3t/a3t-platform.lance".to_string() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorsConfig {
    #[serde(default = "default_cors_origins")]
    pub origins: Vec<String>,
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self { origins: default_cors_origins() }
    }
}

fn default_cors_origins() -> Vec<String> {
    vec![
        "https://console.a3t.io".to_string(),
        "http://localhost:5173".to_string(),
    ]
}

impl Default for FlowrConfig {
    fn default() -> Self {
        Self {
            auth: AuthConfig::default(),
            a3t: A3tConfig::default(),
            cors: CorsConfig::default(),
            security: SecurityConfig::default(),
            storage: StorageConfig::default(),
        }
    }
}

impl FlowrConfig {
    /// Load config from file, falling back to defaults
    pub fn load(path: &str) -> Self {
        let config_path = Path::new(path);

        if config_path.exists() {
            match std::fs::read_to_string(config_path) {
                Ok(contents) => {
                    match serde_yaml::from_str::<FlowrConfig>(&contents) {
                        Ok(config) => {
                            info!(path = %path, "FlowR config loaded");
                            return config;
                        }
                        Err(e) => {
                            warn!(path = %path, error = %e, "Failed to parse config, using defaults");
                        }
                    }
                }
                Err(e) => {
                    warn!(path = %path, error = %e, "Failed to read config file, using defaults");
                }
            }
        } else {
            info!(path = %path, "No config file found, using defaults");
        }

        // Override from env vars
        let mut config = Self::default();
        config.apply_env_overrides();
        config
    }

    /// Apply environment variable overrides
    pub fn apply_env_overrides(&mut self) {
        if let Ok(v) = std::env::var("FLOWR_JWT_SECRET") { self.auth.jwt_secret = v; }
        if let Ok(v) = std::env::var("FLOWR_TOKEN_EXPIRY") { if let Ok(n) = v.parse() { self.auth.token_expiry = n; } }
        if let Ok(v) = std::env::var("FLOWR_REFRESH_EXPIRY") { if let Ok(n) = v.parse() { self.auth.refresh_expiry = n; } }
        if let Ok(v) = std::env::var("A3T_DB_PATH") { self.a3t.db_path = v; }
        if let Ok(v) = std::env::var("STORAGE_BACKEND") { self.storage.backend = v; }
        if let Ok(v) = std::env::var("STORAGE_URL") { self.storage.url = v; }
        if let Ok(v) = std::env::var("STORAGE_EVENT_BACKEND") { self.storage.event_backend = v; }
        if let Ok(v) = std::env::var("STORAGE_EVENT_URL") { self.storage.event_url = v; }
        if let Ok(v) = std::env::var("FLOWR_CORS_ORIGINS") {
            self.cors.origins = v.split(',').map(|s| s.trim().to_string()).collect();
        }
        if let Ok(v) = std::env::var("FLOWR_SECURITY_POLICY_HOOK") { self.security.policy_hook = v; }
    }
}
