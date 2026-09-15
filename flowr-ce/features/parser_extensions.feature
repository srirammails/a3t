# requirements: requirements_runtime.yaml
Feature: CNCF SWF Extension Parsing

  @req-parse-003 @req-ext-001 @parser @conformance @extensions
  Scenario: Parse use block with extensions
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: ext-test
        version: '1.0.0'
      use:
        extensions:
          - logging:
              extend: all
              before:
                - logBefore:
                    call: http
                    with:
                      method: post
                      endpoint: https://logs.example.com
                      body:
                        message: "${ \"Executing \" + $task.reference }"
              after:
                - logAfter:
                    call: http
                    with:
                      method: post
                      endpoint: https://logs.example.com
                      body:
                        message: "${ \"Completed \" + $task.reference }"
          - costGuard:
              extend: call
              when: "${ $context._call_count > 100 }"
              before:
                - block:
                    raise:
                      error:
                        type: https://example.com/errors/budget
                        status: 429
                        title: Budget Exceeded
      do:
        - hello:
            set:
              message: hello
      """
    When I parse the workflow
    Then parsing succeeds
    And the workflow has 2 extensions
    And extension "logging" extends "all"
    And extension "costGuard" extends "call"

  @req-parse-003 @parser @conformance @functions
  Scenario: Parse use block with functions
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: func-test
        version: '1.0.0'
      use:
        functions:
          fetchUser:
            call: http
            with:
              method: GET
              endpoint:
                uri: "https://api.example.com/users/${ .user_id }"
          computeTotal:
            set:
              total: "${ .items | map(.price) | add }"
      do:
        - getUser:
            call: fetchUser
            with:
              user_id: "123"
        - calcTotal:
            call: computeTotal
      """
    When I parse the workflow
    Then parsing succeeds
    And the workflow has 2 functions
    And function "fetchUser" calls "http"
    And function "computeTotal" calls "set"
