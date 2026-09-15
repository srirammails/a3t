# requirements: requirements_clawdflow.yaml
Feature: ClawdFlow Trigger System

  Background:
    Given ClawdFlow is started on port 3099

  @req-trig-001 @smoke
  Scenario: Trigger CRUD lifecycle
    When I POST /triggers with name "test-trigger" and type "webhook" and path "/test/hook"
    Then GET /triggers returns a list containing "test-trigger"
    When I POST /triggers/test-trigger/pause
    Then the trigger "test-trigger" has status "paused"
    When I POST /triggers/test-trigger/enable
    Then the trigger "test-trigger" has status "enabled"
    When I DELETE /triggers/test-trigger
    Then GET /triggers does not contain "test-trigger"

  @req-trig-001
  Scenario: Trigger attach and detach targets
    Given a trigger "multi-target" exists with type "webhook" and path "/multi"
    When I POST /triggers/multi-target/attach with target type "workflow" and agent_id "test-flow"
    Then the trigger "multi-target" has 1 target(s)
    When I POST /triggers/multi-target/detach with target_id "test-flow"
    Then the trigger "multi-target" has 0 target(s)

  @req-trig-002
  Scenario: Webhook trigger fires on POST
    Given a trigger "hook-test" exists with type "webhook" and path "/fire-test"
    Given a trigger target workflow "test-set" is attached to "hook-test"
    When I POST /webhook?path=/fire-test with body {"name": "trigger", "value": 1}
    Then the trigger delivers to 1 target(s)

  @req-trig-002
  Scenario: Cron trigger type parses correctly
    Given a trigger "cron-test" exists with type "cron" and expression "*/5 * * * *"
    Then GET /triggers returns a list containing "cron-test"

  @req-trig-003
  Scenario: Trigger executes workflow directly
    Given the workflow definition at "test-set.yaml"
    Given a trigger "wf-trigger" exists with type "webhook" and path "/wf-test"
    Given a trigger target workflow "test-set" is attached to "wf-trigger"
    When I POST /webhook?path=/wf-test with body {"name": "triggered", "value": 99}
    Then the workflow "test-set" was executed

  @req-trig-010
  Scenario: Triggers persist across restarts
    Given a trigger "persist-trig" exists with type "webhook" and path "/persist"
    When ClawdFlow is restarted
    Then GET /triggers returns a list containing "persist-trig"
