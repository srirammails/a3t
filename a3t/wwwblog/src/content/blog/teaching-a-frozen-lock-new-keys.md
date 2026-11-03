---
title: Teaching a Frozen Lock New Keys
description: How to make your LLM know about enterprise data without breaking the cache (or the bank).
date: 2026-11-01
author: Sriram
tags: [kblam, kv-cache, memory]
image: /covers/frozen-lock-new-keys.png
imageAlt: One padlock labelled frozen weights with five keyholes, and five keys (facts, relations, documents, events, context) turning in it at once.
pinned: true
---

## The problem: an LLM is a frozen lock

An LLM stops learning the day training ends. Every fact it knows was pressed into its weights before release, and nothing it reads afterwards changes them.

I come from a different world. I have spent years with online learning models of the [River](https://riverml.xyz/) kind. They update with every event: one sample in, one small weight change, and the model drifts along with the data. For decisions over data that never stops changing (fraud, pricing, bidding, capacity) that is exactly what you want. The model is a river, not a photograph.

Enterprise data is a river too. Tickets, contracts, prices, org charts and incidents change every hour. So the question I kept coming back to was simple: how do I give a frozen model river-like awareness of my enterprise data, without retraining it and without the serving bill exploding?

This post is my notes from that quest.

## An analogy: keys, locks and grooves

Think of training as pressing keys into a soft lock. Every piece of text is a key. Each time a key is pressed in, the lock's grooves shift slightly so that key turns more easily next time. That pressing is backpropagation: the error from a wrong turn flows back and reshapes the grooves.

The hard part is that one lock must accept every key at once. Trillions of tokens, all pressed into the same set of grooves, until the lock fits all of them well enough. Then the lock is cast in metal. Those are the frozen weights.

Attention heads are what make this lock interesting. They are like combination locks inside the main lock. The grooves themselves are fixed, but which grooves are engaged changes with the dials, and the dials are set by the input. Feed in different text and the same frozen lock behaves differently.

That is the opening. If I cannot recut the grooves, I can still change what the dials see. Every technique below is a different way of setting those dials with enterprise data, at a different cost.

## Step 1: put everything in the prompt

The first move is the obvious one: put the data in the prompt. Prompt engineering is just setting the dials by hand.

Then we got more systematic. We kept embedding vectors of our documents in an external store, and at query time we pulled the nearest ones and appended their text to the prompt. That is RAG, and it works surprisingly well.

It also hits a wall quickly:

- **Context size.** Only so much fits, and enterprise corpora are far larger than any window.
- **Cost.** Every request re-reads the same documents from scratch. Prefill compute grows with every token you append, and attention grows faster than that.
- **Quality.** Models lose track of facts buried in the middle of long contexts.

We were paying, on every single request, to re-teach the lock the same keys it had seen five minutes earlier.

## Step 2: stop re-reading, start caching

When a model reads text, each layer turns every token into a key vector and a value vector. That KV cache is the model's working memory of the context. It is the dial setting, already computed.

So the smarter move was to keep it:

- **Paged KV cache** (vLLM) manages that memory in fixed-size blocks, like OS virtual memory pages, so many requests can share GPU memory without fragmentation.
- **Prefix caching** (vLLM, SGLang's RadixAttention) reuses the cache when a new prompt starts with exactly the same text as an earlier one.
- **[LMCache](https://github.com/LMCache/LMCache)** pushes that cache out of GPU memory into CPU, disk and remote storage, and shares it across engine instances.

This changed the economics. A system prompt or a common document is read once and reused thousands of times.

But prefix caching has a catch: it only works for an exact prefix. In RAG, the same document appears after different system prompts, chat histories and other documents. Change one token before it and the cache is useless. The cache was full of the right knowledge, and we still could not use it.

## Step 3: inject knowledge straight into K and V

The next idea: if a document's KV cache *is* the knowledge, precompute it once per document and splice it into any prompt, wherever the document lands. Two problems appear immediately.

**Positions are wrong.** Each chunk was encoded as if it started at position 0. With RoPE (rotary position embeddings) this is fixable, because position is a rotation applied to each key. Moving a chunk from position 0 to position 4,000 means rotating its cached keys by the difference. Values need nothing. Chunk, cache, rotate into place: cheap and exact.

**Cross-attention is missing.** This is the harder one. Chunks encoded separately never attended to each other, so a fact in document B that depends on document A is lost. Rotation does not fix that. Three families of answers have emerged:

- **Recompute a little.** [CacheBlend](https://arxiv.org/abs/2405.16444) splices cached chunks and recomputes only the small set of tokens whose KV drifts most from a full prefill. It now ships inside LMCache. Newer work (EPIC, Cache-Craft, CacheTune, QCFuse, SAFE-KV) refines which tokens to recompute.
- **Learn the repair.** KVLink adds trainable link tokens between chunks. [CacheRepair](https://huggingface.co/gwang3456/cacherepair) trains a small side-network that predicts the correction to each chunk's cache.
- **Train the model to not need it.** Block-Attention and TurboRAG fine-tune the model so independently encoded chunks just work.

[KBLaM](https://arxiv.org/abs/2410.10450) goes a step further. Instead of caching raw text, it turns each fact (a subject, relation, object triple) into one *knowledge token*: a key-value pair produced by a sentence encoder and small trained adapters. These are added to every attention layer through *rectangular attention*. Prompt tokens attend to knowledge tokens, but knowledge tokens never attend to each other. Cost grows linearly with the size of the knowledge base, and you can add or remove a fact without retraining. Only the adapters are trained.

In lock terms, KBLaM adds a new bank of dials wired straight into every combination lock, one dial per fact. [AtlasKV](https://arxiv.org/abs/2510.17934) (ICLR 2026) then puts a hierarchical index in front of that bank and reports a billion triples in under 20 GB of VRAM.

## Step 4: change the lock itself

Everything so far works around one fact: a transformer's KV cache grows with every token it reads. Some architectures attack that directly.

**Hyena** replaces attention with long convolutions and data-dependent gating. The cost grows close to linearly with sequence length instead of quadratically, which makes very long inputs affordable.

**Mamba** is a selective state space model. Instead of a cache that grows with every token, it carries a fixed-size state and updates it token by token. The selection mechanism decides what to keep and what to forget based on the input. Mamba-2 reframes this as *structured state space duality*, which makes it much faster to train and run on GPUs.

This is the part that excites me, because a Mamba state is the closest thing an LLM has to a River model. It is a running summary updated with each new event. The weights stay frozen, but the state flows with the data.

The trade-off is real. A fixed-size state is lossy, so pure state space models are weaker at exact recall of a specific fact from far back. That is why the practical designs are hybrids: mostly Mamba layers for cheap long context, plus a few attention layers for precise lookup. Jamba, Nemotron-H and IBM Granite 4 all take this route.

## My bet: a mixture, not a single trick

No single technique covers all enterprise knowledge, because enterprise knowledge is not one shape. Some of it is facts, some is relationships, some is long documents, and some is a stream of events. So I am betting on a mixture, with each piece doing the job it is best at.

| Component | Knowledge shape | Role | When data changes |
|-----------|-----------------|------|-------------------|
| KBLaM knowledge tokens | Discrete facts (owner, price, status) | Fact bank wired into every attention layer | Re-encode the changed fact; no retraining |
| RotatE KG embeddings | Relationships between entities | Relation-aware keys, so multi-hop structure shows up in the vectors | Update the affected entities' embeddings |
| DyPRAG | Long documents (contracts, policies, runbooks) | Hypernetwork turns a document into a LoRA in one pass | Regenerate that document's adapter |
| Mamba-2 layers | Event streams and long history | Fixed-size running state for long context | State updates as events arrive |

How they fit together:

- **RotatE feeds KBLaM.** [RotatE](https://arxiv.org/abs/1902.10197) models each relation as a rotation in complex space, so composition and inversion of relations are built in. Using those embeddings to build KBLaM's knowledge keys should give the fact bank a sense of structure, not just a flat list of triples.
- **DyPRAG handles what is not a triple.** Long prose does not compress well into facts. A [DyPRAG](https://arxiv.org/abs/2503.23895) adapter carries it in the parameters for the duration of the request.
- **Mamba-2 is the river.** A hybrid backbone keeps long, evolving context cheap, while a few attention layers stay precise enough to read the KBLaM fact bank.

The principle is the same as with River models: keep the expensive learned part fixed, and make the part that tracks the data cheap to update.

## What is still unsolved

This is a bet, not a result. The open problems are real:

- **Composition.** Every family hits the same wall: artifacts produced independently (chunk caches, trained caches, adapters) interfere when combined. [Cartridges at Scale](https://arxiv.org/abs/2606.04557) found that independently trained KV caches collapse to near-chance accuracy when loaded together, unless they are trained alongside distractors. My mixture will need the same care.
- **Mixing LoRA with knowledge tokens.** I have not seen KBLaM-style fact banks and per-document adapters evaluated together. That interaction is where I expect surprises.
- **Security.** Shared, fused caches leak. [SpliceLeak](https://arxiv.org/abs/2606.21842) shows a side-channel attack on non-prefix KV fusion in multi-tenant serving. Tenant isolation has to be part of the design.
- **Serving.** LMCache understands raw KV. Nothing yet treats chunk caches, knowledge tokens, adapters and recurrent state as one schedulable, cacheable object.

That last gap is the one I find most interesting. The models already have the dials. What is missing is the system that sets them, keeps them current, and does it without breaking the cache, or the bank.

## Sources

- [CacheBlend: Fast Large Language Model Serving for RAG with Cached Knowledge Fusion](https://arxiv.org/abs/2405.16444)
- [LMCache](https://github.com/LMCache/LMCache)
- [KBLaM: Knowledge Base augmented Language Model](https://arxiv.org/abs/2410.10450)
- [AtlasKV: Augmenting LLMs with Billion-Scale Knowledge Graphs in 20GB VRAM](https://arxiv.org/abs/2510.17934)
- [CacheRepair: Learning to Repair Cross-Chunk Context in RAG for KV Cache Fusion](https://huggingface.co/gwang3456/cacherepair)
- [DyPRAG: Dynamic Parametric Retrieval Augmented Generation for Test-time Knowledge Enhancement](https://arxiv.org/abs/2503.23895)
- [RotatE: Knowledge Graph Embedding by Relational Rotation in Complex Space](https://arxiv.org/abs/1902.10197)
- [Cartridges at Scale: Training Modular KV Caches over Large Document Collections](https://arxiv.org/abs/2606.04557)
- [SpliceLeak: Agent-Assisted Side-Channel Attacks on Non-Prefix KV Cache in RAG](https://arxiv.org/abs/2606.21842)
