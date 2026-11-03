# Forma

*A quieter place to think, with AI that works for you.*

Forma is a minimal, local-first knowledge workspace for macOS. It pairs Notion-style editing and organization with plain Markdown files that you own, and adds optional AI help, from cloud providers or local models, that never changes your notes without your approval.

## Status

**Pre-release. In Phase 0 (architecture).** There is no application code yet. See [docs/PROGRESS.md](docs/PROGRESS.md).

## Principles

- **Your files, your folder.** Pages are Markdown files with YAML frontmatter, in a folder structure that mirrors your page tree. No account, no proprietary cloud, and it works offline.
- **Simple.** Nested pages, light databases ("collections"), links, backlinks and fast search, without the configuration overhead.
- **AI is optional and transparent.** Bring your own API key or run a local model with Ollama or LM Studio. Every AI change appears as a preview you approve, and you can undo it.

## Documentation

- [Architecture](docs/ARCHITECTURE.md)
- [Data model and file formats](docs/DATA_MODEL.md)
- [Decision records](docs/DECISIONS.md)
- [Implementation plan](docs/IMPLEMENTATION_PLAN.md)
- [Roadmap](docs/ROADMAP.md)

## License

Not yet chosen. See ADR-026 in [docs/DECISIONS.md](docs/DECISIONS.md).
