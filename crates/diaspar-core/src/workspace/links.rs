//! Page links, wikilinks and backlinks (ARCHITECTURE §6.5, DATA_MODEL §5.4).
//! Links are never rewritten in other files; they are resolved when read.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::Path;

use super::naming::name_key;
use super::state::load_json;
use super::types::{LinkState, PageHit, PageLinkInfo, PageRef, WikiResolution};
use super::{TrashRecord, Workspace};
use crate::error::CoreResult;

impl Workspace {
    /// What each page link points at now: its current title, or that it's in
    /// the trash or gone.
    pub fn resolve_pages(&self, ids: &[String]) -> Vec<PageLinkInfo> {
        let trashed = self.trashed_titles();
        ids.iter()
            .map(|id| {
                let id = id.to_ascii_lowercase();
                match self.index.get(&id) {
                    Some(entry) => PageLinkInfo {
                        id,
                        title: Some(entry.title.clone()),
                        icon: entry.icon.clone(),
                        state: LinkState::Ok,
                    },
                    None => match trashed.get(&id) {
                        Some(title) => PageLinkInfo {
                            id,
                            title: Some(title.clone()),
                            icon: None,
                            state: LinkState::Trashed,
                        },
                        None => PageLinkInfo {
                            id,
                            title: None,
                            icon: None,
                            state: LinkState::Missing,
                        },
                    },
                }
            })
            .collect()
    }

    /// Resolves wikilink texts such as `Title`, `Folder/Title` or `Title#Heading`.
    pub fn resolve_wikilinks(&self, texts: &[String]) -> Vec<WikiResolution> {
        let resolver = self.wiki_resolver();
        texts
            .iter()
            .map(|text| {
                let matches = resolver.matches(text);
                let target = match matches.as_slice() {
                    [only] => self.index.get(only).map(|e| PageRef {
                        id: e.id.clone(),
                        title: e.title.clone(),
                        icon: e.icon.clone(),
                    }),
                    _ => None,
                };
                WikiResolution {
                    text: text.clone(),
                    target,
                    matches: u32::try_from(matches.len()).unwrap_or(u32::MAX),
                }
            })
            .collect()
    }

    /// Pages that link to `id`, by page link or by a wikilink that resolves to it.
    pub fn backlinks(&self, id: &str) -> CoreResult<Vec<PageHit>> {
        self.entry(id)?;
        let mut sources: HashSet<String> = self
            .db
            .page_links_to(id)
            .unwrap_or_else(|err| {
                log::warn!("couldn't read backlinks: {err}");
                Vec::new()
            })
            .into_iter()
            .collect();
        // Only wikilinks whose last part names this page (by title or file
        // name) can resolve to it; the rest are skipped without resolving.
        let entry = self.entry(id)?;
        let names = [
            name_key(&entry.title),
            self.logical_path(entry.location(), entry.dir.is_some())
                .pop()
                .unwrap_or_default(),
        ];
        let resolver = self.wiki_resolver();
        for (source, text) in self.db.wiki_links().unwrap_or_default() {
            let last = name_key(wiki_target(&text).rsplit('/').next().unwrap_or(""));
            if names.contains(&last) && resolver.matches(&text) == [id] {
                sources.insert(source);
            }
        }
        sources.remove(id);
        let mut hits: Vec<PageHit> = sources
            .iter()
            .filter_map(|source| self.hit(source, None))
            .collect();
        hits.sort_by_key(|hit| name_key(&hit.title));
        Ok(hits)
    }

    /// Title, file-name and path lookups for resolving wikilinks, built once
    /// per request instead of once per link.
    fn wiki_resolver(&self) -> WikiResolver {
        let mut resolver = WikiResolver::default();
        for entry in self.index.entries.values() {
            let path = self.logical_path(entry.location(), entry.dir.is_some());
            resolver
                .by_title
                .entry(name_key(&entry.title))
                .or_default()
                .push(entry.id.clone());
            if let Some(stem) = path.last() {
                resolver
                    .by_stem
                    .entry(stem.clone())
                    .or_default()
                    .push(entry.id.clone());
            }
            resolver.paths.push((path, entry.id.clone()));
        }
        resolver
    }

    /// A page's path inside the workspace without `.md`, as name keys.
    fn logical_path(&self, location: &Path, is_dir: bool) -> Vec<String> {
        let relative = location.strip_prefix(&self.root).unwrap_or(location);
        let mut parts: Vec<String> = relative
            .iter()
            .map(|p| name_key(&p.to_string_lossy()))
            .collect();
        if !is_dir
            && let Some(last) = parts.last_mut()
            && let Some(stem) = last
                .strip_suffix(".md")
                .or_else(|| last.strip_suffix(".markdown"))
        {
            *last = stem.to_string();
        }
        parts
    }

    /// Titles of every page in the trash, by ID.
    fn trashed_titles(&self) -> BTreeMap<String, String> {
        let mut titles = BTreeMap::new();
        for item in fs::read_dir(self.diaspar().join("trash"))
            .into_iter()
            .flatten()
            .flatten()
        {
            if let Ok(Some(record)) = load_json::<TrashRecord>(&item.path().join("entry.json")) {
                for id in &record.page_ids {
                    titles.insert(id.clone(), record.title.clone());
                }
            }
        }
        titles
    }
}

/// The part of a wikilink that names the page: no heading, no alias.
fn wiki_target(text: &str) -> &str {
    text.split('#').next().unwrap_or("").trim()
}

#[derive(Default)]
struct WikiResolver {
    by_title: HashMap<String, Vec<String>>,
    by_stem: HashMap<String, Vec<String>>,
    /// Each page's path as name keys, for `Folder/Title` links.
    paths: Vec<(Vec<String>, String)>,
}

impl WikiResolver {
    /// The pages a wikilink text could mean: by path suffix when it contains
    /// `/`, otherwise by title, falling back to file names (DATA_MODEL §5.4).
    fn matches(&self, text: &str) -> Vec<String> {
        let target = wiki_target(text);
        if target.is_empty() {
            return Vec::new();
        }
        let mut ids: Vec<String> = if target.contains('/') {
            let wanted: Vec<String> = target
                .split('/')
                .filter(|p| !p.is_empty())
                .map(name_key)
                .collect();
            self.paths
                .iter()
                .filter(|(path, _)| {
                    path.len() >= wanted.len() && path[path.len() - wanted.len()..] == wanted[..]
                })
                .map(|(_, id)| id.clone())
                .collect()
        } else {
            let key = name_key(target);
            self.by_title
                .get(&key)
                .or_else(|| self.by_stem.get(&key))
                .cloned()
                .unwrap_or_default()
        };
        ids.sort();
        ids
    }
}
