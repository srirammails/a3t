# requirements: requirements_runtime.yaml
Feature: CNCF SWF Authentication and Error Model Parsing

  @req-auth-001 @parser @conformance
  Scenario: Parse basic authentication
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: auth-basic
        version: '1.0.0'
      use:
        authentications:
          admin:
            basic:
              username: admin
              password: secret123
      do:
        - callSecure:
            call: http
            with:
              method: GET
              endpoint:
                uri: https://api.example.com/admin
                authentication:
                  use: admin
      """
    When I parse the workflow
    Then parsing succeeds

  @req-auth-001 @parser @conformance
  Scenario: Parse bearer authentication with expression
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: auth-bearer
        version: '1.0.0'
      use:
        authentications:
          api:
            bearer: "${ $secrets.API_TOKEN }"
      do:
        - callApi:
            call: http
            with:
              method: GET
              endpoint:
                uri: https://api.example.com
                authentication:
                  use: api
      """
    When I parse the workflow
    Then parsing succeeds

  @req-auth-001 @parser @conformance
  Scenario: Parse OAuth2 authentication
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: auth-oauth2
        version: '1.0.0'
      use:
        authentications:
          keycloak:
            oauth2:
              authority: http://keycloak/realms/app/.well-known/openid-configuration
              grant: client-credentials
              client:
                id: workflow-runtime
                secret: "${ $secrets.KEYCLOAK_SECRET }"
              scopes: [api]
              audiences: [runtime]
      do:
        - callSecure:
            call: http
            with:
              method: GET
              endpoint:
                uri: https://api.example.com
                authentication:
                  use: keycloak
      """
    When I parse the workflow
    Then parsing succeeds

  @req-err-001 @parser @conformance
  Scenario: Parse error definition conforming to RFC 7807
    Given the workflow YAML:
      """
      document:
        dsl: '1.0.3'
        namespace: test
        name: error-model
        version: '1.0.0'
      use:
        errors:
          notFound:
            type: https://serverlessworkflow.io/spec/1.0.0/errors/not-found
            status: 404
            title: Not Found
          timeout:
            type: https://serverlessworkflow.io/spec/1.0.0/errors/timeout
            status: 408
            title: Request Timeout
      do:
        - checkExists:
            try:
              do:
                - lookup:
                    call: http
                    with:
                      method: GET
                      endpoint:
                        uri: https://api.example.com/item/123
              catch:
                errors:
                  with:
                    type: https://serverlessworkflow.io/spec/1.0.0/errors/communication
                    status: 404
                do:
                  - handleMissing:
                      raise:
                        error:
                          use: notFound
                          detail: "Item 123 not found"
                          instance: /do/checkExists
      """
    When I parse the workflow
    Then parsing succeeds
