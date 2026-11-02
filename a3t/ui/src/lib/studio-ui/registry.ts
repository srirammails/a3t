/// studio-ui components by the ids kinds name in `views` (studio-core never
/// imports a component). Views get `{ doc, workspace, studio }`; panels get
/// `{ id, doc, workspace, studio }` with `doc` the active document or null.
import type { Component } from 'svelte';
import FlowView from './views/FlowView.svelte';
import PreviewView from './views/PreviewView.svelte';
import FlowAssistPanel from './panels/FlowAssistPanel.svelte';
import RunsPanel from './panels/RunsPanel.svelte';
import ProblemsPanel from './panels/ProblemsPanel.svelte';

export const VIEWS: Record<string, Component<any>> = {
  flow: FlowView,
  preview: PreviewView,
};

export const PANELS: Record<string, Component<any>> = {
  form: FlowAssistPanel,
  ai: FlowAssistPanel,
  runs: RunsPanel,
  problems: ProblemsPanel,
};
