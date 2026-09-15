# requirements: requirements_runtime.yaml
Feature: CNCF SWF Runtime Task Handlers

  Background:
    Given the runtime is initialized with storage

  @req-hand-001 @runtime @call-http @smoke
  Scenario: call:http GET returns status and body
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: http-get
        version: '1.0.0'
      do:
        - fetch:
            call: http
            with:
              method: GET
              endpoint:
                uri: https://httpbin.org/get
            output:
              as: "${ {status: .status, has_body: (.body != null)} }"
      """
    When I execute the workflow with input "{}"
    Then execution succeeds
    And the output field "status" is "200"
    And the output field "has_body" is "true"

  @req-hand-001 @runtime @call-http
  Scenario: call:http POST sends body and headers
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: http-post
        version: '1.0.0'
      do:
        - sendData:
            call: http
            with:
              method: POST
              endpoint:
                uri: https://httpbin.org/post
              headers:
                Content-Type: application/json
                X-Test: hello
              body:
                key: value
            output:
              as: "${ {posted: true, status: .status} }"
      """
    When I execute the workflow with input "{}"
    Then execution succeeds
    And the output field "posted" is "true"

  @req-hand-002 @runtime @smoke
  Scenario: set handler evaluates expressions and returns map
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: set-handler
        version: '1.0.0'
      do:
        - compute:
            set:
              sum: "${ 10 + 32 }"
              greeting: "${ \"Hello, \" + \"World\" }"
              static: true
      """
    When I execute the workflow with input "{}"
    Then execution succeeds
    And the output field "sum" is "42"
    And the output field "greeting" is "Hello, World"
    And the output field "static" is "true"

  @req-hand-004 @runtime
  Scenario: run:workflow executes nested workflow with full pipeline
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: child-flow
        version: '1.0.0'
      do:
        - double:
            set:
              result: "${ .value * 2 }"
      """
    Given the workflow "test/child-flow" is deployed
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: parent-flow
        version: '1.0.0'
      do:
        - callChild:
            run:
              workflow:
                namespace: test
                name: child-flow
                input: "${ {value: 21} }"
            output:
              as: "${ {answer: .result} }"
      """
    When I execute the workflow with input "{}"
    Then execution succeeds
    And the output field "answer" is "42"

  @req-hand-005 @runtime
  Scenario: run:shell executes command and captures stdout
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: shell-handler
        version: '1.0.0'
      do:
        - echoTest:
            run:
              shell:
                command: "echo hello-from-shell"
            output:
              as: "${ {output: .stdout} }"
      """
    When I execute the workflow with input "{}"
    Then execution succeeds
    And the output field "output" contains "hello-from-shell"

  @req-hand-006 @runtime @flow-control
  Scenario: try catches error and executes catch handler
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: try-catch
        version: '1.0.0'
      do:
        - safeDivide:
            try:
              do:
                - divide:
                    raise:
                      error:
                        type: https://example.com/errors/division
                        status: 400
                        title: Division Error
                        detail: Cannot divide by zero
              catch:
                errors:
                  with:
                    status: 400
                do:
                  - recover:
                      set:
                        result: caught
                        error_handled: true
      """
    When I execute the workflow with input "{}"
    Then execution succeeds
    And the output field "error_handled" is "true"
