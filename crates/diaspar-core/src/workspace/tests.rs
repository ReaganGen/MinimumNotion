//! Workspace tests against real temporary folders (IMPLEMENTATION_PLAN §5.2).

use std::fs;
use std::path::{Path, PathBuf};

use super::Workspace;
use super::types::{PageKind, PageStatus, SaveOutcome};

fn temp_workspace() -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().unwrap();
    let ws = Workspace::open(dir.path()).unwrap();
    (dir, ws)
}

fn root(ws: &Workspace) -> PathBuf {
    ws.root().to_path_buf()
}

fn saved_rev(outcome: SaveOutcome) -> String {
    match outcome {
        SaveOutcome::Saved { rev, .. } => rev,
        other => panic!("expected Saved, got {other:?}"),
    }
}

/// Every file under `dir` except `.diaspar/`, as relative paths.
fn user_files(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) {
        for item in fs::read_dir(dir).unwrap().flatten() {
            let path = item.path();
            if item.file_name() == ".diaspar" {
                continue;
            }
            if path.is_dir() {
                walk(base, &path, out);
            } else {
                out.push(path.strip_prefix(base).unwrap().display().to_string());
            }
        }
    }
    walk(dir, dir, &mut out);
    out.sort();
    out
}

#[test]
fn opening_a_folder_writes_only_inside_diaspar() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("Notes.md"), "# Hello\n\nNo frontmatter.\n").unwrap();
    let ws = Workspace::open(dir.path()).unwrap();
    assert_eq!(user_files(dir.path()), ["Notes.md"]);
    assert_eq!(
        fs::read_to_string(dir.path().join("Notes.md")).unwrap(),
        "# Hello\n\nNo frontmatter.\n"
    );
    assert!(dir.path().join(".diaspar/workspace.json").is_file());
    let pages = ws.list_children(None).unwrap();
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0].title, "Notes");
    // Reading a page never writes it.
    let doc = ws.get_page(&pages[0].id).unwrap();
    assert_eq!(doc.body, "# Hello\n\nNo frontmatter.\n");
    assert_eq!(user_files(dir.path()), ["Notes.md"]);
}

#[test]
fn create_edit_and_reopen() {
    let (dir, mut ws) = temp_workspace();
    let page = ws.create_page(None, "Reinforcement Learning").unwrap();
    let doc = ws.get_page(&page.id).unwrap();
    let rev = saved_rev(
        ws.save_page(&page.id, Some(&doc.body_rev), "Some *content*.\n")
            .unwrap(),
    );

    let text = fs::read_to_string(dir.path().join("Reinforcement Learning.md")).unwrap();
    assert!(
        text.starts_with(&format!(
            "---\nid: {}\ntitle: Reinforcement Learning\n",
            page.id
        )),
        "{text}"
    );
    assert!(text.ends_with("---\nSome *content*.\n"));

    drop(ws);
    let ws = Workspace::open(dir.path()).unwrap();
    let doc = ws.get_page(&page.id).unwrap();
    assert_eq!(doc.body, "Some *content*.\n");
    assert_eq!(doc.rev.as_deref(), Some(rev.as_str()));
}

#[test]
fn stale_revision_is_a_conflict_and_writes_nothing() {
    let (dir, mut ws) = temp_workspace();
    let page = ws.create_page(None, "Page").unwrap();
    let base = Some(ws.get_page(&page.id).unwrap().body_rev);
    let path = dir.path().join("Page.md");
    // Someone edits the file outside Diaspar.
    let external = fs::read_to_string(&path)
        .unwrap()
        .replace("---\n\n", "---\nTheir text\n");
    let external = format!("{external}Their text\n");
    fs::write(&path, &external).unwrap();

    match ws
        .save_page(&page.id, base.as_deref(), "My text\n")
        .unwrap()
    {
        SaveOutcome::Conflict { current_body, .. } => {
            assert!(current_body.unwrap().contains("Their text"));
        }
        other => panic!("expected a conflict, got {other:?}"),
    }
    assert_eq!(fs::read_to_string(&path).unwrap(), external);
}

#[test]
fn first_save_of_an_external_file_adds_frontmatter_and_snapshots_the_original() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("Ideas.md"), "Original text\n").unwrap();
    let mut ws = Workspace::open(dir.path()).unwrap();
    let page = &ws.list_children(None).unwrap()[0];
    let id = page.id.clone();
    let rev = ws.get_page(&id).unwrap().body_rev;
    saved_rev(
        ws.save_page(&id, Some(rev.as_str()), "Edited text\n")
            .unwrap(),
    );

    let text = fs::read_to_string(dir.path().join("Ideas.md")).unwrap();
    assert!(
        text.starts_with(&format!("---\nid: {id}\ntitle: Ideas\ncreated_at: ")),
        "{text}"
    );
    assert!(text.ends_with("---\nEdited text\n"));
    let snapshots: Vec<_> = fs::read_dir(dir.path().join(".diaspar/history").join(&id))
        .unwrap()
        .flatten()
        .collect();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(
        fs::read_to_string(snapshots[0].path()).unwrap(),
        "Original text\n"
    );
}

#[test]
fn saving_keeps_unknown_keys_and_comments() {
    let dir = tempfile::tempdir().unwrap();
    let id = "550e8400-e29b-41d4-a716-446655440000";
    let original = format!(
        "---\n# keep me\nid: {id}\ntitle: T\nStatus: Reading\naliases: [a, b]\n---\nBody\n"
    );
    fs::write(dir.path().join("T.md"), &original).unwrap();
    let mut ws = Workspace::open(dir.path()).unwrap();
    let rev = ws.get_page(id).unwrap().body_rev;
    saved_rev(ws.save_page(id, Some(rev.as_str()), "New body\n").unwrap());
    let text = fs::read_to_string(dir.path().join("T.md")).unwrap();
    assert!(
        text.starts_with(&format!(
            "---\n# keep me\nid: {id}\ntitle: T\nStatus: Reading\naliases: [a, b]\ncreated_at: "
        )),
        "{text}"
    );
    assert!(text.ends_with("---\nNew body\n"));
}

#[test]
fn invalid_frontmatter_is_read_only_and_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let original = "---\ntitle: [broken\n---\nBody\n";
    fs::write(dir.path().join("Broken.md"), original).unwrap();
    let mut ws = Workspace::open(dir.path()).unwrap();
    let page = ws.list_children(None).unwrap().remove(0);
    assert!(matches!(page.status, PageStatus::InvalidFrontmatter { .. }));
    let rev = ws.get_page(&page.id).unwrap().body_rev;
    assert!(ws.save_page(&page.id, Some(rev.as_str()), "x\n").is_err());
    assert_eq!(
        fs::read_to_string(dir.path().join("Broken.md")).unwrap(),
        original
    );
}

#[test]
fn renaming_renames_the_file_and_avoids_collisions() {
    let (dir, mut ws) = temp_workspace();
    let a = ws.create_page(None, "Alpha").unwrap();
    ws.create_page(None, "Beta").unwrap();
    ws.set_title(&a.id, "beta").unwrap();
    // "beta" is taken by "Beta.md" on a case-insensitive file system.
    assert_eq!(user_files(dir.path()), ["Beta.md", "beta 2.md"]);
    assert_eq!(ws.get_page(&a.id).unwrap().title, "beta");
    ws.set_title(&a.id, "A/B: plan").unwrap();
    assert_eq!(user_files(dir.path()), ["A-B- plan.md", "Beta.md"]);
}

#[test]
fn adding_a_child_turns_a_leaf_into_a_bundle_and_fixes_relative_links() {
    let (dir, mut ws) = temp_workspace();
    let parent = ws.create_page(None, "Research").unwrap();
    let rev = ws.get_page(&parent.id).unwrap().body_rev;
    saved_rev(
        ws.save_page(&parent.id, Some(rev.as_str()), "![fig](images/fig.png)\n")
            .unwrap(),
    );
    let child = ws.create_page(Some(&parent.id), "Paper A").unwrap();

    assert_eq!(
        user_files(dir.path()),
        ["Research/Paper A.md", "Research/index.md"]
    );
    let doc = ws.get_page(&parent.id).unwrap();
    assert_eq!(doc.kind, PageKind::Bundle);
    assert_eq!(doc.body, "![fig](../images/fig.png)\n");
    let children = ws.list_children(Some(&parent.id)).unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].id, child.id);
    assert_eq!(ws.get_page(&child.id).unwrap().ancestors[0].id, parent.id);
}

#[test]
fn moving_pages_and_preventing_cycles() {
    let (dir, mut ws) = temp_workspace();
    let a = ws.create_page(None, "A").unwrap();
    let b = ws.create_page(Some(&a.id), "B").unwrap();
    let c = ws.create_page(None, "C").unwrap();

    assert!(
        ws.move_page(&a.id, Some(&b.id), None).is_err(),
        "into its own child"
    );
    assert!(
        ws.move_page(&a.id, Some(&a.id), None).is_err(),
        "into itself"
    );

    ws.move_page(&a.id, Some(&c.id), None).unwrap();
    assert_eq!(
        user_files(dir.path()),
        ["C/A/B.md", "C/A/index.md", "C/index.md"]
    );
    assert_eq!(ws.get_page(&b.id).unwrap().ancestors.len(), 2);

    ws.move_page(&b.id, None, None).unwrap();
    assert_eq!(
        user_files(dir.path()),
        ["B.md", "C/A/index.md", "C/index.md"]
    );
    // Reopening finds the same hierarchy.
    drop(ws);
    let ws = Workspace::open(dir.path()).unwrap();
    assert_eq!(
        ws.get_page(&a.id).unwrap().parent_id.as_deref(),
        Some(c.id.as_str())
    );
}

#[test]
fn trash_and_restore_a_subtree() {
    let (dir, mut ws) = temp_workspace();
    let a = ws.create_page(None, "A").unwrap();
    let b = ws.create_page(Some(&a.id), "B").unwrap();
    let entry = ws.trash_page(&a.id).unwrap();
    assert_eq!(entry.page_count, 2);
    assert!(user_files(dir.path()).is_empty());
    assert!(ws.get_page(&b.id).is_err());
    assert_eq!(ws.list_trash().len(), 1);

    let restored = ws.restore_trash(&entry.entry_id).unwrap();
    assert_eq!(restored.as_deref(), Some(a.id.as_str()));
    assert_eq!(user_files(dir.path()), ["A/B.md", "A/index.md"]);
    assert_eq!(
        ws.get_page(&b.id).unwrap().parent_id.as_deref(),
        Some(a.id.as_str())
    );
    assert!(ws.list_trash().is_empty());
}

#[test]
fn restoring_when_the_parent_is_gone_goes_to_the_root() {
    let (dir, mut ws) = temp_workspace();
    let a = ws.create_page(None, "A").unwrap();
    let b = ws.create_page(Some(&a.id), "B").unwrap();
    let child_entry = ws.trash_page(&b.id).unwrap();
    ws.trash_page(&a.id).unwrap();
    ws.restore_trash(&child_entry.entry_id).unwrap();
    assert_eq!(user_files(dir.path()), ["B.md"]);
    assert!(
        ws.restore_trash("../../etc").is_err(),
        "entry IDs are validated"
    );
}

#[test]
fn duplicate_gives_the_whole_subtree_new_ids() {
    let (dir, mut ws) = temp_workspace();
    let a = ws.create_page(None, "A").unwrap();
    let b = ws.create_page(Some(&a.id), "B").unwrap();
    let copy = ws.duplicate_page(&a.id).unwrap();
    assert_ne!(copy.id, a.id);
    assert_eq!(copy.title, "A (copy)");
    assert_eq!(
        user_files(dir.path()),
        ["A (copy)/B.md", "A (copy)/index.md", "A/B.md", "A/index.md"]
    );
    let copied_child = &ws.list_children(Some(&copy.id)).unwrap()[0];
    assert_ne!(copied_child.id, b.id);
    assert!(
        ws.info().notices.is_empty(),
        "no duplicate-ID notices: {:?}",
        ws.info().notices
    );
}

#[test]
fn copied_files_with_the_same_id_are_told_apart() {
    let dir = tempfile::tempdir().unwrap();
    let id = "550e8400-e29b-41d4-a716-446655440000";
    fs::write(
        dir.path().join("Original.md"),
        format!("---\nid: {id}\ntitle: Original\n---\nA\n"),
    )
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    fs::write(
        dir.path().join("Original copy.md"),
        format!("---\nid: {id}\ntitle: Copy\n---\nB\n"),
    )
    .unwrap();
    let mut ws = Workspace::open(dir.path()).unwrap();
    assert_eq!(ws.get_page(id).unwrap().title, "Original");
    assert_eq!(ws.info().notices.len(), 1);
    let copy = ws
        .list_children(None)
        .unwrap()
        .into_iter()
        .find(|p| p.title == "Copy")
        .unwrap();
    assert_ne!(copy.id, id);
    // Saving the copy writes its new ID, which settles it for good.
    let rev = ws.get_page(&copy.id).unwrap().body_rev;
    saved_rev(ws.save_page(&copy.id, Some(rev.as_str()), "B2\n").unwrap());
    drop(ws);
    let ws = Workspace::open(dir.path()).unwrap();
    assert!(ws.info().notices.is_empty());
    assert_eq!(ws.get_page(&copy.id).unwrap().title, "Copy");
}

#[test]
fn a_copy_keeps_its_new_id_when_edited_outside_diaspar() {
    let dir = tempfile::tempdir().unwrap();
    let id = "550e8400-e29b-41d4-a716-446655440000";
    fs::write(
        dir.path().join("A.md"),
        format!("---\nid: {id}\ntitle: A\n---\n"),
    )
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    let copy_path = dir.path().join("B.md");
    fs::write(&copy_path, format!("---\nid: {id}\ntitle: B\n---\n")).unwrap();
    let mut ws = Workspace::open(dir.path()).unwrap();
    assert_eq!(ws.take_notices().len(), 1);
    let copy_id = |ws: &Workspace| {
        ws.list_children(None)
            .unwrap()
            .into_iter()
            .find(|p| p.title == "B")
            .unwrap()
            .id
    };
    let before = copy_id(&ws);
    fs::write(
        &copy_path,
        format!("---\nid: {id}\ntitle: B\n---\nEdited elsewhere.\n"),
    )
    .unwrap();
    ws.reconcile();
    assert_eq!(
        copy_id(&ws),
        before,
        "the open copy must not turn into a new page"
    );
    assert!(ws.take_notices().is_empty(), "the notice isn't repeated");
    assert_eq!(ws.get_page(&before).unwrap().body, "Edited elsewhere.\n");
}

#[test]
fn reconcile_sees_external_edits_moves_and_deletes() {
    let (dir, mut ws) = temp_workspace();
    let a = ws.create_page(None, "A").unwrap();
    let b = ws.create_page(None, "B").unwrap();
    ws.take_changes();

    // Diaspar's own save produces no change report.
    let rev = ws.get_page(&a.id).unwrap().body_rev;
    saved_rev(ws.save_page(&a.id, Some(rev.as_str()), "mine\n").unwrap());
    ws.reconcile();
    assert!(ws.take_changes().is_empty());

    let a_path = dir.path().join("A.md");
    let text = fs::read_to_string(&a_path)
        .unwrap()
        .replace("mine", "theirs");
    fs::write(&a_path, text).unwrap();
    fs::create_dir(dir.path().join("Folder")).unwrap();
    fs::rename(
        dir.path().join("B.md"),
        dir.path().join("Folder/B moved.md"),
    )
    .unwrap();
    ws.reconcile();
    let changes = ws.take_changes();
    assert!(changes.content_changed.contains(&a.id));
    assert!(changes.tree_changed);
    // B kept its identity across the move (its ID is in the file).
    assert!(
        ws.get_page(&b.id)
            .unwrap()
            .ancestors
            .iter()
            .any(|p| p.title == "Folder")
    );

    fs::remove_file(dir.path().join("A.md")).unwrap();
    ws.reconcile();
    assert_eq!(ws.take_changes().removed, vec![a.id.clone()]);
}

#[test]
fn attachments_live_in_the_page_bundle_and_stay_inside_the_workspace() {
    let (dir, mut ws) = temp_workspace();
    let page = ws.create_page(None, "Trip").unwrap();
    let reference = ws
        .import_attachment(&page.id, "My Photo (1).PNG", b"png-bytes")
        .unwrap();
    assert_eq!(
        reference,
        "_assets/My Photo %281%29.png".replace(' ', "%20")
    );
    assert_eq!(
        user_files(dir.path()),
        ["Trip/_assets/My Photo (1).png", "Trip/index.md"]
    );
    let resolved = ws.resolve_asset(&page.id, &reference).unwrap();
    assert_eq!(fs::read(resolved).unwrap(), b"png-bytes");

    fs::write(dir.path().join("../outside-secret.txt"), "secret").ok();
    assert!(
        ws.resolve_asset(&page.id, "../../outside-secret.txt")
            .is_err()
    );
    assert!(ws.resolve_asset(&page.id, "/etc/passwd").is_err());
    assert!(
        ws.resolve_asset(&page.id, "../.diaspar/workspace.json")
            .is_err()
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("/etc/hosts", dir.path().join("Trip/_assets/link")).unwrap();
        assert!(
            ws.resolve_asset(&page.id, "_assets/link").is_err(),
            "symlinks out are refused"
        );
    }
    assert!(ws.import_attachment(&page.id, "notes.md", b"x").is_err());
}

#[test]
fn a_second_open_is_read_only() {
    let (dir, mut ws) = temp_workspace();
    let page = ws.create_page(None, "P").unwrap();
    let mut second = Workspace::open(dir.path()).unwrap();
    assert!(second.info().read_only_reason.is_some());
    assert!(second.create_page(None, "Q").is_err());
    let rev = second.get_page(&page.id).unwrap().body_rev;
    assert!(
        second
            .save_page(&page.id, Some(rev.as_str()), "x\n")
            .is_err()
    );
    drop(ws);
    drop(second);
    let third = Workspace::open(dir.path()).unwrap();
    assert!(
        third.info().read_only_reason.is_none(),
        "the lock is released on drop"
    );
}

#[test]
fn startup_cleans_up_after_a_crash() {
    let dir = tempfile::tempdir().unwrap();
    drop(Workspace::open(dir.path()).unwrap());
    let diaspar = dir.path().join(".diaspar");
    fs::write(diaspar.join("tmp/.Page.md.diaspar-tmp-1-2-3"), "half").unwrap();
    // A leaf-to-bundle conversion that stopped after `mkdir`.
    fs::write(dir.path().join("Page.md"), "text\n").unwrap();
    fs::create_dir(dir.path().join("Page")).unwrap();
    // A trash entry whose page never arrived.
    let orphan = diaspar.join("trash/550e8400-e29b-41d4-a716-446655440000");
    fs::create_dir_all(&orphan).unwrap();
    fs::write(orphan.join("entry.json"), "{}").unwrap();

    let ws = Workspace::open(dir.path()).unwrap();
    assert_eq!(fs::read_dir(diaspar.join("tmp")).unwrap().count(), 0);
    assert!(!dir.path().join("Page").exists());
    assert!(!orphan.exists());
    assert_eq!(ws.list_children(None).unwrap().len(), 1);
}

#[test]
fn order_and_favorites_persist() {
    let (dir, mut ws) = temp_workspace();
    let a = ws.create_page(None, "A").unwrap();
    let b = ws.create_page(None, "B").unwrap();
    let c = ws.create_page(None, "C").unwrap();
    ws.reorder(&c.id, 0).unwrap();
    ws.set_favorite(&b.id, true).unwrap();
    drop(ws);
    let ws = Workspace::open(dir.path()).unwrap();
    let titles: Vec<_> = ws
        .list_children(None)
        .unwrap()
        .into_iter()
        .map(|p| p.title)
        .collect();
    assert_eq!(titles, ["C", "A", "B"]);
    assert_eq!(
        ws.list_favorites()
            .into_iter()
            .map(|p| p.id)
            .collect::<Vec<_>>(),
        [b.id]
    );
    let _ = a;
}

#[test]
fn folder_pages_become_bundles_on_first_write() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("Projects")).unwrap();
    fs::write(dir.path().join("Projects/Plan.md"), "plan\n").unwrap();
    let mut ws = Workspace::open(dir.path()).unwrap();
    let folder = ws.list_children(None).unwrap().remove(0);
    assert_eq!(folder.kind, PageKind::Folder);
    assert!(ws.get_page(&folder.id).unwrap().rev.is_none());
    saved_rev(ws.save_page(&folder.id, None, "Overview\n").unwrap());
    assert_eq!(
        user_files(dir.path()),
        ["Projects/Plan.md", "Projects/index.md"]
    );
    let reopened = ws.list_children(None).unwrap().remove(0);
    assert_eq!(reopened.kind, PageKind::Bundle);
    assert_eq!(reopened.id, folder.id);
}

#[test]
fn recovery_buffers_round_trip() {
    let (_dir, mut ws) = temp_workspace();
    let page = ws.create_page(None, "P").unwrap();
    ws.write_recovery(&page.id, "unsaved words\n", "conflict")
        .unwrap();
    let buffers = ws.list_recovery();
    assert_eq!(buffers.len(), 1);
    assert_eq!(buffers[0].body, "unsaved words\n");
    ws.discard_recovery(&page.id).unwrap();
    assert!(ws.list_recovery().is_empty());
    assert!(ws.write_recovery("../escape", "x", "y").is_err());
}

#[test]
fn refuses_home_and_system_folders() {
    if let Some(home) = std::env::home_dir() {
        assert!(Workspace::open(&home).is_err());
    }
    assert!(Workspace::open(Path::new("/")).is_err());
    let _ = root;
}

#[test]
fn frontmatter_changes_elsewhere_dont_conflict_and_are_kept() {
    let (dir, mut ws) = temp_workspace();
    let page = ws.create_page(None, "Note").unwrap();
    let base = ws.get_page(&page.id).unwrap().body_rev;
    // Another tool adds a tag; the body is untouched.
    let path = dir.path().join("Note.md");
    let edited =
        fs::read_to_string(&path)
            .unwrap()
            .replacen("---\n", "---\ntags:\n  - from-elsewhere\n", 1);
    fs::write(&path, edited).unwrap();

    saved_rev(ws.save_page(&page.id, Some(&base), "My words\n").unwrap());
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("  - from-elsewhere\n"), "{text}");
    assert!(text.ends_with("---\nMy words\n"));
}

#[test]
fn renaming_the_open_page_doesnt_break_its_next_save() {
    let (_dir, mut ws) = temp_workspace();
    let page = ws.create_page(None, "Draft").unwrap();
    let base = ws.get_page(&page.id).unwrap().body_rev;
    let SaveOutcome::Saved { body_rev, .. } = ws.save_page(&page.id, Some(&base), "v1\n").unwrap()
    else {
        panic!("expected a save");
    };
    ws.set_title(&page.id, "Final").unwrap();
    // The editor still holds the body revision from before the rename.
    saved_rev(ws.save_page(&page.id, Some(&body_rev), "v2\n").unwrap());
    let doc = ws.get_page(&page.id).unwrap();
    assert_eq!(doc.title, "Final");
    assert_eq!(doc.body, "v2\n");
}

#[test]
fn counting_entries_stops_at_the_limit_and_skips_hidden_ones() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("a/b")).unwrap();
    fs::write(dir.path().join("a/b/one.md"), "").unwrap();
    fs::write(dir.path().join("two.md"), "").unwrap();
    fs::create_dir_all(dir.path().join(".git/objects")).unwrap();
    fs::write(dir.path().join(".git/objects/x"), "").unwrap();
    // a, a/b, a/b/one.md, two.md
    assert_eq!(super::count_entries(dir.path(), 100), 4);
    assert_eq!(super::count_entries(dir.path(), 2), 3);
    assert!(!super::is_workspace(dir.path()));
    drop(Workspace::open(dir.path()).unwrap());
    assert!(super::is_workspace(dir.path()));
}

// ------------------------------------------------------------ Phase 3: index

fn body_rev(ws: &Workspace, id: &str) -> String {
    ws.get_page(id).unwrap().body_rev
}

fn save(ws: &mut Workspace, id: &str, body: &str) {
    let base = body_rev(ws, id);
    saved_rev(ws.save_page(id, Some(base.as_str()), body).unwrap());
}

fn titles(hits: &[super::types::PageHit]) -> Vec<String> {
    hits.iter().map(|h| h.title.clone()).collect()
}

#[test]
fn search_finds_text_and_follows_edits() {
    let (_dir, mut ws) = temp_workspace();
    let a = ws.create_page(None, "Reading list").unwrap();
    let b = ws.create_page(None, "知识管理").unwrap();
    save(&mut ws, &a.id, "Transformers and protein design.\n");
    save(
        &mut ws,
        &b.id,
        "今天讨论注意力机制在蛋白质结构预测中的应用。\n",
    );

    assert_eq!(titles(&ws.search("protein", 20)), vec!["Reading list"]);
    assert_eq!(
        titles(&ws.search("prot", 20)),
        vec!["Reading list"],
        "prefix"
    );
    assert_eq!(
        titles(&ws.search("tein", 20)),
        vec!["Reading list"],
        "inside a word"
    );
    assert_eq!(titles(&ws.search("蛋白质", 20)), vec!["知识管理"]);
    assert_eq!(
        titles(&ws.search("预测", 20)),
        vec!["知识管理"],
        "two characters"
    );
    assert_eq!(titles(&ws.search("知识", 20)), vec!["知识管理"], "title");
    let hit = &ws.search("protein", 20)[0];
    assert!(
        hit.snippet
            .as_deref()
            .unwrap()
            .contains("\u{1}protein\u{2}")
    );

    save(&mut ws, &a.id, "Now about gardening.\n");
    assert!(ws.search("protein", 20).is_empty());
    assert_eq!(titles(&ws.search("gardening", 20)), vec!["Reading list"]);

    ws.set_title(&a.id, "Garden notes").unwrap();
    assert_eq!(titles(&ws.search("garden", 20)), vec!["Garden notes"]);
    ws.trash_page(&a.id).unwrap();
    assert!(
        ws.search("gardening", 20).is_empty(),
        "trashed pages aren't found"
    );
}

#[test]
fn quick_open_ranks_title_starts_first() {
    let (_dir, mut ws) = temp_workspace();
    for title in [
        "Project notes",
        "Side project",
        "Projection math",
        "Unrelated",
    ] {
        ws.create_page(None, title).unwrap();
    }
    assert_eq!(
        titles(&ws.quick_open("proj", 10)),
        vec!["Project notes", "Projection math", "Side project"]
    );
    assert!(ws.quick_open("  ", 10).is_empty());
}

#[test]
fn the_index_cache_survives_restarts_and_sees_outside_edits() {
    let dir = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("No id.md"),
        "Plain file without frontmatter\n",
    )
    .unwrap();
    let (page, plain_id) = {
        let mut ws = Workspace::open_with(dir.path(), Some(cache.path())).unwrap();
        let page = ws.create_page(None, "Kept").unwrap();
        save(&mut ws, &page.id, "first version\n");
        let plain = ws.quick_open("no id", 5)[0].id.clone();
        (page, plain)
    };
    let file = dir.path().join("Kept.md");
    let text = fs::read_to_string(&file)
        .unwrap()
        .replace("first version", "second version");
    fs::write(&file, text).unwrap();

    let ws = Workspace::open_with(dir.path(), Some(cache.path())).unwrap();
    assert_eq!(titles(&ws.search("second", 10)), vec!["Kept"]);
    assert!(ws.search("first", 10).is_empty());
    assert_eq!(ws.get_page(&page.id).unwrap().title, "Kept");
    assert_eq!(
        ws.quick_open("no id", 5)[0].id,
        plain_id,
        "a page without an id keeps its ID across restarts"
    );
    assert!(user_files(dir.path()).iter().all(|f| !f.ends_with(".db")));
}

#[test]
fn page_links_survive_renames_and_moves_and_wikilinks_resolve_by_title() {
    let (_dir, mut ws) = temp_workspace();
    let a = ws.create_page(None, "Source").unwrap();
    let b = ws.create_page(None, "Target").unwrap();
    let c = ws.create_page(None, "Wiki Target").unwrap();
    let folder = ws.create_page(None, "Folder").unwrap();
    ws.ensure_persisted_id(&b.id).unwrap();
    save(
        &mut ws,
        &a.id,
        &format!(
            "See [Target](diaspar://page/{}) and [[wiki target]].\n",
            b.id
        ),
    );
    let ids = |hits: Vec<super::types::PageHit>| hits.into_iter().map(|h| h.id).collect::<Vec<_>>();
    assert_eq!(ids(ws.backlinks(&b.id).unwrap()), vec![a.id.clone()]);
    assert_eq!(ids(ws.backlinks(&c.id).unwrap()), vec![a.id.clone()]);

    ws.set_title(&b.id, "Renamed target").unwrap();
    ws.move_page(&b.id, Some(&folder.id), None).unwrap();
    assert_eq!(ids(ws.backlinks(&b.id).unwrap()), vec![a.id.clone()]);
    let info = &ws.resolve_pages(std::slice::from_ref(&b.id))[0];
    assert_eq!(info.title.as_deref(), Some("Renamed target"));

    let wiki = ws.resolve_wikilinks(&[
        "Wiki Target".into(),
        "Folder/Renamed target".into(),
        "Nothing".into(),
    ]);
    assert_eq!(wiki[0].target.as_ref().unwrap().id, c.id);
    assert_eq!(wiki[1].target.as_ref().unwrap().id, b.id, "path suffix");
    assert_eq!((wiki[2].matches, wiki[2].target.is_none()), (0, true));

    ws.create_page(None, "Wiki Target").unwrap();
    let ambiguous = ws.resolve_wikilinks(&["wiki target".into()]);
    assert_eq!(
        (ambiguous[0].matches, ambiguous[0].target.is_none()),
        (2, true)
    );
    assert!(
        ws.backlinks(&c.id).unwrap().is_empty(),
        "an ambiguous wikilink links nowhere"
    );
}

#[test]
fn links_to_trashed_pages_heal_on_restore() {
    let (_dir, mut ws) = temp_workspace();
    let page = ws.create_page(None, "Going away").unwrap();
    ws.ensure_persisted_id(&page.id).unwrap();
    let entry = ws.trash_page(&page.id).unwrap();
    let info = &ws.resolve_pages(std::slice::from_ref(&page.id))[0];
    assert_eq!(info.state, super::types::LinkState::Trashed);
    assert_eq!(info.title.as_deref(), Some("Going away"));
    let missing = &ws.resolve_pages(&["11111111-1111-4111-8111-111111111111".into()])[0];
    assert_eq!(missing.state, super::types::LinkState::Missing);
    ws.restore_trash(&entry.entry_id).unwrap();
    let info = &ws.resolve_pages(std::slice::from_ref(&page.id))[0];
    assert_eq!(info.state, super::types::LinkState::Ok);
}

#[test]
fn a_rebuilt_index_equals_the_incremental_one() {
    let dir = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let mut ws = Workspace::open_with(dir.path(), Some(cache.path())).unwrap();
    let a = ws.create_page(None, "Alpha").unwrap();
    let b = ws.create_page(Some(&a.id), "Beta").unwrap();
    ws.ensure_persisted_id(&b.id).unwrap();
    save(
        &mut ws,
        &a.id,
        &format!("Link to [Beta](diaspar://page/{}) and [[Gamma]].\n", b.id),
    );
    save(&mut ws, &b.id, "中文内容 and words\n");
    fs::create_dir(dir.path().join("Loose folder")).unwrap();
    fs::write(
        dir.path().join("Loose folder/Note.md"),
        "---\ntags: [x, y]\nStatus: Done\n---\nText\n",
    )
    .unwrap();
    ws.reconcile();

    let before = ws.db.dump();
    ws.rebuild_index().unwrap();
    assert_eq!(ws.db.dump(), before);
    assert!(!cache.path().read_dir().unwrap().flatten().any(|d| {
        d.path()
            .read_dir()
            .unwrap()
            .flatten()
            .any(|f| f.file_name().to_string_lossy().contains("rebuild"))
    }));
    assert_eq!(titles(&ws.search("中文内容", 10)), vec!["Beta"]);
}

// ------------------------------------------------------- Phase 3: collections

use super::collections::{
    CollectionViews, FilterRule, SchemaOp, SortDirection, SortRule, ViewKind,
};
use crate::collection::{Cell, CellInput, PropertyType};

fn add_property(ws: &mut Workspace, collection: &str, name: &str, kind: PropertyType) -> String {
    let schema = ws
        .update_schema(
            collection,
            &SchemaOp::AddProperty {
                name: name.into(),
                kind,
            },
        )
        .unwrap();
    schema
        .properties
        .iter()
        .find(|p| p.name == name)
        .unwrap()
        .id
        .clone()
}

fn add_option(ws: &mut Workspace, collection: &str, property: &str, name: &str) -> String {
    let schema = ws
        .update_schema(
            collection,
            &SchemaOp::AddOption {
                property_id: property.into(),
                name: name.into(),
            },
        )
        .unwrap();
    let p = schema.properties.iter().find(|p| p.id == property).unwrap();
    p.options
        .iter()
        .find(|o| o.name == name)
        .unwrap()
        .id
        .clone()
}

fn cell(ws: &Workspace, collection: &str, entry: &str, property: &str) -> Cell {
    let data = ws.get_collection(collection).unwrap();
    data.rows.iter().find(|r| r.id == entry).unwrap().cells[property].clone()
}

#[test]
fn collections_hold_typed_values_that_persist() {
    let (dir, mut ws) = temp_workspace();
    let papers = ws.create_collection(None, "Papers").unwrap();
    assert!(papers.is_collection);
    let index = fs::read_to_string(dir.path().join("Papers/index.md")).unwrap();
    assert!(
        index.contains("collection:\n  version: 1\n  properties: []\n"),
        "{index}"
    );

    let status = add_property(&mut ws, &papers.id, "Status", PropertyType::Select);
    add_option(&mut ws, &papers.id, &status, "To read");
    add_option(&mut ws, &papers.id, &status, "Reading");
    let year = add_property(&mut ws, &papers.id, "Year", PropertyType::Number);
    let authors = add_property(&mut ws, &papers.id, "Authors", PropertyType::MultiSelect);
    add_option(&mut ws, &papers.id, &authors, "Vaswani");
    let published = add_property(&mut ws, &papers.id, "Published", PropertyType::Date);
    let read = add_property(&mut ws, &papers.id, "Read", PropertyType::Checkbox);
    let notes = add_property(&mut ws, &papers.id, "Notes", PropertyType::Text);

    let paper = ws
        .create_page(Some(&papers.id), "Attention Is All You Need")
        .unwrap();
    for (property, input) in [
        (
            &status,
            CellInput::Select {
                value: "Reading".into(),
            },
        ),
        (&year, CellInput::Number { value: 2017.0 }),
        (
            &authors,
            CellInput::MultiSelect {
                values: vec!["Vaswani".into()],
            },
        ),
        (
            &published,
            CellInput::Date {
                value: "2017-06-12".into(),
            },
        ),
        (&read, CellInput::Checkbox { value: false }),
        (
            &notes,
            CellInput::Text {
                value: "yes".into(),
            },
        ),
    ] {
        ws.set_property(&paper.id, property, &input).unwrap();
    }
    let file = fs::read_to_string(dir.path().join("Papers/Attention Is All You Need.md")).unwrap();
    for line in [
        "Status: Reading\n",
        "Year: 2017\n",
        "Authors:\n  - Vaswani\n",
        "Published: 2017-06-12\n",
        "Read: false\n",
        "Notes: \"yes\"\n",
    ] {
        assert!(file.contains(line), "missing {line:?} in\n{file}");
    }
    ws.set_property(&paper.id, &notes, &CellInput::Empty)
        .unwrap();
    let file = fs::read_to_string(dir.path().join("Papers/Attention Is All You Need.md")).unwrap();
    assert!(!file.contains("Notes"), "an empty value is an absent key");
    assert!(
        ws.set_property(
            &paper.id,
            &status,
            &CellInput::Select {
                value: "Done".into()
            }
        )
        .is_err()
    );
    assert_eq!(
        titles(&ws.search("vaswani", 10)),
        vec!["Attention Is All You Need"]
    );

    drop(ws);
    let ws = Workspace::open(dir.path()).unwrap();
    assert_eq!(
        cell(&ws, &papers.id, &paper.id, &year),
        Cell::Number {
            value: 2017.0,
            from_text: false
        }
    );
    assert_eq!(
        cell(&ws, &papers.id, &paper.id, &status),
        Cell::Select {
            value: "Reading".into(),
            known: true
        }
    );
    assert_eq!(cell(&ws, &papers.id, &paper.id, &notes), Cell::Empty);
    let data = ws.get_collection(&papers.id).unwrap();
    assert_eq!(data.schema.unwrap().properties.len(), 6);
    assert!(data.problem.is_none());
    assert!(data.rows[0].editable);
    let entry = ws.get_entry(&paper.id).unwrap().unwrap();
    assert_eq!(entry.collection_id, papers.id);
    assert_eq!(
        entry.row.cells[&year],
        Cell::Number {
            value: 2017.0,
            from_text: false
        }
    );
    assert!(
        ws.get_entry(&papers.id).unwrap().is_none(),
        "the collection itself isn't an entry"
    );
}

#[test]
fn invalid_values_are_shown_and_never_rewritten() {
    let (dir, mut ws) = temp_workspace();
    let c = ws.create_collection(None, "Books").unwrap();
    let year = add_property(&mut ws, &c.id, "Year", PropertyType::Number);
    let read = add_property(&mut ws, &c.id, "Read", PropertyType::Checkbox);
    fs::write(
        dir.path().join("Books/Odd.md"),
        "---\ntitle: Odd\nYear: twelve # hand-written\nOther: kept\n---\nBody\n",
    )
    .unwrap();
    ws.reconcile();
    let odd = ws.quick_open("odd", 1)[0].id.clone();
    assert!(matches!(
        cell(&ws, &c.id, &odd, &year),
        Cell::Invalid { .. }
    ));
    ws.set_property(&odd, &read, &CellInput::Checkbox { value: true })
        .unwrap();
    let text = fs::read_to_string(dir.path().join("Books/Odd.md")).unwrap();
    assert!(
        text.contains("Year: twelve # hand-written\nOther: kept\n"),
        "{text}"
    );
    assert!(text.contains("Read: true\n"));
}

#[test]
fn renaming_a_property_or_option_rewrites_entries_as_one_batch() {
    let (dir, mut ws) = temp_workspace();
    let c = ws.create_collection(None, "Tasks").unwrap();
    let state = add_property(&mut ws, &c.id, "State", PropertyType::Select);
    add_option(&mut ws, &c.id, &state, "Todo");
    let done = add_option(&mut ws, &c.id, &state, "Done");
    let labels = add_property(&mut ws, &c.id, "Labels", PropertyType::MultiSelect);
    let urgent = add_option(&mut ws, &c.id, &labels, "Urgent");
    add_option(&mut ws, &c.id, &labels, "Later");
    let a = ws.create_page(Some(&c.id), "A").unwrap();
    let b = ws.create_page(Some(&c.id), "B").unwrap();
    ws.set_property(
        &a.id,
        &state,
        &CellInput::Select {
            value: "Done".into(),
        },
    )
    .unwrap();
    ws.set_property(
        &a.id,
        &labels,
        &CellInput::MultiSelect {
            values: vec!["Urgent".into(), "Later".into()],
        },
    )
    .unwrap();
    ws.set_property(
        &b.id,
        &state,
        &CellInput::Select {
            value: "Todo".into(),
        },
    )
    .unwrap();

    ws.update_schema(
        &c.id,
        &SchemaOp::RenameProperty {
            property_id: state.clone(),
            name: "Status: now".into(),
        },
    )
    .unwrap();
    ws.update_schema(
        &c.id,
        &SchemaOp::RenameOption {
            property_id: state.clone(),
            option_id: done,
            name: "Finished".into(),
        },
    )
    .unwrap();
    ws.update_schema(
        &c.id,
        &SchemaOp::RenameOption {
            property_id: labels.clone(),
            option_id: urgent,
            name: "Now".into(),
        },
    )
    .unwrap();
    let a_text = fs::read_to_string(dir.path().join("Tasks/A.md")).unwrap();
    assert!(a_text.contains("'Status: now': Finished\n"), "{a_text}");
    assert!(a_text.contains("Labels:\n  - Now\n  - Later\n"), "{a_text}");
    assert!(!a_text.contains("State:"));
    let b_text = fs::read_to_string(dir.path().join("Tasks/B.md")).unwrap();
    assert!(b_text.contains("'Status: now': Todo\n"), "{b_text}");
    assert_eq!(
        cell(&ws, &c.id, &a.id, &state),
        Cell::Select {
            value: "Finished".into(),
            known: true
        }
    );
    assert!(
        fs::read_dir(dir.path().join(".diaspar/journal"))
            .unwrap()
            .next()
            .is_none()
    );

    // An entry that already has a key with the new name would end up with two
    // keys, so the rename is refused and nothing changes.
    let a_file = dir.path().join("Tasks/A.md");
    let with_priority = fs::read_to_string(&a_file)
        .unwrap()
        .replace("title: A\n", "title: A\nPriority: high\n");
    fs::write(&a_file, &with_priority).unwrap();
    ws.reconcile();
    let b_before = fs::read_to_string(dir.path().join("Tasks/B.md")).unwrap();
    let refused = ws.update_schema(
        &c.id,
        &SchemaOp::RenameProperty {
            property_id: labels,
            name: "priority".into(),
        },
    );
    assert!(refused.is_err());
    assert_eq!(fs::read_to_string(&a_file).unwrap(), with_priority);
    assert_eq!(
        fs::read_to_string(dir.path().join("Tasks/B.md")).unwrap(),
        b_before
    );
}

#[test]
fn an_interrupted_rename_is_rolled_back_on_the_next_start() {
    let dir = tempfile::tempdir().unwrap();
    let mut ws = Workspace::open(dir.path()).unwrap();
    let c = ws.create_collection(None, "Tasks").unwrap();
    let state = add_property(&mut ws, &c.id, "State", PropertyType::Text);
    for title in ["A", "B", "C"] {
        let page = ws.create_page(Some(&c.id), title).unwrap();
        ws.set_property(&page.id, &state, &CellInput::Text { value: "x".into() })
            .unwrap();
    }
    let snapshot = |dir: &Path| {
        let mut files: Vec<(String, String)> = user_files(dir)
            .into_iter()
            .map(|f| (f.clone(), fs::read_to_string(dir.join(&f)).unwrap()))
            .collect();
        files.sort();
        files
    };
    let before = snapshot(dir.path());
    ws.crash_after_steps = Some(2);
    ws.update_schema(
        &c.id,
        &SchemaOp::RenameProperty {
            property_id: state,
            name: "Renamed".into(),
        },
    )
    .unwrap();
    assert_ne!(
        snapshot(dir.path()),
        before,
        "two files were rewritten before the crash"
    );
    drop(ws);

    let mut ws = Workspace::open(dir.path()).unwrap();
    assert_eq!(snapshot(dir.path()), before, "every file is back as it was");
    assert!(ws.take_notices().iter().any(|n| n.contains("undone")));
    assert!(
        fs::read_dir(dir.path().join(".diaspar/journal"))
            .unwrap()
            .next()
            .is_none()
    );
}

#[test]
fn a_collection_can_become_a_regular_page_again() {
    let (dir, mut ws) = temp_workspace();
    let page = ws.create_page(None, "Ideas").unwrap();
    ws.convert_to_collection(&page.id).unwrap();
    let p = add_property(&mut ws, &page.id, "Score", PropertyType::Number);
    let entry = ws.create_page(Some(&page.id), "One").unwrap();
    ws.set_property(&entry.id, &p, &CellInput::Number { value: 3.0 })
        .unwrap();
    ws.remove_collection(&page.id).unwrap();
    assert!(!ws.get_page(&page.id).unwrap().is_collection);
    assert!(ws.get_collection(&page.id).is_err());
    let entry_text = fs::read_to_string(dir.path().join("Ideas/One.md")).unwrap();
    assert!(
        entry_text.contains("Score: 3\n"),
        "values stay as plain keys"
    );
    // Adding the schema back brings the value back.
    ws.convert_to_collection(&page.id).unwrap();
    let p = add_property(&mut ws, &page.id, "Score", PropertyType::Number);
    assert_eq!(
        cell(&ws, &page.id, &entry.id, &p),
        Cell::Number {
            value: 3.0,
            from_text: false
        }
    );
}

#[test]
fn view_settings_round_trip_and_newer_schemas_are_read_only() {
    let (dir, mut ws) = temp_workspace();
    let c = ws.create_collection(None, "Views").unwrap();
    assert_eq!(ws.get_views(&c.id), CollectionViews::default());
    let mut views = CollectionViews {
        active_view: ViewKind::List,
        ..Default::default()
    };
    views.table.sort.push(SortRule {
        property: "title".into(),
        direction: SortDirection::Desc,
    });
    views.table.filter.push(FilterRule {
        property: "x".into(),
        op: "contains".into(),
        value: Some("a".into()),
    });
    views.table.column_widths.insert("title".into(), 280);
    ws.save_views(&c.id, &views).unwrap();
    assert_eq!(ws.get_views(&c.id), views);
    let json = fs::read_to_string(dir.path().join(".diaspar/views.json")).unwrap();
    assert!(json.contains("\"active_view\": \"list\""), "{json}");

    let file = dir.path().join("Views/index.md");
    let text = fs::read_to_string(&file)
        .unwrap()
        .replace("version: 1", "version: 2");
    fs::write(&file, text).unwrap();
    ws.reconcile();
    let data = ws.get_collection(&c.id).unwrap();
    assert!(data.problem.unwrap().contains("newer"));
    assert!(
        ws.update_schema(
            &c.id,
            &SchemaOp::AddProperty {
                name: "X".into(),
                kind: PropertyType::Text
            }
        )
        .is_err()
    );
}
