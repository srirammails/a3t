# E2 #4 — OIDC AuthnLayer (aesop-tower) — Plan for review

> Status: plan · 2026-07-27 · consumer of `A3T_EE_AESOP_RECONCILIATION.md`.
> **Blocked on one decision: which IdP** (§5). Everything else is scoped below.

## 1. Where we are

- **CE/EE auth today** is HS256: `flowr-server/src/auth.rs` `jwt_auth_middleware` validates a JWT
  with a **shared secret**, and (E0.4a + E2 #5 Phase B) builds `Principal::human(sub,
  roles::canonicalize(role))` into request extensions. Login mints these tokens; passwords are
  checked in `authenticate_user` (with a legacy **SHA-256** path flagged for retirement).
- **`aesop-tower`** ships only the `aesop.*` field/span constants; `AuthnLayer`/`AuditLayer`/
  `MetricsLayer` are commented placeholders (AESOP Phase 1 left them as stubs — this is that work).
- The plan (E2 #4): *"`aesop-tower::AuthnLayer` (RS256/JWKS, OIDC) replaces the HS256 layer in EE;
  also retires the fragile SHA-256 legacy password path."*

## 2. What "OIDC AuthnLayer" actually entails

1. **`aesop-tower::AuthnLayer`** — a `tower::Layer` that, per request:
   - extracts the bearer token; verifies **RS256** signature against the issuer's **JWKS**
     (fetched from `<issuer>/.well-known/openid-configuration` → `jwks_uri`, cached with rotation);
   - validates `iss`, `aud`, `exp`, `nbf`;
   - maps standard OIDC claims → `aesop_core::Principal` (sub → id; a configured roles/groups claim
     → `roles::canonicalize`); inserts `SecurityContext` into extensions.
   - Accepts **runtime-minted agent tokens** for agent-internal calls (a second verifier/keyset).
   - New deps: `jsonwebtoken` (RS256), an async HTTP client for JWKS, a small JWKS cache.
2. **EE wiring** — replace `middleware::from_fn(jwt_auth_middleware)` with the AuthnLayer; the
   handlers already read `Extension<Principal>`, so downstream is unchanged (that was the point of
   E0.4a). Config: `security.authn = "hs256" | "oidc"`, issuer/audience/jwks under `auth.oidc.*`.
3. **Retire SHA-256** — move password verification fully to Argon2 (already partly there?); remove
   the legacy SHA-256 branch. *Independent of OIDC; can land first.*
4. **CE stays HS256** — OIDC is an EE feature (matches "EE connects to SSO"). CE keeps the simple
   shared-secret layer; the AuthnLayer is installed only in the EE image, one line, like the hook.

## 3. Division across CE / EE / AESOP (per the north-star)
- **AESOP** owns the **AuthnLayer** (token→Principal, JWKS, RS256) — it's the "authN mostly in
  aesop" piece; EE just installs it.
- **CE** keeps the `Principal`/`roles` contract + the HS256 dev layer.
- **EE** = CE + `aesop-tower::AuthnLayer` + `security.authn="oidc"` + IdP config. SSO connect =
  point it at the org's IdP.

## 4. Testing without a live IdP
- **Unit**: generate an RS256 keypair in-test, sign a token, serve a static JWKS, assert the layer
  verifies + maps claims + rejects bad `iss`/`aud`/`exp`/signature. No external IdP needed.
- **Integration (needs an IdP)**: a real OIDC provider (see §5) issues a token; EE verifies it
  live on vc-dev. This is the only step that needs the decision.

## 4a. IdP — Keycloak STOOD UP + DURABLE on vc-dev (2026-07-28) ✅

Decision made: **Keycloak**. Deployed in vc-dev ns `a3t` (dev-mode, ephemeral H2 — no storage
class — made durable via **realm-import**, so restarts recreate everything):
- **Deployment** `keycloak` (image `quay.io/keycloak/keycloak:24.0`, `start-dev --import-realm`,
  `KC_HTTP_ENABLED=true`, `KC_HOSTNAME_STRICT=false`) + **Service** `keycloak:8080` (ClusterIP).
- **Realm import** ConfigMap `keycloak-realm` (`a3t-realm.json`) mounted at
  `/opt/keycloak/data/import`. Realm `a3t`, public client `a3t-ee` (direct grants on), realm roles
  `operator`/`admin`, users `alice`/`alice-pw` (operator) + `bob-admin`/`bob-pw` (admin).
- **Master admin**: `admin` / `Kc-A3T-dev-2026` (dev only).
- **GOTCHA**: pinning `KC_HOSTNAME` broke pod-IP access (404) — reverted. With
  `hostname-strict=false` the issuer comes from the request `Host` header, which is exactly what an
  in-cluster consumer sends via the service DNS, so it's naturally consistent. **Do not set
  `KC_HOSTNAME`.** Also, dev-mode H2 is wiped on every restart — the import ConfigMap is what makes
  it durable; edit that + restart to change realm config.

**Concrete config for the AuthnLayer (verified against real tokens):**
- **issuer** = `http://keycloak.a3t.svc:8080/realms/a3t`
- **jwks_uri** = `http://keycloak.a3t.svc:8080/realms/a3t/protocol/openid-connect/certs` (RS256)
- **discovery** = `<issuer>/.well-known/openid-configuration`
- **claim mapping**: `sub`/`preferred_username` → principal id; roles at
  `realm_access.roles` (a `Set<String>` incl. Keycloak defaults like `offline_access`; filter to
  A3T roles or just `canonicalize` each — unknowns pass through). `aud`/`azp` = `a3t-ee`.
- **test token**: `POST <issuer>/protocol/openid-connect/token`
  `client_id=a3t-ee&username=alice&password=alice-pw&grant_type=password&scope=openid`
  (send `Host: keycloak.a3t.svc:8080` when curling via pod IP so `iss` matches).

## 5. Decision required — the IdP  *(RESOLVED: Keycloak, §4a)*
OIDC verification is meaningless without an issuer. Options:
- **(a) Keycloak** self-hosted on the metal box / a vcluster — full control, realistic, more setup.
- **(b) A hosted IdP** (Auth0/Entra/Google) — fastest to a real token, external dependency + creds.
- **(c) A tiny static-JWKS test issuer** — enough to prove the layer end-to-end on vc-dev without a
  full IdP; not a real SSO, but unblocks the live verify.

Recommendation: build the AuthnLayer + unit tests now (no IdP needed), land **SHA-256 retirement**
in parallel (independent), and use **(c)** for the first live verify; stand up **(a) Keycloak** when
real SSO is wanted.

## 6. Sequencing
1. **SHA-256 retirement** (independent, low-risk) — *pending.*
2. ✅ **`aesop-tower::oidc`** DONE — `JwksCache` (fetch + cache by `kid`, refresh on unknown kid),
   `AuthnConfig::from_issuer`, `verify()` (RS256 + `iss`/`aud`/`exp`). 5 unit tests (valid / wrong
   issuer / expired / wrong audience / unknown-kid-fails-closed) with an in-test RSA keypair + static
   keys — no IdP needed. aesop-tower keeps its own dep (jsonwebtoken 9 `use_pem`, reqwest rustls).
3. ✅ **EE wiring** DONE — `security.authn` = `hs256` (default) | `oidc`; the `oidc` arm (EE-gated,
   `dep:aesop-tower`) installs `oidc_auth_middleware` (in `auth.rs`) which verifies via aesop-tower
   and maps claims → `flowr_policy::Principal` (`preferred_username`/`sub` + `realm_access.roles`
   through `roles::canonicalize`). Precedence env → settings → config so a deploy can flip it. CE
   build has no OIDC/aesop-tower (AESOP-free). Behavior-neutral at `hs256`.
4. ✅ **Live verify DONE (`:2026-07-28.8`, vc-dev).** `hs256` (default) neutral: superadmin login +
   flows work, `/edition` unchanged. Flipped `FLOWR_SECURITY_AUTHN=oidc` +
   `FLOWR_SECURITY_OIDC_ISSUER=http://keycloak.a3t.svc:8080/realms/a3t` →
   - `alice` (Keycloak, `operator`) `list_flows` → **200 allowed** (RS256 token verified via JWKS,
     mapped to `Operator`); `create_flow` → **denied** (`cedar denied mcp:call on Tool::create_flow`
     — the E2 #5 authoring RBAC fired on the OIDC identity);
   - superadmin **HS256 token → 401** (OIDC rejects non-Keycloak tokens).
   - Log: `AuthN: OIDC (RS256/JWKS via aesop-tower)`. Toggled back to `hs256` cleanly (superadmin
     restored) — **the one-setting flip works both ways**, mirroring `policy_hook`.
   **Every layer composed: OIDC verify → claim→role mapping → Cedar RBAC → HS256 off.** SHA-256
   password retirement is the last E2 #4 item (independent).

## 7. Risk / rollback
- Steps 1–3 are behavior-neutral while `security.authn=hs256` (the default). Flipping to `oidc` is
  the only behavior change and is per-deployment — HS256 stays as the break-glass, mirroring the
  `security.policy_hook=allow_all` pattern. Auth-path risk is real, so the flip happens only after
  the live verify passes.
