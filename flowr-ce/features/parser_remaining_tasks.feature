# requirements: requirements_runtime.yaml
Feature: CNCF SWF Remaining Task Type Parsing

  @req-task-003 @parser @conformance
  Scenario: Parse call with cataloged function reference
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: catalog-call
        version: '1.0.0'
      use:
        catalogs:
          global:
            endpoint:
              uri: https://github.com/serverlessworkflow/catalog
      do:
        - logIt:
            call: log:0.5.2@global
            with:
              message: Hello from catalog
      """
    When I parse the workflow
    Then parsing succeeds
    And task "logIt" is a "call" task

  @req-task-003 @parser @conformance
  Scenario: Parse call with use.functions reference
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: func-call
        version: '1.0.0'
      use:
        functions:
          greetUser:
            set:
              greeting: "${ \"Hello, \" + .name }"
      do:
        - callFunc:
            call: greetUser
            with:
              name: World
      """
    When I parse the workflow
    Then parsing succeeds
    And the workflow has 1 functions
    And function "greetUser" calls "set"

  @req-task-006 @parser @conformance
  Scenario: Parse switch with when conditions and default
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: switch-parse
        version: '1.0.0'
      do:
        - route:
            switch:
              - high:
                  when: "${ .priority == \"high\" }"
                  then: urgent
              - low:
                  when: "${ .priority == \"low\" }"
                  then: normal
              - fallback:
                  then: normal
        - urgent:
            set:
              queue: urgent
            then: end
        - normal:
            set:
              queue: normal
      """
    When I parse the workflow
    Then parsing succeeds
    And task "route" is a "switch" task

  @req-task-007 @parser @conformance
  Scenario: Parse try with catch and retry
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: try-parse
        version: '1.0.0'
      do:
        - resilientCall:
            try:
              do:
                - callApi:
                    call: http
                    with:
                      method: GET
                      endpoint:
                        uri: https://api.example.com
              catch:
                errors:
                  with:
                    status: 503
                retry:
                  delay:
                    seconds: 2
                  backoff:
                    exponential: {}
                  limit:
                    attempt:
                      count: 3
                do:
                  - fallback:
                      set:
                        error: service unavailable
      """
    When I parse the workflow
    Then parsing succeeds
    And task "resilientCall" is a "try" task

  @req-task-009 @parser @conformance
  Scenario: Parse wait with ISO 8601 duration
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: wait-parse
        version: '1.0.0'
      do:
        - pause30s:
            wait:
              seconds: 30
        - pause2m:
            wait:
              minutes: 2
      """
    When I parse the workflow
    Then parsing succeeds
    And the workflow has 2 tasks
    And task "pause30s" is a "wait" task
    And task "pause2m" is a "wait" task

  @req-task-010 @parser @conformance
  Scenario: Parse emit task with CloudEvents
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: emit-parse
        version: '1.0.0'
      do:
        - notifyOrderPlaced:
            emit:
              event:
                with:
                  type: com.example.order.placed
                  source: /orders
                  data:
                    orderId: "${ .order_id }"
                    total: "${ .total }"
      """
    When I parse the workflow
    Then parsing succeeds
    And task "notifyOrderPlaced" is a "emit" task

  @req-task-011 @parser @conformance
  Scenario: Parse listen task with event filter
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: listen-parse
        version: '1.0.0'
      do:
        - waitForApproval:
            listen:
              to:
                one:
                  with:
                    type: com.example.approval
                    source: /approvals
      """
    When I parse the workflow
    Then parsing succeeds
    And task "waitForApproval" is a "listen" task

  @req-task-012 @parser @conformance
  Scenario: Parse fork task with branches
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: fork-parse
        version: '1.0.0'
      do:
        - parallel:
            fork:
              compete: false
              branches:
                - fetchUser:
                    call: http
                    with:
                      method: GET
                      endpoint:
                        uri: https://api.example.com/user
                - fetchOrders:
                    call: http
                    with:
                      method: GET
                      endpoint:
                        uri: https://api.example.com/orders
      """
    When I parse the workflow
    Then parsing succeeds
    And task "parallel" is a "fork" task

  @req-task-013 @parser @conformance
  Scenario: Parse raise with RFC 7807 error
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: raise-parse
        version: '1.0.0'
      do:
        - rejectRequest:
            raise:
              error:
                type: https://serverlessworkflow.io/spec/1.0.0/errors/validation
                status: 400
                title: Validation Error
                detail: Name field is required
                instance: /do/rejectRequest
      """
    When I parse the workflow
    Then parsing succeeds
    And task "rejectRequest" is a "raise" task
