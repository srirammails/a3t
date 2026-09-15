@phase-1 @ui
Feature: UI Dashboard
  The web dashboard provides an overview of the A3T platform state.

  Background:
    Given FlowR is started on port 3002 with --ui

  @req-UI-001
  Scenario: Dashboard shows system overview
    When the user navigates to /ui/
    Then the page contains a dashboard section
    And the dashboard shows agent count
    And the dashboard shows trigger count
    And the dashboard shows template count
    And the dashboard shows workflow count

  @req-UI-001
  Scenario: Dashboard data refreshes
    Given an agent "test-agent" is spawned
    When the user navigates to /ui/
    Then the agent count is at least 1
