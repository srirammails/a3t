# requirements: requirements_clawdflow.yaml
Feature: ClawdFlow Template Registry

  Background:
    Given ClawdFlow is started on port 3099

  @req-tmpl-001 @smoke
  Scenario: Template CRUD operations
    When I create a template "test-tmpl" at version "1.0.0" via POST /templates
    Then GET /templates returns a list containing "test-tmpl"
    When I GET /templates/test-tmpl
    Then the response contains template "test-tmpl" at version "1.0.0"
    When I delete template "test-tmpl"
    Then GET /templates does not contain "test-tmpl"

  @req-tmpl-002
  Scenario: Template version activation
    When I create a template "versioned" at version "1.0.0" via POST /templates
    When I create a template "versioned" at version "2.0.0" via POST /templates
    Then GET /templates/versioned returns version "1.0.0"
    When I POST /templates/versioned/activate/2.0.0
    Then GET /templates/versioned returns version "2.0.0"
