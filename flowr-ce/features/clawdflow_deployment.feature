# requirements: requirements_clawdflow.yaml
Feature: ClawdFlow Deployment

  @req-deploy-001 @smoke
  Scenario: Docker build produces runnable image
    Given the Dockerfile at "clawdflow/Dockerfile"
    When I run docker build with tag "clawdflow-test:latest"
    Then the build completes with exit code 0

  @req-deploy-001
  Scenario: Container serves HTTP with persistent volume
    Given a running Docker container "clawdflow-test:latest" with port 3099 mapped
    Then the HTTP endpoint is reachable at port 3099
