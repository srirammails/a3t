---
# SAMPLE POST: replace or delete before the first deploy (deploy-bunny.sh refuses to ship sample-* files).
title: Why every agent action needs a policy decision
description: Agents that act on real systems need the same authorization discipline as people. Here is how A3T puts a Cedar check in front of every step.
date: 2026-10-30
author: A3T Team
tags: [policy, security]
pinned: false
---

Most agent demos stop at "the model picked the right tool". In production the harder question
is whether the agent **was allowed** to call that tool, with those arguments, for that tenant,
right now.

## The gap

An LLM-driven agent decides what to do at run time. That makes the usual answer of reviewing the
code before it ships much weaker: the code is the same, the decisions are not.

> If you can't say who allowed an action, you can't put the agent in front of a CISO.

## What A3T does

- Every task in a workflow is checked against **Cedar** policy before it runs.
- The decision, the inputs and the policy version are recorded with the run.
- Denials are first-class results the workflow can handle, not exceptions.

```text
permit (
  principal in Role::"secops-agent",
  action == Action::"isolate_host",
  resource
) when { resource.severity >= 8 };
```

## Next

The [Architecture](https://docs.a3t.io/getting-started/architecture/) page shows where the policy
gate sits in the runtime.
