/// studio-ui components by the ids kinds name in `views` (studio-core never
/// imports a component). Views get `{ doc, workspace, studio }`; panels get
/// `{ id, doc, workspace, studio }` with `doc` the active document or null.
import type { Component } from 'svelte';
import FlowView from './views/FlowView.svelte';
import CodeView from './views/CodeView.svelte';
import TemplateView from './views/TemplateView.svelte';
import ConfigPanel from './panels/ConfigPanel.svelte';
import AiPanel from './panels/AiPanel.svelte';
import QueryView from './views/QueryView.svelte';
import ChatView from './views/ChatView.svelte';
import ToolsView from './views/ToolsView.svelte';
import TriggerView from './views/TriggerView.svelte';
import CanvasView from './views/CanvasView.svelte';
import AgentView from './views/AgentView.svelte';
import AgentFormPanel from './panels/AgentFormPanel.svelte';
import TemplateFieldsPanel from './panels/TemplateFieldsPanel.svelte';
import TemplateAgentsPanel from './panels/TemplateAgentsPanel.svelte';
import AgentRunsPanel from './panels/AgentRunsPanel.svelte';
import RuntimeView from './views/RuntimeView.svelte';
import FlowAssistPanel from './panels/FlowAssistPanel.svelte';
import RunsPanel from './panels/RunsPanel.svelte';
import ProblemsPanel from './panels/ProblemsPanel.svelte';
import ResultsPanel from './panels/ResultsPanel.svelte';
import HistoryPanel from './panels/HistoryPanel.svelte';
import SavedPanel from './panels/SavedPanel.svelte';
import TargetRunsPanel from './panels/TargetRunsPanel.svelte';

export const VIEWS: Record<string, Component<any>> = {
  flow: FlowView,
  code: CodeView,
  template: TemplateView,
  query: QueryView,
  chat: ChatView,
  tools: ToolsView,
  trigger: TriggerView,
  canvas: CanvasView,
  agent: AgentView,
  runtime: RuntimeView,
};

export const PANELS: Record<string, Component<any>> = {
  'flow-form': FlowAssistPanel,
  ai: AiPanel,
  config: ConfigPanel,
  'agent-form': AgentFormPanel,
  'template-fields': TemplateFieldsPanel,
  'template-agents': TemplateAgentsPanel,
  'agent-test': ChatView,
  'agent-runs': AgentRunsPanel,
  runs: RunsPanel,
  problems: ProblemsPanel,
  results: ResultsPanel,
  history: HistoryPanel,
  saved: SavedPanel,
  'target-runs': TargetRunsPanel,
};
