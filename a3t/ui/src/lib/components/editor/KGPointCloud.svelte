<script lang="ts">
  /**
   * KGPointCloud — WebGL 3D point cloud with full 3-axis traversal.
   *
   * Controls:
   *   Left-drag:  Rotate (orbit around look-target)
   *   Right-drag: Pan (slide camera laterally)
   *   Scroll:     Fly forward/backward INTO the depth
   *   WASD:       Move forward/back/left/right relative to camera facing
   *   Q/E:        Move up/down
   *   Shift:      Faster movement
   *   Click:      Select node + smooth fly-to
   */
  import { onMount, onDestroy } from 'svelte';
  import * as THREE from 'three';

  let {
    points,
    edges,
    typeColors,
    onselect,
    focusId = '',
  }: {
    points: { id: string; name: string; type: string; x: number; y: number; z: number; color?: string }[];
    edges?: { source: number; target: number; opacity?: number }[];
    typeColors?: Record<string, string>;
    onselect?: (node: any) => void;
    focusId?: string;
  } = $props();

  let container: HTMLDivElement;
  let tooltip: HTMLDivElement;
  let renderer: THREE.WebGLRenderer;
  let scene: THREE.Scene;
  let camera: THREE.PerspectiveCamera;
  let pointCloud: THREE.Points;
  let edgeLines: THREE.LineSegments | null = null;
  let raycaster: THREE.Raycaster;
  let mouse = new THREE.Vector2();
  let animId: number;
  let hoveredIdx = -1;
  let selectedIdx = -1;
  let highlightRing: THREE.Mesh;

  // ── Camera state ──
  let yaw = 0;          // horizontal rotation (radians)
  let pitch = 0;        // vertical rotation (radians)
  let camTarget = new THREE.Vector3(0, 0, 0);  // orbit center / look target
  let camDist = 200;    // distance from target

  // Key state
  const keys: Record<string, boolean> = {};
  const MOVE_SPEED = 1.5;
  const FAST_MULT = 3;

  // Drag state
  let isDragging = false;
  let dragButton = -1;
  let lastMouseX = 0;
  let lastMouseY = 0;
  let mouseDownX = 0;
  let mouseDownY = 0;
  let didScrollSinceDown = false;
  let lastScrollTime = 0;

  // Fly-to tween
  let tweenActive = false;
  let tweenStart = { target: new THREE.Vector3(), dist: 0, yaw: 0, pitch: 0 };
  let tweenEnd = { target: new THREE.Vector3(), dist: 0, yaw: 0, pitch: 0 };
  let tweenT = 0;

  onMount(() => {
    init();
    animate();
  });

  onDestroy(() => {
    cancelAnimationFrame(animId);
    renderer?.dispose();
    window.removeEventListener('resize', onResize);
    window.removeEventListener('keydown', onKeyDown);
    window.removeEventListener('keyup', onKeyUp);
  });

  $effect(() => {
    if (focusId && pointCloud) {
      const idx = points.findIndex(p => p.id === focusId);
      if (idx >= 0) flyToIndex(idx);
    }
  });

  function updateCameraFromOrbit() {
    // Spherical to cartesian
    const cp = Math.cos(pitch), sp = Math.sin(pitch);
    const cy = Math.cos(yaw), sy = Math.sin(yaw);
    camera.position.set(
      camTarget.x + camDist * cp * sy,
      camTarget.y + camDist * sp,
      camTarget.z + camDist * cp * cy,
    );
    camera.lookAt(camTarget);
  }

  function init() {
    const w = container.clientWidth;
    const h = container.clientHeight;

    scene = new THREE.Scene();
    scene.background = new THREE.Color(0xfaf9f6);
    scene.fog = new THREE.FogExp2(0xfaf9f6, 0.0004);

    camera = new THREE.PerspectiveCamera(60, w / h, 0.1, 8000);

    renderer = new THREE.WebGLRenderer({ antialias: true });
    renderer.setSize(w, h);
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    container.appendChild(renderer.domElement);

    raycaster = new THREE.Raycaster();
    raycaster.params.Points!.threshold = 1.8;

    buildPointCloud();
    if (edges && edges.length > 0) buildEdges();

    // Highlight ring
    const ringGeo = new THREE.RingGeometry(3.5, 4.5, 32);
    const ringMat = new THREE.MeshBasicMaterial({ color: 0x5530a8, side: THREE.DoubleSide, transparent: true, opacity: 0.7 });
    highlightRing = new THREE.Mesh(ringGeo, ringMat);
    highlightRing.visible = false;
    scene.add(highlightRing);

    scene.add(new THREE.AmbientLight(0xffffff, 0.5));

    // Initial camera position
    camDist = 200;
    yaw = 0;
    pitch = 0.3;
    updateCameraFromOrbit();

    // Events
    const el = renderer.domElement;
    el.addEventListener('mousemove', onMouseMove);
    el.addEventListener('mousedown', onMouseDown);
    el.addEventListener('mouseup', onMouseUp);
    el.addEventListener('wheel', onWheel, { passive: false });
    el.addEventListener('click', onClickEvt);
    el.addEventListener('contextmenu', (e) => e.preventDefault());
    window.addEventListener('resize', onResize);
    window.addEventListener('keydown', onKeyDown);
    window.addEventListener('keyup', onKeyUp);
  }

  function buildPointCloud() {
    const n = points.length;
    const positions = new Float32Array(n * 3);
    const colors = new Float32Array(n * 3);

    for (let i = 0; i < n; i++) {
      const p = points[i];
      positions[i * 3]     = p.x;
      positions[i * 3 + 1] = p.y;
      positions[i * 3 + 2] = p.z;

      const hex = p.color ?? typeColors?.[p.type] ?? '#64748b';
      const c = new THREE.Color(hex);
      colors[i * 3]     = c.r;
      colors[i * 3 + 1] = c.g;
      colors[i * 3 + 2] = c.b;
    }

    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute('position', new THREE.BufferAttribute(positions, 3));
    geometry.setAttribute('color', new THREE.BufferAttribute(colors, 3));

    const material = new THREE.PointsMaterial({
      size: 3,
      vertexColors: true,
      sizeAttenuation: true,
      transparent: true,
      opacity: 0.9,
      depthWrite: true,
    });

    pointCloud = new THREE.Points(geometry, material);
    scene.add(pointCloud);
  }

  function buildEdges() {
    if (!edges || edges.length === 0) return;
    const positions: number[] = [];
    const colors: number[] = [];

    for (const e of edges) {
      const s = points[e.source];
      const t = points[e.target];
      if (!s || !t) continue;
      positions.push(s.x, s.y, s.z, t.x, t.y, t.z);
      const srcHex = s.color ?? typeColors?.[s.type] ?? '#a8a396';
      const tgtHex = t.color ?? typeColors?.[t.type] ?? '#a8a396';
      const sc = new THREE.Color(srcHex);
      const tc = new THREE.Color(tgtHex);
      sc.lerp(new THREE.Color(0xd9d4c6), 0.5);
      tc.lerp(new THREE.Color(0xd9d4c6), 0.5);
      colors.push(sc.r, sc.g, sc.b, tc.r, tc.g, tc.b);
    }

    const geo = new THREE.BufferGeometry();
    geo.setAttribute('position', new THREE.Float32BufferAttribute(positions, 3));
    geo.setAttribute('color', new THREE.Float32BufferAttribute(colors, 3));
    const mat = new THREE.LineBasicMaterial({ vertexColors: true, transparent: true, opacity: 0.35, depthWrite: false });
    edgeLines = new THREE.LineSegments(geo, mat);
    scene.add(edgeLines);
  }

  // ── Input handlers ──

  function onKeyDown(e: KeyboardEvent) {
    keys[e.key.toLowerCase()] = true;
    // Prevent page scroll when focused
    if (['w','a','s','d','q','e',' '].includes(e.key.toLowerCase())) e.preventDefault();
  }
  function onKeyUp(e: KeyboardEvent) { keys[e.key.toLowerCase()] = false; }

  function onMouseDown(e: MouseEvent) {
    isDragging = true;
    dragButton = e.button;
    lastMouseX = e.clientX;
    lastMouseY = e.clientY;
    mouseDownX = e.clientX;
    mouseDownY = e.clientY;
    didScrollSinceDown = false;
    tweenActive = false;
  }

  function onMouseUp(_e: MouseEvent) {
    isDragging = false;
    dragButton = -1;
  }

  function onMouseMove(e: MouseEvent) {
    const rect = renderer.domElement.getBoundingClientRect();

    // Raycasting for tooltip
    mouse.x = ((e.clientX - rect.left) / rect.width) * 2 - 1;
    mouse.y = -((e.clientY - rect.top) / rect.height) * 2 + 1;
    raycaster.setFromCamera(mouse, camera);
    const intersects = raycaster.intersectObject(pointCloud);

    if (intersects.length > 0) {
      const idx = intersects[0].index!;
      if (idx !== hoveredIdx) {
        hoveredIdx = idx;
        const p = points[idx];
        tooltip.textContent = `${p.name} (${p.type})`;
        tooltip.style.display = 'block';
      }
      tooltip.style.left = `${e.clientX - rect.left + 12}px`;
      tooltip.style.top = `${e.clientY - rect.top - 8}px`;
      renderer.domElement.style.cursor = 'pointer';
    } else {
      hoveredIdx = -1;
      tooltip.style.display = 'none';
      renderer.domElement.style.cursor = isDragging ? 'grabbing' : 'grab';
    }

    // Dragging
    if (!isDragging) return;
    const dx = e.clientX - lastMouseX;
    const dy = e.clientY - lastMouseY;
    lastMouseX = e.clientX;
    lastMouseY = e.clientY;

    if (dragButton === 0) {
      // Left-drag: orbit rotate
      yaw   -= dx * 0.005;
      pitch += dy * 0.005;
      pitch = Math.max(-Math.PI / 2 + 0.05, Math.min(Math.PI / 2 - 0.05, pitch));
      updateCameraFromOrbit();
    } else if (dragButton === 2) {
      // Right-drag: pan (move target laterally)
      const right = new THREE.Vector3();
      const up = new THREE.Vector3();
      camera.getWorldDirection(new THREE.Vector3()); // ensure matrix is current
      right.setFromMatrixColumn(camera.matrixWorld, 0);
      up.setFromMatrixColumn(camera.matrixWorld, 1);
      const panScale = camDist * 0.002;
      camTarget.add(right.multiplyScalar(-dx * panScale));
      camTarget.add(up.multiplyScalar(dy * panScale));
      updateCameraFromOrbit();
    }
  }

  function onWheel(e: WheelEvent) {
    e.preventDefault();
    tweenActive = false;
    // Scroll → fly forward/backward into depth
    didScrollSinceDown = true;
    lastScrollTime = performance.now();
    const delta = e.deltaY * 0.08;
    const forward = new THREE.Vector3();
    camera.getWorldDirection(forward);
    const moveVec = forward.multiplyScalar(-delta);
    camTarget.add(moveVec);
    // Don't change camDist — keep orbit radius constant
    updateCameraFromOrbit();
  }

  function onClickEvt(e: MouseEvent) {
    // Only fly-to on a genuine click: no drag movement, no recent scroll
    const dx = Math.abs(e.clientX - mouseDownX);
    const dy = Math.abs(e.clientY - mouseDownY);
    const wasDrag = dx > 3 || dy > 3;
    const recentScroll = (performance.now() - lastScrollTime) < 300;

    if (wasDrag || recentScroll) {
      // Still select for the detail panel, but don't fly
      if (hoveredIdx >= 0) {
        selectedIdx = hoveredIdx;
        const p = points[selectedIdx];
        highlightRing.position.set(p.x, p.y, p.z);
        highlightRing.visible = true;
        onselect?.(p);
      }
      return;
    }

    if (hoveredIdx >= 0) {
      selectedIdx = hoveredIdx;
      const p = points[selectedIdx];
      flyToIndex(selectedIdx);
      onselect?.(p);
    }
  }

  // ── WASD movement in animate loop ──
  function processKeys() {
    const speed = (keys['shift'] ? MOVE_SPEED * FAST_MULT : MOVE_SPEED);
    const forward = new THREE.Vector3();
    const right = new THREE.Vector3();
    const up = new THREE.Vector3(0, 1, 0);

    camera.getWorldDirection(forward);
    right.crossVectors(forward, up).normalize();

    const move = new THREE.Vector3();

    if (keys['w'] || keys['arrowup'])    move.add(forward.clone().multiplyScalar(speed));
    if (keys['s'] || keys['arrowdown'])  move.add(forward.clone().multiplyScalar(-speed));
    if (keys['a'] || keys['arrowleft'])  move.add(right.clone().multiplyScalar(-speed));
    if (keys['d'] || keys['arrowright']) move.add(right.clone().multiplyScalar(speed));
    if (keys['q'] || keys[' '])          move.add(up.clone().multiplyScalar(speed));
    if (keys['e'])                        move.add(up.clone().multiplyScalar(-speed));

    if (move.lengthSq() > 0) {
      camTarget.add(move);
      updateCameraFromOrbit();
    }
  }

  // ── Fly-to animation ──
  function flyToIndex(idx: number) {
    const p = points[idx];
    if (!p) return;
    selectedIdx = idx;
    const target = new THREE.Vector3(p.x, p.y, p.z);

    highlightRing.position.copy(target);
    highlightRing.visible = true;

    tweenStart.target.copy(camTarget);
    tweenStart.dist = camDist;
    tweenStart.yaw = yaw;
    tweenStart.pitch = pitch;

    tweenEnd.target.copy(target);
    tweenEnd.dist = 30; // close up
    // Compute yaw/pitch to face the target from current angle (keep similar view direction)
    tweenEnd.yaw = yaw;
    tweenEnd.pitch = pitch;

    tweenT = 0;
    tweenActive = true;
  }

  function processTween() {
    if (!tweenActive) return;
    tweenT += 0.035;
    if (tweenT >= 1) { tweenT = 1; tweenActive = false; }
    const ease = 1 - Math.pow(1 - tweenT, 3);

    camTarget.lerpVectors(tweenStart.target, tweenEnd.target, ease);
    camDist = tweenStart.dist + (tweenEnd.dist - tweenStart.dist) * ease;
    yaw = tweenStart.yaw + (tweenEnd.yaw - tweenStart.yaw) * ease;
    pitch = tweenStart.pitch + (tweenEnd.pitch - tweenStart.pitch) * ease;
    updateCameraFromOrbit();
  }

  // ── Render loop ──
  function animate() {
    animId = requestAnimationFrame(animate);

    processKeys();
    processTween();

    if (highlightRing.visible) {
      highlightRing.lookAt(camera.position);
    }

    renderer.render(scene, camera);
  }

  function onResize() {
    const w = container.clientWidth;
    const h = container.clientHeight;
    camera.aspect = w / h;
    camera.updateProjectionMatrix();
    renderer.setSize(w, h);
  }
</script>

<div class="kg-cloud-wrap" bind:this={container} tabindex="0">
  <div class="kg-tooltip" bind:this={tooltip}></div>
</div>

<style>
  .kg-cloud-wrap {
    width: 100%;
    height: 100%;
    position: relative;
    overflow: hidden;
    background: var(--bg-page, #faf9f6);
    outline: none;
  }
  .kg-cloud-wrap :global(canvas) {
    display: block;
    width: 100% !important;
    height: 100% !important;
  }
  .kg-tooltip {
    display: none;
    position: absolute;
    padding: 4px 10px;
    background: var(--text-1, #1a1a1a);
    color: var(--bg-page, #faf9f6);
    font-size: 11px;
    font-weight: 600;
    font-family: var(--font-mono, monospace);
    border-radius: 4px;
    pointer-events: none;
    z-index: 10;
    box-shadow: 0 2px 8px rgba(0,0,0,0.15);
    white-space: nowrap;
  }
</style>
