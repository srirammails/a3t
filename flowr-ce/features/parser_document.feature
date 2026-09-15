# requirements: requirements_runtime.yaml
Feature: CNCF SWF Document Parsing

  @req-parse-001 @parser @conformance @smoke
  Scenario: Parse a valid CNCF SWF 1.0.3 document
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: hello
        version: '1.0.0'
      do:
        - greet:
            set:
              message: hello
      """
    When I parse the workflow
    Then parsing succeeds
    And the document dsl is "1.0.3"
    And the document namespace is "test"
    And the document name is "hello"

  @req-parse-002 @parser @conformance @smoke
  Scenario: Parse a workflow with multiple named tasks in do block
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: default
        name: multi-task
        version: '1.0.0'
      do:
        - step1:
            set:
              a: 1
        - step2:
            set:
              b: 2
        - step3:
            set:
              c: 3
      """
    When I parse the workflow
    Then parsing succeeds
    And the workflow has 3 tasks
    And task "step1" is a "set" task
    And task "step2" is a "set" task
    And task "step3" is a "set" task

  @req-parse-004 @parser @smoke
  Scenario: Reject workflow missing document block
    Given the workflow YAML:
      """
      do:
        - greet:
            set:
              message: hello
      """
    When I parse the workflow
    Then parsing fails with error containing "document"

  @req-parse-005 @parser @smoke
  Scenario: Reject workflow missing do block
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: empty
        version: '1.0.0'
      """
    When I parse the workflow
    Then parsing fails with error containing "do"
