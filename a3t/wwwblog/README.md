# A3T Blog (blog.a3t.io)

Static Astro site styled like a3t.io (`../www`). Home page sections, top to bottom:

1. **Pinned**: the hero. The newest post with `pinned: true`, else the newest post.
2. **Recent**: every other post, 3 / 2 / 1 columns.
3. **Library**: papers, posts, videos and podcasts we link to, with commentary. Full list at `/library/`.
4. **Links**: one line each (title, note, URL), from `src/content/links.yaml`.

Every post ends with a **Book a demo** + **Get started in the docs** call to action.

```bash
npm install
npm run dev        # http://localhost:4321, drafts included
./deploy-bunny.sh --build   # production build into dist/
./deploy-bunny.sh  # build + upload to Bunny (needs .env.bunny)
```

Uses Astro 5 because this pod has Node 20 (Astro 6 needs Node 22.12+).

## Writing a post: `src/content/blog/<slug>.md` (or `.mdx`)

The file name is the URL: `my-post.md` → `blog.a3t.io/my-post/`.

```yaml
---
title: Why every agent action needs a policy decision
description: One or two sentences. Also the LinkedIn and Google snippet (≤ 200 chars).
date: 2026-11-03
author: A3T Team          # optional
tags: [policy, security]  # first tag picks the cover colour (see TAG_TONES in src/consts.ts)
image: /covers/policy.png # optional, in public/; 1200×627 makes a good LinkedIn card
imageAlt: ...
pinned: true              # optional, makes it the hero
draft: true               # optional, hidden from the build
---
```

**Video or audio post**: add `media`. The player replaces the cover; the body holds notes or a transcript.

```yaml
media:
  type: video                                  # or audio
  src: https://www.youtube.com/watch?v=<id>    # YouTube, Vimeo, an embed URL (Spotify, podcast host),
                                               # or a file: /media/tour.mp4, /media/ep1.mp3, a Bunny CDN URL
  poster: /covers/tour.png                     # optional, for video files
  duration: 12 min                             # optional, shown on cards
```

YouTube loads only a thumbnail until the reader clicks play (via youtube-nocookie). Large media
files are better on Bunny Stream or a CDN than in `public/`.

**In `.mdx` posts** you can also use:

```mdx
<Video src="https://youtu.be/<id>" title="Demo" />
<Audio src="/media/clip.mp3" title="Clip" />
<TryIt href="https://docs.a3t.io/concepts/memory/">Memory concepts in the docs</TryIt>
<BookDemo inline title="Want to see this on your data?" />
```

## Library entry: `src/content/library/<slug>.md`

The Markdown body is your commentary.

```yaml
---
title: "KBLaM: Knowledge Base augmented Language Model"
url: https://arxiv.org/abs/2410.10450
kind: paper            # paper | post | video | podcast | repo | book
source: arXiv · ICLR 2025
authors: Isazawa et al.
year: 2024
added: 2026-10-29      # sort order (newest first)
tags: [knowledge-graph]
related: my-kblam-post # optional, shows "Our take →" linking to that post
media:                 # optional, plays a talk or episode inline
  type: video
  src: https://www.youtube.com/watch?v=<id>
---
Why it matters to us, in a paragraph or two.
```

## Links: `src/content/links.yaml`

```yaml
- id: quickstart
  title: Quick start
  url: https://docs.a3t.io/getting-started/quickstart/
  note: Run A3T and your first governed agent.
```

## Before the first deploy

Delete or replace the `sample-*` files in `src/content/blog` and `src/content/library`.
`deploy-bunny.sh` refuses to upload while any of them is published (not `draft: true`).

## Local preview on the code-server pod

```bash
BLOG_BASE=/absproxy/4330 npm run dev -- --port 4330 --host 0.0.0.0
```

Open https://coder.dev.pubcontext.com/absproxy/4330/ (drafts included). Use `absproxy`, not `proxy`:
`/proxy/` strips the path prefix, which breaks the site's links.

## Moving to a3t.io/blog

Set `SITE_URL` in `src/consts.ts` to `https://a3t.io/blog`, build with `BLOG_BASE=/blog`, and deploy
with `BUNNY_DEST=blog` to the a3t.io zone. Internal links go through `url()` in `src/lib/url.ts`, so
they follow the base.
