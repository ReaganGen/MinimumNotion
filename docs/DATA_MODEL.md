# Forma Data Model

Status: Phase 0 baseline (2026-11-02). This is the normative specification of Forma's durable formats. Code that reads or writes workspace files must follow it. When a phase changes a rule, update this file in the same change and record why in [DECISIONS.md](DECISIONS.md).

Terminology: **durable** means stored in the workspace folder and part of the user's data. **Derived** means computed from durable data, either cached in the index or computed on demand. **Advisory** means durable but non-essential: if it is lost, Forma falls back to a sensible default and no content is lost.

---

## 1. Entities

| Entity | Identity | Authoritative storage | Notes |
|---|---|---|---|
| Workspace | UUID | `.forma/workspace.json` | One active workspace per app session in v0.1 |
| Page | UUID | A Markdown file (`*.md` or `<dir>/index.md`) | See §3–§4 |
| Collection | The UUID of its collection page | `collection:` block in the collection page's frontmatter | A collection *is* a page (§6) |
| Collection Property | Short ID, unique within its collection | The collection schema | Values live in entry pages' frontmatter |
| Page Link | (source page, ordinal) | Source page body Markdown | Derived into the index |
| Attachment | (page, relative name) | File in the page's `_assets/` folder | Never stored in SQLite |
| Document Revision | Content hash (current) / snapshot file (past) | Current: derived. Past: `.forma/history/` | §8 |
| Trash Entry | UUID | `.forma/trash/<entry-id>/` | §7.4 |
| Operation (AI) | UUID | `.forma/operations/<op-id>.json` | For undo and history (§7.5) |

### 1.1 Page fields

| Field (spec) | Where it lives | Kind |
|---|---|---|
| Stable UUID | frontmatter `id` | durable (lazily persisted, §4.4) |
| Title | frontmatter `title` (fallback: filename stem) | durable |
| Markdown content | file body after frontmatter | durable |
| Creation timestamp | frontmatter `created_at` (fallback: file birth time) | durable |
| Modification timestamp | frontmatter `updated_at` (fallback: file mtime) | durable |
| Parent page reference | the directory that contains the page | durable, **derived from location** and never stored twice |
| Revision identifier | BLAKE3 of the file bytes | derived |
| Favorite status | `.forma/tree.json` → `favorites` | advisory |
| Sibling order | `.forma/tree.json` → `order` | advisory |
| Optional icon | frontmatter `icon` | durable |
| Tags | frontmatter `tags` | durable |
| Optional Collection membership | the page's parent is a collection page | durable, derived from location |
| Collection property values | top-level frontmatter keys named after the properties | durable |

---

## 2. Workspace directory layout

```
<Workspace root>/                      the folder the user selected
├── <Page>.md                          leaf page
├── <Page>/                            bundle page (has children and/or attachments)
│   ├── index.md                       the bundle page's own file
│   ├── _assets/                       its attachments
│   ├── <Child>.md
│   └── <Child>/index.md …
├── <Folder>/                          folder page: a directory without index.md (§3.2)
└── .forma/
    ├── workspace.json                 §7.1
    ├── tree.json                      §7.2  (advisory)
    ├── views.json                     §6.5  (advisory)
    ├── history/<page-id>/<ts>_<rev>_<reason>.md      §8.2
    ├── trash/<entry-id>/{entry.json, <Page>.md | <Page>/}   §7.4
    ├── operations/<op-id>.json        §7.5
    ├── journal/<journal-id>.json      §7.6
    ├── recovery/<page-id>.md + .json  §7.7
    ├── tmp/                           atomic-write staging; emptied on start
    └── lock                           advisory flock target
```

Outside the workspace, in `~/Library/Application Support/<bundle-id>/`: `settings.json` (§11), `index/<workspace-key>/index.db` (§9) and `logs/`. Secrets live in the Keychain (§11.2).

The workspace root is the page-tree root. It is not itself a page. Opening any existing folder makes it a workspace. Forma creates `.forma/` and writes nothing else until the user acts (§4.4).

---

## 3. Page files

### 3.1 Page kinds

| Kind | On disk | Title fallback | Created by Forma when |
|---|---|---|---|
| leaf | `<stem>.md` | stem | New page with no children and no attachments |
| bundle | `<stem>/index.md` (+ `_assets/`, child pages) | directory name | A leaf gains its first child or attachment (converted by `mkdir` + `rename`, ARCHITECTURE §3.1) |
| folder | `<stem>/` with no `index.md` | directory name | Never. These are created externally. Forma creates `index.md` the first time it has to write such a page |

- A bundle stays a bundle after its last child or attachment is removed. Forma never converts it back automatically, which avoids churn.
- Forma creates the extension `.md`. When reading, `.md` and `.markdown` are accepted case-insensitively.
- An `index.md` directly in the workspace root is an ordinary leaf page.
- **Relative references during conversion.** A leaf's body may contain hand-written relative references, meaning anything other than `forma:` links, absolute URLs and `_assets/`. Moving the file one level deeper would break them, so the conversion prefixes each with `../` using the parser's source offsets. Because this variant also rewrites the body, it records a `bundle_conversion` journal intent first (§7.6).
- **Limitation (v0.1):** when a page is *moved*, hand-written relative references to files outside its own `_assets/` aren't rewritten and may break. Forma shows them as broken references. Forma's own references (`forma://` links and `_assets/`) never break.

### 3.2 Discovery rules

When scanning (or reconciling) a directory:

1. Skip entries whose names start with `.`, and skip all symlinks. Symlinks are never followed.
2. Skip `_assets/` inside a bundle. That directory holds attachments, not pages.
3. A Markdown file other than a bundle's own `index.md` is a leaf page.
4. A directory is a bundle if it contains `index.md`, and a folder page otherwise.
5. Other files are attachments or unmanaged files. They are ignored by the page tree and always preserved.
6. If `Foo.md` and a directory `Foo/` sit side by side (for example, from another tool's export), both are treated as separate pages. Forma never creates this situation itself.

### 3.3 Filenames

The file stem is derived from the title. It is human-friendly and **not authoritative**.

1. Apply Unicode NFC normalization. Replace newlines and tabs with a space, and collapse runs of whitespace.
2. Replace `/ \ : * ? " < > |` and control characters with `-`. This keeps names portable to a future Windows build.
3. Trim leading and trailing spaces and dots. Strip leading `.` so the file can't become hidden.
4. Truncate to at most **100 bytes** of UTF-8, cutting at a grapheme boundary.
5. If the result is empty, use `Untitled`. If it equals a reserved name (`index`, `_assets`, `.forma`, compared case-insensitively), append ` 1`.
6. Uniqueness among siblings is **case-insensitive and normalization-insensitive**, matching APFS defaults. `Name.md`, `Name/` and any case variant claim the same slot. On a collision, append ` 2`, ` 3` and so on.

Renaming a page in Forma updates `title` and then renames the file or directory, deferred while the user is typing (ARCHITECTURE §6.4). If a user renames a file in Finder, Forma keeps the frontmatter `title` and doesn't rename the file back.

---

## 4. Frontmatter

### 4.1 Syntax

- Frontmatter is present only if the file begins with `---` followed by a newline (after an optional UTF-8 BOM, which is stripped). It ends at the next line that is exactly `---` or `...`.
- The content must be YAML 1.2 whose top level is a **mapping**. Anything else, including duplicate keys, is *invalid frontmatter* (§4.5).
- A file without frontmatter is valid. All metadata then comes from fallbacks.

### 4.2 Reserved keys

| Key | Type | Written by Forma | Read rules |
|---|---|---|---|
| `id` | UUID string | Always, once persisted | Must parse as a UUID. If not, the page is invalid (§4.5) |
| `title` | string | Always | Missing → fallback (§1.1). Newlines become spaces |
| `icon` | string (one emoji grapheme in v0.1) | When set | Unrecognized value → ignored for display and preserved |
| `tags` | list of strings | When non-empty | A scalar string counts as one tag. Duplicates (case-insensitive) are collapsed for display |
| `created_at` | RFC 3339 UTC timestamp | Always | Date-only or other RFC 3339 forms are accepted |
| `updated_at` | RFC 3339 UTC timestamp | On every content or metadata write | As above |
| `collection` | mapping (§6.2) | Collection pages only | Invalid schema → the collection opens read-only with an error |

Reserved keys are matched case-sensitively when reading. Collection property names may not equal any reserved key, compared case-insensitively. The key `forma` is also reserved, for future namespaced metadata.

All other keys are **user keys**. Forma preserves them byte for byte (§4.6). They may hold collection property values (§6.3).

### 4.3 Example (Forma-created leaf page)

```markdown
---
id: 550e8400-e29b-41d4-a716-446655440000
title: Reinforcement Learning
icon: "🧠"
tags:
  - ai
  - learning
created_at: 2026-11-02T10:00:00Z
updated_at: 2026-11-02T10:30:00Z
---
Content goes here.
```

The title isn't repeated as an `# H1` in the body. That would make two authoritative copies of the title, so the editor shows `title` as the page heading instead. An H1 the user writes in the body is ordinary content.

For newly created files, Forma writes keys in this order: `id, title, icon, tags, created_at, updated_at`, then property values in schema order, then `collection`. In existing files the order is preserved.

### 4.4 ID persistence and duplicates

- Index-only IDs are assigned to pages discovered without an `id`.
- **Invariant:** before Forma creates any durable reference to a page, it calls `ensure_persisted_id(page)`. Durable references are links, favorites, order entries, history snapshots, operation records and journal steps. Trash entries aren't on this list: they record IDs for information only and are restored by path, so trashing a large external folder writes nothing into its files. That call writes `id` into the file using the surgical edit described in §4.6.
- On duplicate `id`s, the original keeps the ID: that is the file at the path the index already associates with the ID, or failing that the earliest birth time, then the first path in order. The other file gets a new UUID and the user is notified. Both files are kept.

### 4.5 Invalid frontmatter

If the frontmatter can't be parsed, isn't a mapping or has an invalid `id`:

- The page appears in the tree with a warning badge and opens **read-only**, showing the parse error with line and column. It offers *Open in default editor* and *Reveal in Finder*.
- Forma never writes to the file while it is invalid. After the file is fixed externally, the watcher reconciles it and the page becomes editable again.

### 4.6 Surgical frontmatter editing

Forma never re-serializes the whole frontmatter block. To change keys, it:

1. Parses the block with a span-aware YAML 1.2 parser to find the byte range of each **top-level** key and its value. Each range runs from the key's line to the start of the next top-level key or the end of the block.
2. Replaces only the ranges of the keys being changed with freshly emitted YAML (§4.7). New keys are appended at the end of the block in the order given in §4.3. Removed keys have their range deleted.
3. Leaves every other byte untouched, including comments, user keys, key order, anchors, flow style and quoting.

If the frontmatter uses a construct that prevents safe span edits, Forma treats the page's metadata as read-only. Such constructs include a top-level flow mapping, or a key that Forma must change being defined through an alias or merge key. Body editing still works in that case. The body is replaced and the frontmatter bytes are kept verbatim, without updating `updated_at`.

### 4.7 YAML emission rules (values Forma writes)

- **Strings:** use plain style only if the value would be read back as the same string under *both* YAML 1.2 core and YAML 1.1. Use double quotes, with escapes, when the string:
  - is empty
  - has leading or trailing whitespace
  - looks like a boolean or null in any case (`true`, `false`, `yes`, `no`, `on`, `off`, `y`, `n`, `null`, `~`)
  - looks like a number (including octal, hex and sexagesimal forms) or a date or timestamp
  - starts with an indicator character (`` - ? : , [ ] { } # & * ! | > ' " % @ ` ``)
  - contains `: ` or ` #`
  - contains a newline, a control character or an emoji (emoji are quoted for safety with older parsers)
- **Lists:** block style, `  - item`, indented 2 spaces under the key. An empty list removes the key.
- **Booleans:** `true` or `false`. **Numbers:** shortest round-trip decimal. Non-finite numbers are never written.
- **Dates:** a date-only value is written unquoted, as `2026-11-02`. A timestamp is written unquoted in UTC with seconds, as `2026-11-02T10:30:00Z`.
- Mappings (only `collection`) use block style.

---

## 5. Markdown dialect

### 5.1 Base

The base dialect is **CommonMark** with these **GitHub Flavored Markdown** extensions: tables, task list items, strikethrough and autolinks. On top of that come Forma's own conventions: math, page links (§5.4) and read support for wikilinks.

The fixture corpus in `tests/fixtures/markdown/` is the executable definition of this section. It is created in Phase 2, and where a rule here is ambiguous the corpus decides. Any change to a rule must come with a corpus change.

### 5.2 Constructs, accepted input, canonical output

| Construct | Accepted on read | Written as |
|---|---|---|
| Paragraph | — | Text. Blocks separated by one blank line |
| Heading | ATX `#`–`######`, Setext | ATX. Levels 1–6 are kept. The UI offers H1–H3 |
| Bold / italic / strike | `**` `__` / `*` `_` / `~~` `~` | `**x**` / `*x*` / `~~x~~` |
| Inline code | any backtick fence | The shortest backtick fence that doesn't clash with the content |
| Link | inline, reference-style, `<autolink>`, GFM bare URLs | Inline `[text](url "title")`. Bare and angle-bracket autolinks keep their original form when text equals URL |
| Image | inline, reference-style | `![alt](src "title")` |
| Bullet list | `-` `*` `+` | `-` |
| Ordered list | `1.` `1)` | `1.` style, start number preserved, then sequential numbering |
| Task item | `- [ ]`, `- [x]`, `- [X]` | `- [ ]` / `- [x]` |
| Nested list | CommonMark indentation | Content-aligned (2 spaces under `-`, 3 under `1.`) |
| Blockquote | `>` | `> ` |
| Code block | fenced (`` ``` `` or `~~~`), indented | `` ``` `` fence with info string. The fence is lengthened if the content contains one |
| Horizontal rule | `---` `***` `___` | `---` |
| Table | GFM pipe table with alignment row | GFM pipe table, alignment preserved, columns padded to equal width with the alignment row matching. Cells hold inline content only. `|` is escaped. A hard break in a cell is written as `<br>` |
| Hard line break | two trailing spaces, `\` | `\` at end of line |
| Inline math | `$…$` (opening `$` not followed by a space, closing `$` not preceded by a space or followed by a digit) | `$…$` |
| Block math | `$$` on its own line … `$$` | `$$\n…\n$$` |
| Page link | `[label](forma://page/<uuid>)` | Same, with the label set to the target's current title |
| Wikilink | `[[Title]]`, `[[Title\|alias]]`, `[[Folder/Title]]` | **Kept exactly as written** |
| Raw HTML (block or inline) | anything | **Kept exactly as written.** Displayed as text, never rendered |
| Footnotes, definition lists, other syntax | anything | **Kept exactly as written** in a raw block or as literal inline text |

### 5.3 Normalization on write

These rules apply only when Forma writes a body after an actual edit (ARCHITECTURE §6.3):

- LF line endings, a single trailing newline, UTF-8 without a BOM.
- **Minimal escaping.** A character is escaped only if leaving it unescaped would change how that position parses. For example, a literal `$` is escaped only where it would otherwise open math, and a leading `#` only where it would otherwise start a heading.
- Body text isn't Unicode-normalized. Only filenames are.
- Reference-style links and images are written in inline form. The link stays the same; only its syntax changes, and the open-time fidelity notice reports it.

### 5.4 Links and references

- **Page link URL scheme:** `forma://page/<uuid>`, optionally followed by `#<heading-slug>`. Forma resolves the UUID through the index. An unknown UUID renders as a broken link and is kept unchanged.
- **Wikilink resolution (read-only):** `[[Folder/Title]]` is matched against path suffixes, and `[[Title]]` against titles, falling back to filename stems. Exactly one match is a resolved link. Zero or several matches show as an unresolved link with a hint. The index stores what it resolved; the text in the file is never changed.
- **Attachment and image sources:**
  - A relative path resolves against the directory of the page's file. Forma writes `_assets/<name>` for its own attachments.
  - A path that resolves outside the workspace isn't loaded.
  - `https:` image URLs are loaded automatically. `http:` image URLs are kept but not loaded (ADR-031, ARCHITECTURE §6.6).
  - `data:` image URLs are kept and not rendered in v0.1.
- **Allowed link schemes when activated:** `forma:`, `http:`, `https:`, `mailto:`. Any other scheme is shown but not opened.

---

## 6. Collections

### 6.1 Concept

A collection is a page (always a bundle) whose frontmatter contains a `collection:` schema. Its **direct child pages are its entries**. Membership comes from location: moving a page out of the collection ends its membership. Its property values stay in its frontmatter as inert user keys and become active again if it moves back. A page therefore belongs to at most one collection in v0.1.

Supported property types: `text`, `number`, `date`, `checkbox`, `select`, `multi_select`. Views: `list` and `table`. Excluded (ROADMAP): formulas, rollups, relations, kanban, calendar, timeline, charts and automations.

Future extension: explicit membership (for example a `collections:` key) can be added without migrating existing data, because location-based membership stays valid.

### 6.2 Schema (in the collection page's frontmatter)

```yaml
collection:
  version: 1
  properties:
    - id: k7q2m9xd
      name: Status
      type: select
      options:
        - id: r4t8
          name: To read
          color: gray
        - id: p2n6
          name: Reading
          color: blue
    - id: x3v8b1cw
      name: Year
      type: number
    - id: m5h2j7ka
      name: Authors
      type: multi_select
      options: []
    - id: d9f4s2le
      name: Published
      type: date
    - id: c1z6n3ru
      name: Read
      type: checkbox
```

Validation rules:

- `version` must be `1`. A higher version opens the collection read-only, marked "needs a newer Forma".
- Property `id`: 8 characters, `[a-z0-9]`, unique within the collection. Option `id`: 4 or more characters, unique within the property. IDs are generated randomly and never reused.
- Property `name`: 1–64 characters after trimming, unique case-insensitively, and not a reserved key (§4.2).
- `color` is one of `gray, brown, orange, yellow, green, blue, purple, pink, red`, mapped to theme tokens.
- Built-in pseudo-properties, `title`, `created_at` and `updated_at`, can be sorted and filtered but aren't declared in the schema.

### 6.3 Entry values (top-level keys in the entry's frontmatter)

```yaml
---
id: 0c6a5f3e-6c1d-4b8e-9a63-2f1d1b0a9e77
title: Attention Is All You Need
created_at: 2026-11-02T10:00:00Z
updated_at: 2026-11-02T10:05:00Z
Status: Reading
Year: 2017
Authors:
  - Vaswani
  - Shazeer
Published: 2017-06-12
Read: false
---
```

| Type | Written as | Also accepted on read | Invalid value handling |
|---|---|---|---|
| text | string (quoted per §4.7) | numbers and bools (shown as text) | — |
| number | YAML int or float | numeric strings, used for sorting and flagged | non-numeric → shown with a warning and preserved |
| date | `YYYY-MM-DD` | RFC 3339 timestamps (the date part is shown, the original kept until edited) | unparseable → warning, preserved |
| checkbox | `true` / `false` | — | anything else → warning, preserved |
| select | option **name** | — | unknown name → "unknown option" chip that can be added to the schema |
| multi_select | list of option names | a single scalar becomes a one-item list | unknown names → as above |

- An empty value is written as an **absent key**, never as `null`.
- An invalid value is never rewritten unless the user edits that cell.
- Values are keyed by property *name* so the files stay readable, and other tools that read top-level frontmatter can use them. The cost is that renames rewrite entries (§6.4).

### 6.4 Schema changes

| Change | Effect on entry files |
|---|---|
| Add property | None |
| Remove property | **None.** Existing values become inert user keys (non-destructive) and become active again if a property with the same name is added back |
| Rename property | Batch rewrite of the key in every entry that has it. Journaled, with protected snapshots (§7.6) |
| Change property type | Schema only. Existing values are re-interpreted under the new type. Values that don't fit are flagged and never converted automatically |
| Add or rename option | Rename means a batch rewrite of the values (journaled) |
| Remove option | Schema only. Values become "unknown option" |

### 6.5 Views (`.forma/views.json`, advisory)

```json
{
  "format_version": 1,
  "collections": {
    "<collection-page-id>": {
      "active_view": "table",
      "views": {
        "table": {
          "sort":   [{ "property": "x3v8b1cw", "direction": "desc" }],
          "filter": [{ "property": "k7q2m9xd", "op": "is", "value": "Reading" }],
          "hidden": ["c1z6n3ru"],
          "column_widths": { "title": 280, "k7q2m9xd": 140 }
        },
        "list": { "sort": [], "filter": [] }
      }
    }
  }
}
```

The filter operators, combined with AND only in v0.1, are:

| Property type | Operators |
|---|---|
| text and title | `contains`, `not_contains`, `empty`, `not_empty` |
| number | `eq`, `neq`, `lt`, `lte`, `gt`, `gte`, `empty`, `not_empty` |
| date | `is`, `before`, `after`, `empty`, `not_empty` |
| checkbox | `is` |
| select | `is`, `is_not`, `empty`, `not_empty` |
| multi_select | `contains`, `not_contains`, `empty`, `not_empty` |

Without views.json, or when an entry references an unknown property, Forma falls back to a table sorted by title. Filter values are always written as strings (numbers, `YYYY-MM-DD` dates, `true`/`false`, option names). The pseudo-properties `title`, `created_at` and `updated_at` can be used in `sort` and `filter`.

---

## 7. `.forma/` metadata files

All JSON files are written atomically and carry a `format_version`.

### 7.1 `workspace.json`

```json
{ "format_version": 1, "id": "<uuid>", "name": "My Workspace",
  "created_at": "2026-11-02T10:00:00Z", "created_by": "Forma 0.1.0" }
```

If `format_version` is higher than this build supports, the workspace opens **read-only** with an explanation. If it is lower, migrations run (§10) after a backup of `.forma/` metadata.

### 7.2 `tree.json` (advisory)

```json
{ "format_version": 1,
  "order": { "root": ["<id>", "<id>"], "<parent-id>": ["<id>"] },
  "favorites": ["<id>"] }
```

Unknown IDs are ignored. Pages missing from an order list are appended in natural title order. Without this file, all siblings are sorted by natural title order and there are no favorites.

### 7.3 Attachments

- Attachments are stored in `<bundle>/_assets/<name>`. The name is the sanitized original filename (or `image-<UTC timestamp>.<ext>` for pasted images), made unique within `_assets/`.
- There is no attachment database. The index records references found in bodies and whether each file exists.
- Files larger than 100 MB need confirmation. Attachments are copied into the workspace, never moved. `.md` files can't be attached; they are offered as page imports in a later version.

### 7.4 Trash: `trash/<entry-id>/entry.json`

```json
{ "format_version": 1, "entry_id": "<uuid>", "trashed_at": "...",
  "root_page_id": "<uuid>", "title": "Paper A", "kind": "leaf|bundle|folder",
  "original_parent_id": "<uuid>|null", "original_rel_path": "Research/Paper A.md",
  "page_ids": ["<uuid>", "..."] }
```

The trashed file or bundle sits next to `entry.json` under its original name. `page_ids` lists the persisted IDs found in the subtree. It is informational, and restore doesn't depend on it.

### 7.5 Operations: `operations/<op-id>.json`

```json
{ "format_version": 1, "op_id": "<uuid>", "applied_at": "...", "source": "ai",
  "request": "Organize my reading notes", "summary": "Moved 3 pages, edited 1",
  "provider": "anthropic", "model": "<model-id>",
  "ops": [
    { "type": "edit_page",   "page_id": "…", "before_rev": "…", "after_rev": "…",
      "snapshot": "history/<page-id>/20261102T103000Z_<rev>_ai.md" },
    { "type": "create_page", "page_id": "…", "after_rev": "…" },
    { "type": "move_page",   "page_id": "…", "from_parent": "…", "to_parent": "…" },
    { "type": "set_properties", "page_id": "…", "before_rev": "…", "after_rev": "…", "snapshot": "…" }
  ],
  "undone_by": null }
```

Forma keeps the last 100 operations. When it prunes an older operation, that operation's protected snapshots stop being protected.

### 7.6 Journal: `journal/<journal-id>.json`

Used for multi-file batches (ChangeSet apply, property rename, option rename) and for the rare `bundle_conversion` that rewrites relative references (§3.1). Phase 3 writes `kind: "collection_rename"`, with each step's snapshot in `history/<page-id>/…_batch.md`; a rename that would give an entry two keys with the same name is refused before anything is written.

```json
{ "format_version": 1, "journal_id": "…", "kind": "changeset_apply", "started_at": "…",
  "steps": [ { "page_id": "…", "before_rev": "…", "after_rev": "…", "snapshot": "…" } ] }
```

`after_rev` is computed before writing. Recovery on startup is **always a rollback**. For each step:

- If the current revision equals `before_rev`, the step was never applied, so nothing is done.
- If it equals `after_rev`, the snapshot is restored.
- If it is anything else, the file has been modified since and is left untouched. The user is told.
- `bundle_conversion` steps also record `from_path` and `to_path`. Rolling one back moves the file back to `from_path` before restoring the snapshot.

The user then sees "An interrupted operation was undone".

### 7.7 Recovery: `recovery/<page-id>.md` and `.json`

These hold editor buffers that couldn't be saved normally (conflict, deleted file, save failure). The `.json` file records `{ page_id, title, saved_at, reason }`, where `reason` is `conflict` or `unsaved`. Recovery buffers are offered back to the user on the next launch and deleted only after the user resolves them (keep as a new page, or discard), or when the editor that kept one later saves that page successfully.

---

## 8. Revisions and history

### 8.1 Current revision

`rev = hex(blake3(file_bytes))[0..16]`. A folder page without `index.md` has no revision. Every mutation of an existing page requires `base_rev` (ARCHITECTURE §4.5).

`body_rev = hex(blake3(body_bytes))[0..16]`, where the body is everything after the frontmatter. The editor's saves are checked against it, so a frontmatter change made meanwhile isn't a conflict (ADR-033). A folder page's body is empty.

### 8.2 Snapshots

Snapshots are stored at `.forma/history/<page-id>/<YYYYMMDDTHHMMSSZ>_<rev>_<reason>.md` and are full copies of the file bytes.

`reason` is one of:

- `autosave`
- `external` (content not produced by Forma)
- `ai`
- `restore`
- `conflict`
- `undo`
- `batch`

Policy: ARCHITECTURE §4.6. Restoring a snapshot is a normal write: the current content is snapshotted first, then replaced, so restores can themselves be undone.

---

## 9. SQLite index schema

This is a cache and safe to delete. A `schema_version` mismatch drops and rebuilds it. The schema below is `schema_version = 1`, finalized in Phase 3 (`crates/forma-core/src/index.rs`):

```sql
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);  -- schema_version

CREATE TABLE pages (
  id            TEXT PRIMARY KEY,
  rel_path      TEXT NOT NULL,          -- file (leaf) or directory (bundle, folder), '/' separators
  parent_id     TEXT,                   -- NULL = root
  kind          TEXT NOT NULL,          -- leaf | bundle | folder
  title         TEXT NOT NULL,
  icon          TEXT,
  rev           TEXT,
  size          INTEGER,                -- with mtime_ns: skip unchanged files on open
  mtime_ns      INTEGER,
  id_persisted  INTEGER NOT NULL,
  status        TEXT NOT NULL,          -- ok | metadata_readonly | invalid_frontmatter
  status_detail TEXT,
  is_collection INTEGER NOT NULL DEFAULT 0,
  frontmatter   TEXT                    -- the top-level mapping as JSON (collection values)
);
CREATE INDEX pages_parent ON pages(parent_id);

CREATE TABLE tags  (page_id TEXT NOT NULL, tag TEXT NOT NULL, PRIMARY KEY (page_id, tag));
CREATE TABLE links (source_id TEXT NOT NULL, ordinal INTEGER NOT NULL,
                    kind TEXT NOT NULL,      -- page | wiki
                    target TEXT NOT NULL,    -- page ID, or the wikilink's text
                    label TEXT NOT NULL,
                    PRIMARY KEY (source_id, ordinal));
CREATE INDEX links_target ON links(kind, target);

-- Both full-text tables use the pages row's rowid.
CREATE VIRTUAL TABLE pages_fts USING fts5(title, body, tags, props,
  tokenize = 'unicode61 remove_diacritics 2');
CREATE VIRTUAL TABLE pages_trigram USING fts5(text,   -- title, body and property text
  tokenize = 'trigram remove_diacritics 1');
-- Phase 5 adds: passages(id, page_id, ordinal, heading_path, start, end) + passages_fts(text).
```

Changes from the initial draft:
- `rel_path` isn't `UNIQUE`: two pages can trade places within one rescan.
- Wikilinks are stored by their text and resolved when read, because titles change.
- There is no `attachments` table yet (nothing reads it) and no `property_values` table: collection values are coerced from `frontmatter` when a collection is read.
- `created_at` and `updated_at` are read from `frontmatter`.

---

## 10. Document Service API surface

These are conceptual signatures, finalized per phase as Tauri commands with `ts-rs` types. Every mutation that changes an existing page file takes `base_rev`. Arguments are IDs, never paths.

**Reads**

```
workspace_info()                             → WorkspaceInfo
search(query)                                → [PageHit]               (≤ 50, ranked; Phase 3)
quick_open(query)                            → [PageHit]               (titles; Phase 3)
backlinks(id)                                → [PageHit]               (page links + resolved wikilinks)
resolve_pages(ids)                           → [PageLinkInfo]          (live titles: ok | trashed | missing)
resolve_wikilinks(texts)                     → [WikiResolution]
get_collection(id)                           → CollectionData { schema, problem?, rows }
get_entry(id)                                → EntryData | null
get_views(collection_id)                     → CollectionViews
list_children(parent_id?)                    → [PageSummary]            (ordered)
get_page(id)                                 → PageDocument { meta, body, rev, status, fidelity_notice? }
get_ancestors(id)                            → [PageSummary]            (breadcrumbs)
get_backlinks(id)                            → [Backlink]
list_favorites()                             → [PageSummary]
list_trash()                                 → [TrashEntry]
list_history(id)                             → [SnapshotInfo]
get_snapshot(id, snapshot_id)                → String
search(query, opts)                          → SearchPage<SearchHit>
query_collection(collection_id, view)        → Page<CollectionRow>
```

**Mutations**

```
ensure_persisted_id(id)                                     → ()   (before any link to the page)
rebuild_index()                                             → ()
create_collection(parent_id?, title) / convert_to_collection(id) / remove_collection(id)
update_schema(collection_id, op)                            → Schema   (renames are journaled batches, §7.6)
set_property(page_id, property_id, value)                   → ()       (surgical; absent key when empty)
save_views(collection_id, views)                            → ()
create_page(parent_id?, title, body?, position?)            → PageMeta
save_page(id, base_body_rev, body)                          → Saved{rev, body_rev} | Conflict{current_body_rev, current_body}
save_page_over_conflict(id, body)                           → Saved{…}       ("keep mine": snapshots the disk version first)
set_title(id, title) / set_icon(id, icon?)                  → ()             (surgical, no base: ADR-033; set_title renames the file)
update_meta(id, base_rev, { tags? })                        → Saved{rev} | Conflict{current}   (Phase 3)
move_page(id, new_parent_id?, position?)                    → PageMeta       (cycle-checked)
reorder_page(id, position)                                  → ()             (tree.json only)
duplicate_page(id)                                          → PageMeta       (subtree, new IDs)
trash_page(id)                                              → TrashEntry
restore_trash(entry_id)                                     → PageMeta
delete_trash_entry(entry_id)                                → ()             (permanent; user-confirmed)
set_favorite(id, bool)                                      → ()
import_attachment(page_id, source: Bytes | DropToken, name) → AttachmentRef     (DropToken: ARCHITECTURE §2.1)
restore_snapshot(id, snapshot_id, base_rev)                 → Saved{rev} | Conflict{current}
resolve_conflict(id, choice, buffer?)                       → …
create_collection(parent_id?, title, schema) / convert_to_collection(id, base_rev, schema)
update_schema(collection_id, base_rev, schema_ops)          → Saved | Conflict | Batch{journal}
set_properties(page_id, base_rev, set, unset)               → Saved{rev} | Conflict{current}
apply_changeset(changeset_id, op_ids)                       → AppliedOperation | Conflicts   (Phase 6)
undo_operation(op_id)                                       → AppliedOperation | Conflicts   (Phase 6)
rebuild_index()                                             → ()
```

---

## 11. App-level data (outside the workspace)

### 11.1 `settings.json`

```json
{
  "format_version": 1,
  "appearance": { "theme": "system" },
  "workspace": { "last_path": "/Users/me/FormaWorkspace" },
  "layout": { "sidebar": { "visible": true, "width": 260 }, "ai_panel": { "visible": false, "width": 360 } },
  "ai": {
    "enabled": false,
    "active_provider": "<provider-config-id>",
    "providers": [
      { "id": "<uuid>", "kind": "anthropic | openai | openai_compatible | ollama | lmstudio",
        "name": "Anthropic", "base_url": "https://api.anthropic.com", "model": "<model-id>",
        "has_credential": true,
        "options": { "temperature": 0.3, "max_output_tokens": 2048,
                     "context_window_override": null, "tool_calling_override": null } }
    ],
    "cloud_acknowledged": { "<provider-config-id>": "2026-11-02T10:00:00Z" }
  }
}
```

### 11.2 Keychain

Generic password items, with service `<bundle-id>.ai-provider` and account `<provider-config-id>`. These are created, replaced and deleted only through the settings UI. Their values never cross IPC to the webview.

### 11.3 Not persisted in v0.1

- AI chat transcripts. They are in memory for the session only. Applied operations are recorded (§7.5).
- Any telemetry.

---

## 12. Versioning and migration

| Format | Version field | On newer-than-supported | On older |
|---|---|---|---|
| Workspace metadata | `workspace.json.format_version` | Open read-only | Migrate after backing up `.forma/*.json` |
| tree.json, views.json | `format_version` | Ignore the file (advisory) and don't overwrite it | Migrate |
| Collection schema | `collection.version` | Collection read-only | Migrate on the next schema edit only |
| Page frontmatter | none (keys are stable within workspace format 1) | — | — |
| SQLite index | `meta.schema_version` | Rebuild | Rebuild |

The migration framework exists from Phase 2 with version 1 as its only version. Each migration step is a pure function with tests.
