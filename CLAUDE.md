# Diaspar — Claude Code Development Rules

## Project Identity

Diaspar is a local-first, AI-native knowledge management application for macOS. It combines:

- Notion-inspired editing and organization
- Markdown-native local storage
- AI-assisted knowledge management

The initial release is v0.1.0. The repository folder is named `MinimumNotion`. The product name is **Diaspar**.

## Execution Transparency & Explainability

The user must always understand what you are doing, why you are doing it, and what has actually been accomplished.

Follow these rules throughout development:

1. Before Execution

Before starting any significant implementation task, briefly explain in Simplified Chinese:

* What: What you intend to implement or modify.
* Why: Why this work is necessary.
* How: Your proposed technical approach.
* Expected Result: What should work after completion.

Do not begin substantial implementation work without first communicating the plan.

2. During Execution

Provide concise progress updates in Simplified Chinese at meaningful milestones.

For each major action, explain:

* What has been completed.
* What is currently being worked on.
* Why the chosen approach is appropriate.
* Whether any unexpected problems have occurred.

When using subagents, explain their assigned responsibilities and report their results.

Do not overwhelm the user with raw logs or trivial implementation details.

3. After Execution

At the end of each significant task, provide a structured report in Simplified Chinese containing:

A. Summary
What was accomplished, explained in plain language.

B. Changes
Important files, components, and functionality added or modified.

C. Technical Rationale
Why the main implementation decisions were made.

D. Verification
What tests or checks were actually performed and their results.

E. Limitations
What remains incomplete, unverified, or potentially problematic.

F. Next Steps
What should happen next and whether user approval is required.

4. Explain Decisions, Not Just Actions

Do not merely state that a file was created, a command was executed, or a dependency was installed.

Explain the purpose and architectural significance of important changes.

For example:

Bad:
“Created context-manager.ts and installed dependencies.”

Good:
“Implemented the Context Manager to assemble relevant workspace information before each agent invocation. This allows different conversations and agents to share project knowledge without sending the entire conversation history to every model.”

5. Distinguish Facts from Assumptions

Clearly distinguish:

* Verified results.
* Architectural assumptions.
* Proposed solutions.
* Unresolved questions.
* Known limitations.

Never claim that a feature works unless it has been appropriately tested.

6. Maintain an Execution Journal

Maintain docs/PROGRESS.md as the persistent implementation journal. Don't create a separate status file, because two status documents drift apart.

After each significant task, update it with:

* Completed work.
* Important technical decisions.
* Verification results.
* Known issues.
* Current implementation phase.
* Next planned actions.

Keep this document concise, accurate, and useful for future sessions.

7. User Control

The user is the product owner and must retain control over major architectural and product decisions.

If a task requires changing previously approved requirements, explain the reason and obtain approval before proceeding.

The goal is not simply to complete tasks autonomously.

The goal is to make autonomous execution understandable, inspectable, and verifiable.

## Product Principles

1. Simplicity first.
2. Local-first by default.
3. Markdown-native documents.
4. AI is optional.
5. AI modifications require approval.
6. User data must remain portable.
7. Reliability is more important than feature count.

## Technology Stack

- Tauri 2, Rust (Cargo workspace: `crates/diaspar-core`, `crates/diaspar-ai`, `src-tauri`)
- React 19, TypeScript **6.0** (not 7; see ADR-016), Vite 8
- Tailwind CSS 4, shadcn/ui, Lucide, CSS design tokens
- Tiptap 3 / ProseMirror (`@tiptap/markdown`, gated by a round-trip spike; see ADR-015)
- SQLite / FTS5 (`rusqlite`, bundled), used as a cache only
- Markdown / YAML frontmatter
- Biome (lint and format), Vitest + Testing Library, `cargo test` / `cargo clippy`
- npm, Node 24 LTS

Verified versions: `docs/IMPLEMENTATION_PLAN.md` §2. Re-verify before adding any dependency.

## Architecture Rules

1. Markdown files (with frontmatter) and `.diaspar/*.json` are the durable representation.
2. SQLite is only an index and cache. It lives in Application Support, not in the workspace, and is always rebuildable.
3. All workspace file mutations go through `DocumentService` (`diaspar-core`).
4. UI components never modify workspace files directly. IPC arguments are page IDs, never paths.
5. AI tools never modify workspace files directly. They only build ChangeSets, and applying one is a user-only command.
6. Pages use stable UUIDs in frontmatter, persisted lazily. Run `ensure_persisted_id` before creating any durable reference to a page.
7. Revisions are content hashes. Every body write carries the base body revision. Metadata edits are surgical, applied to the current file, and snapshot outside content first (ADR-033).
8. Conflicting changes are never silently overwritten. When in doubt, snapshot to history first.
9. Core functionality works offline. The webview has no network access except loading `https:` images embedded in notes (ADR-031). Audio and video load only from the workspace (ADR-049). AI chat output and AI change previews never load remote images. All AI traffic goes through Rust.
10. No unnecessary cloud dependencies, telemetry, accounts or license checks.
11. The directory tree mirrors the page tree: `Name.md` for a leaf, `Name/index.md` for a bundle, attachments in `_assets/`.
12. Page links are `[Title](diaspar://page/<uuid>)`. Never rewrite other files on rename or move.
13. Frontmatter is edited surgically. Preserve unknown keys, comments and order.
14. Never write a file the user didn't change. Opening a page must not write it.
15. Never render raw HTML from documents or AI output. Keep the CSP strict. The one exception is an HTML embed block, shown only in a sandboxed frame with no scripts, no requests and no clicks, and never in AI chat or AI previews (ADR-049).

The normative formats are in `docs/DATA_MODEL.md`. Code that disagrees with it is a bug, unless DATA_MODEL and DECISIONS are updated in the same change.

## AI Rules

1. AI must remain optional; the app is complete without it.
2. Support cloud API providers (OpenAI, Anthropic, OpenAI-compatible endpoints) and local models (Ollama, LM Studio).
3. Use only officially supported authentication: API keys, or none for local servers. Consumer chat subscriptions don't include API access. Never extract sessions.
4. Store credentials in the macOS Keychain only. Keys never reach the webview, logs or the workspace.
5. AI writes require explicit approval of a visible preview.
6. AI modifications must be reversible, with undo backed by the operation log and snapshots.
7. Restrict AI tools to the selected workspace: IDs only, no paths, no shell, no network.
8. Treat retrieved documents and model output as untrusted input.
9. Don't expose unrestricted filesystem or shell access.
10. Citations must refer to sources that were actually provided. Never fabricate.
11. Clearly indicate when data is sent to a cloud provider.

## Design Rules

1. Follow a minimalist, Notion-inspired design. Don't copy Notion's branding or assets.
2. Prioritize typography and whitespace.
3. Avoid decorative dashboards, heavy shadows, glassmorphism, loud gradients and bright accents.
4. Avoid unnecessary UI complexity and settings.
5. Use consistent design tokens (`src/styles/tokens.css`), with intentionally designed light and dark themes.
6. Keep the AI panel optional and collapsible. It is hidden by default.
7. Preserve keyboard-first workflows, and don't override standard macOS shortcuts.
8. Every control must work. If a feature isn't built yet, show an honest empty state, not a fake control.

## Development Rules

1. Work one phase at a time.
2. Don't begin the next phase without user approval ("Proceed to Phase N").
3. Plan before implementation.
4. Test after implementation.
5. Review code before reporting completion.
6. Fix critical issues before closing a phase.
7. Never claim tests passed unless they were executed. Report exact commands and results.
8. Never present placeholder functionality as complete.
9. Preserve completed functionality.
10. Update documentation after each phase.
11. Prefer a smaller complete feature over a larger incomplete one.
12. Avoid unnecessary dependencies and abstractions. No empty directories or speculative modules.

## Phase Completion Protocol

For every phase: Plan → Implement → Test → Review → Report → Stop.

The report covers: phase summary, implemented features, architecture decisions, files changed, verification (exact commands and results), manual testing steps, known issues, next phase, and a status of COMPLETE, INCOMPLETE or BLOCKED.

Wait for explicit approval before continuing.

## Session Startup Protocol

At the beginning of each session:

1. Read CLAUDE.md.
2. Read docs/PROGRESS.md.
3. Read docs/ARCHITECTURE.md.
4. Read docs/DECISIONS.md.
5. Inspect Git status.
6. Identify the approved active phase.
7. Continue only that phase.

If the documentation and the repository disagree, inspect the code and tests before proceeding. Never assume a phase is complete just because the documentation says so.

## Git Rules

1. Make descriptive commits (for example `feat: implement local document editor`).
2. Never commit credentials, API keys or local secrets.
3. Never commit private workspace data. Test workspaces live in temporary directories or in the gitignored `sandbox-workspaces/`.
4. Don't push without explicit authorization.
5. Don't rewrite history without approval.
6. Keep clean phase boundaries, with one milestone commit per completed phase.

## Out of Scope for v0.1.0

- Cloud sync
- Multiplayer collaboration
- Obsidian import
- Mobile apps
- Browser app
- Teamspaces
- Complex Notion databases (formulas, rollups, relations, kanban, calendar, timeline, charts, automations). Board and gallery views are planned for v0.2.0 (Phase 10); formulas, rollups, relations, calendar, timeline, charts and automations stay out of scope there too.
- Graph view
- Plugin marketplace
- Billing
- Accounts
- Telemetry

## Documentation

- `docs/ARCHITECTURE.md`: system design, the Document Service, security and the risk register
- `docs/DATA_MODEL.md`: the normative on-disk formats and Markdown dialect
- `docs/DECISIONS.md`: ADRs. Don't change a foundational decision without a superseding ADR
- `docs/IMPLEMENTATION_PLAN.md`: per-phase tasks, tests and acceptance criteria
- `docs/PROGRESS.md`: current status and the implementation journal, updated after every significant task
- `docs/ROADMAP.md`: phases, out-of-scope list and later candidates
- `docs/IMPLEMENTATION_AUDIT.md`: the 2026-11-02 audit of Phases 0–1, open findings, and the resolution log

Use CLAUDE.md for permanent rules only, never as a development log.

## Priority

Correctness > Data Integrity > Simplicity > Maintainability > Performance > Feature Count
