// The app icon set, shared by the rail and the Squares palette.
//
// These used to live inline in Sidebar.svelte, which meant the palette could not
// show an app's real icon without a second copy of the table drifting out of sync.
// Values are SVG path fragments, rendered inside a 16x16 `<svg>` by the caller.

export const APP_ICONS: Record<string, string> = {
    project:   '<path d="M1.5 4.5a1 1 0 011-1h3.5l1.5 1.5h6a1 1 0 011 1v6.5a1 1 0 01-1 1h-11a1 1 0 01-1-1z"/><path d="M5 9.5h6M5 11.5h3"/>',
    dashboard: '<rect x="2" y="2" width="5" height="5" rx="0.5"/><rect x="9" y="2" width="5" height="5" rx="0.5"/><rect x="2" y="9" width="5" height="5" rx="0.5"/><rect x="9" y="9" width="5" height="5" rx="0.5"/>',
    chat:      '<path d="M2 3h12a1 1 0 011 1v7a1 1 0 01-1 1H6l-3 2v-2H2a1 1 0 01-1-1V4a1 1 0 011-1z"/><path d="M5 7h.01M8 7h.01M11 7h.01" stroke-width="2" stroke-linecap="round"/>',
    grid:      '<path d="M3 2h10a1 1 0 011 1v10a1 1 0 01-1 1H3a1 1 0 01-1-1V3a1 1 0 011-1z"/><path d="M5 6h6M5 9h4"/>',
    agent:     '<rect x="3" y="1" width="10" height="7" rx="2"/><path d="M6 4h.01M10 4h.01" stroke-width="2" stroke-linecap="round"/><path d="M5 8v2a3 3 0 006 0V8"/><path d="M8 13v2"/>',
    canvas:    '<rect x="1" y="1" width="14" height="14" rx="1.5"/><path d="M1 10h14"/><path d="M4 1v9M7 1v9M10 1v9M13 1v9"/><path d="M4 5h3M10 5h3"/>',
    resources: '<path d="M8 1v4M8 11v4M1 8h4M11 8h4"/><circle cx="8" cy="8" r="3"/><circle cx="8" cy="1" r="1" fill="currentColor"/><circle cx="8" cy="15" r="1" fill="currentColor"/><circle cx="1" cy="8" r="1" fill="currentColor"/><circle cx="15" cy="8" r="1" fill="currentColor"/>',
    cabinet:   '<rect x="2" y="1" width="12" height="14" rx="1.5"/><path d="M2 5.5h12M2 10.5h12"/><circle cx="8" cy="3.25" r="0.7" fill="currentColor"/><circle cx="8" cy="8" r="0.7" fill="currentColor"/><circle cx="8" cy="12.75" r="0.7" fill="currentColor"/>',
    kg:        '<circle cx="8" cy="2" r="1.8"/><circle cx="2.5" cy="13" r="1.8"/><circle cx="13.5" cy="13" r="1.8"/><path d="M8 3.8V8M6.5 9.5L3.5 11.5M9.5 9.5L12.5 11.5M4.3 13h7.4"/><circle cx="8" cy="8.5" r="1.2" fill="currentColor"/>',
    knowledge: '<path d="M2 13V4a1 1 0 011-1h4l2 2h4a1 1 0 011 1v7a1 1 0 01-1 1H3a1 1 0 01-1-1z"/><path d="M6 9l2-2 2 2"/><path d="M8 7v4"/>',
    memory:    '<path d="M4 2h8a2 2 0 012 2v8a2 2 0 01-2 2H4a2 2 0 01-2-2V4a2 2 0 012-2z"/><path d="M6 2v12M10 2v12M2 6h12M2 10h12"/>',
    play:      '<polygon points="3,2 13,8 3,14"/>',
    recon:     '<circle cx="8" cy="8" r="6"/><path d="M5 8l2 2 4-4"/>',
    logs:      '<path d="M2 4h12M2 8h8M2 12h10"/>',
    flow:      '<rect x="1" y="2" width="5" height="4" rx="1"/><rect x="10" y="2" width="5" height="4" rx="1"/><rect x="5.5" y="10" width="5" height="4" rx="1"/><path d="M3.5 6v2a2 2 0 002 2h5a2 2 0 002-2V6"/><path d="M8 10V8"/>',
    test:      '<path d="M6 1v4M10 1v4M4 5h8a1 1 0 011 1v2l-2 2v4H5V10L3 8V6a1 1 0 011-1z"/><path d="M6 10h4"/>',
    console:   '<rect x="1" y="2" width="14" height="12" rx="1.5"/><path d="M4 6l2 2-2 2"/><path d="M7 10h5"/><circle cx="12" cy="5" r="0.8" fill="currentColor"/>',
    database:  '<ellipse cx="8" cy="4" rx="5.5" ry="2.5"/><path d="M2.5 4v4c0 1.38 2.46 2.5 5.5 2.5s5.5-1.12 5.5-2.5V4"/><path d="M2.5 8c0 1.38 2.46 2.5 5.5 2.5s5.5-1.12 5.5-2.5"/><path d="M2.5 8v4c0 1.38 2.46 2.5 5.5 2.5s5.5-1.12 5.5-2.5V8"/>',
    runtime:   '<rect x="1.5" y="2" width="13" height="5" rx="1"/><rect x="1.5" y="9" width="13" height="5" rx="1"/><circle cx="4.5" cy="4.5" r="0.8" fill="currentColor"/><circle cx="4.5" cy="11.5" r="0.8" fill="currentColor"/><path d="M8 4.5h4M8 11.5h4"/>',
    runs:      '<polygon points="3,2 13,8 3,14"/><path d="M13 2v12"/>',
    trigger:   '<path d="M13 2L5 9l3 1-5 5 8-7-3-1z"/>',
    terminal:  '<rect x="1" y="2" width="14" height="12" rx="1.5"/><path d="M4 6l3 2-3 2M8 10h4"/>',
    settings:  '<path d="M8 10.5a2.5 2.5 0 100-5 2.5 2.5 0 000 5z"/><path d="M13 8a5 5 0 00-.1-1l1.5-1.2-1.5-2.6-1.8.7A5 5 0 009.9 3l-.3-1.9H6.4L6.1 3a5 5 0 00-1.2.7l-1.8-.7-1.5 2.6L3.1 7A5 5 0 003 8a5 5 0 00.1 1l-1.5 1.2 1.5 2.6 1.8-.7c.4.3.8.5 1.2.7l.3 1.9h3.2l.3-1.9c.4-.2.8-.4 1.2-.7l1.8.7 1.5-2.6L12.9 9c.1-.3.1-.6.1-1z"/>',
    shield:    '<path d="M8 1l5.5 2.2v4c0 3.5-2.3 6.4-5.5 7.6C4.8 13.6 2.5 10.7 2.5 7.2v-4L8 1z"/><path d="M5.8 8l1.6 1.6L10.4 6.5"/>',
    idcard:    '<rect x="1.5" y="3" width="13" height="10" rx="1.5"/><circle cx="5.5" cy="7.5" r="1.5"/><path d="M3.5 11c.3-1.2 1.1-1.8 2-1.8s1.7.6 2 1.8"/><path d="M9.5 6.5h3.5M9.5 9h3"/>',
};

/// Fall back to a plain square rather than rendering nothing, so an app added to
/// the menu with an unknown icon name is still clickable.
export function appIcon(name: string | undefined): string {
  return (name && APP_ICONS[name]) || '<rect x="2" y="2" width="12" height="12" rx="1"/>';
}
