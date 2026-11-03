# Forma Roadmap

## v0.1.0: the first usable release (macOS)

| Phase | Name | Outcome | Status |
|---|---|---|---|
| 0 | Architecture and Project Planning | Architecture, data model, decisions, plan, development rules | Complete |
| 1 | macOS Application Foundation | A polished three-panel app shell with themes, shortcuts and a settings foundation | Complete |
| 2 | Document System and Editor | Reliable local Markdown pages: nesting, autosave, history, trash, external-change safety | Complete |
| 3 | Collections, Links, and Search | Simple databases (list and table views), ID-stable page links, backlinks, FTS5 search | Implemented; awaiting approval |
| 4 | AI Provider Integration | OpenAI, Anthropic, OpenAI-compatible endpoints, Ollama and LM Studio; Keychain credentials; streaming chat | Not started |
| 5 | AI Knowledge Retrieval | Read-only Q&A, summaries and cited answers over the user's notes | Not started |
| 6 | AI Agent and Knowledge Management | Proposed changes with diff preview, approval, conflict checks and undo | Not started |
| 7 | Product Polish and Release | Accessibility, performance, backup and export, CI, DMG packaging | Not started |

Details: [IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md). Live status: [PROGRESS.md](PROGRESS.md).

## Explicitly out of scope for v0.1.0

These are not implemented in v0.1.0 unless a future phase explicitly asks for them:

- Obsidian import
- Cloud sync
- Multiplayer collaboration and teamspaces
- User accounts and subscription billing
- Mobile apps, a browser app, Windows and Linux builds
- Calendar, kanban, timeline and chart views
- Formula and rollup properties, complex relations
- Workflow automations
- Graph visualization
- A plugin marketplace
- Hosted AI infrastructure
- Telemetry and license checks

## Candidates after v0.1.0 (unordered, not committed)

These ideas follow naturally from the v0.1 architecture. Each still needs its own planning and approval.

- **`forma://` deep links.** Register the URL scheme so page links open Forma from other apps (builds on ADR-007).
- **Semantic retrieval.** An optional local embedding model behind the existing `Retriever` trait, combined with BM25 into hybrid search.
- **Multiple workspaces** and a workspace switcher. The index is already keyed per workspace.
- **Tabs or split view** for having more than one page open.
- **Lazy loading of remote images** with an explicit "Load" action per image.
- **Code block syntax highlighting.**
- **More collection features.** Membership in several collections, more views, and relations, added only if they keep things simple.
- **Windows and Linux builds.** `forma-core` and `forma-ai` contain no macOS-specific code. The filename rules already avoid characters Windows reserves.
- **Managed sync and collaboration**, the planned commercial offering. These build on stable IDs, the operation log and history (ARCHITECTURE §13).
- **TypeScript 7** once its ecosystem catches up (ADR-016).
