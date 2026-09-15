@phase-1 @ui
Feature: UI Templates Page
  Create, version, and manage agent templates.

  Background:
    Given FlowR is started on port 3002 with --ui

  @req-UI-003
  Scenario: Templates page lists all templates
    Given a template "rtb-bidder" version "1.0.0" exists
    And a template "rtb-bidder" version "2.0.0" exists
    When the user navigates to /ui/templates
    Then the page lists template "rtb-bidder"
    And the template shows 2 versions
    And the template shows active version "1.0.0"

  @req-UI-003
  Scenario: User creates a template via YAML editor
    When the user navigates to /ui/templates
    And the user clicks "New Template"
    And the user enters template YAML:
      """
      name: my-agent
      version: 1.0.0
      spec:
        steps:
          - name: greet
            type: set
            config:
              message: hello
      """
    And the user clicks "Create"
    Then the template "my-agent" appears in the list
    And the template has version "1.0.0"

  @req-UI-003
  Scenario: User activates a template version
    Given a template "rtb-bidder" version "1.0.0" exists with active=true
    And a template "rtb-bidder" version "2.0.0" exists with active=false
    When the user clicks "Activate" on version "2.0.0"
    Then the active version changes to "2.0.0"
