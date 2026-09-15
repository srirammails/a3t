@phase-1 @ui
Feature: UI Workflows Page
  Create, execute, and monitor workflows.

  Background:
    Given FlowR is started on port 3002 with --ui

  @req-UI-005
  Scenario: Workflows page lists all workflows
    Given a workflow "test-set" exists
    And a workflow "test-call-llm" exists
    When the user navigates to /ui/workflows
    Then the page lists workflow "test-set"
    And the page lists workflow "test-call-llm"

  @req-UI-005
  Scenario: User creates a workflow via YAML editor
    When the user navigates to /ui/workflows
    And the user clicks "New Workflow"
    And the user enters workflow YAML:
      """
      namespace: default
      name: ui-test
      version: "1.0.0"
      enabled: true
      steps:
        - name: greet
          type: set
          config:
            message: hello from UI
          export.as: "."
      """
    And the user clicks "Create"
    Then the workflow "ui-test" appears in the list

  @req-UI-005
  Scenario: User executes a workflow with JSON input
    Given a workflow "test-set" exists
    When the user selects workflow "test-set"
    And the user enters input JSON '{"name":"World","value":42}'
    And the user clicks "Execute"
    Then the execution status shows "Completed"
    And the execution output is displayed

  @req-UI-005
  Scenario: User views execution history
    Given a workflow "test-set" has been executed 3 times
    When the user selects workflow "test-set"
    Then the execution history shows 3 runs
    And each run shows status, duration, and timestamp
