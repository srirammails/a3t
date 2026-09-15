//! The canonical A3T role vocabulary — the single source of truth for role
//! names, shared by CE, EE, and every Cedar policy (analogous to the `actions`
//! registry). Lives in the CE contract crate because *roles are infrastructure*:
//! CE defines the vocabulary; EE's AuthN layer (JWT today, OIDC/SSO later) maps a
//! provider's role claim onto these names, and the policies check them.
//!
//! The platform issues a single lowercase role per user (`superadmin`, `admin`,
//! `operator`, `viewer`); [`canonicalize`] maps that onto the canonical role(s).

/// Full control, including InfoSec/policy authoring.
pub const SUPER_ADMIN: &str = "SuperAdmin";
/// InfoSec: owns Cedar policy resources (the SoD seam, §6.4). Granted by
/// `superadmin` today; a distinct assignable role once roles are first-class.
pub const SECURITY_ADMIN: &str = "SecurityAdmin";
/// Platform administration (resources, settings, users) short of policy.
pub const ADMIN: &str = "Admin";
/// Runs flows/agents; no administrative mutation.
pub const OPERATOR: &str = "Operator";
/// Read-only.
pub const VIEWER: &str = "Viewer";

/// Every canonical role. Policies and the (future) role-seeding reference this.
pub const ALL: &[&str] = &[SUPER_ADMIN, SECURITY_ADMIN, ADMIN, OPERATOR, VIEWER];

/// Map a provider's `role` claim to the canonical role(s) it grants, **expanding
/// the hierarchy** so a higher role implicitly holds every lower one:
/// `SuperAdmin ⊃ Admin ⊃ Operator ⊃ Viewer`. Expanding here (rather than encoding
/// hierarchy in Cedar) keeps every policy a flat membership check —
/// `principal.roles.contains("Admin")` matches a superadmin too. `superadmin`
/// also grants `SecurityAdmin` (the InfoSec seam) until that becomes separately
/// assignable. Unknown roles pass through unchanged — never silently drop a role.
pub fn canonicalize(provider_role: &str) -> Vec<String> {
    match provider_role.trim().to_ascii_lowercase().as_str() {
        "superadmin" | "super_admin" | "super-admin" => vec![
            SUPER_ADMIN.into(),
            SECURITY_ADMIN.into(),
            ADMIN.into(),
            OPERATOR.into(),
            VIEWER.into(),
        ],
        "securityadmin" | "security_admin" | "security-admin" => vec![SECURITY_ADMIN.into()],
        "admin" => vec![ADMIN.into(), OPERATOR.into(), VIEWER.into()],
        // `operator` is the developer tier: full authoring (flows/agents/templates/
        // resources) via `Admin`, but never the InfoSec seam (`SecurityAdmin`).
        "operator" | "developer" => vec![ADMIN.into(), OPERATOR.into(), VIEWER.into()],
        "viewer" => vec![VIEWER.into()],
        _ => vec![provider_role.to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has(role: &str, s: &str) -> bool {
        canonicalize(role).contains(&s.to_string())
    }

    #[test]
    fn superadmin_grants_infosec_and_all_lower_roles() {
        for r in [SUPER_ADMIN, SECURITY_ADMIN, ADMIN, OPERATOR, VIEWER] {
            assert!(has("superadmin", r), "superadmin should grant {r}");
        }
    }

    #[test]
    fn hierarchy_expands_downward_only() {
        // admin holds admin/operator/viewer but NOT superadmin/securityadmin.
        assert!(has("admin", ADMIN) && has("admin", OPERATOR) && has("admin", VIEWER));
        assert!(!has("admin", SUPER_ADMIN) && !has("admin", SECURITY_ADMIN));
        // viewer holds only viewer.
        assert_eq!(canonicalize("viewer"), vec![VIEWER]);
    }

    #[test]
    fn unknown_role_passes_through() {
        assert_eq!(canonicalize("custom-role"), vec!["custom-role".to_string()]);
    }
}
