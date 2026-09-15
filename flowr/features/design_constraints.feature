@phase-2
Feature: Design Constraints
  Cross-cutting rules that apply across all phases.

  # =========================================================================
  # REQ-DESIGN-001: jq only, no second templating language
  # =========================================================================

  @req-DESIGN-001
  Scenario: Parameters use jq expressions, not Go templates
    Given an agent YAML file "fixtures/jq-params.yaml" with content:
      """
      name: jq-test
      mode: ephemeral
      parameters:
        greeting: hello
      baseFlow:
        inline:
          name: test
          steps:
            - name: greet
              type: set
              config:
                msg: "${ .agent.parameters.greeting }"
              export.as: "."
      """
    When I run `clawdflow validate fixtures/jq-params.yaml`
    Then validation succeeds with exit code 0

  # =========================================================================
  # REQ-DESIGN-002: Strict validation, no lazy resolution
  # =========================================================================

  @req-DESIGN-002 @smoke
  Scenario: Missing flow reference fails at validation, not at invocation
    Given an agent YAML file "fixtures/lazy.yaml" with content:
      """
      name: lazy-agent
      mode: ephemeral
      baseFlow:
        path: ./flows/nonexistent.yaml
      """
    When I run `clawdflow validate fixtures/lazy.yaml`
    Then validation fails with error containing "nonexistent.yaml"
    And the error message contains file path "flows/nonexistent.yaml"

  # =========================================================================
  # REQ-DESIGN-003: Flow ownership
  # =========================================================================

  @req-DESIGN-003
  Scenario: Path flows are agent-owned, ref flows are shared
    Given an agent YAML file "fixtures/ownership.yaml" with content:
      """
      name: owner-test
      mode: ephemeral
      baseFlow:
        path: ./flows/main.yaml
      proceduralActions:
        - name: owned_flow
          flow:
            path: ./flows/owned.yaml
        - name: shared_flow
          flow:
            ref: flows/shared/logger
      """
    When I run `clawdflow validate fixtures/ownership.yaml`
    Then procedural action "owned_flow" is marked as "agent"
    And procedural action "shared_flow" is marked as "shared"

  # =========================================================================
  # REQ-DESIGN-004: CLI and controller use API only
  # =========================================================================

  @req-DESIGN-004
  Scenario: CLI agent create maps to POST /agents
    Given an agent YAML file "fixtures/cli-agent.yaml" with content:
      """
      name: cli-test
      mode: ephemeral
      baseFlow:
        path: ./flows/main.yaml
      """
    When I run `clawdflow run fixtures/cli-agent.yaml`
    Then the agent "cli-test" is running
    When I GET /agents/cli-test
    Then the response status is 200
    And the response field "name" equals "cli-test"
