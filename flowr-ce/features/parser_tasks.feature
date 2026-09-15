# requirements: requirements_runtime.yaml
Feature: CNCF SWF Task Type Parsing

  @req-task-001 @parser @conformance
  Scenario: Parse all 12 task types
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: all-tasks
        version: '1.0.0'
      do:
        - callHttp:
            call: http
            with:
              method: GET
              endpoint:
                uri: https://example.com
        - doBlock:
            do:
              - inner:
                  set:
                    x: 1
        - emitEvent:
            emit:
              event:
                with:
                  type: test.event
                  source: test
        - forLoop:
            for:
              each: item
              in: "${ [1,2,3] }"
              do:
                - process:
                    set:
                      val: "${ $item }"
        - forkBranches:
            fork:
              branches:
                - branchA:
                    set:
                      a: 1
                - branchB:
                    set:
                      b: 2
        - listenForEvent:
            listen:
              to:
                one:
                  with:
                    type: order.placed
        - raiseError:
            raise:
              error:
                type: https://example.com/errors/test
                status: 400
                title: Test Error
        - runShell:
            run:
              shell:
                command: echo hello
        - setValue:
            set:
              greeting: hello
        - switchCase:
            switch:
              - caseA:
                  when: "${ .status == \"active\" }"
                  then: continue
              - default:
                  then: end
        - tryCatch:
            try:
              do:
                - attempt:
                    set:
                      tried: true
              catch:
                errors:
                  with:
                    status: 500
                do:
                  - recover:
                      set:
                        recovered: true
        - waitDuration:
            wait:
              seconds: 1
      """
    When I parse the workflow
    Then parsing succeeds
    And the workflow has 12 tasks
    And task "callHttp" is a "call" task
    And task "doBlock" is a "do" task
    And task "emitEvent" is a "emit" task
    And task "forLoop" is a "for" task
    And task "forkBranches" is a "fork" task
    And task "listenForEvent" is a "listen" task
    And task "raiseError" is a "raise" task
    And task "runShell" is a "run" task
    And task "setValue" is a "set" task
    And task "switchCase" is a "switch" task
    And task "tryCatch" is a "try" task
    And task "waitDuration" is a "wait" task

  @req-task-002 @parser @conformance
  Scenario: Parse call:http with full config
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: http-call
        version: '1.0.0'
      do:
        - fetchData:
            call: http
            with:
              method: POST
              endpoint:
                uri: https://api.example.com/data
              headers:
                Authorization: "Bearer token123"
                Content-Type: application/json
              body:
                query: test
      """
    When I parse the workflow
    Then parsing succeeds
    And task "fetchData" is a "call" task

  @req-task-004 @parser @conformance @smoke
  Scenario: Parse set task with expressions
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: set-test
        version: '1.0.0'
      do:
        - compute:
            set:
              doubled: "${ .value * 2 }"
              greeting: "${ \"Hello, \" + .name }"
              static: 42
      """
    When I parse the workflow
    Then parsing succeeds
    And task "compute" is a "set" task

  @req-task-005 @parser @conformance
  Scenario: Parse for task with while condition
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: for-while
        version: '1.0.0'
      do:
        - loop:
            for:
              each: i
              in: "${ [range(100)] }"
              while: "${ $context._continue == true }"
              do:
                - step:
                    set:
                      count: "${ $context.count + 1 }"
      """
    When I parse the workflow
    Then parsing succeeds
    And task "loop" is a "for" task

  @req-task-008 @parser @conformance
  Scenario: Parse run task variants
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: run-variants
        version: '1.0.0'
      do:
        - shellCmd:
            run:
              shell:
                command: echo hello
        - scriptRun:
            run:
              script:
                language: python
                code: |
                  result = {"sum": 1 + 2}
        - nestedFlow:
            run:
              workflow:
                namespace: default
                name: child-flow
                version: '1.0.0'
                input: "${ {data: .payload} }"
      """
    When I parse the workflow
    Then parsing succeeds
    And the workflow has 3 tasks
    And task "shellCmd" is a "run" task
    And task "scriptRun" is a "run" task
    And task "nestedFlow" is a "run" task
