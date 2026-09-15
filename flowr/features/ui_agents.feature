@phase-1 @ui
Feature: UI Agents Page
  Manage and observe agents through the web dashboard.

  Background:
    Given FlowR is started on port 3002 with --ui
    And a resource "openai-gpt" of type "openai" exists
    And a resource "agent-memory" of type "clawdb" exists

  @req-UI-002
  Scenario: Agents page lists live agents
    Given an agent "obs-agent" is spawned with mode "persistent"
    When the user navigates to /ui/agents
    Then the page lists agent "obs-agent"
    And the agent shows mode "persistent"
    And the agent shows status "running"

  @req-UI-002
  Scenario: Agent detail shows health metrics
    Given an agent "obs-agent" is spawned with mode "persistent"
    And the agent has processed 3 turns
    When the user navigates to /ui/agents/obs-agent
    Then the page shows total turns "3"
    And the page shows average latency
    And the page shows success rate

  @req-UI-002
  Scenario: Agent detail shows conversation history
    Given an agent "obs-agent" is spawned with mode "persistent"
    And the agent has processed a message "What is 2+2?"
    When the user navigates to /ui/agents/obs-agent
    Then the conversation history shows the input "What is 2+2?"
    And the conversation history shows the agent response
    And each turn shows duration in milliseconds
