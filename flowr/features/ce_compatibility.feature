@phase-1
Feature: CE Compatibility
  FlowR EE runs in CE mode by default, behaving identically to ClawdFlow.

  Background:
    Given FlowR is started on port 3002

  @req-COMPAT-001
  Scenario: FlowR builds on CE crates without modification
    Then the server health check returns status "ok"
    And the server version matches the CE version

  @req-COMPAT-002
  Scenario: CE mode starts with LanceDB storage
    When no --dragonfly flag is provided
    And no --iggy flag is provided
    Then the storage backend is "lancedb"
    And the server starts successfully

  @req-COMPAT-002
  Scenario: All 22 task types are available
    When the user lists task types
    Then the response contains 22 or more task types
    And the response includes task type "set"
    And the response includes task type "call:agent"
    And the response includes task type "call:llm"
    And the response includes task type "call:http"

  @req-COMPAT-002
  Scenario: Agent modes work in CE mode
    Given a resource "openai-gpt" of type "openai" exists
    And a resource "agent-memory" of type "clawdb" exists
    And a resource "test-agent" of type "agent" exists with mode "ephemeral"
    When the user creates a workflow "test-agent-flow" with a call:agent step
    And the user executes workflow "test-agent-flow"
    Then the execution completes with status "Completed"
    And the output contains field "success" with value true

  @req-COMPAT-003
  Scenario: All MCP tools are available
    When the user lists MCP tools
    Then the response contains at least 16 tools
    And the tools include "list_flows"
    And the tools include "create_resource"
    And the tools include "execute_flow"

  @req-COMPAT-003
  Scenario: CE HTTP endpoints are available
    Then GET /agents returns status 200
    And GET /triggers returns status 200
    And GET /templates returns status 200
    And GET /health returns status 200

  @req-COMPAT-004
  Scenario: FlowR starts without UI by default
    When no --ui flag is provided
    Then GET /ui/ returns status 404
    And all CE endpoints still function

  @req-COMPAT-004
  Scenario: FlowR starts with UI when --ui flag is set
    Given FlowR is started on port 3002 with --ui
    Then GET /ui/ returns status 200
    And the response contains HTML content
    And all CE endpoints still function
