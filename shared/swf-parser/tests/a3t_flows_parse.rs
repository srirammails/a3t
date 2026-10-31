//! Pre-flight: the real A3T CRUD flows (now carrying `metadata.a3t.policy`
//! annotations) must parse through swf-parser, not just a generic YAML loader.
use std::path::Path;

#[test]
fn annotated_a3t_flows_parse() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../a3t/backend/flows/a3t");
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

/// The project, org and profile flows parse.
#[test]
fn a3t_project_flows_parse() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../a3t/backend/flows");
    for (f, ns) in [("system/a3t_projects_install.yaml", "system"), ("a3t/a3t_profile.yaml", "a3t"),
                    ("a3t/a3t_orgs.yaml", "a3t"), ("a3t/a3t_projects.yaml", "a3t")] {
        let p = dir.join(f);
        let yaml = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()));
        let wf = swf_parser::parse_yaml(&yaml).unwrap_or_else(|e| panic!("{f} failed to parse: {e}"));
        assert_eq!(wf.document.namespace, ns, "{f}");
    }
}

/// Every flow file parses, and sits in the namespace its folder says: system/
/// → system, a3t/ → a3t, tests/system → system-tests, tests/a3t → a3t-test.
#[test]
fn a3t_flows_are_organised_by_owner() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../a3t/backend/flows");
    for (dir, ns) in [("system", "system"), ("a3t", "a3t"), ("tests/system", "system-tests"), ("tests/a3t", "a3t-test")] {
        for e in std::fs::read_dir(root.join(dir)).unwrap() {
            let p = e.unwrap().path();
            if p.extension().and_then(|x| x.to_str()) != Some("yaml") { continue; }
            let yaml = std::fs::read_to_string(&p).unwrap();
            if !yaml.starts_with("document:") { continue; } // a schema doc, or the old apiVersion format
            let wf = swf_parser::parse_yaml(&yaml).unwrap_or_else(|e| panic!("{} failed to parse: {e}", p.display()));
            assert_eq!(wf.document.namespace, ns, "{}", p.display());
        }
    }
}
