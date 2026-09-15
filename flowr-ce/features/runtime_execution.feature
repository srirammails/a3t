# requirements: requirements_runtime.yaml
Feature: CNCF SWF Runtime Execution

  Background:
    Given the runtime is initialized with storage

  @req-exec-001 @runtime @smoke
  Scenario: Sequential task execution with data passing
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: sequential
        version: '1.0.0'
      do:
        - init:
            set:
              value: 10
        - double:
            set:
              value: "${ .value * 2 }"
        - result:
            set:
              final: "${ .value }"
      """
    When I execute the workflow with input "{}"
    Then execution succeeds
    And the output field "final" is "20"

  @req-exec-002 @runtime @data-flow
  Scenario: Task skipped by if condition
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: if-skip
        version: '1.0.0'
      do:
        - init:
            set:
              ran_expensive: false
        - expensive:
            if: "${ .skip != true }"
            set:
              ran_expensive: true
        - result:
            set:
              skipped: "${ .ran_expensive == false }"
      """
    When I execute the workflow with input "{ \"skip\": true }"
    Then execution succeeds
    And the output field "skipped" is "true"
    And task "expensive" was skipped

  @req-exec-003 @runtime @data-flow
  Scenario: Input and output transformations
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: transforms
        version: '1.0.0'
      do:
        - transform:
            set:
              doubled: "${ .x * 2 }"
            input:
              from: "${ {x: .raw_value} }"
            output:
              as: "${ {result: .doubled} }"
      """
    When I execute the workflow with input "{ \"raw_value\": 21 }"
    Then execution succeeds
    And the output field "result" is "42"

  @req-exec-004 @runtime @data-flow
  Scenario: Schema validation rejects invalid input
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: schema-reject
        version: '1.0.0'
      input:
        schema:
          type: object
          required: [name]
          properties:
            name:
              type: string
      do:
        - greet:
            set:
              message: "${ \"Hello \" + .name }"
      """
    When I execute the workflow with input "{}"
    Then execution fails with error containing "validation"

  @req-exec-005 @runtime
  Scenario: Runtime expression arguments available
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: expr-args
        version: '1.0.0'
      do:
        - checkArgs:
            set:
              has_context: "${ $context != null }"
              has_workflow: "${ $workflow != null }"
              has_runtime: "${ $runtime != null }"
              workflow_name: "${ $workflow.name }"
      """
    When I execute the workflow with input "{}"
    Then execution succeeds
    And the output field "has_context" is "true"
    And the output field "has_workflow" is "true"
    And the output field "workflow_name" is "expr-args"

  @req-exec-006 @runtime @extensions
  Scenario: Extensions fire before and after matched tasks
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: ext-fire
        version: '1.0.0'
      use:
        extensions:
          - audit:
              extend: all
              before:
                - markBefore:
                    set:
                      _before_fired: true
              after:
                - markAfter:
                    set:
                      _after_fired: true
      do:
        - main:
            set:
              value: 42
      """
    When I execute the workflow with input "{}"
    Then execution succeeds
    And the extension "audit" fired

  @req-exec-007 @runtime @functions
  Scenario: use.functions resolves function references
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: func-resolve
        version: '1.0.0'
      use:
        functions:
          addTen:
            set:
              result: "${ .value + 10 }"
      do:
        - compute:
            call: addTen
            with:
              value: 32
      """
    When I execute the workflow with input "{}"
    Then execution succeeds
    And the output field "result" is "42"

  @req-exec-008 @runtime @flow-control
  Scenario: Flow directive end stops execution
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: early-exit
        version: '1.0.0'
      do:
        - first:
            set:
              step: 1
            then: end
        - second:
            set:
              step: 2
      """
    When I execute the workflow with input "{}"
    Then execution succeeds
    And the output field "step" is "1"
    And task "second" was skipped

  @req-hand-003 @runtime @flow-control
  Scenario: For loop with while condition exits early
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: for-while
        version: '1.0.0'
      do:
        - init:
            set:
              count: 0
              limit: 3
            export:
              as: "."
        - loop:
            for:
              each: i
              in: "${ [range(100)] }"
              while: "${ $context.count < $context.limit }"
              do:
                - increment:
                    set:
                      count: "${ $context.count + 1 }"
                    export:
                      as: "."
        - result:
            set:
              final_count: "${ $context.count }"
      """
    When I execute the workflow with input "{}"
    Then execution succeeds
    And the output field "final_count" is "3"

  @req-hand-007 @runtime @flow-control
  Scenario: Switch selects first matching case
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: switch-test
        version: '1.0.0'
      do:
        - route:
            switch:
              - approved:
                  when: "${ .status == \"approved\" }"
                  then: handleApproved
              - rejected:
                  when: "${ .status == \"rejected\" }"
                  then: handleRejected
              - fallback:
                  then: handleDefault
        - handleApproved:
            set:
              result: approved
            then: end
        - handleRejected:
            set:
              result: rejected
            then: end
        - handleDefault:
            set:
              result: unknown
      """
    When I execute the workflow with input "{ \"status\": \"approved\" }"
    Then execution succeeds
    And the output field "result" is "approved"

  @req-plug-001 @req-plug-002 @runtime
  Scenario: Runtime selects provider by DSL version
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: version-check
        version: '1.0.0'
      do:
        - check:
            set:
              executed: true
      """
    When I execute the workflow
    Then execution succeeds
    And the output field "executed" is "true"
