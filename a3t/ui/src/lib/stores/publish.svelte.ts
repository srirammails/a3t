// The Publish dialog's state: which item is being published. One dialog serves
// every app (mounted in the layout); items open it with `openPublish`.

export type Publishable = {
  kind: 'flow' | 'resource' | 'trigger' | 'template';
  /// The project it is in.
  from: string;
  /// Flow / resource name, or a trigger's id.
  ref: string;
  label: string;
  /// A template's spec (published by creating a copy in the target project).
  template?: { name: string; version: string; spec_yaml: string; tags?: string; vertical?: string };
};

let current = $state<Publishable | null>(null);

export function openPublish(item: Publishable) { current = item; }
export function closePublish() { current = null; }
export function publishing(): Publishable | null { return current; }
