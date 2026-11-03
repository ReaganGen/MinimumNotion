//! Multi-file batches (DATA_MODEL §7.6): renaming a collection property or
//! option rewrites every entry that uses it. A journal lists every step
//! before the first file is written; if Forma stops halfway, the next start
//! rolls the whole batch back.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::atomic::write_atomic_staged;
use crate::error::{CoreResult, io_at};

use super::history;
use super::scan::{hash_rev, new_id};
use super::state::{FORMAT_VERSION, load_json, save_json};
use super::{Workspace, now_rfc3339};

/// One file in a batch.
pub(super) struct BatchWrite {
    pub id: String,
    pub file: PathBuf,
    pub old_text: String,
    pub new_text: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct JournalFile {
    format_version: u32,
    journal_id: String,
    kind: String,
    started_at: String,
    steps: Vec<JournalStep>,
}

#[derive(Debug, Serialize, Deserialize)]
struct JournalStep {
    page_id: String,
    /// The page file, relative to the workspace root.
    path: String,
    before_rev: String,
    after_rev: String,
    /// The snapshot of the file before the batch, relative to `.forma/`.
    snapshot: String,
}

impl Workspace {
    fn journal_dir(&self) -> PathBuf {
        self.forma().join("journal")
    }

    /// Writes every file in `writes`, or (after a crash) none of them.
    pub(super) fn apply_batch(&mut self, kind: &str, writes: Vec<BatchWrite>) -> CoreResult<()> {
        let forma = self.forma();
        let mut steps = Vec::new();
        for write in &writes {
            let snapshot = history::snapshot(
                &forma.join("history"),
                &write.id,
                &write.old_text,
                "batch",
                &self.staging(),
            )?;
            steps.push(JournalStep {
                page_id: write.id.clone(),
                path: relative(&self.root, &write.file),
                before_rev: hash_rev(write.old_text.as_bytes()),
                after_rev: hash_rev(write.new_text.as_bytes()),
                snapshot: relative(&forma, &snapshot),
            });
        }
        let journal_id = new_id();
        let journal_path = self.journal_dir().join(format!("{journal_id}.json"));
        let journal = JournalFile {
            format_version: FORMAT_VERSION,
            journal_id,
            kind: kind.into(),
            started_at: now_rfc3339(),
            steps,
        };
        save_json(&journal_path, &journal, &self.staging())?;

        for (done, write) in writes.iter().enumerate() {
            #[cfg(test)]
            if self.crash_after_steps == Some(done) {
                return Ok(());
            }
            let _ = done;
            // Snapshots were taken above, so no further snapshot here.
            self.write_page_file(&write.id, &write.file, &write.new_text, None, None)?;
        }
        fs::remove_file(&journal_path).map_err(io_at(&journal_path))?;
        Ok(())
    }

    /// Rolls back batches that didn't finish (startup recovery). Each step is
    /// undone only if its file is exactly as the batch left it.
    pub(super) fn roll_back_journals(&mut self) {
        let dir = self.journal_dir();
        let Ok(read) = fs::read_dir(&dir) else { return };
        for item in read.flatten() {
            let path = item.path();
            if path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            let journal: JournalFile = match load_json(&path) {
                Ok(Some(journal)) => journal,
                _ => {
                    log::warn!("unreadable journal {path:?} left in place");
                    continue;
                }
            };
            let mut untouched = 0;
            for step in &journal.steps {
                let file = self.root.join(&step.path);
                let current = fs::read(&file).ok().map(|b| hash_rev(&b));
                if current.as_deref() == Some(step.before_rev.as_str()) {
                    continue;
                }
                if current.as_deref() == Some(step.after_rev.as_str()) {
                    let snapshot = self.forma().join(&step.snapshot);
                    match fs::read(&snapshot) {
                        Ok(old) => {
                            if let Err(err) =
                                write_atomic_staged(&file, &old, Some(&self.staging()))
                            {
                                log::warn!("couldn't roll back {file:?}: {err}");
                            }
                        }
                        Err(err) => log::warn!("snapshot {snapshot:?} missing: {err}"),
                    }
                } else {
                    untouched += 1;
                }
            }
            let mut notice = "An interrupted change to a collection was undone.".to_string();
            if untouched > 0 {
                notice.push_str(&format!(
                    " {untouched} page(s) had changed since and were left as they are."
                ));
            }
            self.notices.push(notice);
            let _ = fs::remove_file(&path);
        }
    }
}

fn relative(base: &Path, path: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}
