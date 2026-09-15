@phase-1 @ui
Feature: UI Embedded Assets
  UI is embedded in the binary with no external dependencies.

  @req-UI-007
  Scenario: UI assets are served from the binary
    Given FlowR is started on port 3002 with --ui
    Then GET /ui/ returns HTML with status 200
    And GET /ui/app.js returns JavaScript with status 200
    And GET /ui/style.css returns CSS with status 200
    And no external CDN requests are required

  @req-UI-007
  Scenario: UI works without internet access
    Given FlowR is started on port 3002 with --ui
    And the server has no internet access
    Then GET /ui/ returns a fully functional page
    And all JavaScript and CSS load from local assets

  @req-UI-008 @should
  Scenario: Input form generated from JSON Schema
    Given a template "form-agent" exists with input_schema:
      """
      {
        "type": "object",
        "properties": {
          "query": {"type": "string", "description": "Search query"},
          "limit": {"type": "integer", "default": 10}
        },
        "required": ["query"]
      }
      """
    When the user navigates to the template detail page
    Then the page renders a form with field "query" (text, required)
    And the page renders a form with field "limit" (number, default 10)

  @req-UI-009 @should
  Scenario: Agent output rendered with HTML template
    Given an agent template with output_template "<h2>Result</h2><p>{{output}}</p>"
    When the agent produces output "The answer is 42"
    Then the UI renders "<h2>Result</h2><p>The answer is 42</p>"
