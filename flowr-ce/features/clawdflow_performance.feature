# requirements: requirements_clawdflow.yaml
Feature: ClawdFlow Performance

  Background:
    Given ClawdFlow is started on port 3099

  @req-perf-001 @slow
  Scenario: Inline Lua p99 latency under 100 microseconds
    Given the workflow definition at "test-inline-lua.yaml"
    Then the p99 latency over 1000 iterations is under 100us for workflow "test-inline-lua"

  @req-perf-002 @slow
  Scenario: Inline JavaScript p99 latency under 200 microseconds
    Given the workflow definition at "test-inline-js.yaml"
    Then the p99 latency over 1000 iterations is under 200us for workflow "test-inline-js"
