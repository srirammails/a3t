//! Pre-flight: the real A3T CRUD flows (now carrying `metadata.a3t.policy`
//! annotations) must parse through swf-parser, not just a generic YAML loader.
use std::path::Path;

#[test]
fn annotated_a3t_flows_parse() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../a3t/backend/flows");
    let mut checked = 0;
    for f in [
        "templates_crud.yaml", "agents_crud.yaml", "canvas_crud.yaml",
        "resources_crud.yaml", "settings_crud.yaml", "users_crud.yaml",
        "audit_crud.yaml",
    ] {
        let p = dir.join(f);
        let yaml = std::fs::read_to_string(&p)
            .unwrap_or_else(|e| panic!("read {}: {e}", p.display()));
        let wf = swf_parser::parse_yaml(&yaml)
            .unwrap_or_else(|e| panic!("{f} failed to parse: {e}"));
        // and the annotation must survive parsing onto the task
        let annotated = wf.tasks.iter().filter(|t| {
            t.metadata.as_ref()
                .and_then(|m| m.get("a3t")).and_then(|a| a.get("policy")).is_some()
        }).count();
        assert!(annotated > 0, "{f}: no policy annotation survived parsing");
        println!("{f}: OK, {annotated} annotated steps");
        checked += 1;
    }
    assert_eq!(checked, 7);
}
