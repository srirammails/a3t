//! Auth HTTP routes — login, signup, refresh, me
//!
//! These endpoints are PUBLIC (no JWT required).
//! They authenticate against the A3T users table in LanceDB.

use axum::{
    extract::Extension,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tracing::{info, warn};

use crate::auth::{
    authenticate_user, create_refresh_token, create_token, extract_bearer_token, hash_password,
    validate_token, AuthConfig,
};

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct SignupRequest {
    pub name: String,
    pub email: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct AuthResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub user: UserInfo,
}

#[derive(Serialize)]
pub struct UserInfo {
    pub id: String,
    pub org_id: String,
    pub email: String,
    pub name: String,
    pub role: String,
}

/// POST /auth/login
pub async fn login_handler(
    Extension(config): Extension<Arc<AuthConfig>>,
    Json(body): Json<LoginRequest>,
) -> impl IntoResponse {
    if body.email.is_empty() || body.password.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Email and password required" })),
        ).into_response();
    }

    // Resolve the identity: try the local A3T user store first (superadmin, local
    // signups); on failure fall back to Keycloak (EE) so SSO-provisioned users can
    // sign in through the same email/password form. `(user_id, org, role, email, name)`.
    let resolved = match authenticate_user(&config.a3t_db_path, &body.email, &body.password).await {
        Ok(t) => Some(t),
        Err(local_err) => {
            let kc = keycloak_login(&config, &body.email, &body.password).await;
            if kc.is_none() {
                warn!(email = %body.email, error = %local_err, "Login failed (local + oidc)");
            }
            kc
        }
    };

    let Some((user_id, org_id, role, email, name)) = resolved else {
        return (StatusCode::UNAUTHORIZED, Json(json!({ "error": "Invalid credentials" }))).into_response();
    };

    let access_token = match create_token(&config, &user_id, &org_id, &role, &email, &name) {
        Ok(t) => t,
        Err(e) => {
            warn!(error = %e, "Failed to create token");
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e }))).into_response();
        }
    };
    let refresh_token = match create_refresh_token(&config, &user_id) {
        Ok(t) => t,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e }))).into_response(),
    };

    info!(user = %user_id, email = %email, role = %role, "Login successful");
    Json(AuthResponse {
        access_token,
        refresh_token,
        token_type: "Bearer".to_string(),
        expires_in: config.token_expiry_secs,
        user: UserInfo { id: user_id, org_id, email, name, role },
    }).into_response()
}

/// Split an OIDC issuer (`http://kc:8080/realms/a3t`) into (base_url, realm).
fn split_issuer(issuer: &str) -> Option<(String, String)> {
    let s = issuer.trim_end_matches('/');
    let idx = s.find("/realms/")?;
    let base = s[..idx].to_string();
    let realm = s[idx + "/realms/".len()..].to_string();
    if base.is_empty() || realm.is_empty() { None } else { Some((base, realm)) }
}

/// Map a user's Keycloak realm roles to a single A3T role for menu/RBAC gating.
fn pick_role(roles: &[String]) -> String {
    let has = |r: &str| roles.iter().any(|x| x.eq_ignore_ascii_case(r));
    if has("superadmin") { "superadmin" }
    else if has("securityadmin") { "securityadmin" }
    else if has("admin") { "admin" }
    else if has("operator") || has("developer") { "operator" }
    else { "viewer" }.to_string()
}

/// EE login fallback: authenticate against Keycloak (password grant) and return an
/// A3T identity tuple. Returns None if OIDC isn't configured or the login fails.
async fn keycloak_login(
    config: &AuthConfig,
    email: &str,
    password: &str,
) -> Option<(String, String, String, String, String)> {
    #[cfg(feature = "ee")]
    {
        if config.oidc_issuer.is_empty() { return None; }
        let (base, realm) = split_issuer(&config.oidc_issuer)?;
        let client = reqwest::Client::new();
        match aesop_tower::oidc_admin::password_login(&client, &base, &realm, &config.oidc_client_id, email, password).await {
            Ok(id) => {
                let role = pick_role(&id.roles);
                let user_id = if id.sub.is_empty() { format!("kc-{email}") } else { id.sub.clone() };
                let out_email = id.email.clone().unwrap_or_else(|| email.to_string());
                let name = id.name.clone().unwrap_or_else(|| id.username.clone());
                info!(user = %user_id, email = %out_email, role = %role, "Login successful (oidc)");
                Some((user_id, "org-default".to_string(), role, out_email, name))
            }
            Err(e) => { warn!(email = %email, error = %e, "OIDC login failed"); None }
        }
    }
    #[cfg(not(feature = "ee"))]
    {
        let _ = (config, email, password);
        None
    }
}

/// POST /auth/signup
pub async fn signup_handler(
    Extension(config): Extension<Arc<AuthConfig>>,
    Json(body): Json<SignupRequest>,
) -> impl IntoResponse {
    if body.name.is_empty() || body.email.is_empty() || body.password.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "All fields required" }))).into_response();
    }
    if body.password.len() < 8 {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Password must be at least 8 characters" }))).into_response();
    }
    if !body.email.contains('@') {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Invalid email" }))).into_response();
    }

    // Open A3T database
    let conn = match lancedb::connect(&config.a3t_db_path).execute().await {
        Ok(c) => c,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": format!("Database error: {}", e) }))).into_response();
        }
    };

    // Check if email already exists
    let table = match conn.open_table("users").execute().await {
        Ok(t) => t,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": format!("Table error: {}", e) }))).into_response();
        }
    };

    let filter = format!("email = '{}'", body.email.replace('\'', "''"));
    use lancedb::query::{ExecutableQuery, QueryBase};
    let existing: Vec<arrow_array::RecordBatch> = match table.query().only_if(filter).limit(1).execute().await {
        Ok(stream) => {
            use futures::TryStreamExt;
            match stream.try_collect().await {
                Ok(b) => b,
                Err(e) => {
                    return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": format!("Query error: {}", e) }))).into_response();
                }
            }
        }
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": format!("Query error: {}", e) }))).into_response();
        }
    };

    if !existing.is_empty() && existing[0].num_rows() > 0 {
        return (StatusCode::CONFLICT, Json(json!({ "error": "Email already registered" }))).into_response();
    }

    // Create user
    let user_id = format!("usr-{}", uuid_short());
    let org_id = "org-default".to_string(); // Default org for now
    let role = "viewer".to_string(); // New signups get viewer role
    let now = chrono::Utc::now().to_rfc3339();
    let password_hash = hash_password(&body.password);

    let schema = std::sync::Arc::new(arrow_schema::Schema::new(vec![
        arrow_schema::Field::new("id", arrow_schema::DataType::Utf8, true),
        arrow_schema::Field::new("org_id", arrow_schema::DataType::Utf8, true),
        arrow_schema::Field::new("email", arrow_schema::DataType::Utf8, true),
        arrow_schema::Field::new("name", arrow_schema::DataType::Utf8, true),
        arrow_schema::Field::new("password_hash", arrow_schema::DataType::Utf8, true),
        arrow_schema::Field::new("role", arrow_schema::DataType::Utf8, true),
        arrow_schema::Field::new("avatar", arrow_schema::DataType::Utf8, true),
        arrow_schema::Field::new("status", arrow_schema::DataType::Utf8, true),
        arrow_schema::Field::new("last_login_at", arrow_schema::DataType::Utf8, true),
        arrow_schema::Field::new("created_at", arrow_schema::DataType::Utf8, true),
        arrow_schema::Field::new("updated_at", arrow_schema::DataType::Utf8, true),
    ]));

    let batch = match arrow_array::RecordBatch::try_new(
        schema.clone(),
        vec![
            std::sync::Arc::new(arrow_array::StringArray::from(vec![user_id.as_str()])),
            std::sync::Arc::new(arrow_array::StringArray::from(vec![org_id.as_str()])),
            std::sync::Arc::new(arrow_array::StringArray::from(vec![body.email.as_str()])),
            std::sync::Arc::new(arrow_array::StringArray::from(vec![body.name.as_str()])),
            std::sync::Arc::new(arrow_array::StringArray::from(vec![password_hash.as_str()])),
            std::sync::Arc::new(arrow_array::StringArray::from(vec![role.as_str()])),
            std::sync::Arc::new(arrow_array::StringArray::from(vec![""])),
            std::sync::Arc::new(arrow_array::StringArray::from(vec!["active"])),
            std::sync::Arc::new(arrow_array::StringArray::from(vec![""])),
            std::sync::Arc::new(arrow_array::StringArray::from(vec![now.as_str()])),
            std::sync::Arc::new(arrow_array::StringArray::from(vec![now.as_str()])),
        ],
    ) {
        Ok(b) => b,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": format!("Batch error: {}", e) }))).into_response();
        }
    };

    let batches = arrow_array::RecordBatchIterator::new(vec![Ok(batch)], schema);
    if let Err(e) = table.add(batches).execute().await {
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": format!("Insert error: {}", e) }))).into_response();
    }

    // Generate tokens
    let access_token = match create_token(&config, &user_id, &org_id, &role, &body.email, &body.name) {
        Ok(t) => t,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e }))).into_response();
        }
    };
    let refresh_token = match create_refresh_token(&config, &user_id) {
        Ok(t) => t,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e }))).into_response();
        }
    };

    info!(user = %user_id, email = %body.email, "Signup successful");

    (StatusCode::CREATED, Json(AuthResponse {
        access_token,
        refresh_token,
        token_type: "Bearer".to_string(),
        expires_in: config.token_expiry_secs,
        user: UserInfo { id: user_id, org_id, email: body.email, name: body.name, role },
    })).into_response()
}

/// GET /auth/me — returns current user from JWT claims
/// Return the caller's identity from their bearer token.
///
/// This route is merged outside the authenticated router, so it never passes
/// through `jwt_auth_middleware` and cannot rely on `Claims` being present in
/// request extensions — depending on that extractor made every call fail with
/// 500 instead of 401. The token is therefore validated here directly.
pub async fn me_handler(
    Extension(config): Extension<Arc<AuthConfig>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let token = match extract_bearer_token(&headers) {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Missing or invalid Authorization header. Use: Bearer <token>" })),
            )
                .into_response();
        }
    };

    match validate_token(&token, &config.jwt_secret, &config.resource_uri) {
        Ok(claims) => Json(UserInfo {
            id: claims.sub,
            org_id: claims.org,
            email: claims.email,
            name: claims.name,
            role: claims.role,
        })
        .into_response(),
        Err(e) => (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": format!("Invalid token: {}", e) })),
        )
            .into_response(),
    }
}

fn uuid_short() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
    format!("{:x}", t)
}
