/// Studio layout, one per app (Flow Studio, DB Console, Logs, Runtime…): which
/// panel each region shows, its size, whether it is minimized to a stack on
/// its edge, and which panel (or the editor area, `editor`) is maximized to
/// fill the studio. Kept per browser under `a3t.studio.<app>`.

export type RegionId = 'left' | 'right' | 'bottom';
export type Panel = { id: string; title: string };
export type Tab = { id: string; title: string; icon?: string; dirty?: boolean; hint?: string };
export type RegionState = { size: number; minimized: boolean; active: string };
export type StudioLayout = {
  left: RegionState;
  right: RegionState;
  bottom: RegionState;
  maximized: string | null;
  /// Panels the user moved from their home region: to another region, or into
  /// the editor area as a tab (`center`). Kept.
  placement: Record<string, RegionId | 'center'>;
  /// The panel shown in the editor area instead of a document, if any (not kept).
  centerPanel: string | null;
};

/// Drag payload for a panel (its id).
export const PANEL_MIME = 'application/x-a3t-panel';

export const DEFAULT_LAYOUT: StudioLayout = {
  left: { size: 280, minimized: false, active: '' },
  right: { size: 340, minimized: false, active: '' },
  bottom: { size: 300, minimized: true, active: '' },
  maximized: null,
  placement: {},
  centerPanel: null,
};

export class Studio {
  readonly key: string;
  state = $state<StudioLayout>(structuredClone(DEFAULT_LAYOUT));

  constructor(app: string, defaults: Partial<StudioLayout> = {}) {
    this.key = `a3t.studio.${app}`;
    const base: StudioLayout = { ...structuredClone(DEFAULT_LAYOUT), ...structuredClone(defaults) };
    try {
      const v = JSON.parse(localStorage.getItem(this.key) ?? 'null');
      if (v && v.left && v.bottom) {
        this.state = { ...base, ...v, right: v.right ?? base.right, placement: v.placement ?? {}, maximized: null, centerPanel: null };
        return;
      }
    } catch { /* private window */ }
    this.state = base;
  }

  save() {
    try { localStorage.setItem(this.key, JSON.stringify({ ...this.state, maximized: null, centerPanel: null })); } catch { /* private window */ }
  }

  /// Move a panel to a region, or into the editor area as a tab (shown there now).
  place(panel: string, where: RegionId | 'center') {
    this.state.placement[panel] = where;
    if (where === 'center') this.state.centerPanel = panel;
    else {
      if (this.state.centerPanel === panel) this.state.centerPanel = null;
      this.state[where].active = panel;
      this.state[where].minimized = false;
    }
    this.save();
  }

  /// Send a panel back to its home region, minimized there.
  unplace(panel: string, home: RegionId) {
    delete this.state.placement[panel];
    if (this.state.centerPanel === panel) this.state.centerPanel = null;
    this.state[home].active = panel;
    this.state[home].minimized = true;
    this.save();
  }

  /// Show `panel` in `region`, restoring the region if it was minimized.
  reveal(region: RegionId, panel: string) {
    this.state[region].active = panel;
    this.state[region].minimized = false;
    this.save();
  }

  minimize(region: RegionId) {
    if (this.state.maximized) this.state.maximized = null;
    this.state[region].minimized = true;
    this.save();
  }

  toggleMaximize(panel: string) {
    this.state.maximized = this.state.maximized === panel ? null : panel;
  }

  resize(region: RegionId, size: number) {
    const [min, max] = region === 'bottom' ? [120, Math.max(160, window.innerHeight - 160)] : [180, Math.max(220, window.innerWidth * 0.6)];
    this.state[region].size = Math.min(max, Math.max(min, Math.round(size)));
  }
}
