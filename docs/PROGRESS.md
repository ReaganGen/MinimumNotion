# Forma Development Progress

## Current Phase

Phase 0: Architecture and Project Planning

## Status

Deliverables complete. **Awaiting user approval** to proceed to Phase 1.

## Completed Phases

None. Phase 0 counts as complete only after the user approves it.

## Implemented Features

No application code yet. That is intentional in Phase 0.

Phase 0 deliverables:

- `CLAUDE.md`: durable development rules
- `docs/ARCHITECTURE.md`: system design, the Document Service, index, editor pipeline, AI architecture, security model, collaboration extension points and the risk register (R1–R16)
- `docs/DATA_MODEL.md`: normative on-disk formats (layout, filenames, frontmatter, Markdown dialect, collections, `.forma/` files, the index schema and the API surface)
- `docs/DECISIONS.md`: ADR-001 to ADR-028. ADR-026 (license) is **Proposed**
- `docs/IMPLEMENTATION_PLAN.md`: toolchain prerequisites, verified dependency versions, and Phases 1–7 with tasks, tests and acceptance criteria
- `docs/ROADMAP.md`
- `README.md`, `.gitignore`, and the git repository with its initial commit

## Tests

No code exists, so no automated tests apply in Phase 0. Verification done in this phase:

- Dependency versions and peer compatibility were checked against the npm registry and crates.io on 2026-11-02 (IMPLEMENTATION_PLAN §2). This found two compatibility limits that shaped decisions:
  - `typescript-eslint` doesn't support TypeScript 7, which led to TypeScript 6.0 with Biome (ADR-016).
  - `@tiptap/extension-mathematics` accepts KaTeX ≤ 0.18, so KaTeX is pinned to 0.18.x (ADR-015).
- Confirmed that the bundled build of `libsqlite3-sys` enables `SQLITE_ENABLE_FTS5` (from its `build.rs`). This will be confirmed again with a test in Phase 3.
- Inventoried the toolchain on the development machine.

## Known Issues and Open Decisions

1. **The toolchain is missing.** Node.js and Rust aren't installed, so Phase 1 is blocked until they are (IMPLEMENTATION_PLAN §1). Xcode Command Line Tools are present.
2. **License (ADR-026) needs approval.** Recommended: Apache-2.0 plus a trademark policy. The alternative is AGPL-3.0 with a CLA. No `LICENSE` file has been created.
3. **The bundle identifier needs confirmation before Phase 1.** Proposed: `app.forma.desktop`. It is hard to change later because it determines the Keychain service name and the app-data path.
4. **Apple Developer ID.** Needed for signing and notarization in Phase 7. Without it, builds are ad-hoc signed and Gatekeeper warns users.
5. **The name "Forma"** may collide with existing products (for example Autodesk Forma). A trademark search is recommended before public release (risk R15).
6. Phase 2 opens with a decision gate on Markdown round-trip fidelity (ADR-015, risk R1).

## Next Phase

**Phase 1: macOS Application Foundation.** Scaffold Tauri 2, React and Vite. Add design tokens, the three-panel resizable layout, light, dark and system themes, the macOS window, menus and shortcuts, and the settings foundation. No persistence of documents and no AI. See IMPLEMENTATION_PLAN §4.
