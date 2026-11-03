---
title: We Built the Greatest Knowledge Machine in History. We Can't Read It.
description: LLMs encode more information per token than any structure humans have ever designed. They also waste most of it, because we don't yet understand what we put inside them.
date: 2026-05-10
author: Sriram
tags: [knowledge-graph, llm]
image: /covers/knowledge-machine.png
imageAlt: Cover image for "We Built the Greatest Knowledge Machine in History. We Can't Read It."
---

### First Principles

---

## Knowledge is Not Data. Knowledge is in Relations.

We use the words interchangeably, but they mean different things. Data is a fact in isolation. A phone number. A temperature. A name. Data is inert until something connects it to something else.

Knowledge is what happens at the connection.

*Paris* is a word. *France* is a word. The knowledge lives in the edge between them: *Paris is the capital of France.* Remove the relation, and you have two nouns. Add the relation, and you have a fact that implies hundreds of other facts - geography, history, governance, culture.

This is not a new idea. ConceptNet - a large-scale semantic network built at MIT - encodes human knowledge exactly this way. Every entry is a triple:

![](/images/knowledge-machine/01.png)

The formula is simple:

> *Knowledge = Concept + Relation*

What this means structurally is that knowledge, at its core, is a **graph**. Nodes are concepts and entities. Edges are relations. The richness of what you know is determined not by how many nodes you have, but by how densely and accurately they are connected.

---

### Representation

---

### From a Normalised Schema to a Graph to a Matrix

If you were building a database to store human knowledge, you would design it as a normalised relational schema. Three tables. Clean. Explicit.

![](/images/knowledge-machine/02.png)

*Every fact in human knowledge can be expressed as a row in relation_instance. Paris IsA Capital → one row. Three foreign keys. One weight.*

This schema is expressive, but it has a shape - and that shape is a **graph**. Each entity is a node. Each row in the relation table is a directed, typed edge. Draw it out and you get something like this:

![](/images/knowledge-machine/03.png)

*A tiny fragment of the knowledge graph around "Paris". Each node is a concept. Each edge is a named relation.*

Now here is where mathematics enters cleanly. Any graph can be represented as a **matrix**. You list all nodes along both axes. A cell gets a value if an edge exists between those two nodes - the type of relation, its direction, its weight.

![](/images/knowledge-machine/04.png)

*Mostly empty. Knowledge graphs are sparse — most concepts are not directly related. That sparsity is meaningful.*

Each row in this matrix is a **vector** - a list of numbers representing one concept's relationships to all others. This is the bridge between symbolic knowledge representation and the linear algebra that machines compute over.

Schema → Graph → Matrix → Vectors. Each transformation preserves the information. None of these are approximations. They are equivalent representations of the same underlying structure.

> *Knowledge has always been a vector space. We just didn't call it that.*

---

### The LLM

---

### An Enormous Token Table With Unknown Everything

A Large Language Model, at its core, is also a lookup table. But it is a lookup table that has lost its column names.

GPT-3 has a vocabulary of roughly 50,000 tokens - subword units that cover essentially all of English and most common patterns in other languages. Each token is assigned a vector. In GPT-3, each of those vectors has **12,288 dimensions** - each dimension stored as a 32-bit float.

![](/images/knowledge-machine/05.png)

*The knowledge schema has named, interpretable columns. The LLM token table has 12,288 unnamed dimensions. The dimensions encode entities, concepts, and relations — we just don't know which dimension encodes what.*

Each token in this table is a point in a 12,288-dimensional space. Tokens that are semantically related end up close together in that space. The word *king* minus the vector for *man* plus the vector for *woman* famously yields something very close to *queen.* The relation "gender" exists as a direction in the vector space - even though nobody ever labelled it.

![](/images/knowledge-machine/06.png)

*Paris and France have similar vectors. The "IsCapitalOf" relation exists somewhere in the delta between them, but there is no column named "IsCapitalOf". It is entangled across all 12,288 dimensions simultaneously.*

This is the key insight: the knowledge is in there. The relations are in there. But they are not stored as clean triples in a relation table. They are **entangled** across thousands of dimensions, spread across millions of weight matrices, distributed through 96 layers of transformer attention.

The entities are unknown. The relation types are unknown. The concepts are unknown. Everything is floating in a high-dimensional soup that happens to behave correctly when you ask it questions.

---

### Training

---

### Backpropagation: Mapping the Unknown by Approximation

So how does the model learn which values go in those 12,288 dimensions for each token? Not by being told. By being shown billions of sentences and asked to predict the next word, and then being corrected when it gets it wrong.

This process has two interleaved mechanisms:

![](/images/knowledge-machine/07.png)

Here is the critical detail: this process trains on **batches** - thousands of sentences at once, not the full corpus simultaneously. This is not merely a hardware compromise. It is architecturally essential.

Training on all data simultaneously would cause gradients to cancel out - sentence A says push weight X up, sentence B says push weight X down, they average to zero, the model stops learning. Batches introduce noise. And the noise is useful: it prevents the model from getting stuck in shallow valleys and helps it find more generalisable solutions. This was discovered somewhat by accident. It is now called Stochastic Gradient Descent - "stochastic" meaning deliberately random.

The loss landscape this process navigates is not a clean bowl with a single lowest point. It is a billion-dimensional terrain with:

![](/images/knowledge-machine/08.png)

The final trained model lands wherever the stochastic stumble through this terrain happened to stop. Nobody can tell you whether it reached the true global minimum. We measure it on benchmarks and accept it as sufficient.

The weights of a trained model are not a clean, optimal encoding of knowledge. They are a **geological accident** - the residue of a noisy optimisation process that solved the problem well enough, not perfectly.

> *Backpropagation doesn't find truth. It finds a neighbourhood of truth that was reachable from a random starting point, via a noisy downhill walk.*

There is also a subtler problem lurking here. When the model trains on a new batch, the weights shift to fit that batch - potentially degrading what previous batches taught. There is no hard guarantee that learning sentence 2 preserves the knowledge from sentence 1. This is called **catastrophic forgetting**, and it remains an open research problem. Batching and repeated shuffled epochs mitigate it - but they do not solve it. The weights are a *negotiated average* across all the training data, not a faithful encoding of each individual fact.

---

### The Waste Problem

---

### A Theoretical Capacity We Cannot Fill, Cannot Read, and Cannot Compress

Now let us talk about the numbers. Because they are staggering.

![](/images/knowledge-machine/09.png)

### Why Float32?

Float32 was not chosen for inference. It was chosen for **training**. During gradient descent, weight updates are vanishingly small - on the order of 0.000001 per step, scaled by the learning rate. If your numerical precision is too coarse, that update disappears - rounded away to nothing. The model stops learning. Float32 provides approximately 7 significant decimal digits of precision, which is enough to preserve gradient signals through billions of update steps.

But once training is complete, the weights are frozen. No more gradient descent. No more tiny updates. The model is doing inference - essentially a series of large matrix multiplications. **The reason for Float32 is gone.**

And yet most models at inference time still load Float32 weights. Every forward pass carries 32 bits of precision for values that only needed that precision during training. It is like building a rocket to get to your office, then keeping the rocket fuelled and idling in the car park because you might theoretically need it.

### Why We Can't Just Compress It

The obvious question is: why not quantise the weights after training? Reduce Float32 to Int8 or Int4 — cut memory by 4x or 8x, reduce compute proportionally.

We do this. It works surprisingly well. GPTQ, AWQ, bitsandbytes - these are all post-training quantisation methods that produce smaller, faster models with acceptable degradation on standard benchmarks.

But acceptable is not lossless. Every quantisation scheme in production today is an approximation. They minimise measurable degradation. They do not eliminate it. And the reason they cannot is deep:

![](/images/knowledge-machine/10.png)

The inability to do lossless quantisation and the inability to extract a clean knowledge graph are **the same problem wearing different clothes.**

If you could map the latent space - if you knew which dimensions encoded which concepts, which directions encoded which relations - you could represent the model's knowledge as a sparse, structured graph. You could then compress that graph efficiently. You'd know exactly what you were storing, what was redundant, and what mattered.

But you can't. So you carry 32 bits of precision for values whose meaning you don't understand, hoping none of it is waste.

> *We can't compress the model losslessly for exactly the same reason we can't read its knowledge back out: the relational graph is entangled across 12,288 dimensions, and we have no map.*

---

### What Comes Next

---

The dominant narrative in AI right now is scale. More parameters. More data. More compute. Bigger models trained on larger clusters consuming more energy.

But scale is a workaround for a deeper unsolved problem. We are scaling because we don't know how to be efficient. We are running massive Float32 forward passes because we don't know how to compress. We are using trillion-parameter models because we don't know how to extract and reuse structured knowledge.

The next genuine leap is not in scale. It is in **decomposition** - understanding what is actually inside these models well enough to:

![](/images/knowledge-machine/11.png)

We started with a simple formula: *Knowledge = Concept + Relation.* A formula that fits neatly in a normalised database schema. A graph. A matrix. A vector space.

LLMs learned to traverse that vector space with superhuman fluency. But they learned it blindly — through a noisy, stochastic stumble down a landscape nobody could see, arriving at a solution nobody can fully read.

We built the most powerful knowledge compression machine in history.

We just haven't built the decompressor yet.

That is the gap. And closing it - not building bigger models, but building tools to understand the ones we have - is where the next decade of real progress lives.

---

*Originally published on [LinkedIn](https://www.linkedin.com/pulse/we-built-greatest-knowledge-machine-history-cant-read-sriram-reddy-18cee/) on 10 May 2026.*
