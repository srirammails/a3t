# requirements: requirements_clawdflow.yaml
Feature: ClawdFlow Agent Template (A3T)

  Background:
    Given ClawdFlow is started on port 3099
    Given the workflow "system/agent-template" is deployed
    Given an LLM resource "default-llm" is configured

  @req-agent-001 @smoke
  Scenario: Agent executes as pure CNCF workflow via run:workflow
    Given the workflow definition at "test-call-agent.yaml"
    When I execute workflow "test-call-agent" with input {}
    Then the workflow completes with status completed
    And the workflow output field "content" is not null

  @req-agent-001
  Scenario: Agent dispatches tool calls via call:mcp
    Given the workflow definition at "test-agent-tools.yaml"
    When I execute workflow "test-agent-tools" with input {}
    Then the workflow completes with status completed
    And the workflow output field "used_tools" equals "true"

  @req-agent-002
  Scenario: Agent template has 10 lifecycle extension hooks
    Given the workflow definition at "agent-template.yaml"
    When I parse the workflow
    Then parsing succeeds
    And the workflow has 10 extensions

  @req-agent-003
  Scenario: Agent session persists conversation across turns
    Given the workflow definition at "test-agent-multiturn.yaml"
    When I execute workflow "test-agent-multiturn" with input {}
    Then the workflow completes with status completed
    And the workflow output field "test_passed" equals "true"
