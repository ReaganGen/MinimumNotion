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

/// Every file under `dir` except `.forma/`, as relative paths.
fn user_files(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) {
        for item in fs::read_dir(dir).unwrap().flatten() {
            let path = item.path();
            if item.file_name() == ".forma" {
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
fn opening_a_folder_writes_only_inside_forma() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("Notes.md"), "# Hello\n\nNo frontmatter.\n").unwrap();
    let ws = Workspace::open(dir.path()).unwrap();
    assert_eq!(user_files(dir.path()), ["Notes.md"]);
    assert_eq!(
        fs::read_to_string(dir.path().join("Notes.md")).unwrap(),
        "# Hello\n\nNo frontmatter.\n"
    );
    assert!(dir.path().join(".forma/workspace.json").is_file());
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
    // Someone edits the file outside Forma.
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
    let snapshots: Vec<_> = fs::read_dir(dir.path().join(".forma/history").join(&id))
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
fn a_copy_keeps_its_new_id_when_edited_outside_forma() {
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

    // Forma's own save produces no change report.
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
        ws.resolve_asset(&page.id, "../.forma/workspace.json")
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
    let forma = dir.path().join(".forma");
    fs::write(forma.join("tmp/.Page.md.forma-tmp-1-2-3"), "half").unwrap();
    // A leaf-to-bundle conversion that stopped after `mkdir`.
    fs::write(dir.path().join("Page.md"), "text\n").unwrap();
    fs::create_dir(dir.path().join("Page")).unwrap();
    // A trash entry whose page never arrived.
    let orphan = forma.join("trash/550e8400-e29b-41d4-a716-446655440000");
    fs::create_dir_all(&orphan).unwrap();
    fs::write(orphan.join("entry.json"), "{}").unwrap();

    let ws = Workspace::open(dir.path()).unwrap();
    assert_eq!(fs::read_dir(forma.join("tmp")).unwrap().count(), 0);
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
