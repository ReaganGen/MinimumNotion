//! Measures workspace operations on a large workspace (IMPLEMENTATION_PLAN
//! §6.2, performance). Generate one first:
//!
//!   node scripts/generate-workspace.mjs sandbox-workspaces/perf 5000
//!   cargo run --release -p forma-core --example measure -- sandbox-workspaces/perf
//!
//! The index cache goes to a temporary folder, so the first open is cold.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use forma_core::workspace::Workspace;
use forma_core::workspace::retrieval::{RetrievalFilter, Retriever};

fn time<T>(label: &str, runs: u32, mut f: impl FnMut() -> T) -> T {
    let mut best = Duration::MAX;
    let mut total = Duration::ZERO;
    let mut out = None;
    for _ in 0..runs {
        let start = Instant::now();
        out = Some(f());
        let took = start.elapsed();
        best = best.min(took);
        total += took;
    }
    println!(
        "{label:<44} best {:>8.2} ms   mean {:>8.2} ms   ({runs} runs)",
        best.as_secs_f64() * 1000.0,
        total.as_secs_f64() * 1000.0 / f64::from(runs)
    );
    out.expect("at least one run")
}

fn main() {
    let root = PathBuf::from(std::env::args().nth(1).expect("usage: measure <workspace>"));
    let cache = std::env::temp_dir().join(format!("forma-measure-{}", std::process::id()));
    let pages = walkdir_count(&root);
    println!("workspace {root:?}: {pages} Markdown files\n");

    let ws = time("open, empty index cache (cold)", 1, || {
        Workspace::open_with(&root, Some(&cache)).unwrap()
    });
    drop(ws);
    let mut ws = time("open, warm index cache", 3, || {
        Workspace::open_with(&root, Some(&cache)).unwrap()
    });
    time("list top-level pages", 20, || {
        ws.list_children(None).unwrap().len()
    });
    let section = ws.quick_open("Section 1", 1)[0].id.clone();
    time("list a section's pages", 20, || {
        ws.list_children(Some(&section)).unwrap().len()
    });

    for query in [
        "attention",
        "prot",
        "attention protein design",
        "tention",
        "蛋白质",
        "知识",
        "zzzz",
    ] {
        let hits = time(&format!("search {query:?}"), 20, || ws.search(query, 50));
        println!("{:<44} {} hits", "", hits.len());
    }
    for question in [
        "How does attention help protein design?",
        "what did I write about the garden plan",
        "注意力机制在蛋白质结构预测中有什么用",
        "知识",
    ] {
        let found = time(&format!("retrieve passages {question:?}"), 20, || {
            ws.read_view()
                .retrieve(question, 8, &RetrievalFilter::default())
        });
        println!("{:<44} {} passages", "", found.len());
    }
    time("quick open \"garden\"", 20, || {
        ws.quick_open("garden", 30).len()
    });
    let target = ws.quick_open("Section 2", 1)[0].id.clone();
    let any_page = ws.list_children(Some(&target)).unwrap()[0].id.clone();
    let links = time("backlinks of a page", 20, || {
        ws.backlinks(&any_page).unwrap().len()
    });
    println!("{:<44} {links} backlinks", "");
    let collection = ws.quick_open("Reading list", 1)[0].id.clone();
    let rows = time("load the collection", 10, || {
        ws.get_collection(&collection).unwrap().rows.len()
    });
    println!("{:<44} {rows} entries", "");
    time("rescan with nothing changed", 5, || ws.reconcile());
    time("rebuild the index", 1, || ws.rebuild_index().unwrap());

    let size: u64 = std::fs::read_dir(&cache)
        .into_iter()
        .flatten()
        .flatten()
        .flat_map(|d| std::fs::read_dir(d.path()).into_iter().flatten().flatten())
        .filter_map(|f| f.metadata().ok())
        .map(|m| m.len())
        .sum();
    println!("\nindex size on disk: {:.1} MB", size as f64 / 1_000_000.0);
    let _ = std::fs::remove_dir_all(&cache);
}

fn walkdir_count(dir: &std::path::Path) -> usize {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|item| {
            let path = item.path();
            if item.file_name().to_string_lossy().starts_with('.') {
                0
            } else if path.is_dir() {
                walkdir_count(&path)
            } else {
                usize::from(path.extension().is_some_and(|e| e == "md"))
            }
        })
        .sum()
}
