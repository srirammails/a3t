//! JWT Authentication Middleware (FlowR EE)
//!
//! Validates Bearer tokens on all requests except public paths.
//! Tokens are issued by the /auth/login endpoint against the A3T users table.
//!
//! JWT Claims: { sub: user_id, org: org_id, role: "admin"|"operator"|"viewer", exp: timestamp }

use axum::{
    extract::Request,
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// JWT claims embedded in every token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// User ID (from A3T users table)
    pub sub: String,
    /// Organization ID
    pub org: String,
    /// User role: admin, operator, viewer
    pub role: String,
    /// User email (for convenience)
    pub email: String,
    /// User name
    pub name: String,
    /// Audience — the resource this token is valid for (RFC 8707). Binds the
    /// token to this MCP server so it cannot be replayed against another.
    #[serde(default)]
    pub aud: String,
    /// Token expiry (unix timestamp)
    pub exp: usize,
    /// Issued at (unix timestamp)
    pub iat: usize,
}

/// Auth configuration
#[derive(Clone)]
pub struct AuthConfig {
    /// JWT signing secret (from env: FLOWR_JWT_SECRET)
    pub jwt_secret: String,
    /// Token expiry duration in seconds (default: 24h)
    pub token_expiry_secs: u64,
    /// Refresh token expiry in seconds (default: 7d)
    pub refresh_expiry_secs: u64,
    /// A3T platform database path (for user lookups)
    pub a3t_db_path: String,
    /// Where `users` is read and written: the embedded LanceDB at `a3t_db_path`, or
    /// ADB platform tables when `a3t/a3t-platform-db` says so (ADB stage G) —
    /// looked up per login, so switching (Settings) needs no restart.
    pub platform: flowr_ce::platform_db::PlatformSource,
    /// Canonical external URL of this MCP server (e.g. https://console.a3t.io).
    /// Used as the OAuth 2.0 resource identifier / token audience (RFC 8707)
    /// and in the WWW-Authenticate resource_metadata pointer (RFC 9728).
    pub resource_uri: String,
    /// OIDC issuer (Keycloak realm URL) for the login fallback; empty disables it.
    pub oidc_issuer: String,
    /// Public OIDC client id for the password-grant login fallback.
    pub oidc_client_id: String,
}

impl AuthConfig {
    pub fn from_env() -> Self {
        Self {
            jwt_secret: std::env::var("FLOWR_JWT_SECRET")
                .unwrap_or_else(|_| "flowr-dev-secret-change-in-production".to_string()),
            token_expiry_secs: std::env::var("FLOWR_TOKEN_EXPIRY")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(24 * 60 * 60),
            refresh_expiry_secs: std::env::var("FLOWR_REFRESH_EXPIRY")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(7 * 24 * 60 * 60),
            a3t_db_path: std::env::var("A3T_DB_PATH")
                .unwrap_or_else(|_| "/data/a3t/a3t-platform.lance".to_string()),
            platform: flowr_ce::platform_db::PlatformSource::Fixed(Arc::new(flowr_ce::platform_db::LanceDbPlatform::new(
                std::env::var("A3T_DB_PATH").unwrap_or_else(|_| "/data/a3t/a3t-platform.lance".to_string()),
            ))),
            resource_uri: std::env::var("MCP_RESOURCE_URI")
                .unwrap_or_else(|_| "http://localhost:3002".to_string()),
            oidc_issuer: std::env::var("FLOWR_SECURITY_OIDC_ISSUER").unwrap_or_default(),
            oidc_client_id: std::env::var("FLOWR_SECURITY_OIDC_CLIENT_ID")
                .unwrap_or_else(|_| "a3t-ee".to_string()),
        }
    }
}

/// Endpoints reachable without a bearer token.
///
/// Everything else — including the MCP endpoints `/message` and `/sse`, and all
/// flow/agent/trigger/resource management — now requires a valid JWT. The MCP
/// admin surface is no longer public.
fn is_public(path: &str) -> bool {
    // Exact matches: auth bootstrap, health probes, and the OAuth discovery
    // metadata, which by definition must be fetchable before authenticating.
    const EXACT: &[&str] = &[
        "/auth/login",
        "/auth/signup",
        "/auth/refresh",
        "/auth/me", // validates its own bearer token internally
        "/health",
        "/ready",
        "/live",
        "/.well-known/oauth-protected-resource",
        "/.well-known/oauth-authorization-server",
    ];
    if EXACT.contains(&path) {
        return true;
    }
    // Webhooks are invoked by external systems that hold no user JWT; they are
    // authorized at the trigger level, not by this middleware.
    path == "/webhook" || path.starts_with("/webhook/")
}

/// RFC 9728 challenge telling an unauthenticated client where to find this
/// server's protected-resource metadata (and thus how to obtain a token).
fn www_authenticate(resource_uri: &str) -> String {
    format!(
        "Bearer resource_metadata=\"{}/.well-known/oauth-protected-resource\"",
        resource_uri.trim_end_matches('/')
    )
}

fn unauthorized(resource_uri: &str, msg: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(
            axum::http::header::WWW_AUTHENTICATE,
            www_authenticate(resource_uri),
        )],
        Json(json!({ "error": msg })),
    )
        .into_response()
}

/// JWT auth middleware — validates Bearer token on non-public paths.
pub async fn jwt_auth_middleware(
    headers: HeaderMap,
    mut request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();

    if is_public(&path) {
        // Public (e.g. webhooks are trigger-authorized), but honor a valid token
        // when one IS presented — so the console chat's authenticated webhook runs
        // on behalf of the real user. Without this the flow runs as Service, the
        // agent gets `on_behalf_of: None`, and its policy-gated tool calls
        // (`mcp:call`) are denied by the baseline. No/invalid token → stays public.
        if let Some(config) = request.extensions().get::<Arc<AuthConfig>>().cloned() {
            if let Some(token) = extract_bearer_token(&headers) {
                if let Ok(claims) = validate_token(&token, &config.jwt_secret, &config.resource_uri) {
                    let principal = flowr_policy::Principal::human(
                        claims.sub.clone(),
                        flowr_policy::roles::canonicalize(&claims.role),
                    );
                    request.extensions_mut().insert(claims);
                    request.extensions_mut().insert(principal);
                }
            }
        }
        return next.run(request).await;
    }

    // AuthConfig carries both the signing secret and the resource URI advertised
    // in 401 challenges, so it is fetched before any auth decision.
    let config = match request.extensions().get::<Arc<AuthConfig>>() {
        Some(c) => c.clone(),
        None => {
            warn!("AuthConfig not found in request extensions");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Auth not configured" })),
            )
                .into_response();
        }
    };

    let token = match extract_bearer_token(&headers) {
        Some(t) => t,
        None => {
            debug!(path = %path, "Missing Authorization header");
            return unauthorized(
                &config.resource_uri,
                "Missing or invalid Authorization header. Use: Bearer <token>",
            );
        }
    };

    match validate_token(&token, &config.jwt_secret, &config.resource_uri) {
        Ok(claims) => {
            debug!(user = %claims.sub, role = %claims.role, "Authenticated");
            // E0.4a: map the validated identity into the shared Principal type so
            // the execution layer (site A choke point) can authorize the real
            // caller without depending on this crate's Claims type. The AuthN
            // layer produces identity; the policy hook decides what it grants.
            // Map the provider's role claim onto the canonical A3T role vocabulary
            // (CE contract) so Cedar policies check stable names, independent of how
            // any given IdP spells them. `superadmin` also grants `SecurityAdmin`.
            let principal = flowr_policy::Principal::human(
                claims.sub.clone(),
                flowr_policy::roles::canonicalize(&claims.role),
            );
            request.extensions_mut().insert(claims);
            request.extensions_mut().insert(principal);
            next.run(request).await
        }
        Err(e) => {
            debug!(error = %e, "JWT validation failed");
            unauthorized(&config.resource_uri, &format!("Invalid token: {}", e))
        }
    }
}

/// Extract Bearer token from Authorization header
pub(crate) fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.trim().to_string())
}

/// Create a JWT token for a user
pub fn create_token(config: &AuthConfig, user_id: &str, org_id: &str, role: &str, email: &str, name: &str) -> Result<String, String> {
    let now = chrono::Utc::now().timestamp() as usize;
    let claims = Claims {
        sub: user_id.to_string(),
        org: org_id.to_string(),
        role: role.to_string(),
        email: email.to_string(),
        name: name.to_string(),
        aud: config.resource_uri.clone(),
        exp: now + config.token_expiry_secs as usize,
        iat: now,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )
    .map_err(|e| format!("Failed to create token: {}", e))
}

/// Create a refresh token (longer expiry, minimal claims)
pub fn create_refresh_token(config: &AuthConfig, user_id: &str) -> Result<String, String> {
    let now = chrono::Utc::now().timestamp() as usize;

    #[derive(Serialize)]
    struct RefreshClaims { sub: String, exp: usize, iat: usize, refresh: bool }

    encode(
        &Header::default(),
        &RefreshClaims {
            sub: user_id.to_string(),
            exp: now + config.refresh_expiry_secs as usize,
            iat: now,
            refresh: true,
        },
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )
    .map_err(|e| format!("Failed to create refresh token: {}", e))
}

/// Validate a JWT token and return claims
pub fn validate_token(token: &str, secret: &str, expected_audience: &str) -> Result<Claims, String> {
    let mut validation = Validation::default();
    // RFC 8707: reject any token not minted for this resource. jsonwebtoken
    // requires the token's `aud` to be a member of this set.
    validation.set_audience(&[expected_audience]);
    validation.set_required_spec_claims(&["exp", "aud"]);
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map(|data| data.claims)
    .map_err(|e| format!("{}", e))
}

/// Hash a password with Argon2id, returning a PHC-format string that embeds
/// the salt and parameters. Each call produces a different output for the same
/// input, so hashes must be compared with `verify_password`, never with `==`.
pub fn hash_password(password: &str) -> String {
    use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
    use argon2::Argon2;

    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .expect("argon2 hashing failed")
        .to_string()
}

/// Verify a password against a stored hash.
///
/// Verify a password against an Argon2id PHC hash. Argon2 only — the legacy
/// unsalted SHA-256 path is retired (E2 #4): a non-Argon2 stored hash is rejected
/// (such a user must reset their password). All platform-created hashes are
/// Argon2 (`hash_password`), so this affects only pre-migration rows, if any.
pub fn verify_password(password: &str, stored: &str) -> bool {
    use argon2::password_hash::{PasswordHash, PasswordVerifier};
    use argon2::Argon2;

    if !stored.starts_with("$argon2") {
        warn!("rejecting login against a non-Argon2 password hash (SHA-256 retired); user must reset");
        return false;
    }
    PasswordHash::new(stored)
        .map(|parsed| {
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok()
        })
        .unwrap_or(false)
}

/// Lookup user by email in the A3T platform database and verify password
pub async fn authenticate_user(
    db: &dyn flowr_ce::platform_db::PlatformDb,
    email: &str,
    password: &str,
) -> Result<(String, String, String, String, String), String> {
    // Query by email
    let filter = format!("email = '{}'", email.replace('\'', "''"));
    let batches: Vec<arrow_array::RecordBatch> = db
        .query("users", Some(&filter), 1)
        .await
        .map_err(|e| format!("Failed to read users: {}", e))?
        .into_iter()
        .filter(|b| b.num_rows() > 0)
        .collect();

    if batches.is_empty() {
        return Err("Invalid email or password".to_string());
    }

    let batch = &batches[0];

    // Extract fields
    let get_str = |name: &str| -> Result<String, String> {
        batch
            .column_by_name(name)
            .and_then(|c| c.as_any().downcast_ref::<arrow_array::StringArray>())
            .map(|a| a.value(0).to_string())
            .ok_or_else(|| format!("Missing column: {}", name))
    };

    let stored_hash = get_str("password_hash")?;

    if !verify_password(password, &stored_hash) {
        return Err("Invalid email or password".to_string());
    }

    let status = get_str("status")?;
    if status != "active" {
        return Err(format!("Account is {}", status));
    }

    let user_id = get_str("id")?;
    let org_id = get_str("org_id")?;
    let role = get_str("role")?;
    let name = get_str("name")?;

    Ok((user_id, org_id, role, email.to_string(), name))
}


// ── E2 #4: OIDC AuthN (EE) — RS256/JWKS via aesop-tower::AuthnLayer ──
// Installed instead of the HS256 middleware when `security.authn = "oidc"`.
// Verification (signature, iss/aud/exp) lives in aesop-tower; this middleware
// is the thin consumer glue that maps validated claims onto the CE `Principal`.
#[cfg(feature = "ee")]
pub struct OidcState {
    pub cache: aesop_tower::oidc::JwksCache,
    pub config: aesop_tower::oidc::AuthnConfig,
}

#[cfg(feature = "ee")]
pub async fn oidc_auth_middleware(
    headers: HeaderMap,
    mut request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();
    if is_public(&path) {
        // Opportunistic auth on public paths (webhooks): honor a valid token if
        // presented so the console chat runs on behalf of the real user (agent
        // gets `on_behalf_of`); absent/invalid token → stays public.
        if let Some(state) = request.extensions().get::<Arc<OidcState>>().cloned() {
            if let Some(token) = extract_bearer_token(&headers) {
                if let Ok(claims) = aesop_tower::oidc::verify(&token, &state.cache, &state.config).await {
                    let principal = principal_from_oidc_claims(&claims);
                    request.extensions_mut().insert(principal);
                }
            }
        }
        return next.run(request).await;
    }
    let state = match request.extensions().get::<Arc<OidcState>>() {
        Some(s) => s.clone(),
        None => {
            warn!("OidcState not found in request extensions");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "OIDC not configured" })),
            )
                .into_response();
        }
    };
    let token = match extract_bearer_token(&headers) {
        Some(t) => t,
        None => return unauthorized(&state.config.issuer, "Missing bearer token"),
    };
    match aesop_tower::oidc::verify(&token, &state.cache, &state.config).await {
        Ok(claims) => {
            let principal = principal_from_oidc_claims(&claims);
            debug!(sub = %principal.id(), "OIDC authenticated");
            request.extensions_mut().insert(principal);
            next.run(request).await
        }
        Err(e) => {
            debug!(error = %e, path = %path, "OIDC verification failed");
            unauthorized(&state.config.issuer, "Invalid or expired token")
        }
    }
}

/// Dual AuthN: accept BOTH the local HS256 token (the console) and a Keycloak
/// OIDC (RS256/JWKS) token (external MCP clients such as Claude Desktop). Tries
/// HS256 first, then OIDC. Requires both `AuthConfig` and `OidcState` extensions.
#[cfg(feature = "ee")]
pub async fn dual_auth_middleware(headers: HeaderMap, mut request: Request, next: Next) -> Response {
    let path = request.uri().path().to_string();
    let resource_uri = request
        .extensions()
        .get::<Arc<AuthConfig>>()
        .map(|c| c.resource_uri.clone())
        .unwrap_or_default();

    // Try to validate a bearer token as HS256 first, then OIDC. Returns true if
    // a principal was attached.
    async fn attach(headers: &HeaderMap, request: &mut Request) -> bool {
        let Some(token) = extract_bearer_token(headers) else { return false };
        if let Some(config) = request.extensions().get::<Arc<AuthConfig>>().cloned() {
            if let Ok(claims) = validate_token(&token, &config.jwt_secret, &config.resource_uri) {
                let principal = flowr_policy::Principal::human(
                    claims.sub.clone(),
                    flowr_policy::roles::canonicalize(&claims.role),
                );
                request.extensions_mut().insert(claims);
                request.extensions_mut().insert(principal);
                return true;
            }
        }
        if let Some(state) = request.extensions().get::<Arc<OidcState>>().cloned() {
            if let Ok(claims) = aesop_tower::oidc::verify(&token, &state.cache, &state.config).await {
                request.extensions_mut().insert(principal_from_oidc_claims(&claims));
                return true;
            }
        }
        false
    }

    if is_public(&path) {
        // Opportunistic on public paths (webhooks): attach a principal if either
        // token type validates; absent/invalid → stays public.
        let _ = attach(&headers, &mut request).await;
        return next.run(request).await;
    }
    if extract_bearer_token(&headers).is_none() {
        return unauthorized(&resource_uri, "Missing or invalid Authorization header. Use: Bearer <token>");
    }
    if attach(&headers, &mut request).await {
        return next.run(request).await;
    }
    unauthorized(&resource_uri, "Invalid token (neither local HS256 nor OIDC verification succeeded)")
}

/// Map validated OIDC claims onto the CE `Principal`. `preferred_username` (else
/// `sub`) is the id; realm roles (Keycloak `realm_access.roles`) are each pushed
/// through `roles::canonicalize` (unknown IdP roles pass through unchanged).
#[cfg(feature = "ee")]
fn principal_from_oidc_claims(claims: &serde_json::Value) -> flowr_policy::Principal {
    let id = claims
        .get("preferred_username")
        .and_then(|v| v.as_str())
        .or_else(|| claims.get("sub").and_then(|v| v.as_str()))
        .unwrap_or("unknown")
        .to_string();
    let mut roles: Vec<String> = Vec::new();
    if let Some(rs) = claims
        .get("realm_access")
        .and_then(|v| v.get("roles"))
        .and_then(|v| v.as_array())
    {
        for r in rs {
            if let Some(s) = r.as_str() {
                for canon in flowr_policy::roles::canonicalize(s) {
                    if !roles.contains(&canon) {
                        roles.push(canon);
                    }
                }
            }
        }
    }
    flowr_policy::Principal::human(id, roles)
}
