@phase-4
Feature: Admin API Completeness
  The Admin API is the single control plane.
  CLI and future K8s controller are both clients.

  Background:
    Given FlowR is started on port 3002

  # =========================================================================
  # REQ-API-001: API is the single control plane
  # =========================================================================

  @req-API-001 @smoke
  Scenario: Agent created via API matches agent created via CLI
    Given an agent YAML file "fixtures/api-agent.yaml" with content:
      """
      name: api-test
      mode: ephemeral
      baseFlow:
        path: ./flows/main.yaml
      """
    When I POST /agents with the agent YAML at "fixtures/api-agent.yaml"
    Then the response status is 201
    When I GET /agents/api-test
    Then the response status is 200
    And the response field "name" equals "api-test"
    And the response field "mode" equals "ephemeral"

  # =========================================================================
  # REQ-API-002: Agent lifecycle CRUD
  # =========================================================================

  @req-API-002
  Scenario: Full agent lifecycle via API
    When I POST /agents with the agent YAML at "fixtures/api-agent.yaml"
    Then the response status is 201
    When I GET /agents
    Then the response is a JSON array with at least 1 entries
    When I POST /agents/api-test/pause
    Then the response status is 200
    When I GET /agents/api-test
    Then the API response includes agent status "paused"
    When I POST /agents/api-test/resume
    Then the response status is 200
    When I GET /agents/api-test
    Then the API response includes agent status "running"
    When I DELETE /agents/api-test
    Then the response status is 200
    When I GET /agents/api-test
    Then the response status is 404

  @req-API-002
  Scenario: PATCH validates before applying
    When I POST /agents with the agent YAML at "fixtures/api-agent.yaml"
    Then the response status is 201
    When I PATCH /agents/api-test with '{"mode":"turbo"}'
    Then the response status is 400
    And the response body contains "mode"

  # =========================================================================
  # REQ-API-003: Template rendering
  # =========================================================================

  @req-API-003
  Scenario: Render template with parameters
    Given a template "rtb-bidder" is registered at version "v1.0.0"
    When I POST /templates/rtb-bidder/render with parameters '{"aql_namespace":"test-bidder"}'
    Then the response status is 200
    And the effective spec has field "adb.namespace" equal to "test-bidder"

  @req-API-003
  Scenario: Render template with missing required parameter
    Given a template "rtb-bidder" is registered at version "v1.0.0"
    When I POST /templates/rtb-bidder/render with parameters '{}'
    Then the response status is 400
    And the response body contains "required"

  # =========================================================================
  # REQ-API-004: Agent status
  # =========================================================================

  @req-API-004
  Scenario: Detailed agent runtime status
    When I POST /agents with the agent YAML at "fixtures/persistent-agent.yaml"
    When I GET /agents/persistent-test/status
    Then the response status is 200
    And the response field "mode" equals "persistent"
    And the response field "uptime_secs" equals 0
    And the response body contains "tools_count"
    And the response body contains "metrics"

  # =========================================================================
  # REQ-API-005: Agent events
  # =========================================================================

  @req-API-005
  Scenario: Agent lifecycle events
    When I POST /agents with the agent YAML at "fixtures/persistent-agent.yaml"
    When I GET /agents/persistent-test/events
    Then the response status is 200
    And the events list includes event type "spawned"
    When I DELETE /agents/persistent-test
    When I GET /agents/persistent-test/events
    Then the events list includes event type "stopped"
