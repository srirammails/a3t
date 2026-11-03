---
# SAMPLE POST: replace or delete before the first deploy.
title: A workflow runtime built for agents
description: Why A3T runs agents as CNCF Serverless Workflows on a Rust runtime, and what that buys you in latency, cost and auditability.
date: 2026-10-17
tags: [runtime, platform]
---

An agent is a workflow whose next step is chosen at run time. A3T runs them on a Rust runtime
that implements the CNCF Serverless Workflow specification.

## Why a workflow format

- Steps, retries and timeouts are declared, not hidden in prompt glue.
- Each step is a unit that policy can check and memory can record.
- The same definition runs locally, on a single node and on a cluster.

## Why Rust

Predictable latency and a small footprint matter when an agent sits in a trading or bidding path.
