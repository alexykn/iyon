# AGENTS.md

## Defaults
- Be brief and concise in your replies.
- Preserve existing behavior unless the task explicitly requires a change.
- Keep changes tightly scoped and aligned with the existing architecture.
- Do not modify unrelated files.
- Do not edit `AGENTS.md` unless explicitly asked.

## Ask First
- Adding or changing third-party dependencies.
- Changes to public APIs.
- Changes spanning multiple files.

## Implementation
- Prefer the simplest solution that fits the existing design.
- Avoid unnecessary refactors, abstractions, or formatting-only changes.
- Do not silently swallow exceptions.

## Control Flow & Structure
- Prefer flat control flow. Avoid unnecessary nesting; if logic gets too deep, extract a helper or return early.
- Keep behavior local unless clearly visible friction points or boundaries show up.
- Use sensible function and module boundaries so related logic stays together, do not create oversized functions or files.

## Verification
- Run the smallest relevant checks for the files you changed before finishing.
- Run broader test/lint suites only if explicitly requested or if the change clearly warrants it.
- Do not add new tests unless explicitly asked.

## Python
- Use `uv` for environment and dependency management.
- Work inside `.venv`; create it with `uv venv` if missing.
- Sync dependencies with `uv sync`.
- Use `pyproject.toml` for configuration.
- Lint/fix with `uv run ruff check --fix`.
- Format with `uv run ruff format`.
- Type-check with `uv run ty check`.
- Use `pytest` for tests.
- Prefer Python 3.12+ features and native type annotations.
- Prefer `@dataclass` or Pydantic for structured data over long parameter lists.
- Prefer `ABC` over `Protocol` unless structural typing is specifically needed.
