# Renders cncf-workflow-dsl.html: the CNCF Serverless Workflow schema tree laid out as a floor.
# Screenshot it at 1200x627 to public/covers/cncf-workflow-dsl.png.
W, H = 1800, 980
C = {'root': ('#0f0f0f', '#faf9f6'), 'doc': ('#eaf1ff', '#1a3fa8'), 'use': ('#e0ebff', '#143e95'),
     'do': ('#fbf0e0', '#7a4710'), 'io': ('#f4f2ed', '#3a3833'), 'task': ('#f3ecfa', '#4d2a78'),
     'call': ('#e3f4ef', '#0a4c3a')}
nodes, edges = [], []
def node(x, y, label, tone, w=None):
    w = w or 26 + 15 * len(label); nodes.append((x, y, w, label, tone)); return (x, y)
def edge(a, b):
    (x1, y1), (x2, y2) = a, b; my = (y1 + y2) / 2
    edges.append(f'M{x1},{y1} V{my} H{x2} V{y2}')
root = node(900, 905, 'workflow', 'root', 190)
lvl1 = [('document', 'doc'), ('input', 'io'), ('use', 'use'), ('do', 'do'), ('output', 'io'), ('timeout', 'io'), ('schedule', 'io')]
xs1 = [330, 520, 700, 900, 1100, 1280, 1470]
l1 = {}
for (lab, t), x in zip(lvl1, xs1):
    l1[lab] = node(x, 770, lab, t, 150 if lab == 'do' else None); edge(root, l1[lab])
for lab, x in zip(['functions', 'errors', 'retries', 'secrets'], [500, 610, 715, 800]):
    pass
tasks = ['set', 'switch', 'call', 'for', 'fork', 'try', 'emit', 'listen', 'wait', 'raise', 'run']
xs2 = [900 + (i - 5) * 112 for i in range(len(tasks))]
l2 = {}
for lab, x in zip(tasks, xs2):
    l2[lab] = node(x, 610, lab, 'task'); edge(l1['do'], l2[lab])
for lab, x in zip(['http', 'grpc', 'openapi', 'asyncapi', 'a2a'], [400, 510, 630, 770, 890]):
    edge(l2['call'], node(x, 450, lab, 'call'))
for lab, x in zip(['functions', 'errors', 'retries', 'secrets'], [470, 600, 720, 840]):
    pass
for lab, x in zip(['container', 'script', 'shell', 'workflow'], [1110, 1260, 1380, 1510]):
    edge(l2['run'], node(x, 450, lab, 'call'))
for lab, x in zip(['functions', 'retries', 'errors'], [560, 700, 820]):
    pass
svg = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}">',
       '<defs><pattern id="g" width="60" height="60" patternUnits="userSpaceOnUse"><path d="M60 0H0V60" fill="none" stroke="#e2ddd0" stroke-width="1.5"/></pattern></defs>',
       f'<rect width="{W}" height="{H}" fill="url(#g)"/>',
       f'<path d="{" ".join(edges)}" fill="none" stroke="#b9b3a3" stroke-width="3"/>']
for x, y, w, lab, t in nodes:
    bg, ink = C[t]; h = 54 if t in ('root', 'do') else 44
    svg.append(f'<rect x="{x - w/2}" y="{y - h/2}" width="{w}" height="{h}" rx="10" fill="{bg}" stroke="{ink}" stroke-opacity=".35" stroke-width="2"/>')
    svg.append(f'<text x="{x}" y="{y + 7}" text-anchor="middle" font-family="IBM Plex Mono" font-weight="600" font-size="{22 if h == 54 else 19}" fill="{ink}">{lab}</text>')
svg.append('</svg>')
html = f'''<!doctype html><html><head><meta charset="utf-8">
<link href="https://fonts.googleapis.com/css2?family=IBM+Plex+Mono:wght@500;600&family=Playfair+Display:ital,wght@0,500;1,500&display=swap" rel="stylesheet">
<style>
html,body{{margin:0;width:1200px;height:627px;overflow:hidden;background:#faf9f6}}
.stage{{position:absolute;inset:0;perspective:900px;perspective-origin:50% 0%}}
.floor{{position:absolute;left:{(1200 - W) // 2}px;top:{627 - H + 40}px;width:{W}px;height:{H}px;transform:rotateX(56deg);transform-origin:50% 100%}}
.fade{{position:absolute;inset:0;background:linear-gradient(#faf9f6 0%,#faf9f6 30%,rgba(250,249,246,0) 46%)}}
.title{{position:absolute;left:64px;top:46px}}
.kicker{{font:500 15px 'IBM Plex Mono',monospace;letter-spacing:.16em;color:#6e6a60;text-transform:uppercase}}
h1{{margin:14px 0 0;font:500 62px/1.04 'Playfair Display',Georgia,serif;letter-spacing:-.02em;color:#0f0f0f}}
h1 em{{color:#c47028}}
.sub{{margin-top:12px;font:500 16px 'IBM Plex Mono',monospace;color:#6e6a60}}
</style></head><body>
<div class="stage"><div class="floor">{"".join(svg)}</div></div><div class="fade"></div>
<div class="title"><div class="kicker">A3T · Blog</div><h1>CNCF Workflow DSL, <em>explained</em></h1>
<div class="sub">document · do · call · set · switch · for · fork · try · emit · listen</div></div>
</body></html>'''
open('cncf-workflow-dsl.html', 'w').write(html)
