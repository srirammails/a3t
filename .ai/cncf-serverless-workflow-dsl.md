# Serverless Workflow DSL

## Table of Contents

- [Abstract](#abstract)
- [Motivation](#motivation)
- [Priority of Constituencies](#priority-of-constituencies)
- [Design](#design)
- [Concepts](#concepts)
  + [Workflow](#workflow)
    - [Status Phases](#status-phases)
    - [Lifecycle Events](#lifecycle-events)
      + [Workflow Lifecycle Events](#workflow-lifecycle-events)
      + [Task Lifecycle Events](#task-lifecycle-events)
    - [Components](#components)
      + [Task](#task)
    - [Scheduling](#scheduling)
      + [Event-Driven Scheduling](#event-driven-scheduling)
  + [Task Flow](#task-flow)
  + [Data Flow](#data-flow)
  + [Runtime Expressions](#runtime-expressions)
    - [Arguments](#runtime-expression-arguments)
  + [Fault Tolerance](#fault-tolerance)
  + [Timeouts](#timeouts)
  + [Catalogs](#catalogs)
    + [File Structure](#file-structure)
    + [Default Catalog](#default-catalog)
    + [Using Cataloged Functions](#using-cataloged-functions)
  + [Interoperability](#interoperability)
    - [Supported Protocols](#supported-protocols)
    - [Custom and Non-Standard Interactions](#custom-and-non-standard-interactions)
      + [Creating a Custom Function](#creating-a-custom-function)
      + [Using a Custom Function](#using-a-custom-function)
      + [Publishing a Custom Function](#publishing-a-custom-function)
  + [Events](#events)
  + [Extensions](#extensions)
  + [External Resources](#external-resources)
  + [Authentication](#authentication)

## Abstract

This document proposes the creation of a Domain Specific Language (DSL) called Serverless Workflow, tailored for building platform agnostic workflows. 

Serverless Workflow aims to simplify the orchestration of complex processes across diverse environments, providing developers with a unified syntax and set of tools for defining and executing serverless workflows.

## Motivation

Serverless computing has gained popularity for its ability to abstract away infrastructure management tasks, enabling developers to focus on application logic. However, orchestrating serverless workflows across multiple environments often involves dealing with disparate tools and platforms, leading to complexity and inefficiency.

Serverless Workflow addresses this challenge by providing a DSL specifically designed for serverless workflow orchestration. By abstracting away the underlying infrastructure complexities and offering a modular and extensible framework, Serverless Workflow aims to streamline the development, deployment, and management of serverless workflows.

## Priority of Constituencies

Inspired by the [Priority of Constituencies](https://www.w3.org/TR/2024/NOTE-design-principles-20240718/#priority-of-constituencies) principle from the W3C Design Principles, the Serverless Workflow DSL prioritizes the following constituencies (collectively referred to as "users"):

- Authors: people authoring and reading workflows
- Operators: people running and operating a runtime implementation of the specification
- Implementors: people implementing a specification compliant runtime
- Specifications writers: people working on the specifications of Serverless Workflow

If a trade-off needs to be made, always put author's needs above all.

Similarly, when beginning to design an API, be sure to understand and document the user needs that the API aims to address.

Author needs come before the needs of operators, which come before the needs of runtime implementors, which come before the needs of specification writers, which come before theoretical purity.

Like all principles, this isn't absolute. Ease of operations affects the perceived reliability of authors' workflows. Implementors have to prioritize finite engineering resources, which affects how features reach authors. Specification writers also have finite resources, and theoretical concerns reflect the underlying needs of all of these groups.

## Design

The Serverless Workflow DSL is crafted with a design philosophy that prioritizes clarity, expressiveness, and ease of use. Its foundation lies in linguistic fluency, emphasizing readability and comprehension. By adopting a fluent style, the DSL promotes intuitive understanding through natural language constructs. Verbs are employed in the imperative tense to denote actions, enhancing clarity and directness in expressing workflow logic. This imperative approach empowers developers to articulate their intentions succinctly and effectively.

The DSL also embraces the principle of implicit default behaviors, sparing authors from unnecessary repetition and enhancing the conciseness of workflow definitions. For instance, default settings alleviate the burden of explicitly defining every detail, streamlining the workflow design process. Furthermore, the DSL allows both inline declaration of components or the creation of reusable elements, granting flexibility in workflow composition. This flexibility allows developers to seamlessly integrate inline task definitions without imposing rigid structural requirements.

Moreover, the DSL eschews strong-typed enumerations wherever feasible, fostering extensibility and adaptability across different runtime environments. While maintaining portability is crucial, the DSL prioritizes customization options for extensions and runtimes, enabling tailored implementations to suit diverse use cases. Additionally, the DSL favors universally understood terms over technical jargon, enhancing accessibility and comprehension for a broader audience.

- Embrace linguistic fluency for enhanced readability and understanding.
- Utilize imperative verbs to convey actions directly and clearly.
- Employ implicit default behaviors to reduce redundancy and streamline workflow definitions.
- Enable the declaration and effortless import of shared components by supporting external references
- Encourage the declaration of components inline for situations where reusability is unnecessary, prioritizing ease of use in such cases.
- Prioritize flexibility over strong-typed enumerations for enhanced extensibility.
- Opt for universally understood terms to improve accessibility and comprehension.

## Concepts

### Workflow

A Serverless Workflow is a sequence of specific tasks that are executed in a defined order. By default, this order follows the declaration sequence within the workflow definition. Workflows are designed to automate processes and orchestrate various serverless functions and services. 

Workflows can be triggered in different ways: upon request, scheduled using CRON expressions, or initiated upon correlation with specific events. 

Additionally, workflows may optionally accept inputs and produce outputs, allowing for data processing and transformation within the workflow execution.

#### Status Phases

Both workflows and tasks in the Serverless Workflow DSL can exist in several phases, each indicating the current state of the workflow/task execution. These phases include:

| Phase |    Description |
| --- | --- |
| `pending` |    The workflow/task has been initiated and is pending execution. |
| `running` |    The workflow/task is currently in progress. |
| `waiting` |    The workflow/task execution is temporarily paused, awaiting either inbound event(s) or a specified time interval as defined by a `wait` task. |
| `suspended` |    The workflow/task execution has been manually paused by a user and will remain halted until explicitly resumed. |
| `cancelled` |    The workflow/task execution has been terminated before completion. |
| `faulted` |    The workflow/task execution has encountered an error. |
| `completed` |    The workflow/task ran to completion. |

Additionally, the flow of execution within a workflow can be controlled using flow directives, which provide instructions to the workflow engine on how to manage and handle specific aspects of workflow execution.

#### Lifecycle Events

Lifecycle cloud events standardize notifications for key state changes in workflows and tasks—such as start, suspend, resume, cancel, fault, complete, and status change. These events carry consistent information like IDs, status transitions, timestamps, and relevant metadata, enabling users to reliably respond to updates, enhancing interoperability and simplifying integration across different implementations.

Runtimes are expected to publish these events upon state changes. While using the HTTP protocol binding with structured content mode is recommended, other transports adhering to the CloudEvents specification may be used.

##### Workflow Lifecycle Events

| Type | Data | Required | Description |
|:----:|:----:|:--------:|:------------|
| `io.serverlessworkflow.workflow.started.v1` | `workflowStartedEvent` | `yes` | Notifies about the start of a workflow. |
| `io.serverlessworkflow.workflow.suspended.v1` | `workflowSuspendedEvent` | `yes` | Notifies about suspending a workflow execution. |
| `io.serverlessworkflow.workflow.resumed.v1` | `workflowResumedEvent` | `yes` | Notifies about resuming a workflow execution. |
| `io.serverlessworkflow.workflow.correlation-started.v1` | `workflowCorrelationStartedEvent` | `yes` | Notifies about a workflow starting to correlate events. |
| `io.serverlessworkflow.workflow.correlation-completed.v1` | `workflowCorrelationCompletedEvent` | `yes` | Notifies about a workflow completing an event correlation. |
| `io.serverlessworkflow.workflow.cancelled.v1` | `workflowCancelledEvent` | `yes` | Notifies about the cancellation of a workflow execution. |
| `io.serverlessworkflow.workflow.faulted.v1` | `workflowFaultedEvent` | `yes` | Notifies about a workflow being faulted. |
| `io.serverlessworkflow.workflow.completed.v1` | `workflowCompletedEvent` | `yes` | Notifies about the completion of a workflow execution. |
| `io.serverlessworkflow.workflow.status-changed.v1` | `workflowStatusChangedEvent` | `no` | Notifies about the change of a workflow's status phase. |

##### Task Lifecycle Events

| Type | Data | Required | Description |
|:----:|:----:|:--------:|:------------|
| `io.serverlessworkflow.task.created.v1` | `taskCreatedEvent` | `yes` | Notifies about the creation of a task. |
| `io.serverlessworkflow.task.started.v1` | `taskStartedEvent` | `yes` | Notifies about the start of a task. |
| `io.serverlessworkflow.task.suspended.v1` | `taskSuspendedEvent` | `yes` | Notifies about suspending a task's execution. |
| `io.serverlessworkflow.task.resumed.v1` | `taskResumedEvent` | `yes` | Notifies about resuming a task's execution. |
| `io.serverlessworkflow.task.retried.v1` | `taskRetriedEvent` | `yes` | Notifies about retrying a task's execution. |
| `io.serverlessworkflow.task.cancelled.v1` | `taskCancelledEvent` | `yes` | Notifies about the cancellation of a task's execution. |
| `io.serverlessworkflow.task.faulted.v1` | `taskFaultedEvent` | `yes` | Notifies about a task being faulted. |
| `io.serverlessworkflow.task.completed.v1` | `taskCompletedEvent` | `yes` | Notifies about the completion of a task's execution. |
| `io.serverlessworkflow.task.status-changed.v1` | `taskStatusChangedEvent` | `no` | Notifies about the change of a task's status phase. |

#### Components

Serverless Workflow DSL allows for defining reusable components that can be referenced across the workflow. These include:

- Authentications
- Errors
- Extensions
- Functions
- Retries
- Secrets

##### Task

Tasks are the fundamental computing units of a workflow. They define the different types of actions that a workflow can perform, including the ability to mutate their input and output data. Tasks can also write to and modify the context data, enabling complex and dynamic workflow behaviors.

The Serverless Workflow DSL defines several default task types that runtimes **must** implement:

- **Call**, used to call services and/or functions.
- **Do**, used to define one or more subtasks to perform in sequence.
- **Emit**, used to emit events.
- **For**, used to iterate over a collection of items, and conditionally perform a task for each of them.
- **Fork**, used to define one or more two subtasks to perform in parallel.
- **Listen**, used to listen for an event or more.
- **Raise**, used to raise an error and potentially fault the workflow.
- **Run**, used to run a container, a script, a shell command or even another workflow. 
- **Set**, used to dynamically set the workflow's data during the its execution. 
- **Switch**, used to dynamically select and execute one of multiple alternative paths based on specified conditions
- **Try**, used to attempt executing a specified task, and to handle any resulting errors gracefully, allowing the workflow to continue without interruption.
- **Wait**, used to pause or wait for a specified duration before proceeding to the next task.

##### Secret

Secrets are sensitive information required by a workflow to securely access protected resources or services. They provide a way to securely store and manage credentials, tokens, or other sensitive data used during workflow execution.

Runtime **must** implement a mechanism capable of providing the workflow with the data contained within the defined secrets. If a workflow attempts to access a secret to which it does not have access rights or which does not exist, runtimes **must** raise an error with type `https://serverlessworkflow.io/spec/1.0.0/errors/authorization` and status `403`.

#### Scheduling

Workflow scheduling in ServerlessWorkflow allows developers to specify when and how their workflows should be executed, ensuring timely response to events and efficient resource utilization. It offers four key properties: `every`, `cron`, `after`, and `on`. 

- The `every` property defines the interval for workflow execution, ensuring periodic runs regardless of the previous run's status. 
- With `cron`, developers can use CRON expressions to schedule workflow execution at specific times or intervals.
- `after` specifies a delay duration before restarting the workflow after completion.
- `on` enables event-driven scheduling, triggering workflow execution based on specified events.

##### Event-driven scheduling

###### Input of event-driven scheduled workflows

In event-driven scheduled workflows, the input is structured as an array containing the events that trigger the execution of the workflow. This array serves as a vital resource, providing workflow authors access to all relevant data associated with each triggering event.

Authors can reference individual events within the array using syntax such as $workflow.input[index], where index indicates the event's position, starting from 0.

###### Distinguishing event-driven scheduling from start `listen` Tasks

While both `schedule.on` and a start listener task enable event-driven execution of workflows, they serve distinct purposes:

- **`schedule.on`**: Manages the creation of new workflow instances based on external events. Faults or timeouts are typically invisible to the user.
- **Start `listen` task**: Operates within an already instantiated workflow. Failures can directly impact the workflow instance.

### Task Flow

A workflow begins with the first task defined. Once the task has been executed:

- `continue`: the next task should be executed (implicitly next in declaration order, or explicitly via `then`)
- `fault`: uncaught error halts execution, transitions to `faulted` status
- `end`: explicitly and gracefully ends the workflow's execution

### Data Flow

In Serverless Workflow DSL, data flow management is crucial to ensure that the right data is passed between tasks and to the workflow itself.

Data flows through a workflow based on these transformation stages:

1. **Validate Workflow Input** — against `input.schema`
2. **Transform Workflow Input** — via `input.from` expression
3. **Validate Task Input** — against task's `input.schema`
4. **Transform Task Input** — via task's `input.from` expression
5. **Transform Task Output** — via `output.as` expression
6. **Validate Task Output** — against `output.schema`
7. **Update Workflow Context** — via `export.as` expression
8. **Validate Exported Context** — against `export.schema`
9. **Continue Workflow** — pass transformed output as raw input to next task
10. **Transform Workflow Output** — via workflow's `output.as` expression
11. **Validate Workflow Output** — against workflow's `output.schema`

### Runtime Expressions

Runtime expressions serve as dynamic elements that enable flexible and adaptable workflow behaviors. They allow for variables, functions, and operators to create logic that responds to changing conditions and input data.

All runtimes **must** support the default runtime expression language: **jq**.

#### Runtime expression arguments

| Name | Type | Description |
|:-----|:----:|:------------|
| context | `map` | The workflow's context data. |
| input | `any` | The task's transformed input. |
| output | `any` | The task's transformed output. |
| secrets | `map` | A key/value map of the workflow secrets (only in `input.from`). |
| authorization | `authorizationDescriptor` | Describes the resolved authorization. |
| task | `taskDescriptor` | Describes the current task. |
| workflow | `workflowDescriptor` | Describes the current workflow. |
| runtime | `runtimeDescriptor` | Describes the runtime. |

##### Runtime Descriptor

| Name | Type | Description |
|:-----|:----:|:------------|
| name | `string` | A human friendly name for the runtime. |
| version | `string` | The version of the runtime. |
| metadata | `map` | Implementation specific key-value pairs. |

##### Workflow Descriptor

| Name | Type | Description |
|:-----|:----:|:------------|
| id | `string` | A unique id of the workflow execution. |
| definition | `map` | The workflow's definition as a parsed object. |
| input | `any` | The workflow's raw input (before `input.from`). |
| startedAt | `dateTimeDescriptor` | The start time of the execution. |

##### Task Descriptor

| Name | Type | Description |
|:-----|:----:|:------------|
| name | `string` | The task's name. |
| reference | `string` | The task's reference (e.g. `/do/2/myTask`). |
| definition | `map` | The task's definition as a parsed object. |
| input | `any` | The task's raw input (before `input.from`). |
| output | `any` | The task's raw output (before `output.as`). |
| startedAt | `dateTimeDescriptor` | The start time of the task. |

### Fault Tolerance

The DSL provides robust mechanisms to identify, describe, and handle errors effectively.

#### Errors

Errors are described using the Problem Details RFC (RFC 7807):

```yaml
type: https://serverlessworkflow.io/spec/1.0.0/errors/communication
title: Service Unavailable
status: 503
detail: The service is currently unavailable. Please try again later.
instance: /do/getPetById
```

#### Retries

```yaml
try:
  call: http
  with:
    method: get
    endpoint:
      uri: https://example-service.com/healthz
catch:
  errors:
    with:
      status: 503
  retry:
    delay:
      seconds: 3
    backoff:
      linear: {}
    limit:
      attempt:
        count: 5
```

### Timeouts

Workflows and tasks can be configured to timeout after a defined amount of time. A timeout error **must** have type `https://serverlessworkflow.io/spec/1.0.0/errors/timeout` and status `408`.

### Catalogs

A **resource catalog** is an external collection of reusable components (functions, etc.) that can be referenced and imported into workflows.

Each catalog is defined by an `endpoint` property specifying the root URL where resources are hosted.

#### File Structure

Catalogs must follow a specific file structure for portability and standardization.

#### Default Catalog

Runtimes may optionally define a "default" catalog used implicitly by all workflows:

```
{functionName}:{functionVersion}@default
```

#### Using Cataloged Functions

Format: `{functionName}:{functionVersion}@{catalogName}`

```yaml
document:
  dsl: '1.0.3'
  namespace: test
  name: catalog-example
  version: '0.1.0'
use:
  catalogs:
    global:
      endpoint:
        uri: https://github.com/serverlessworkflow/catalog
do:
  - log:
      call: log:0.5.2@global
      with:
        message: The cataloged custom function has been successfully called
```

### Interoperability

#### Supported Protocols

- **HTTP**: Standard HTTP requests to web services
- **gRPC**: Remote Procedure Call using gRPC
- **AsyncAPI**: Asynchronous messaging protocols
- **OpenAPI**: Services with OpenAPI specifications
- **A2A**: Interaction with A2A servers (agents)

#### Custom and Non-Standard Interactions

The DSL provides mechanisms for custom processes via shell commands, scripts, or containers.

##### Creating a Custom Function

1. Create directory: `/serverless-workflow/functions/my-custom-function/1.0.0`
2. Create `function.yaml` with input/output schema and task definition
3. Commit and push to repository
4. Optionally submit to Serverless Workflow Catalog

##### Using a Custom Function

```yaml
document:
  dsl: '1.0.3'
  namespace: default
  name: customFunctionWorkflow
  version: '0.1.0'
do:
  - validateEmail:
      call: https://github.com/myorg/functions/validateEmailAddress@v1.0.0
      with:
        emailAddress: ${ .userEmail }
```

### Events

Events facilitate communication and coordination between components and services, adhering to the CloudEvents specification.

#### Emitting Events

The Emit task allows workflows to publish events to event brokers or messaging systems.

#### Listening for Events

The Listen task provides a mechanism for workflows to await and react to external events.

### Extensions

Extensions offer a flexible way to extend the functionality of tasks within a workflow. They allow developers to inject custom logic, perform pre- or post-processing tasks, and modify task behavior dynamically.

Properties:

- **`extend`**: Specifies the type of task to extend.
- **`when`**: Conditionally applies the extension based on runtime expressions.
- **`before`**: Executes tasks before the extended task.
- **`after`**: Executes tasks after the extended task completes.

*Sample logging extension:*
```yaml
document:
  dsl: '1.0.3'
  namespace: test
  name: sample-workflow
  version: '0.1.0'
use:
  extensions:
    - logging:
        extend: all
        before:
          - sendLog:
              call: http
              with:
                method: post
                endpoint: https://fake.log.collector.com
                body:
                  message: "${ \"Executing task '\($task.reference)'...\" }"
        after:
          - sendLog:
              call: http
              with:
                method: post
                endpoint: https://fake.log.collector.com
                body:
                  message: "${ \"Executed task '\($task.reference)'...\" }"
do:
  - sampleTask:
      call: http
      with:
        method: get
        uri: https://fake.com/sample
```

### External Resources

External resources allow you to define and access external assets or services required for workflow execution. Each resource is identified by a unique name and associated with a URI. Optionally, you can specify an authentication policy.

### Authentication

Authentication specifies the method by which the workflow accesses protected resources or services.

Supported mechanisms:

- **Basic Authentication**: Username-password pair
```yaml
sampleBasic:
  basic:
    username: admin
    password: 123
```

- **Bearer Authentication**: Bearer token
```yaml
sampleBearer:
  bearer: ${ .user.token }
```

- **OAuth2 Authentication**: OAuth2 authorization framework
```yaml
sampleOAuth2:
  oauth2:
    authority: http://keycloak/realms/fake-authority/.well-known/openid-configuration
    grant: client-credentials
    client:
      id: workflow-runtime
      secret: workflow-runtime-client-secret
    scopes: [ api ]
    audiences: [ runtime ]
```
