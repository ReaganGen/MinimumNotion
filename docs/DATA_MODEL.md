# Diaspar Data Model

Status: Phase 0 baseline (2026-11-02). Names updated on 2026-11-03 (ADR-046): the product was called Forma, with `.forma/`, `forma://page/` links and the reserved key `forma`. This is the normative specification of Diaspar's durable formats. Code that reads or writes workspace files must follow it. When a phase changes a rule, update this file in the same change and record why in [DECISIONS.md](DECISIONS.md).

Terminology: **durable** means stored in the workspace folder and part of the user's data. **Derived** means computed from durable data, either cached in the index or computed on demand. **Advisory** means durable but non-essential: if it is lost, Diaspar falls back to a sensible default and no content is lost.

---

## 1. Entities

| Entity | Identity | Authoritative storage | Notes |
|---|---|---|---|
| Workspace | UUID | `.diaspar/workspace.json` | One active workspace per app session in v0.1 |
| Page | UUID | A Markdown file (`*.md` or `<dir>/index.md`) | See §3–§4 |
| Collection | The UUID of its collection page | `collection:` block in the collection page's frontmatter | A collection *is* a page (§6) |
| Collection Property | Short ID, unique within its collection | The collection schema | Values live in entry pages' frontmatter |
| Page Link | (source page, ordinal) | Source page body Markdown | Derived into the index |
| Attachment | (page, relative name) | File in the page's `_assets/` folder | Never stored in SQLite |
| Document Revision | Content hash (current) / snapshot file (past) | Current: derived. Past: `.diaspar/history/` | §8 |
| Trash Entry | UUID | `.diaspar/trash/<entry-id>/` | §7.4 |
| Operation (AI) | UUID | `.diaspar/operations/<op-id>.json` | For undo and history (§7.5) |

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
| Favorite status | `.diaspar/tree.json` → `favorites` | advisory |
| Sibling order | `.diaspar/tree.json` → `order` | advisory |
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
└── .diaspar/
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

Outside the workspace, in `~/Library/Application Support/<bundle-id>/`: `settings.json` (§11), `.window-state.json` (window size and position) and `index/<workspace-key>/index.db` (§9). The log is `~/Library/Logs/<bundle-id>/diaspar.log`. Secrets live in the Keychain (§11.2).

The workspace root is the page-tree root. It is not itself a page. Opening any existing folder makes it a workspace. Diaspar creates `.diaspar/` and writes nothing else until the user acts (§4.4).

---

## 3. Page files

### 3.1 Page kinds

| Kind | On disk | Title fallback | Created by Diaspar when |
|---|---|---|---|
| leaf | `<stem>.md` | stem | New page with no children and no attachments |
| bundle | `<stem>/index.md` (+ `_assets/`, child pages) | directory name | A leaf gains its first child or attachment (converted by `mkdir` + `rename`, ARCHITECTURE §3.1) |
| folder | `<stem>/` with no `index.md` | directory name | Never. These are created externally. Diaspar creates `index.md` the first time it has to write such a page |

- A bundle stays a bundle after its last child or attachment is removed. Diaspar never converts it back automatically, which avoids churn.
- Diaspar creates the extension `.md`. When reading, `.md` and `.markdown` are accepted case-insensitively.
- An `index.md` directly in the workspace root is an ordinary leaf page.
- **Relative references during conversion.** A leaf's body may contain hand-written relative references, meaning anything other than `diaspar:` links, absolute URLs and `_assets/`. Moving the file one level deeper would break them, so the conversion prefixes each with `../` using the parser's source offsets. Because this variant also rewrites the body, it records a `bundle_conversion` journal intent first (§7.6).
- **Limitation (v0.1):** when a page is *moved*, hand-written relative references to files outside its own `_assets/` aren't rewritten and may break. Diaspar shows them as broken references. Diaspar's own references (`diaspar://` links and `_assets/`) never break.

### 3.2 Discovery rules

When scanning (or reconciling) a directory:

1. Skip entries whose names start with `.`, and skip all symlinks. Symlinks are never followed.
2. Skip `_assets/` inside a bundle. That directory holds attachments, not pages.
3. A Markdown file other than a bundle's own `index.md` is a leaf page.
4. A directory is a bundle if it contains `index.md`, and a folder page otherwise.
5. Other files are attachments or unmanaged files. They are ignored by the page tree and always preserved.
6. If `Foo.md` and a directory `Foo/` sit side by side (for example, from another tool's export), both are treated as separate pages. Diaspar never creates this situation itself.

### 3.3 Filenames

The file stem is derived from the title. It is human-friendly and **not authoritative**.

1. Apply Unicode NFC normalization. Replace newlines and tabs with a space, and collapse runs of whitespace.
2. Replace `/ \ : * ? " < > |` and control characters with `-`. This keeps names portable to a future Windows build.
3. Trim leading and trailing spaces and dots. Strip leading `.` so the file can't become hidden.
4. Truncate to at most **100 bytes** of UTF-8, cutting at a grapheme boundary.
5. If the result is empty, use `Untitled`. If it equals a reserved name (`index`, `_assets`, `.diaspar`, compared case-insensitively), append ` 1`.
6. Uniqueness among siblings is **case-insensitive and normalization-insensitive**, matching APFS defaults. `Name.md`, `Name/` and any case variant claim the same slot. On a collision, append ` 2`, ` 3` and so on.

Renaming a page in Diaspar updates `title` and then renames the file or directory, deferred while the user is typing (ARCHITECTURE §6.4). If a user renames a file in Finder, Diaspar keeps the frontmatter `title` and doesn't rename the file back.

---

## 4. Frontmatter

### 4.1 Syntax

- Frontmatter is present only if the file begins with `---` followed by a newline (after an optional UTF-8 BOM, which is stripped). It ends at the next line that is exactly `---` or `...`.
- The content must be YAML 1.2 whose top level is a **mapping**. Anything else, including duplicate keys, is *invalid frontmatter* (§4.5).
- A file without frontmatter is valid. All metadata then comes from fallbacks.

### 4.2 Reserved keys

| Key | Type | Written by Diaspar | Read rules |
|---|---|---|---|
| `id` | UUID string | Always, once persisted | Must parse as a UUID. If not, the page is invalid (§4.5) |
| `title` | string | Always | Missing → fallback (§1.1). Newlines become spaces |
| `icon` | string (one emoji grapheme in v0.1) | When set | Unrecognized value → ignored for display and preserved |
| `tags` | list of strings | When non-empty | A scalar string counts as one tag. Duplicates (case-insensitive) are collapsed for display |
| `created_at` | RFC 3339 UTC timestamp | Always | Date-only or other RFC 3339 forms are accepted |
| `updated_at` | RFC 3339 UTC timestamp | On every content or metadata write | As above |
| `collection` | mapping (§6.2) | Collection pages only | Invalid schema → the collection opens read-only with an error |

Reserved keys are matched case-sensitively when reading. Collection property names may not equal any reserved key, compared case-insensitively. The key `diaspar` is also reserved, for future namespaced metadata.

All other keys are **user keys**. Diaspar preserves them byte for byte (§4.6). They may hold collection property values (§6.3).

### 4.3 Example (Diaspar-created leaf page)

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

For newly created files, Diaspar writes keys in this order: `id, title, icon, tags, created_at, updated_at`, then property values in schema order, then `collection`. In existing files the order is preserved.

### 4.4 ID persistence and duplicates

- Index-only IDs are assigned to pages discovered without an `id`.
- **Invariant:** before Diaspar creates any durable reference to a page, it calls `ensure_persisted_id(page)`. Durable references are links, favorites, order entries, history snapshots, operation records and journal steps. Trash entries aren't on this list: they record IDs for information only and are restored by path, so trashing a large external folder writes nothing into its files. That call writes `id` into the file using the surgical edit described in §4.6.
- On duplicate `id`s, the original keeps the ID: that is the file at the path the index already associates with the ID, or failing that the earliest birth time, then the first path in order. The other file gets a new UUID and the user is notified. Both files are kept.

### 4.5 Invalid frontmatter

If the frontmatter can't be parsed, isn't a mapping or has an invalid `id`:

- The page appears in the tree with a warning badge and opens **read-only**, showing the parse error with line and column. It offers *Open in default editor* and *Reveal in Finder*.
- Diaspar never writes to the file while it is invalid. After the file is fixed externally, the watcher reconciles it and the page becomes editable again.

### 4.6 Surgical frontmatter editing

Diaspar never re-serializes the whole frontmatter block. To change keys, it:

1. Parses the block with a span-aware YAML 1.2 parser to find the byte range of each **top-level** key and its value. Each range runs from the key's line to the start of the next top-level key or the end of the block.
2. Replaces only the ranges of the keys being changed with freshly emitted YAML (§4.7). New keys are appended at the end of the block in the order given in §4.3. Removed keys have their range deleted.
3. Leaves every other byte untouched, including comments, user keys, key order, anchors, flow style and quoting.

If the frontmatter uses a construct that prevents safe span edits, Diaspar treats the page's metadata as read-only. Such constructs include a top-level flow mapping, or a key that Diaspar must change being defined through an alias or merge key. Body editing still works in that case. The body is replaced and the frontmatter bytes are kept verbatim, without updating `updated_at`.

### 4.7 YAML emission rules (values Diaspar writes)

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

The base dialect is **CommonMark** with these **GitHub Flavored Markdown** extensions: tables, task list items, strikethrough and autolinks. On top of that come Diaspar's own conventions: math, page links (§5.4) and read support for wikilinks.

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
| Page link | `[label](diaspar://page/<uuid>)` | Same, with the label set to the target's current title |
| Wikilink | `[[Title]]`, `[[Title\|alias]]`, `[[Folder/Title]]` | **Kept exactly as written** |
| Raw HTML (block or inline) | anything | **Kept exactly as written.** Displayed as text, never rendered |
| Footnotes, definition lists, other syntax | anything | **Kept exactly as written** in a raw block or as literal inline text |

### 5.3 Normalization on write

These rules apply only when Diaspar writes a body after an actual edit (ARCHITECTURE §6.3):

- LF line endings, a single trailing newline, UTF-8 without a BOM.
- **Minimal escaping.** A character is escaped only if leaving it unescaped would change how that position parses. For example, a literal `$` is escaped only where it would otherwise open math, and a leading `#` only where it would otherwise start a heading.
- Body text isn't Unicode-normalized. Only filenames are.
- Reference-style links and images are written in inline form. The link stays the same; only its syntax changes, and the open-time fidelity notice reports it.

### 5.4 Links and references

- **Page link URL scheme:** `diaspar://page/<uuid>`, optionally followed by `#<heading-slug>`. Diaspar resolves the UUID through the index. An unknown UUID renders as a broken link and is kept unchanged.
- **Wikilink resolution (read-only):** `[[Folder/Title]]` is matched against path suffixes, and `[[Title]]` against titles, falling back to filename stems. Exactly one match is a resolved link. Zero or several matches show as an unresolved link with a hint. The index stores what it resolved; the text in the file is never changed.
- **Attachment and image sources:**
  - A relative path resolves against the directory of the page's file. Diaspar writes `_assets/<name>` for its own attachments.
  - A path that resolves outside the workspace isn't loaded.
  - `https:` image URLs are loaded automatically. `http:` image URLs are kept but not loaded (ADR-031, ARCHITECTURE §6.6).
  - `data:` image URLs are kept and not rendered in v0.1.
- **Allowed link schemes when activated:** `diaspar:`, `http:`, `https:`, `mailto:`. Any other scheme is shown but not opened.

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

- `version` must be `1`. A higher version opens the collection read-only, marked "needs a newer Diaspar".
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

### 6.5 Views (`.diaspar/views.json`, advisory)

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

Without views.json, or when an entry references an unknown property, Diaspar falls back to a table sorted by title. Filter values are always written as strings (numbers, `YYYY-MM-DD` dates, `true`/`false`, option names). The pseudo-properties `title`, `created_at` and `updated_at` can be used in `sort` and `filter`.

---

## 7. `.diaspar/` metadata files

All JSON files are written atomically and carry a `format_version`.

### 7.1 `workspace.json`

```json
{ "format_version": 1, "id": "<uuid>", "name": "My Workspace",
  "created_at": "2026-11-02T10:00:00Z", "created_by": "Diaspar 0.1.0" }
```

If `format_version` is higher than this build supports, the workspace opens **read-only** with an explanation. If it is lower, migrations run (§10) after a backup of `.diaspar/` metadata.

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

Written when the user applies an AI proposal or undoes one (Phase 6, ADR-043), inside the same transaction as the change:

```json
{ "format_version": 1, "op_id": "<uuid>", "seq": 12, "applied_at": "...", "source": "ai",
  "request": "Organize my reading notes", "summary": "Created 1 page, moved 3 pages",
  "provider": "Anthropic", "model": "<model-id>",
  "ops": [
    { "type": "edit_page",      "page_id": "…", "before_rev": "…", "after_rev": "…",
      "snapshot": "history/<page-id>/20261102T103000Z_<rev>_ai.md" },
    { "type": "set_properties", "page_id": "…", "before_rev": "…", "after_rev": "…", "snapshot": "…" },
    { "type": "create_page",    "page_id": "…", "after_rev": "…" },
    { "type": "move_page",      "page_id": "…", "from_parent": "…|null", "to_parent": "…|null" },
    { "type": "trash_page",     "page_id": "…", "entry_id": "…" },
    { "type": "restore_page",   "page_id": "…", "rev": "…" }
  ],
  "undoes": null }
```

- `source` is `ai`, or `undo` for a record that undid another (`undoes` names it). Undoing an `undo` record redoes the change. `trash_page` and `restore_page` appear in undo records: undoing a creation trashes the page, and undoing that restores it.
- An operation is undone when a record that `undoes` it exists and isn't undone itself. Records are never rewritten.
- `seq` orders the records (timestamps can tie). Diaspar keeps the newest 100; when it removes an older one, that operation's snapshots stop being protected from history pruning (§8.2).

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

**Transactions** (Phase 6): applying an AI proposal (`kind: "changeset_apply"`) or undoing one (`"operation_undo"`) journals every file action before it's done, instead of listing steps in advance:

```json
{ "format_version": 1, "journal_id": "…", "kind": "changeset_apply", "started_at": "…",
  "actions": [
    { "action": "make_dir", "path": "Projects" },
    { "action": "rename",   "from": "Projects.md", "to": "Projects/index.md" },
    { "action": "write",    "page_id": "…", "path": "Plan.md", "before_rev": "…|null",
      "after_rev": "…", "snapshot": ".diaspar/history/<page-id>/…_ai.md|null" }
  ] }
```

Paths are relative to the workspace root. Rollback goes newest first: a `write` whose file still has `after_rev` gets its snapshot back (or, with no `before_rev`, is removed); a `rename` is reversed if its destination exists and its source doesn't; a `make_dir` is removed if empty. Anything else is left as it is, and counted in the notice ("An interrupted AI change was undone").

### 7.7 Recovery: `recovery/<page-id>.md` and `.json`

These hold editor buffers that couldn't be saved normally (conflict, deleted file, save failure). The `.json` file records `{ page_id, title, saved_at, reason }`, where `reason` is `conflict` or `unsaved`. Recovery buffers are offered back to the user on the next launch and deleted only after the user resolves them (keep as a new page, or discard), or when the editor that kept one later saves that page successfully.

### 7.8 Backup archives and exports (outside the workspace)

Settings → Data writes these where the user chooses (ADR-044). Neither is read by Diaspar except a backup being restored.

- **Backup:** a zip, by default `<name> backup <YYYY-MM-DD>.zip`. Every entry is under one top folder named after the workspace: `<name>/Page.md`, `<name>/.diaspar/workspace.json`, … It holds every file of the workspace, `.diaspar/` included, except `.diaspar/tmp/` and `.diaspar/lock`. Entries are Deflate-compressed. Directory entries may be present; symbolic links are never written.
- **Restore** accepts a zip with relative entry names and no links that holds at least one `.md` file or a `.diaspar/workspace.json`. If all entries share one top folder, its name becomes the new folder's name and its contents are unpacked into it; otherwise the zip's file name is used. An existing name gets " 2", " 3"…
- **Export:** a folder `<name> export <YYYY-MM-DD>` with the workspace's files and attachments, without `.diaspar/`. It is a plain Markdown tree that Diaspar or any other app can open; opening it in Diaspar makes it a new workspace.

---

## 8. Revisions and history

### 8.1 Current revision

`rev = hex(blake3(file_bytes))[0..16]`. A folder page without `index.md` has no revision. Every mutation of an existing page requires `base_rev` (ARCHITECTURE §4.5).

`body_rev = hex(blake3(body_bytes))[0..16]`, where the body is everything after the frontmatter. The editor's saves are checked against it, so a frontmatter change made meanwhile isn't a conflict (ADR-033). A folder page's body is empty.

### 8.2 Snapshots

Snapshots are stored at `.diaspar/history/<page-id>/<YYYYMMDDTHHMMSSZ>_<rev>_<reason>.md` and are full copies of the file bytes.

`reason` is one of:

- `autosave`
- `external` (content not produced by Diaspar)
- `ai`
- `restore`
- `conflict`
- `undo`
- `batch`

Policy: ARCHITECTURE §4.6. Restoring a snapshot is a normal write: the current content is snapshotted first, then replaced, so restores can themselves be undone.

---

## 9. SQLite index schema

This is a cache and safe to delete. A `schema_version` mismatch drops and rebuilds it. The schema below is `schema_version = 2`: Phase 3's tables plus Phase 5's passages (`crates/diaspar-core/src/index.rs`):

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

-- Schema 2 (Phase 5, ADR-041): what the AI assistant retrieves from.
CREATE TABLE passages (
  page_id      TEXT NOT NULL,
  ordinal      INTEGER NOT NULL,
  heading_path TEXT NOT NULL,           -- JSON array of the headings above it, outermost first
  byte_start   INTEGER NOT NULL,        -- its range in the page body (after the frontmatter)
  byte_end     INTEGER NOT NULL,
  markdown     TEXT NOT NULL,           -- body[byte_start..byte_end]
  PRIMARY KEY (page_id, ordinal));
-- Keyed by the passages row's rowid. `title` is the page's; `heading` joins heading_path with " › ".
CREATE VIRTUAL TABLE passages_fts USING fts5(title, heading, text,
  tokenize = 'porter unicode61 remove_diacritics 2');
```

Passages are 250–400 tokens, cut at headings and between blocks; a block longer than that is cut at lines, then sentences, then characters (`crates/diaspar-core/src/passages.rs`). CJK questions find pages through `pages_trigram` and rank their passages in Rust, so there is no passage trigram table.

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
apply_changeset(changeset_id, change_ids)                   → Applied{operation} | Conflicts{conflicts}   (Phase 6; the review dialog only)
reject_changeset(changeset_id)                              → ()             (forgets the proposal; nothing was written)
list_operations()                                           → [OperationInfo]   (newest first)
undo_operation(op_id)                                       → Applied{operation} | Conflicts{conflicts}   (an undo's undo redoes)
rebuild_index()                                             → ()
```

---

## 11. App-level data (outside the workspace)

### 11.1 `settings.json`

```json
{
  "format_version": 1,
  "appearance": { "theme": "system" },
  "workspace": { "last_path": "/Users/me/DiasparWorkspace" },
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
    "cloud_consent": { "<provider-config-id>": "2026-11-03T10:00:00Z" }
  }
}
```

Notes on `ai` (Phase 4):

- `enabled` is `false` by default, and the section is written only once the user changes an AI setting.
- `base_url` is `http` or `https` with no query, credentials or trailing slash. A provider whose host is a loopback address counts as local.
- `options` values are `null` for "the provider's default". `temperature` is 0–2 and isn't offered for Anthropic. `max_output_tokens` is 1–1,000,000, `context_window_override` 512–10,000,000. `tool_calling_override` is the user's answer to "can this model call tools?" (Settings → AI → "Look things up in your notes": Automatic is `null`, On `true`, Off `false`).
- `cloud_consent` records when the user agreed that messages, and the note excerpts they need, may go to that provider (Phase 5, ADR-042). Entries are removed when a provider's scheme, host or port changes, and when the provider is removed. Phase 4 recorded a narrower agreement under `cloud_acknowledged`; that key is no longer read, and it disappears from the file the next time the AI settings are saved.

### 11.2 Keychain

Generic password items in the login keychain, with service `<bundle-id>.ai-provider` (currently `app.diaspar.desktop.ai-provider`) and account `<provider-config-id>`. These are created, replaced and deleted only through the settings UI, and removing a provider deletes its item. Their values never cross IPC to the webview.

### 11.3 Not persisted in v0.1

- AI chat transcripts. They are in memory for the session only. Applied operations are recorded (§7.5).
- Any telemetry.

---

## 12. Versioning and migration

| Format | Version field | On newer-than-supported | On older |
|---|---|---|---|
| Workspace metadata | `workspace.json.format_version` | Open read-only | Migrate after backing up `.diaspar/*.json` |
| tree.json, views.json | `format_version` | Ignore the file (advisory) and don't overwrite it | Migrate |
| Collection schema | `collection.version` | Collection read-only | Migrate on the next schema edit only |
| Page frontmatter | none (keys are stable within workspace format 1) | — | — |
| SQLite index | `meta.schema_version` | Rebuild | Rebuild |

The migration framework exists from Phase 2 with version 1 as its only version. Each migration step is a pure function with tests.
