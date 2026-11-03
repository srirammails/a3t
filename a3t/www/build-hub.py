#!/usr/bin/env python3
"""Draw the platform illustration (isometric, 3D-style) into index.html.

AgentLM sits on a stack of enterprise memory and reads it directly; workflow steps feed
it; a policy gate stands at the tenant edge; a frontier LLM floats outside. Used by
build-how.py, which places it at the top of the How it works section and its CSS between
the HUB-CSS markers. Edit this file, not the SVG.
"""
S = 34                     # pixels per iso unit
CX, SY = 0.866 * S, 0.5 * S
OX, OY = 420, 170          # screen position of iso origin
W, H = 1000, 540


def P(x, y, z=0.0):
    return (OX + (x - y) * CX, OY + (x + y) * SY - z * S)


def pts(*ps):
    return " ".join(f"{px:.1f},{py:.1f}" for px, py in ps)


def box(x, y, z, w, d, h, c, extra=""):
    """Iso box. c = (top, left, right, stroke). Visible faces: top, y+d face, x+w face."""
    top, left, right, stroke = c
    t = [P(x, y, z + h), P(x + w, y, z + h), P(x + w, y + d, z + h), P(x, y + d, z + h)]
    l = [P(x, y + d, z), P(x + w, y + d, z), P(x + w, y + d, z + h), P(x, y + d, z + h)]
    r = [P(x + w, y, z), P(x + w, y + d, z), P(x + w, y + d, z + h), P(x + w, y, z + h)]
    st = f'stroke="{stroke}" stroke-width="0.9" stroke-linejoin="round"'
    return (f'<g{extra}><polygon points="{pts(*l)}" fill="{left}" {st}/>'
            f'<polygon points="{pts(*r)}" fill="{right}" {st}/>'
            f'<polygon points="{pts(*t)}" fill="{top}" {st}/></g>')


def top_matrix(x, y, z):
    """Transform that lays text flat on a top face, reading along +x."""
    ex, ey = P(x, y, z)
    return f"matrix(0.866 0.5 -0.866 0.5 {ex:.1f} {ey:.1f})"


def callout(x, y, title, sub, anchor="start", tx=None, ty=None, k="n"):
    """A dot on the object, a leader line, and a two-line label."""
    px, py = x, y
    lx, ly = (tx, ty)
    dx = 6 if anchor == "start" else -6
    return (f'<g class="iso-co {k}"><circle cx="{px:.1f}" cy="{py:.1f}" r="3.2"/>'
            f'<path d="M{px:.1f} {py:.1f} L{lx:.1f} {ly:.1f}"/>'
            f'<text x="{lx + dx:.1f}" y="{ly - 4:.1f}" text-anchor="{anchor}" class="t">{title}</text>'
            f'<text x="{lx + dx:.1f}" y="{ly + 13:.1f}" text-anchor="{anchor}" class="s">{sub}</text></g>')


SLAB = ("#fdfbf5", "#eeeae0", "#e2ddd0", "#d9d4c6")
MEM = [("#e7f5f0", "#bfe3d4", "#9fd3bd", "#5fb89a"),
       ("#dcf0e8", "#b2dcca", "#90cbb2", "#4fae8d"),
       ("#d1ebe1", "#a5d5c0", "#81c3a7", "#3fa481"),
       ("#c6e6da", "#98cfb7", "#73bb9d", "#2f9a76")]
CHIP = ("#2b2925", "#1d1c19", "#121110", "#3a3833")
STEP = ("#f3ecfa", "#dccbf2", "#c4a8e8", "#9a72cc")
GATE = ("#e6efff", "#c5d8fb", "#a6c2f6", "#5a86de")


def svg():
    o = [f'<svg class="iso-svg" viewBox="0 0 {W} {H}" role="img" aria-labelledby="hubCap" xmlns="http://www.w3.org/2000/svg">',
         '<defs>'
         '<filter id="isoSh" x="-30%" y="-30%" width="160%" height="180%"><feDropShadow dx="0" dy="10" stdDeviation="10" flood-color="#1e190f" flood-opacity=".18"/></filter>'
         '<linearGradient id="isoBeam" x1="0" y1="1" x2="0" y2="0"><stop offset="0" stop-color="#23987f" stop-opacity=".2"/><stop offset="1" stop-color="#7d4bbb" stop-opacity=".95"/></linearGradient>'
         '<radialGradient id="isoGlow"><stop offset="0" stop-color="#b28be0" stop-opacity=".5"/><stop offset="1" stop-color="#b28be0" stop-opacity="0"/></radialGradient>'
         '<marker id="isoArr" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="7" markerHeight="7" orient="auto"><path d="M0 0L10 5L0 10z" fill="#c47028"/></marker>'
         '<marker id="isoArrV" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="6" markerHeight="6" orient="auto"><path d="M0 0L10 5L0 10z" fill="#9a72cc"/></marker>'
         '</defs>']

    # Tenant slab with a faint grid; its name runs along the right-hand edge.
    SW, SD = 11, 8
    o.append(f'<g filter="url(#isoSh)">{box(0, 0, -0.55, SW, SD, 0.55, SLAB)}</g>')
    grid = []
    for i in range(1, SW):
        a, b = P(i, 0), P(i, SD)
        grid.append(f'M{a[0]:.1f} {a[1]:.1f} L{b[0]:.1f} {b[1]:.1f}')
    for j in range(1, SD):
        a, b = P(0, j), P(SW, j)
        grid.append(f'M{a[0]:.1f} {a[1]:.1f} L{b[0]:.1f} {b[1]:.1f}')
    o.append(f'<path d="{" ".join(grid)}" class="iso-grid"/>')
    tx, ty = P(SW, SD - 0.4, -0.12)
    o.append(f'<text transform="matrix(0.866 -0.5 0 1 {tx:.1f} {ty:.1f})" class="iso-tag">YOUR TENANT</text>')

    # Enterprise memory: four named layers.
    mx, my, mw, md = 3.5, 2.0, 4.0, 4.0
    names = ["Sessions", "Knowledge graph", "Procedures", "Past cases"]
    lh, gap = 0.42, 0.62
    for i, c in enumerate(MEM):
        z = i * gap
        o.append(box(mx, my, z, mw, md, lh, c))
        lx, ly = P(mx + 0.25, my + md, z + lh)
        o.append(f'<text transform="matrix(0.866 0.5 0 1 {lx:.1f} {ly:.1f})" y="10.5" class="iso-layer">{names[i]}</text>')
    mem_top = 3 * gap + lh

    # Attention beams from memory up into AgentLM.
    chip_z = mem_top + 1.1
    cx0, cy0, cw, cd = 4.0, 2.5, 3.0, 3.0
    gxy = P(cx0 + cw / 2, cy0 + cd / 2, mem_top)
    o.append(f'<ellipse cx="{gxy[0]:.1f}" cy="{gxy[1]:.1f}" rx="120" ry="52" fill="url(#isoGlow)"/>')
    beams = [(cx0 + 0.4, cy0 + cd), (cx0 + 1.2, cy0 + cd), (cx0 + 2.0, cy0 + cd), (cx0 + 2.8, cy0 + cd),
             (cx0 + cw, cy0 + 2.2), (cx0 + cw, cy0 + 1.4), (cx0 + cw, cy0 + 0.6)]
    for bx, by in beams:
        p1, p2 = P(bx, by, mem_top), P(bx, by, chip_z)
        o.append(f'<path d="M{p1[0]:.1f} {p1[1]:.1f} L{p2[0]:.1f} {p2[1]:.1f}" class="iso-beam"/>')

    # AgentLM chip, its name laid on the top face reading up and to the right.
    o.append(f'<g filter="url(#isoSh)">{box(cx0, cy0, chip_z, cw, cd, 0.7, CHIP)}</g>')
    zt = chip_z + 0.7
    edge = [P(cx0 + .15, cy0 + .15, zt), P(cx0 + cw - .15, cy0 + .15, zt), P(cx0 + cw - .15, cy0 + cd - .15, zt), P(cx0 + .15, cy0 + cd - .15, zt)]
    o.append(f'<polygon points="{pts(*edge)}" class="iso-chip-edge"/>')
    ex, ey = P(cx0 + 1.25, cy0 + cd - 0.3, zt)
    o.append(f'<text transform="matrix(0.866 -0.5 0.866 0.5 {ex:.1f} {ey:.1f})" class="iso-chip-t">AgentLM</text>')
    bl = P(cx0 + 1.5, cy0 + cd, (mem_top + chip_z) / 2)
    o.append(f'<g class="iso-pill"><rect x="{bl[0] - 34:.1f}" y="{bl[1] - 10:.1f}" width="68" height="20" rx="10"/><text x="{bl[0]:.1f}" y="{bl[1] + 4:.1f}">attends</text></g>')

    # Workflow steps on a track, front-left, flowing towards memory.
    a, b = P(0.2, 7.25, 0.01), P(4.3, 7.25, 0.01)
    o.append(f'<path d="M{a[0]:.1f} {a[1]:.1f} L{b[0]:.1f} {b[1]:.1f}" class="iso-track" marker-end="url(#isoArrV)"/>')
    for x in (0.4, 1.6, 2.8):
        o.append(box(x, 6.8, 0, 0.9, 0.9, 0.7, STEP))

    # Policy gate at the right edge of the tenant.
    gx0, gy0, gd, gh = 10.2, 2.0, 3.2, 2.6
    o.append(box(gx0, gy0, 0, 0.35, gd, gh, GATE))
    sx, sy = P(gx0 + 0.35, gy0 + gd / 2, gh / 2 + 0.15)
    o.append(f'<g transform="matrix(0.866 -0.5 0 1 {sx:.1f} {sy:.1f})"><path d="M0 -15 L12 -10 V-2 C12 6 6 11 0 14 C-6 11 -12 6 -12 -2 V-10 Z" class="iso-shield"/><path d="M-5 -1 L-1 3 L6 -5" class="iso-shield-tick"/></g>')

    # Frontier LLM: a cloud outside the tenant.
    fx, fy = 880, 150
    o.append(f'<g filter="url(#isoSh)"><path d="M{fx - 70} {fy + 26} C{fx - 96} {fy + 26} {fx - 96} {fy - 12} {fx - 66} {fy - 12} C{fx - 62} {fy - 42} {fx - 22} {fy - 48} {fx - 6} {fy - 26} C{fx + 10} {fy - 50} {fx + 56} {fy - 40} {fx + 54} {fy - 10} C{fx + 84} {fy - 8} {fx + 82} {fy + 26} {fx + 56} {fy + 26} Z" class="iso-cloud"/></g>')
    o.append(f'<text x="{fx - 6}" y="{fy + 6}" class="iso-cloud-t">Frontier LLM</text>')
    o.append(f'<text x="{fx - 6}" y="{fy + 52}" class="iso-cloud-s">only what AgentLM</text>')
    o.append(f'<text x="{fx - 6}" y="{fy + 67}" class="iso-cloud-s">can’t solve</text>')

    # Route: AgentLM → policy gate → cloud.
    r0 = P(cx0 + cw, cy0 + 1.2, chip_z + 0.5)
    r1 = P(gx0 + 0.2, gy0 + 1.0, gh)
    o.append(f'<path d="M{r0[0]:.1f} {r0[1]:.1f} Q{(r0[0] + r1[0]) / 2:.1f} {min(r0[1], r1[1]) - 80:.1f} {r1[0]:.1f} {r1[1]:.1f}" class="iso-route"/>')
    o.append(f'<path d="M{r1[0]:.1f} {r1[1]:.1f} Q{r1[0] + 30:.1f} {r1[1] - 90:.1f} {fx - 92:.1f} {fy + 14:.1f}" class="iso-route" marker-end="url(#isoArr)"/>')

    # Callouts.
    o.append(callout(*P(cx0 + 0.3, cy0 + 0.3, zt), "AgentLM", "reads your memory, then solves or routes", "end", 330, 96, "lm"))
    o.append(callout(*P(mx, my + 1.0, 1.2), "Enterprise memory", "cases · procedures · links", "end", 250, 214, "mem"))
    o.append(callout(*P(1.0, 7.7, 0.35), "Workflows", "the known path, run as code", "start", 150, 446, "wf"))
    o.append(callout(*P(gx0 + 0.35, gy0 + gd, 2.0), "Governance", "policy on every action and escalation", "start", 735, 430, "pol"))

    o.append("</svg>")
    return "\n".join(o)


CSS = r"""
  .iso-svg { display: block; width: 100%; min-width: 640px; height: auto; font-family: var(--font-sans); }
  .iso-grid { fill: none; stroke: #ece7db; stroke-width: .8; }
  .iso-tag { font-family: var(--font-mono); font-size: 11px; font-weight: 600; letter-spacing: .12em; fill: var(--text-3); }
  .iso-track { fill: none; stroke: #c4a8e8; stroke-width: 2; stroke-dasharray: 2 5; stroke-linecap: round; }
  .iso-beam { stroke: url(#isoBeam); stroke-width: 2.4; stroke-linecap: round; stroke-dasharray: 3 5; animation: iso-flow 1.6s linear infinite; }
  @keyframes iso-flow { to { stroke-dashoffset: -16; } }
  .iso-chip-edge { fill: none; stroke: #b28be0; stroke-width: 1.2; opacity: .8; }
  .iso-chip-t { font-family: var(--font-serif); font-size: 20px; font-weight: 600; fill: #fff; }
  .iso-layer { font-family: var(--font-mono); font-size: 9px; font-weight: 600; letter-spacing: .04em; fill: #0a4c3a; }
  .iso-shield { fill: #fff; stroke: #2a5fd4; stroke-width: 1.6; }
  .iso-shield-tick { fill: none; stroke: #2a5fd4; stroke-width: 1.8; stroke-linecap: round; stroke-linejoin: round; }
  .iso-cloud { fill: #fbf0e0; stroke: #c47028; stroke-width: 1.4; stroke-dasharray: 5 4; }
  .iso-cloud-t { font-family: var(--font-serif); font-size: 17px; font-weight: 500; text-anchor: middle; fill: var(--text-1); }
  .iso-cloud-s { font-family: var(--font-mono); font-size: 11px; text-anchor: middle; fill: var(--checkpoint-ink); }
  .iso-route { fill: none; stroke: #c47028; stroke-width: 1.8; stroke-dasharray: 6 5; }
  .iso-pill rect { fill: var(--bg-surface); stroke: #b9a3dc; }
  .iso-pill text { font-family: var(--font-mono); font-size: 11px; text-anchor: middle; fill: var(--signal-ink); }
  .iso-co circle { fill: var(--bg-surface); stroke-width: 1.6; }
  .iso-co path { fill: none; stroke-width: 1; stroke-dasharray: 2 3; }
  .iso-co .t { font-family: var(--font-serif); font-size: 18px; font-weight: 500; fill: var(--text-1); }
  .iso-co .s { font-family: var(--font-mono); font-size: 11px; fill: var(--text-3); }
  .iso-co.lm circle, .iso-co.lm path { stroke: var(--text-1); }
  .iso-co.mem circle, .iso-co.mem path { stroke: var(--ok); }
  .iso-co.wf circle, .iso-co.wf path { stroke: var(--signal); }
  .iso-co.pol circle, .iso-co.pol path { stroke: var(--k8s); }
  @media (prefers-reduced-motion: reduce) { .iso-beam { animation: none; } }
"""
