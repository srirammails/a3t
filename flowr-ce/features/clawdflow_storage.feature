# requirements: requirements_clawdflow.yaml
Feature: ClawdFlow Storage

  Background:
    Given ClawdFlow is started on port 3099

  @req-store-001 @smoke
  Scenario: LanceDB stores and retrieves workflows
    Given the workflow definition at "test-set.yaml"
    When I create a flow "test-store" via the API
    Then the flow "test-store" is retrievable via the API
    When I delete the flow "test-store" via the API
    Then the flow "test-store" is not found via the API

  @req-store-001 @smoke
  Scenario: Data persists across restarts
    When I set state key "persist-test" to value "hello"
    Given ClawdFlow is restarted
    Then getting state key "persist-test" returns "hello"

  @req-store-002 @smoke
  Scenario: State store supports key-value with TTL
    When I set state key "ttl-test" to value "ephemeral" with TTL 1 second
    Then getting state key "ttl-test" returns "ephemeral"
    When 2s elapses
    Then getting state key "ttl-test" returns null

  @req-store-002
  Scenario: State store delete removes key
    When I set state key "del-test" to value "to-delete"
    Then getting state key "del-test" returns "to-delete"
    When I execute workflow "state-delete" with input {"key": "del-test"}
    Then getting state key "del-test" returns null
