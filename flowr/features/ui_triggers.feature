@phase-1 @ui
Feature: UI Triggers Page
  Create, manage, and test triggers through the web dashboard.

  Background:
    Given FlowR is started on port 3002 with --ui

  @req-UI-004
  Scenario: Triggers page lists all triggers
    Given a trigger "prod-alerts" of type "webhook" with path "/alerts" exists
    When the user navigates to /ui/triggers
    Then the page lists trigger "prod-alerts"
    And the trigger shows type "webhook"
    And the trigger shows path "/alerts"
    And the trigger shows status "active"

  @req-UI-004
  Scenario: User creates a webhook trigger
    When the user navigates to /ui/triggers
    And the user clicks "New Trigger"
    And the user selects type "webhook"
    And the user enters name "test-trigger"
    And the user enters path "/test"
    And the user clicks "Create"
    Then the trigger "test-trigger" appears in the list

  @req-UI-004
  Scenario: User attaches an agent target to a trigger
    Given a trigger "prod-alerts" exists
    And an agent "ops-agent" is spawned
    When the user clicks "Attach Target" on trigger "prod-alerts"
    And the user selects target type "agent"
    And the user selects agent "ops-agent"
    And the user clicks "Attach"
    Then the trigger shows 1 target

  @req-UI-004
  Scenario: User fires a test webhook from the UI
    Given a trigger "prod-alerts" of type "webhook" with path "/alerts" exists
    And the trigger has agent "ops-agent" attached
    When the user clicks "Test Fire" on trigger "prod-alerts"
    And the user enters test payload '{"severity":"test"}'
    And the user clicks "Fire"
    Then the response shows delivered count of 1

  @req-UI-004
  Scenario: User pauses and enables a trigger
    Given a trigger "prod-alerts" exists with status "active"
    When the user clicks "Pause" on trigger "prod-alerts"
    Then the trigger status changes to "paused"
    When the user clicks "Enable" on trigger "prod-alerts"
    Then the trigger status changes to "active"
