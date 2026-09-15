# requirements: requirements_clawdflow.yaml
Feature: ClawdFlow Security

  @req-sec-001 @smoke
  Scenario: API keys are masked in resource API responses
    Given ClawdFlow is started on port 3099
    Given an LLM resource "sec-test" is configured with API key "sk-secret-key-12345678"
    When I call MCP tool "get_resource" with name "sec-test"
    Then the MCP result contains "..."
    And the captured logs do not contain "sk-secret-key-12345678"
