# ARCHITECTURE — Iyon application repository

This repository owns the Iyon application, protocol, and kernel.
It does not own the generic terminal UI framework.

Application code consumes the TUI through public APIs only:

```text
Iyon plugins / runtime orchestration
        |
        +------> iyon-api / iyon-core / iyon-core-native
        |
        `------> @iyon/tui (currently @iyon/runtime/tui / iyon:tui during migration)
```

## Ownership

This repository owns:

- `iyon-api` protocol and schema types;
- `iyon-core` kernel behavior;
- the application native binding for core/API/credentials;
- runtime orchestration, providers, agents, tools, and approvals;
- Iyon state reduction, scenes, tool cards, composer/footer/spinner policy;
- CLI, SDK, plugins, packaging, and product tests.

The external `alexykn/iyon-tui` repository owns generic terminal mechanics,
semantic Views, History, streams, projections, layout, rendering, themes,
controls, its native addon, and its public TypeScript facade.

## Boundary rules

- Use only public `@iyon/tui` APIs. During S2–S4 compatibility, use the public
  `@iyon/runtime/tui` or `iyon:tui` entrypoint; never deep-import TUI internals.
- Do not import generated View ABI modules, retained-DAG internals, NodeId,
  NativeRef, native component registries, or TUI addon implementation modules.
- Product behavior stays here. Do not modify the TUI framework to encode agent,
  provider, model, prompt, tool, approval, transcript, queue, or product-status
  semantics.
- The application native addon must contain no TUI handles after S5.

## Temporary S2 compatibility

The extraction intentionally retains local TUI compatibility paths so the
application checkout remains behavior-neutral and testable until the public
package/addon split lands. Every retained path and removal tranche is listed in
`APPLICATION-EXTRACTION-COMPATIBILITY.md`. They are not shared ownership.

Run local boundary checks with:

```sh
bun run check:ownership
```
