@phase-2
Feature: Agent YAML Loading
  clawdflow run and clawdflow validate commands.
  Strict validation at load time. Clear errors with file paths.

  Background:
    Given the ADB default is "localhost:6379"

  # =========================================================================
  # REQ-LOADER-001: clawdflow run
  # =========================================================================

  @req-LOADER-001 @smoke
  Scenario: Load and run an ephemeral agent from YAML
    Given an agent YAML file "fixtures/simple-agent.yaml" with content:
      """
      name: simple
      mode: ephemeral
      triggers:
        - type: http
          path: /ask
          port: 8080
      baseFlow:
        path: ./flows/echo.yaml
      """
    Given a flow YAML file "fixtures/flows/echo.yaml" with content:
      """
      name: echo
      steps:
        - name: echo
          type: set
          config:
            response: "hello"
          export.as: "."
      """
    When I run `clawdflow run fixtures/simple-agent.yaml`
    Then the agent "simple" is running
    And the agent "simple" is running in mode "ephemeral"

  @req-LOADER-001
  Scenario: Load fails for invalid YAML with clear error
    Given an agent YAML file "fixtures/bad-yaml.yaml" with content:
      """
      name: [invalid
      mode: nope
      """
    When I run `clawdflow run fixtures/bad-yaml.yaml`
    Then validation fails with error containing "bad-yaml.yaml"
    And the error message contains file path "fixtures/bad-yaml.yaml"

  # =========================================================================
  # REQ-LOADER-002: clawdflow validate
  # =========================================================================

  @req-LOADER-002 @smoke
  Scenario: Validate passes for well-formed agent YAML
    Given an agent YAML file "fixtures/valid.yaml" with content:
      """
      name: validator-test
      mode: persistent
      triggers:
        - type: http
          path: /test
      baseFlow:
        path: ./flows/main.yaml
      adb:
        address: localhost:6379
        namespace: validator-test
      """
    Given a flow YAML file "fixtures/flows/main.yaml" with content:
      """
      name: main
      steps:
        - name: start
          type: set
          config: { ok: true }
      """
    When I run `clawdflow validate fixtures/valid.yaml`
    Then validation succeeds with exit code 0

  @req-LOADER-002
  Scenario: Validate reports missing flow file
    Given an agent YAML file "fixtures/missing-flow.yaml" with content:
      """
      name: missing-ref
      mode: ephemeral
      baseFlow:
        path: ./flows/does-not-exist.yaml
      """
    When I run `clawdflow validate fixtures/missing-flow.yaml`
    Then validation fails with error containing "does-not-exist.yaml"

  @req-LOADER-002
  Scenario: Validate reports schema violations
    Given an agent YAML file "fixtures/bad-mode.yaml" with content:
      """
      name: bad-mode
      mode: turbo
      """
    When I run `clawdflow validate fixtures/bad-mode.yaml`
    Then validation fails with error containing "mode"

  # =========================================================================
  # REQ-LOADER-003: Relative path resolution
  # =========================================================================

  @req-LOADER-003
  Scenario: Flow paths resolve relative to agent YAML location
    Given an agent YAML file "fixtures/nested/agent.yaml" with content:
      """
      name: nested
      mode: ephemeral
      baseFlow:
        path: ./flows/main.yaml
      """
    Given a flow YAML file "fixtures/nested/flows/main.yaml" with content:
      """
      name: main
      steps:
        - name: s
          type: set
          config: { ok: true }
      """
    When I run `clawdflow validate fixtures/nested/agent.yaml`
    Then validation succeeds with exit code 0

  # =========================================================================
  # REQ-LOADER-004: Parameter injection via jq
  # =========================================================================

  @req-LOADER-004
  Scenario: Parameters are accessible in flow expressions
    Given an agent YAML file "fixtures/params.yaml" with content:
      """
      name: param-agent
      mode: ephemeral
      parameters:
        floor_multiplier: 1.22
        max_cpm: 5.00
      triggers:
        - type: http
          path: /bid
      baseFlow:
        path: ./flows/use-params.yaml
      """
    Given a flow YAML file "fixtures/flows/use-params.yaml" with content:
      """
      name: use-params
      steps:
        - name: apply
          type: set
          config:
            multiplier: "${ .agent.parameters.floor_multiplier }"
          export.as: "."
      """
    When I run `clawdflow run fixtures/params.yaml`
    Then the agent "param-agent" is running
    And the agent "param-agent" has parameter "floor_multiplier" with value "1.22"
