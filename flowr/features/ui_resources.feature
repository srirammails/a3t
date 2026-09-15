@phase-1 @ui
Feature: UI Resources Page
  Manage LLM, ClawDB, MCP, and agent resource configurations.

  Background:
    Given FlowR is started on port 3002 with --ui

  @req-UI-006
  Scenario: Resources page lists all resources
    Given a resource "openai-gpt" of type "openai" exists
    And a resource "agent-memory" of type "clawdb" exists
    When the user navigates to /ui/resources
    Then the page lists resource "openai-gpt" with type "openai"
    And the page lists resource "agent-memory" with type "clawdb"

  @req-UI-006
  Scenario: User creates a new resource
    When the user navigates to /ui/resources
    And the user clicks "New Resource"
    And the user enters name "my-llm"
    And the user selects type "openai"
    And the user enters config JSON:
      """
      {
        "provider": "openai",
        "api_key": "sk-test",
        "model": "gpt-4o-mini"
      }
      """
    And the user clicks "Create"
    Then the resource "my-llm" appears in the list

  @req-UI-006
  Scenario: User views resource details
    Given a resource "openai-gpt" of type "openai" exists
    When the user clicks on resource "openai-gpt"
    Then the detail panel shows type "openai"
    And the detail panel shows config with model name
    And the API key is masked

  @req-UI-006
  Scenario: User deletes a resource
    Given a resource "temp-resource" of type "openai" exists
    When the user clicks "Delete" on resource "temp-resource"
    And the user confirms deletion
    Then the resource "temp-resource" is removed from the list
