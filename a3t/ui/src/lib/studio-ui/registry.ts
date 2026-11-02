/// studio-ui components by the ids kinds name in `views` (studio-core never
/// imports a component). Views get `{ doc, workspace, studio }`; panels get
/// `{ id, doc, workspace, studio }` with `doc` the active document or null.
import type { Component } from 'svelte';
import FlowView from './views/FlowView.svelte';
import ConfigView from './views/ConfigView.svelte';
import QueryView from './views/QueryView.svelte';
import FlowAssistPanel from './panels/FlowAssistPanel.svelte';
import RunsPanel from './panels/RunsPanel.svelte';
import ProblemsPanel from './panels/ProblemsPanel.svelte';
import ResultsPanel from './panels/ResultsPanel.svelte';
import HistoryPanel from './panels/HistoryPanel.svelte';
import SavedPanel from './panels/SavedPanel.svelte';

export const VIEWS: Record<string, Component<any>> = {
  flow: FlowView,
  config: ConfigView,
  query: QueryView,
};

export const PANELS: Record<string, Component<any>> = {
  form: FlowAssistPanel,
  ai: FlowAssistPanel,
  runs: RunsPanel,
  problems: ProblemsPanel,
  results: ResultsPanel,
  history: HistoryPanel,
  saved: SavedPanel,
};
