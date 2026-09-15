# requirements: requirements_clawdflow.yaml
Feature: ClawdFlow Engine

  @req-engine-001 @smoke
  Scenario: ClawdFlow starts and serves health endpoint
    Given ClawdFlow is started on port 3099
    Then the HTTP endpoint is reachable at port 3099
    When I call MCP tool "list_task_types" with name "all"
    Then the MCP result contains "set"

  @req-engine-002 @smoke
  Scenario: ClawdFlow supports all CNCF SWF 1.0.3 task types
    Given ClawdFlow is started on port 3099
    When I list MCP tools
    Then the tool list contains "execute_flow"
    And the tool list contains "create_flow"

  @req-engine-003 @smoke
  Scenario: Expression engine evaluates jq with runtime arguments
    Given ClawdFlow is started on port 3099
    Given the workflow definition at "test-set.yaml"
    When I execute workflow "test-set" with input {"name": "World", "value": 42}
    Then the workflow completes with status completed
