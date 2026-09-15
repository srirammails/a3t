"""
A3T Platform Schema Install Script
Creates LanceDB tables for the A3T platform.
Idempotent — skips existing tables.

Can be run standalone or called from the a3t_schema_install flow.

Usage:
    python a3t_schema_install.py [--db-path /path/to/lance.db]
"""

import os
import sys
import json
import uuid
import hashlib
from datetime import datetime, timezone

try:
    import lancedb
    import pyarrow as pa
except ImportError:
    print("ERROR: lancedb and pyarrow required. Install: pip install lancedb pyarrow")
    sys.exit(1)


# ── Default DB path ──
DEFAULT_DB_PATH = os.environ.get(
    "A3T_DB_PATH",
    os.path.expanduser("~/.local/share/a3t/a3t.lance")
)


# ── Schema definitions ──
# Each table is defined as: { name: str, columns: [(name, pa.type, nullable)] }

TABLES = {
    # ── Organizations ──
    "orgs": [
        ("id", pa.utf8(), False),
        ("name", pa.utf8(), False),
        ("slug", pa.utf8(), False),
        ("plan", pa.utf8(), False),           # free|pro|enterprise
        ("settings", pa.utf8(), True),        # JSON
        ("created_at", pa.utf8(), False),
        ("updated_at", pa.utf8(), False),
    ],

    # ── Users ──
    "users": [
        ("id", pa.utf8(), False),
        ("org_id", pa.utf8(), False),
        ("email", pa.utf8(), False),
        ("name", pa.utf8(), False),
        ("password_hash", pa.utf8(), False),
        ("role", pa.utf8(), False),           # admin|operator|viewer
        ("avatar", pa.utf8(), True),
        ("status", pa.utf8(), False),         # active|suspended|invited
        ("last_login_at", pa.utf8(), True),
        ("created_at", pa.utf8(), False),
        ("updated_at", pa.utf8(), False),
    ],

    # ── Sessions ──
    "sessions": [
        ("id", pa.utf8(), False),
        ("user_id", pa.utf8(), False),
        ("token_hash", pa.utf8(), False),
        ("refresh_hash", pa.utf8(), False),
        ("ip", pa.utf8(), True),
        ("user_agent", pa.utf8(), True),
        ("expires_at", pa.utf8(), False),
        ("created_at", pa.utf8(), False),
    ],

    # ── API Keys ──
    "api_keys": [
        ("id", pa.utf8(), False),
        ("user_id", pa.utf8(), False),
        ("org_id", pa.utf8(), False),
        ("key_hash", pa.utf8(), False),
        ("name", pa.utf8(), False),
        ("scopes", pa.utf8(), False),         # JSON array
        ("last_used_at", pa.utf8(), True),
        ("expires_at", pa.utf8(), True),
        ("created_at", pa.utf8(), False),
    ],

    # ── Invitations ──
    "invitations": [
        ("id", pa.utf8(), False),
        ("org_id", pa.utf8(), False),
        ("email", pa.utf8(), False),
        ("role", pa.utf8(), False),
        ("invited_by", pa.utf8(), False),
        ("token", pa.utf8(), False),
        ("status", pa.utf8(), False),         # pending|accepted|expired
        ("expires_at", pa.utf8(), False),
        ("created_at", pa.utf8(), False),
    ],

    # ── Roles (RBAC) ──
    "roles": [
        ("id", pa.utf8(), False),
        ("org_id", pa.utf8(), False),
        ("name", pa.utf8(), False),
        ("description", pa.utf8(), True),
        ("is_system", pa.utf8(), False),      # true|false
        ("created_at", pa.utf8(), False),
    ],

    # ── Permissions (RBAC) ──
    "permissions": [
        ("id", pa.utf8(), False),
        ("role_id", pa.utf8(), False),
        ("resource_type", pa.utf8(), False),  # template|agent|canvas|resource|flow|memory|settings
        ("action", pa.utf8(), False),         # create|read|update|delete|execute|deploy
        ("scope", pa.utf8(), False),          # own|org|all
        ("created_at", pa.utf8(), False),
    ],

    # ── Template Registry ──
    "template_registry": [
        ("id", pa.utf8(), False),
        ("org_id", pa.utf8(), False),
        ("name", pa.utf8(), False),
        ("version", pa.utf8(), False),
        ("source", pa.utf8(), False),         # my|training|community
        ("spec_yaml", pa.utf8(), False),
        ("digest", pa.utf8(), True),
        ("signed_by", pa.utf8(), True),
        ("tags", pa.utf8(), True),            # JSON array
        ("vertical", pa.utf8(), True),
        ("published", pa.utf8(), False),      # true|false
        ("created_by", pa.utf8(), False),
        ("created_at", pa.utf8(), False),
        ("updated_at", pa.utf8(), False),
    ],

    # ── Agent Configs ──
    "agent_configs": [
        ("id", pa.utf8(), False),
        ("org_id", pa.utf8(), False),
        ("name", pa.utf8(), False),
        ("template_id", pa.utf8(), True),
        ("template_version", pa.utf8(), True),
        ("mode", pa.utf8(), False),           # ephemeral|session|persistent|reactive
        ("variables", pa.utf8(), True),       # JSON
        ("resource_bindings", pa.utf8(), True),  # JSON
        ("namespace", pa.utf8(), False),
        ("status", pa.utf8(), False),         # draft|deployed|stopped
        ("created_by", pa.utf8(), False),
        ("created_at", pa.utf8(), False),
        ("updated_at", pa.utf8(), False),
    ],

    # ── Agent Events ──
    "agent_events": [
        ("id", pa.utf8(), False),
        ("agent_config_id", pa.utf8(), False),
        ("event_type", pa.utf8(), False),     # deployed|scaled|paused|stopped|error|restarted
        ("payload", pa.utf8(), True),         # JSON
        ("created_at", pa.utf8(), False),
    ],

    # ── Resource Registry ──
    "resource_registry": [
        ("id", pa.utf8(), False),
        ("org_id", pa.utf8(), False),
        ("name", pa.utf8(), False),
        ("type", pa.utf8(), False),           # mcp|database|llm|variable|flow|script
        ("config", pa.utf8(), False),         # JSON
        ("health_status", pa.utf8(), False),  # healthy|degraded|error|unknown
        ("last_checked_at", pa.utf8(), True),
        ("created_by", pa.utf8(), False),
        ("created_at", pa.utf8(), False),
        ("updated_at", pa.utf8(), False),
    ],

    # ── Canvas Specs ──
    "canvas_specs": [
        ("id", pa.utf8(), False),
        ("org_id", pa.utf8(), False),
        ("name", pa.utf8(), False),
        ("namespace", pa.utf8(), False),
        ("spec_yaml", pa.utf8(), False),
        ("generation", pa.utf8(), False),
        ("status", pa.utf8(), False),         # draft|applied|reconciling|synced|drifted
        ("created_by", pa.utf8(), False),
        ("created_at", pa.utf8(), False),
        ("updated_at", pa.utf8(), False),
    ],

    # ── Canvas Events ──
    "canvas_events": [
        ("id", pa.utf8(), False),
        ("canvas_id", pa.utf8(), False),
        ("event_type", pa.utf8(), False),     # applied|reconciled|drifted|rolled_back
        ("payload", pa.utf8(), True),         # JSON
        ("created_at", pa.utf8(), False),
    ],

    # ── Audit Log ──
    "audit_log": [
        ("id", pa.utf8(), False),
        ("org_id", pa.utf8(), False),
        ("user_id", pa.utf8(), False),
        ("action", pa.utf8(), False),         # create|update|delete|login|logout|deploy|execute
        ("resource_type", pa.utf8(), False),
        ("resource_id", pa.utf8(), True),
        ("details", pa.utf8(), True),         # JSON
        ("ip", pa.utf8(), True),
        ("created_at", pa.utf8(), False),
    ],

    # ── Settings ──
    "settings": [
        ("id", pa.utf8(), False),
        ("org_id", pa.utf8(), False),
        ("key", pa.utf8(), False),
        ("value", pa.utf8(), False),          # JSON
        ("updated_by", pa.utf8(), True),
        ("updated_at", pa.utf8(), False),
    ],
}


def get_db(db_path: str = None) -> lancedb.DBConnection:
    """Open or create a LanceDB connection."""
    path = db_path or DEFAULT_DB_PATH
    os.makedirs(os.path.dirname(path), exist_ok=True)
    return lancedb.connect(path)


def create_table(db: lancedb.DBConnection, table_name: str, columns: list, existing: list = None) -> str:
    """Create a single table if it doesn't exist. Returns status message."""
    if existing is not None and table_name in existing:
        return f"SKIP {table_name} (already exists)"

    # Build Arrow schema
    fields = [pa.field(name, dtype, nullable=nullable) for name, dtype, nullable in columns]
    schema = pa.schema(fields)

    # Create empty table with schema — catch "already exists" for safety
    empty_batch = pa.RecordBatch.from_pydict(
        {name: [] for name, _, _ in columns},
        schema=schema,
    )
    try:
        db.create_table(table_name, [empty_batch])
        return f"CREATE {table_name} ({len(columns)} columns)"
    except (ValueError, Exception) as e:
        if "already exists" in str(e):
            return f"SKIP {table_name} (already exists)"
        raise


def seed_defaults(db: lancedb.DBConnection, org_id: str = None) -> list:
    """Seed default org, admin user, roles, and permissions."""
    now = datetime.now(timezone.utc).isoformat()
    results = []

    # ── Default org ──
    if org_id is None:
        org_id = str(uuid.uuid4())

    orgs_table = db.open_table("orgs")
    existing_orgs = orgs_table.to_arrow()
    if existing_orgs.num_rows == 0:
        orgs_table.add([{
            "id": org_id,
            "name": "Default Organization",
            "slug": "default",
            "plan": "pro",
            "settings": json.dumps({}),
            "created_at": now,
            "updated_at": now,
        }])
        results.append(f"SEED org: Default Organization ({org_id})")
    else:
        org_id = existing_orgs.column("id")[0].as_py()
        results.append(f"SKIP org (exists: {org_id})")

    # ── Default admin user ──
    users_table = db.open_table("users")
    existing_users = users_table.to_arrow()
    if existing_users.num_rows == 0:
        admin_id = str(uuid.uuid4())
        # Default password: "admin123" — MUST be changed on first login
        password_hash = hashlib.sha256("admin123".encode()).hexdigest()
        users_table.add([{
            "id": admin_id,
            "org_id": org_id,
            "email": "admin@a3t.local",
            "name": "Admin",
            "password_hash": password_hash,
            "role": "admin",
            "avatar": None,
            "status": "active",
            "last_login_at": None,
            "created_at": now,
            "updated_at": now,
        }])
        results.append(f"SEED user: admin@a3t.local ({admin_id})")
    else:
        results.append(f"SKIP users (exists: {existing_users.num_rows} users)")

    # ── Default roles ──
    roles_table = db.open_table("roles")
    existing_roles = roles_table.to_arrow()
    if existing_roles.num_rows == 0:
        default_roles = [
            {"id": str(uuid.uuid4()), "org_id": org_id, "name": "admin",
             "description": "Full platform access", "is_system": "true", "created_at": now},
            {"id": str(uuid.uuid4()), "org_id": org_id, "name": "operator",
             "description": "Deploy and manage agents, read templates", "is_system": "true", "created_at": now},
            {"id": str(uuid.uuid4()), "org_id": org_id, "name": "viewer",
             "description": "Read-only access", "is_system": "true", "created_at": now},
        ]
        roles_table.add(default_roles)
        results.append(f"SEED roles: admin, operator, viewer")

        # ── Default permissions ──
        perms_table = db.open_table("permissions")
        role_map = {r["name"]: r["id"] for r in default_roles}

        resource_types = ["template", "agent", "canvas", "resource", "flow", "memory", "settings"]
        all_actions = ["create", "read", "update", "delete", "execute", "deploy"]

        perms = []

        # Admin: all permissions on all resources
        for rt in resource_types:
            for action in all_actions:
                perms.append({
                    "id": str(uuid.uuid4()),
                    "role_id": role_map["admin"],
                    "resource_type": rt,
                    "action": action,
                    "scope": "org",
                    "created_at": now,
                })

        # Operator: CRUD + lifecycle on agents, read + deploy on templates, read + apply on canvas
        operator_perms = {
            "template": ["read", "deploy"],
            "agent": ["create", "read", "update", "delete", "execute"],
            "canvas": ["read", "deploy"],
            "resource": ["read"],
            "flow": ["read", "execute"],
            "memory": ["read"],
            "settings": ["read"],
        }
        for rt, actions in operator_perms.items():
            for action in actions:
                perms.append({
                    "id": str(uuid.uuid4()),
                    "role_id": role_map["operator"],
                    "resource_type": rt,
                    "action": action,
                    "scope": "org",
                    "created_at": now,
                })

        # Viewer: read-only on everything except settings
        for rt in resource_types:
            if rt != "settings":
                perms.append({
                    "id": str(uuid.uuid4()),
                    "role_id": role_map["viewer"],
                    "resource_type": rt,
                    "action": "read",
                    "scope": "org",
                    "created_at": now,
                })

        perms_table.add(perms)
        results.append(f"SEED permissions: {len(perms)} rules")
    else:
        results.append(f"SKIP roles (exists: {existing_roles.num_rows} roles)")

    return results


def install(db_path: str = None) -> dict:
    """Run full schema install — create all tables and seed defaults."""
    db = get_db(db_path)
    results = {"tables": [], "seeds": [], "db_path": db_path or DEFAULT_DB_PATH}

    # Get existing tables once
    existing = db.table_names() if hasattr(db, 'table_names') else db.list_tables()

    # Create all tables
    for table_name, columns in TABLES.items():
        status = create_table(db, table_name, columns, existing)
        results["tables"].append(status)
        print(f"  {status}")

    # Seed defaults
    print("\nSeeding defaults...")
    seed_results = seed_defaults(db)
    results["seeds"] = seed_results
    for s in seed_results:
        print(f"  {s}")

    return results


# ── CLI entry point ──
if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description="A3T Platform Schema Install")
    parser.add_argument("--db-path", default=None, help=f"LanceDB path (default: {DEFAULT_DB_PATH})")
    args = parser.parse_args()

    print(f"A3T Schema Install")
    print(f"Database: {args.db_path or DEFAULT_DB_PATH}")
    print(f"Tables: {len(TABLES)}")
    print()

    result = install(args.db_path)

    print(f"\nDone. {len(result['tables'])} tables, {len(result['seeds'])} seed operations.")
