# Forma

*A quieter place to think, with AI that works for you.*

Forma is a minimal, local-first knowledge workspace for macOS. It pairs Notion-style editing and organization with plain Markdown files that you own, and adds optional AI help, from cloud providers or local models, that never changes your notes without your approval.

## Status

**Version 0.1.0, pre-release.** All seven phases of the v0.1.0 plan are implemented: the app shell; Markdown pages with a Notion-style editor, autosave, history, trash and outside-edit safety; collections, page links, backlinks and full-text search (including Chinese); an optional AI assistant (OpenAI, Anthropic, OpenAI-compatible servers, Ollama or LM Studio, with keys in the macOS Keychain) that answers questions about your notes with citations and, in Edit mode, proposes changes you review, apply all at once or not at all, and can undo; and export, backup, restore and diagnostics. AI is off by default.

The macOS build is a universal app (Apple silicon and Intel) for macOS 13 or later. It is **ad-hoc signed and not notarized**, so macOS asks you to allow it the first time; see [Installing](docs/USER_GUIDE.md#installing). Progress and measured performance: [docs/PROGRESS.md](docs/PROGRESS.md).

## Development

Requirements: macOS 13 or later, Xcode Command Line Tools, Rust (via rustup) and Node 24 or later.

```bash
npm install
npm run tauri:dev      # run the app with hot reload
npm run check          # typecheck, lint, and test both TypeScript and Rust
npx tauri build --target universal-apple-darwin --bundles app,dmg
                       # Forma.app and a DMG in target/universal-apple-darwin/release/bundle/
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for the rules, checks and release steps.

## Principles

- **Your files, your folder.** Pages are Markdown files with YAML frontmatter, in a folder structure that mirrors your page tree. No account, no proprietary cloud, and it works offline.
- **Simple.** Nested pages, light databases ("collections"), links, backlinks and fast search, without the configuration overhead.
- **AI is optional and transparent.** Bring your own API key or run a local model with Ollama or LM Studio. Every AI change appears as a preview you approve, and you can undo it.

## Documentation

- [User guide](docs/USER_GUIDE.md)
- [Contributing](CONTRIBUTING.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Data model and file formats](docs/DATA_MODEL.md)
- [Decision records](docs/DECISIONS.md)
- [Implementation plan](docs/IMPLEMENTATION_PLAN.md)
- [Roadmap](docs/ROADMAP.md)

## License

[Apache License 2.0](LICENSE). See ADR-026 in [docs/DECISIONS.md](docs/DECISIONS.md) for why.
