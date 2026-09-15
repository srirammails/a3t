@phase-2
Feature: Agent YAML Schema
  The agent YAML schema defines what an agent does.
  No deployment concerns. Stable before CRD work begins.

  # =========================================================================
  # REQ-SCHEMA-001: Agent YAML shape
  # =========================================================================

  @req-SCHEMA-001 @smoke
  Scenario: Ephemeral agent YAML validates successfully
    Given an agent YAML file "fixtures/url-classifier.yaml" with content:
      """
      name: url-classifier
      mode: ephemeral
      description: Classifies URLs by content category
      triggers:
        - type: http
          path: /classify
          port: 8080
          timeout: 500ms
      baseFlow:
        path: ./flows/classify-url.yaml
      proceduralActions:
        - name: fetch_page
          flow:
            path: ./flows/fetch-page.yaml
      adb:
        address: localhost:6379
      """
    When I run `clawdflow validate fixtures/url-classifier.yaml`
    Then validation succeeds with exit code 0

  @req-SCHEMA-001
  Scenario: Agent YAML rejects unknown fields
    Given an agent YAML file "fixtures/bad-field.yaml" with content:
      """
      name: bad-agent
      mode: ephemeral
      replicas: 3
      """
    When I run `clawdflow validate fixtures/bad-field.yaml`
    Then validation fails with error containing "unknown field"
    And the agent YAML is rejected with "replicas"

  @req-SCHEMA-001
  Scenario: Agent YAML rejects missing required fields
    Given an agent YAML file "fixtures/no-name.yaml" with content:
      """
      mode: ephemeral
      """
    When I run `clawdflow validate fixtures/no-name.yaml`
    Then validation fails with error containing "name"

  # =========================================================================
  # REQ-SCHEMA-002: Top-level triggers
  # =========================================================================

  @req-SCHEMA-002
  Scenario: Triggers are top-level across all modes
    Given an agent YAML file "fixtures/reactive-agent.yaml" with content:
      """
      name: rtb-bidder
      mode: reactive
      triggers:
        - type: iggy
          address: localhost:8090
          topic: bid-requests
          consumerGroup: rtb-bidder
        - type: cron
          schedule: "*/5 * * * *"
          flow: aggregate-outcomes
      reactive:
        inbox:
          capacity: 1000
          overflow: drop-oldest
      baseFlow:
        path: ./flows/bidder-main.yaml
      proceduralActions:
        - name: aggregate-outcomes
          flow:
            path: ./flows/aggregate.yaml
      adb:
        address: localhost:6379
        namespace: rtb-bidder
      """
    When I run `clawdflow validate fixtures/reactive-agent.yaml`
    Then validation succeeds with exit code 0

  # =========================================================================
  # REQ-SCHEMA-003: Procedural actions (path/ref/inline)
  # =========================================================================

  @req-SCHEMA-003
  Scenario: Procedural action with path reference
    Given an agent YAML file "fixtures/path-action.yaml" with content:
      """
      name: test-agent
      mode: ephemeral
      baseFlow:
        path: ./flows/main.yaml
      proceduralActions:
        - name: score_page
          flow:
            path: ./flows/score-page.yaml
      """
    Given a flow YAML file "fixtures/flows/main.yaml" with content:
      """
      name: main
      steps:
        - name: start
          type: set
          config: { started: true }
      """
    Given a flow YAML file "fixtures/flows/score-page.yaml" with content:
      """
      name: score-page
      steps:
        - name: score
          type: set
          config: { scored: true }
      """
    When I run `clawdflow validate fixtures/path-action.yaml`
    Then validation succeeds with exit code 0
    And procedural action "score_page" resolves to flow at "fixtures/flows/score-page.yaml"

  @req-SCHEMA-003
  Scenario: Procedural action with ref (shared library)
    Given an agent YAML file "fixtures/ref-action.yaml" with content:
      """
      name: test-agent
      mode: ephemeral
      baseFlow:
        path: ./flows/main.yaml
      proceduralActions:
        - name: common_logger
          flow:
            ref: flows/shared/logger
      """
    When I run `clawdflow validate fixtures/ref-action.yaml`
    Then validation succeeds with exit code 0
    And procedural action "common_logger" is marked as "shared"

  @req-SCHEMA-003
  Scenario: Procedural action rejects multiple sources
    Given an agent YAML file "fixtures/multi-source.yaml" with content:
      """
      name: test-agent
      mode: ephemeral
      baseFlow:
        path: ./flows/main.yaml
      proceduralActions:
        - name: bad_action
          flow:
            path: ./flows/score.yaml
            ref: flows/shared/score
      """
    When I run `clawdflow validate fixtures/multi-source.yaml`
    Then validation fails with error containing "exactly one"

  # =========================================================================
  # REQ-SCHEMA-004: AQL namespace as identity
  # =========================================================================

  @req-SCHEMA-004
  Scenario: AQL namespace defaults to agent name
    Given an agent YAML file "fixtures/no-ns.yaml" with content:
      """
      name: my-agent
      mode: persistent
      baseFlow:
        path: ./flows/main.yaml
      adb:
        address: localhost:6379
      """
    When I run `clawdflow run fixtures/no-ns.yaml`
    Then the agent "my-agent" is running
    And the agent "my-agent" has aql_namespace "my-agent"

  @req-SCHEMA-004
  Scenario: Duplicate AQL namespace rejected
    Given an agent YAML file "fixtures/agent-a.yaml" with content:
      """
      name: agent-a
      mode: persistent
      adb:
        address: localhost:6379
        namespace: shared-ns
      baseFlow:
        path: ./flows/main.yaml
      """
    Given an agent YAML file "fixtures/agent-b.yaml" with content:
      """
      name: agent-b
      mode: persistent
      adb:
        address: localhost:6379
        namespace: shared-ns
      baseFlow:
        path: ./flows/main.yaml
      """
    When I run `clawdflow run fixtures/agent-a.yaml`
    Then the agent "agent-a" is running
    When I run `clawdflow run fixtures/agent-b.yaml`
    Then the runtime rejects the agent with error "namespace already in use"

  # =========================================================================
  # REQ-SCHEMA-005: Trigger types
  # =========================================================================

  @req-SCHEMA-005
  Scenario Outline: Trigger type validation
    Given an agent YAML file "fixtures/trigger-<type>.yaml" with content:
      """
      name: trigger-test
      mode: reactive
      triggers:
        - type: <type>
          <fields>
      baseFlow:
        path: ./flows/main.yaml
      """
    When I run `clawdflow validate fixtures/trigger-<type>.yaml`
    Then validation succeeds with exit code 0

    Examples:
      | type      | fields                                                |
      | http      | path: /test\n          port: 8080                     |
      | cron      | schedule: "*/5 * * * *"                               |
      | iggy      | address: localhost:8090\n          topic: test-topic   |

  @req-SCHEMA-005
  Scenario: Cron trigger with direct flow routing
    Given an agent YAML file "fixtures/cron-flow.yaml" with content:
      """
      name: cron-agent
      mode: reactive
      triggers:
        - type: cron
          schedule: "0 * * * *"
          flow: hourly-check
      baseFlow:
        path: ./flows/main.yaml
      proceduralActions:
        - name: hourly-check
          flow:
            path: ./flows/hourly.yaml
      """
    When I run `clawdflow validate fixtures/cron-flow.yaml`
    Then validation succeeds with exit code 0

  # =========================================================================
  # REQ-SCHEMA-006: Session config
  # =========================================================================

  @req-SCHEMA-006
  Scenario: Session agent with TTL and ID expression
    Given an agent YAML file "fixtures/session.yaml" with content:
      """
      name: chat-bot
      mode: session
      triggers:
        - type: http
          path: /chat
          port: 8080
      session:
        ttl: 30m
        idExpression: "${ .headers.\"x-session-id\" }"
      baseFlow:
        path: ./flows/chat.yaml
      """
    When I run `clawdflow validate fixtures/session.yaml`
    Then validation succeeds with exit code 0

  # =========================================================================
  # REQ-SCHEMA-007: Reactive inbox config
  # =========================================================================

  @req-SCHEMA-007
  Scenario: Reactive inbox with capacity and overflow
    Given an agent YAML file "fixtures/inbox.yaml" with content:
      """
      name: bidder
      mode: reactive
      triggers:
        - type: http
          path: /bid
      reactive:
        inbox:
          capacity: 500
          overflow: drop-oldest
      baseFlow:
        path: ./flows/bid.yaml
      """
    When I run `clawdflow validate fixtures/inbox.yaml`
    Then validation succeeds with exit code 0

  # =========================================================================
  # REQ-SCHEMA-009: No deployment concerns
  # =========================================================================

  @req-SCHEMA-009
  Scenario: Agent YAML rejects deployment fields
    Given an agent YAML file "fixtures/deployment.yaml" with content:
      """
      name: bad-agent
      mode: persistent
      replicas: 3
      resources:
        cpu: 500m
        memory: 1Gi
      baseFlow:
        path: ./flows/main.yaml
      """
    When I run `clawdflow validate fixtures/deployment.yaml`
    Then validation fails with error containing "unknown field"
