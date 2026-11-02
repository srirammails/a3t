/// What the Studio needs from where it runs. The web dash and the VS Code
/// extension each implement this once; nothing else in studio-core or
/// studio-ui knows which host it is in.
export interface StudioHost {
  /// Small per-user state (open tabs, layout): web localStorage, VS Code workspace state.
  storage: { get(key: string): unknown; set(key: string, value: unknown): void };
  confirm(message: string): Promise<boolean>;
  clipboard(text: string): Promise<void>;
  /// Show a Console section page (e.g. `resources`, `triggers`).
  openConsole(section: string, title: string): void;
}
