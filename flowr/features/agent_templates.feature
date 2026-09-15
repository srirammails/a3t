@phase-3
Feature: Agent Templates + Parameter Binding
  Templates are reusable agent blueprints with typed parameters.
  An agent YAML references a template instead of defining everything inline.

  # =========================================================================
  # REQ-TMPL-001: Template with parameter declarations
  # =========================================================================

  @req-TMPL-001
  Scenario: Template declares typed parameters
    Given a template YAML file "fixtures/templates/rtb-bidder.yaml" with content:
      """
      name: rtb-bidder
      version: v1.2.0
      parameters:
        - name: aql_namespace
          type: string
          required: true
        - name: floor_multiplier
          type: float
          default: 1.18
          description: Multiplier applied to floor price
      mode: reactive
      triggers:
        - type: http
          path: /bid
      baseFlow:
        path: ./flows/bidder-main.yaml
      """
    When I run `clawdflow validate fixtures/templates/rtb-bidder.yaml`
    Then validation succeeds with exit code 0

  @req-TMPL-001
  Scenario: Agent YAML references template with parameters
    Given a template YAML file "fixtures/templates/rtb-bidder.yaml" with content:
      """
      name: rtb-bidder
      version: v1.2.0
      parameters:
        - name: aql_namespace
          type: string
          required: true
        - name: floor_multiplier
          type: float
          default: 1.18
      mode: reactive
      triggers:
        - type: http
          path: /bid
      baseFlow:
        path: ./flows/bidder-main.yaml
      """
    Given an agent YAML "fixtures/my-bidder.yaml" referencing template "fixtures/templates/rtb-bidder.yaml" with parameters:
      | name             | value        |
      | aql_namespace    | my-bidder    |
      | floor_multiplier | 1.22         |
    When I run `clawdflow validate fixtures/my-bidder.yaml`
    Then validation succeeds with exit code 0

  @req-TMPL-001
  Scenario: Missing required template parameter fails validation
    Given a template YAML file "fixtures/templates/rtb-bidder.yaml" with content:
      """
      name: rtb-bidder
      version: v1.2.0
      parameters:
        - name: aql_namespace
          type: string
          required: true
      mode: ephemeral
      baseFlow:
        path: ./flows/main.yaml
      """
    Given an agent YAML "fixtures/no-param.yaml" referencing template "fixtures/templates/rtb-bidder.yaml" with parameters:
      | name             | value |
    When I run `clawdflow validate fixtures/no-param.yaml`
    Then validation fails with error containing "aql_namespace"

  # =========================================================================
  # REQ-TMPL-002: jq expression substitution at load time
  # =========================================================================

  @req-TMPL-002
  Scenario: Structural parameters resolved at load time
    Given a template YAML file "fixtures/templates/topic-template.yaml" with content:
      """
      name: topic-agent
      version: v1.0.0
      parameters:
        - name: aql_namespace
          type: string
          required: true
      mode: reactive
      triggers:
        - type: iggy
          address: localhost:8090
          topic: "${ .parameters.aql_namespace }-requests"
          consumerGroup: "${ .parameters.aql_namespace }"
      baseFlow:
        path: ./flows/main.yaml
      adb:
        address: localhost:6379
        namespace: "${ .parameters.aql_namespace }"
      """
    Given an agent YAML "fixtures/resolved.yaml" referencing template "fixtures/templates/topic-template.yaml" with parameters:
      | name          | value     |
      | aql_namespace | my-bidder |
    When I POST /templates/topic-agent/render with parameters '{"aql_namespace":"my-bidder"}'
    Then the effective spec has field "triggers[0].topic" equal to "my-bidder-requests"
    And the effective spec has field "adb.namespace" equal to "my-bidder"

  # =========================================================================
  # REQ-TMPL-003: Inline and template-based equivalence
  # =========================================================================

  @req-TMPL-003
  Scenario: Template-based agent produces same spec as inline
    Given a template YAML file "fixtures/templates/echo-template.yaml" with content:
      """
      name: echo
      version: v1.0.0
      parameters:
        - name: greeting
          type: string
          default: hello
      mode: ephemeral
      triggers:
        - type: http
          path: /echo
      baseFlow:
        inline:
          name: echo-flow
          steps:
            - name: greet
              type: set
              config:
                message: "${ .agent.parameters.greeting }"
              export.as: "."
      """
    Given an agent YAML file "fixtures/echo-inline.yaml" with content:
      """
      name: echo-agent
      mode: ephemeral
      parameters:
        greeting: hello
      triggers:
        - type: http
          path: /echo
      baseFlow:
        inline:
          name: echo-flow
          steps:
            - name: greet
              type: set
              config:
                message: "${ .agent.parameters.greeting }"
              export.as: "."
      """
    When I run `clawdflow run fixtures/echo-inline.yaml`
    Then the agent "echo-agent" is running
    Then the rendered template output matches the inline agent YAML
