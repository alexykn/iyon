# ARCHITECTURE — Iyon application repository

This repository owns the Iyon application, protocol, and kernel.
It does not own the generic terminal UI framework.

Application code consumes the TUI through public APIs only:

```text
Iyon plugins / runtime orchestration
        |
        +------> iyon-api / iyon-core / iyon-core-native
        |
        `------> @iyon/tui (optional application-owned `iyon:tui` alias)
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

- Use only public `@iyon/tui` APIs. The application-owned `iyon:tui` alias may
  re-export that package for bundler compatibility; never deep-import TUI internals.
- Do not import generated View ABI modules, retained-DAG internals, NodeId,
  NativeRef, native component registries, or TUI addon implementation modules.
- Product behavior stays here. Do not modify the TUI framework to encode agent,
  provider, model, prompt, tool, approval, transcript, queue, or product-status
  semantics.
- The application native addon contains only core/API/credential handles. TUI
  handles and View ABI operations belong to the external addon.

## Exact external pins

- Rust consumes `alexykn/iyon-tui` revision
  `e322f10dff490c1423d988982c0782c22774f85d`.
- TypeScript consumes the same revision as `@iyon/tui`.
- The TUI addon is staged through the package-owned
  `iyon-tui-native-stage` command; the core addon is staged separately as
  `iyon-core-native.node`.
- No local TUI source, ABI generator, or TUI native contract remains here.

Run local boundary checks with:

```sh
bun run check:ownership
```
