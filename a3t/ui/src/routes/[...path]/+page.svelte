<!--
  Catch-all so deep links are real routes.

  Navigation here is panel-based (`panels.svelte.ts` + history.pushState), and the
  layout's PanelStack renders everything — but SvelteKit's client router still has
  to be able to MATCH the URL. With only `/`, `/login` and `/signup` defined, any
  deep link threw at startup:

      Not found: /chat/session-1789321381701-jig5v6

  on a reload or a back/forward to `/chat/<session>`, `/resources/<category>` or
  `/flow-editor/<ns>~<name>`. This renders nothing, exactly like `+page.svelte`;
  its only job is to make those paths resolvable.
-->
