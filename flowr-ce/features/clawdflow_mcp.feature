# requirements: requirements_clawdflow.yaml
Feature: ClawdFlow MCP Protocol

  @req-mcp-001 @smoke
  Scenario: MCP tool listing returns all tools
    Given ClawdFlow is started on port 3099
    When I list MCP tools
    Then the tool list contains "list_flows"
    And the tool list contains "create_flow"
    And the tool list contains "execute_flow"
    And the tool list contains "create_resource"
    And the tool list contains "send_message"
    And the tool list contains "create_agent_template"

  @req-mcp-002
  Scenario: MCP HTTP/SSE mode serves on configurable port
    Given ClawdFlow is started on port 3099
    Then the MCP endpoint is reachable at port 3099

  @req-mcp-002
  Scenario: MCP stdio mode communicates via stdin/stdout
    Given ClawdFlow is started with "--mode stdio"
    When I send an MCP request via stdin
    Then an MCP response is received on stdout
