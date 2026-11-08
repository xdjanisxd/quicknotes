# Repository Guidelines

## Project Structure & Module Organization

This workspace currently contains no visible application source, tests, assets, or project manifests. The hidden `.agents/`, `.codex/`, `.aws/`, and `.git/` directories are environment-managed locations; do not use them for project content.

Keep new documentation in clearly named Markdown files. If the project grows, group related documents by topic and introduce source, test, or asset directories only when needed. Update this guide when a concrete layout is established.

## Build, Test, and Development Commands

No build system, package manager, development server, or automated test command is currently configured. Documentation can be edited directly without a build step.

- Run `rg --files --hidden -g '!.git/**'` to inspect available files.
- Once Git metadata is available, run `git diff --check` to detect whitespace errors and `git diff` to review changes before committing.

Document exact setup and validation commands here when tooling is introduced.

## Coding Style & Naming Conventions

Use descriptive filenames such as `project-overview.md`. Write Markdown with one top-level heading, logically ordered subheadings, fenced code blocks for commands, and relative links to repository files. Keep explanations concise and use consistent list formatting.

No language-specific formatter or linter is configured. When adding code, follow the conventions of the chosen language and document any required formatting tools.

## Testing Guidelines

No testing framework or coverage requirement is established. For documentation changes, verify command examples, links, spelling, and Markdown rendering. When executable code is introduced, add tests alongside the implementation and document their location and invocation.

## Commit & Pull Request Guidelines

Git history is unavailable in this workspace, so no existing commit convention can be confirmed. Use short, imperative commit subjects, for example `Add setup notes`.

Pull requests should explain the purpose, summarize changes, and state validation performed. Link related issues when applicable; include screenshots for changes whose rendered appearance matters.
