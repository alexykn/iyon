# Bundled providers

The Mock, OpenRouter, and OpenAI Codex providers are ordinary extension
packages. They register through the same `iyon.providers.register` contribution
used by third-party packages. The runtime must fail with an explicit
`no provider registered` error if their registrations are removed; it never
constructs a native Rust provider as a fallback.
