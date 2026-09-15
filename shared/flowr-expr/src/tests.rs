//! Tests for the expression engine

use serde_json::json;

use crate::{ExpressionEngine, Scope};

#[test]
fn test_is_expr() {
    assert!(ExpressionEngine::is_expr("${ .foo }"));
    assert!(ExpressionEngine::is_expr("${.foo}"));
    assert!(ExpressionEngine::is_expr("  ${ .foo }  "));
    assert!(!ExpressionEngine::is_expr("hello"));
    assert!(!ExpressionEngine::is_expr("$foo"));
    assert!(!ExpressionEngine::is_expr("{ .foo }"));
}

#[test]
fn test_contains_expr() {
    assert!(ExpressionEngine::contains_expr("Hello ${ .name }!"));
    assert!(ExpressionEngine::contains_expr("${.foo}"));
    assert!(!ExpressionEngine::contains_expr("hello"));
    assert!(!ExpressionEngine::contains_expr("$foo"));
}

#[test]
fn test_extract_expr() {
    assert_eq!(
        ExpressionEngine::extract_expr("${ .foo }").unwrap(),
        ".foo"
    );
    assert_eq!(
        ExpressionEngine::extract_expr("${.bar.baz}").unwrap(),
        ".bar.baz"
    );
    assert_eq!(
        ExpressionEngine::extract_expr("${ .items | length }").unwrap(),
        ".items | length"
    );
}

#[test]
fn test_simple_field_access() {
    let engine = ExpressionEngine::new();
    let input = json!({
        "name": "Alice",
        "age": 30
    });
    let scope = Scope::new();

    let result = engine.eval(&input, "${ .name }", &scope).unwrap();
    assert_eq!(result, json!("Alice"));

    let result = engine.eval(&input, "${ .age }", &scope).unwrap();
    assert_eq!(result, json!(30));
}

#[test]
fn test_nested_field_access() {
    let engine = ExpressionEngine::new();
    let input = json!({
        "order": {
            "items": [
                { "name": "item1", "price": 10 },
                { "name": "item2", "price": 20 }
            ],
            "total": 30
        }
    });
    let scope = Scope::new();

    let result = engine.eval(&input, "${ .order.total }", &scope).unwrap();
    assert_eq!(result, json!(30));

    let result = engine.eval(&input, "${ .order.items[0].name }", &scope).unwrap();
    assert_eq!(result, json!("item1"));
}

#[test]
fn test_jq_operations() {
    let engine = ExpressionEngine::new();
    let input = json!({
        "items": [1, 2, 3, 4, 5]
    });
    let scope = Scope::new();

    // Length
    let result = engine.eval(&input, "${ .items | length }", &scope).unwrap();
    assert_eq!(result, json!(5));

    // Sum
    let result = engine.eval(&input, "${ .items | add }", &scope).unwrap();
    assert_eq!(result, json!(15));

    // Map
    let result = engine.eval(&input, "${ .items | map(. * 2) }", &scope).unwrap();
    assert_eq!(result, json!([2, 4, 6, 8, 10]));

    // Select/filter
    let result = engine.eval(&input, "${ .items | map(select(. > 3)) }", &scope).unwrap();
    assert_eq!(result, json!([4, 5]));
}

#[test]
fn test_boolean_expressions() {
    let engine = ExpressionEngine::new();
    let input = json!({
        "amount": 1500,
        "status": "active"
    });
    let scope = Scope::new();

    let result = engine.eval(&input, "${ .amount > 1000 }", &scope).unwrap();
    assert_eq!(result, json!(true));

    let result = engine.eval(&input, "${ .amount < 1000 }", &scope).unwrap();
    assert_eq!(result, json!(false));

    let result = engine.eval(&input, "${ .status == \"active\" }", &scope).unwrap();
    assert_eq!(result, json!(true));
}

#[test]
fn test_scope_variables() {
    let engine = ExpressionEngine::new();
    let input = json!({ "data": "test" });

    let scope = Scope::builder()
        .runtime(json!({ "name": "flowr", "version": "0.1.0" }))
        .workflow(json!({ "id": "wf-123", "name": "test-workflow" }))
        .task(json!({ "name": "step1", "reference": "do/step1" }))
        .context(json!({ "userId": "user-456" }))
        .build();

    // Access $runtime
    let result = engine.eval(&input, "${ $runtime.name }", &scope).unwrap();
    assert_eq!(result, json!("flowr"));

    // Access $workflow
    let result = engine.eval(&input, "${ $workflow.id }", &scope).unwrap();
    assert_eq!(result, json!("wf-123"));

    // Access $task
    let result = engine.eval(&input, "${ $task.name }", &scope).unwrap();
    assert_eq!(result, json!("step1"));

    // Access $context
    let result = engine.eval(&input, "${ $context.userId }", &scope).unwrap();
    assert_eq!(result, json!("user-456"));
}

#[test]
fn test_recursive_evaluation() {
    let engine = ExpressionEngine::new();
    let input = json!({
        "firstName": "John",
        "lastName": "Doe",
        "items": [10, 20, 30]
    });
    let scope = Scope::new();

    // Object with expressions
    let template = json!({
        "greeting": "${ \"Hello \" + .firstName }",
        "total": "${ .items | add }",
        "nested": {
            "fullName": "${ .firstName + \" \" + .lastName }"
        }
    });

    let result = engine.eval_recursive(&template, &input, &scope).unwrap();

    assert_eq!(result["greeting"], json!("Hello John"));
    assert_eq!(result["total"], json!(60));
    assert_eq!(result["nested"]["fullName"], json!("John Doe"));
}

#[test]
fn test_array_recursive_evaluation() {
    let engine = ExpressionEngine::new();
    let input = json!({
        "values": [1, 2, 3]
    });
    let scope = Scope::new();

    let template = json!([
        "${ .values[0] }",
        "${ .values[1] }",
        "${ .values | add }"
    ]);

    let result = engine.eval_recursive(&template, &input, &scope).unwrap();
    assert_eq!(result, json!([1, 2, 6]));
}

#[test]
fn test_string_interpolation() {
    let engine = ExpressionEngine::new();
    let input = json!({
        "name": "Alice",
        "count": 5
    });
    let scope = Scope::new();

    let template = json!("Hello ${ .name }, you have ${ .count } items");
    let result = engine.eval_recursive(&template, &input, &scope).unwrap();

    assert_eq!(result, json!("Hello Alice, you have 5 items"));
}

#[test]
fn test_cache_behavior() {
    let engine = ExpressionEngine::new();
    let input = json!({ "x": 1 });
    let scope = Scope::new();

    assert_eq!(engine.cache_size(), 0);

    // First evaluation - compiles and caches
    engine.eval(&input, "${ .x }", &scope).unwrap();
    assert_eq!(engine.cache_size(), 1);

    // Second evaluation - uses cache
    engine.eval(&input, "${ .x }", &scope).unwrap();
    assert_eq!(engine.cache_size(), 1);

    // Different expression
    engine.eval(&input, "${ .x + 1 }", &scope).unwrap();
    assert_eq!(engine.cache_size(), 2);

    // Clear cache
    engine.clear_cache();
    assert_eq!(engine.cache_size(), 0);
}

#[test]
fn test_complex_jq_pipeline() {
    let engine = ExpressionEngine::new();
    let input = json!({
        "orders": [
            { "id": 1, "items": [{ "price": 10 }, { "price": 20 }] },
            { "id": 2, "items": [{ "price": 30 }, { "price": 40 }] }
        ]
    });
    let scope = Scope::new();

    // Sum all item prices across all orders
    let result = engine.eval(
        &input,
        "${ .orders | map(.items | map(.price) | add) | add }",
        &scope
    ).unwrap();
    assert_eq!(result, json!(100));
}

#[test]
fn test_raw_jq_without_wrapper() {
    let engine = ExpressionEngine::new();
    let input = json!({ "x": 42 });
    let scope = Scope::new();

    // Without ${ } wrapper
    let result = engine.eval(&input, ".x", &scope).unwrap();
    assert_eq!(result, json!(42));
}

#[test]
fn test_null_handling() {
    let engine = ExpressionEngine::new();
    let input = json!({ "a": null, "b": 1 });
    let scope = Scope::new();

    let result = engine.eval(&input, "${ .a }", &scope).unwrap();
    assert_eq!(result, json!(null));

    let result = engine.eval(&input, "${ .nonexistent }", &scope).unwrap();
    assert_eq!(result, json!(null));
}

#[test]
fn test_passthrough_literals() {
    let engine = ExpressionEngine::new();
    let input = json!({});
    let scope = Scope::new();

    // Non-expression values should pass through unchanged
    let template = json!({
        "number": 42,
        "string": "hello",
        "bool": true,
        "null": null,
        "array": [1, 2, 3]
    });

    let result = engine.eval_recursive(&template, &input, &scope).unwrap();
    assert_eq!(result, template);
}
