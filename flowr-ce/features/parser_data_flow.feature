# requirements: requirements_runtime.yaml
Feature: CNCF SWF Data Flow Parsing

  @req-data-001 @parser @conformance @data-flow
  Scenario: Parse task with input transformation and schema
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: input-transform
        version: '1.0.0'
      do:
        - process:
            set:
              result: "${ .filtered }"
            input:
              from: "${ {filtered: .raw | select(. > 0)} }"
              schema:
                type: object
                required: [filtered]
      """
    When I parse the workflow
    Then parsing succeeds

  @req-data-002 @parser @conformance @data-flow
  Scenario: Parse task with output transformation and schema
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: output-transform
        version: '1.0.0'
      do:
        - fetch:
            call: http
            with:
              method: GET
              endpoint:
                uri: https://api.example.com
            output:
              as: "${ {data: .body, status: .status} }"
              schema:
                type: object
                required: [data, status]
      """
    When I parse the workflow
    Then parsing succeeds

  @req-data-003 @parser @conformance @data-flow
  Scenario: Parse task with export to context
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: export-context
        version: '1.0.0'
      do:
        - compute:
            set:
              total: "${ .a + .b }"
            export:
              as: "${ {running_total: .total} }"
      """
    When I parse the workflow
    Then parsing succeeds

  @req-data-004 @parser @conformance @data-flow
  Scenario: Parse task with if condition
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: conditional
        version: '1.0.0'
      do:
        - expensive:
            if: "${ .should_run == true }"
            set:
              result: computed
      """
    When I parse the workflow
    Then parsing succeeds

  @req-data-005 @parser @conformance
  Scenario: Parse task with timeout
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: timeout-test
        version: '1.0.0'
      do:
        - slowCall:
            call: http
            with:
              method: GET
              endpoint:
                uri: https://slow.example.com
            timeout:
              after:
                seconds: 30
      """
    When I parse the workflow
    Then parsing succeeds
