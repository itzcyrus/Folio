//! `machie doc` and `machie search` — the Docs + Search feature (v0.2.x, ADR-0005).
//!
//! Design rules:
//! - Files stay on disk; we only index them. `doc remove` never deletes the file.
//! - Every subcommand supports `--json` so scripts (and future AI drivers) can consume
//!   output mechanically. Human output stays compact and aligned.
//! - Before any read/search we lazily reconcile the index with the filesystem.

use clap::Subcommand;
use machie_db::{Db, Doc};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum DocCmd {
    /// Index a file (it stays where it is; re-running refreshes the entry)
    Add {
        /// Path to the file
        path: PathBuf,
        /// Optional display title (defaults to the filename)
        #[arg(long)]
        title: Option<String>,
        /// Comma-separated tags
        #[arg(long, short)]
        tags: Option<String>,
        /// Machine-readable output
        #[arg(long)]
        json: bool,
    },
    /// List indexed documents
    List {
        #[arg(long)]
        json: bool,
    },
    /// Show one document (by id or exact path), including its stored body preview
    Show {
        /// Document id or path
        reference: String,
        /// Print the full extracted body instead of a preview
        #[arg(long)]
        full: bool,
        #[arg(long)]
        json: bool,
    },
    /// Remove a document from the index (the file itself is NOT deleted)
    Remove {
        /// Document id or path
        reference: String,
        #[arg(long)]
        json: bool,
    },
    /// Set tags on a document (replaces existing tags)
    Tag {
        /// Document id or path
        reference: String,
        /// Comma-separated tags
        tags: String,
        #[arg(long)]
        json: bool,
    },
    /// Re-check all indexed files against the disk (edits are picked up automatically
    /// by other commands too; use this for an explicit sweep)
    Refresh {
        #[arg(long)]
        json: bool,
    },
    /// Aggregate counts across everything Machie knows about
    Stats {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum SearchCmd {
    /// Search document content, filenames, titles and tags (deterministic, no AI)
    Query {
        /// The query text
        query: String,
        /// Maximum number of results
        #[arg(long, default_value_t = 10)]
        limit: usize,
        /// Restrict to a document kind (repeatable): text, binary
        #[arg(long = "type", value_name = "KIND")]
        kinds: Vec<String>,
        /// Only search inside these paths (comma-separated prefixes)
        #[arg(long, value_name = "PREFIXES")]
        in_paths: Option<String>,
        #[arg(long)]
        json: bool,
    },
}

// ---------- JSON shapes ----------

#[derive(Serialize)]
struct DocJson {
    id: i64,
    path: String,
    title: String,
    kind: String,
    size_bytes: i64,
    tags: Vec<String>,
    status: String,
}

#[derive(Serialize)]
struct HitJson {
    id: i64,
    path: String,
    title: String,
    score: f64,
    name_match: bool,
    tag_match: bool,
    snippet: String,
}

impl From<&Doc> for DocJson {
    fn from(d: &Doc) -> Self {
        DocJson {
            id: d.id,
            path: d.path.clone(),
            title: d.title.clone(),
            kind: d.kind.clone(),
            size_bytes: d.size_bytes,
            tags: split_tags(&d.tags),
            status: d.status.clone(),
        }
    }
}

fn split_tags(tags: &str) -> Vec<String> {
    if tags.is_empty() {
        return Vec::new();
    }
    tags.split(',').map(|s| s.to_string()).collect()
}

fn print_json<T: Serialize>(v: &T) {
    println!(
        "{}",
        serde_json::to_string_pretty(v).unwrap_or_else(|_| "{}".into())
    );
}

fn human_size(bytes: i64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut n = bytes as f64;
    let mut u = 0;
    while n >= 1024.0 && u < 3 {
        n /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{bytes} B")
    } else {
        format!("{n:.1} {}", UNITS[u])
    }
}

/// Resolve "12", "/some/path.txt" or a bare filename like "todo.txt" to a Doc.
fn resolve(db: &Db, reference: &str) -> Result<Doc, Box<dyn std::error::Error>> {
    if let Ok(id) = reference.parse::<i64>() {
        if let Some(d) = db.get_document(id)? {
            return Ok(d);
        }
    }
    let p = PathBuf::from(reference);
    if let Some(d) = db.get_document_by_path(&p)? {
        return Ok(d);
    }
    // Fall back to matching the basename against indexed titles (e.g. `doc show todo.txt`).
    let wanted = reference.to_lowercase();
    let by_name: Vec<Doc> = db
        .list_documents()?
        .into_iter()
        .filter(|d| d.title.to_lowercase() == wanted)
        .collect();
    match by_name.len() {
        1 => Ok(by_name.into_iter().next().unwrap()),
        0 => Err(format!(
            "no indexed document matches '{reference}' (try `machie doc list`)"
        )
        .into()),
        n => {
            let paths: Vec<&str> = by_name.iter().map(|d| d.path.as_str()).collect();
            Err(format!(
                "{n} indexed documents are named '{reference}' — disambiguate with the id or full path: {}",
                paths.join(", ")
            )
            .into())
        }
    }
}

pub fn run_doc(action: DocCmd, db: &Db) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        DocCmd::Add { path, title, tags, json } => {
            if path.is_dir() {
                // Directory convenience: index text-ish files up to depth 3.
                let mut added = 0usize;
                let mut skipped = 0usize;
                for entry in walkdir_shallow(&path, 3) {
                    match db.add_document(&entry, title.as_deref()) {
                        Ok(id) => {
                            if let Some(t) = &tags {
                                db.set_tags(id, t)?;
                            }
                            added += 1;
                        }
                        Err(_) => skipped += 1,
                    }
                }
                if json {
                    print_json(&serde_json::json!({ "added": added, "skipped": skipped }));
                } else {
                    println!("Indexed {added} file(s) from {} ({skipped} unreadable skipped).", path.display());
                    println!("Tip: `machie search <term>` now covers them.");
                }
            } else {
                let id = db.add_document(&path, title.as_deref())?;
                if let Some(t) = &tags {
                    db.set_tags(id, t)?;
                }
                let d = db.get_document(id)?.expect("just added");
                if json {
                    print_json(&DocJson::from(&d));
                } else {
                    println!("Indexed #{} {} ({}, {}){}", d.id, d.path, d.kind, human_size(d.size_bytes),
                        if d.kind == "binary" { " — metadata only, text extraction for this type comes in v0.2.3" } else { "" });
                }
            }
        }
        DocCmd::List { json } => {
            let _ = db.refresh_documents()?;
            let docs = db.list_documents()?;
            if json {
                print_json(&docs.iter().map(DocJson::from).collect::<Vec<_>>());
            } else if docs.is_empty() {
                println!("No documents indexed yet.");
                println!("Try:  machie doc add ~/notes/todo.md   or   machie doc add ./myfolder");
            } else {
                println!("{:<4} {:<7} {:<9} {:>8}  {}", "id", "kind", "status", "size", "title / tags");
                for d in &docs {
                    let tags = if d.tags.is_empty() { String::new() } else { format!("  [{}]", d.tags) };
                    println!(
                        "{:<4} {:<7} {:<9} {:>8}  {}{tags}",
                        d.id, d.kind, d.status, human_size(d.size_bytes), d.title
                    );
                }
                println!("({} document(s))", docs.len());
            }
        }
        DocCmd::Show { reference, full, json } => {
            let _ = db.refresh_documents()?;
            let d = resolve(db, &reference)?;
            let body = db.document_body(d.id)?.unwrap_or_default();
            if json {
                let mut obj = serde_json::to_value(DocJson::from(&d)).unwrap();
                obj["body"] = serde_json::json!(body);
                print_json(&obj);
            } else {
                println!("#{}  {}", d.id, d.path);
                println!("title: {} | kind: {} | status: {} | size: {}", d.title, d.kind, d.status, human_size(d.size_bytes));
                if !d.tags.is_empty() {
                    println!("tags:  {}", d.tags);
                }
                println!("hash:  {}", &d.content_hash[..16.min(d.content_hash.len())]);
                println!("---");
                if d.kind == "binary" {
                    println!("(no extracted text for this file type yet — PDF/DOCX support lands in v0.2.3)");
                } else if body.trim().is_empty() {
                    println!("(empty file)");
                } else if full {
                    print!("{body}");
                    if !body.ends_with('\n') {
                        println!();
                    }
                } else {
                    let preview: String = body.chars().take(600).collect();
                    let more = body.chars().count().saturating_sub(600);
                    println!("{preview}");
                    if more > 0 {
                        println!("… (+{more} chars — use --full)");
                    }
                }
            }
        }
        DocCmd::Remove { reference, json } => {
            let d = resolve(db, &reference)?;
            db.remove_document(d.id)?;
            if json {
                print_json(&serde_json::json!({ "removed_id": d.id, "file_untouched": true }));
            } else {
                println!("Removed #{} '{}' from the index. The file at {} was left alone.", d.id, d.title, d.path);
            }
        }
        DocCmd::Tag { reference, tags, json } => {
            let d = resolve(db, &reference)?;
            db.set_tags(d.id, &tags)?;
            let d = db.get_document(d.id)?.expect("exists");
            if json {
                print_json(&DocJson::from(&d));
            } else {
                println!("#{} tags set to: {}", d.id, d.tags);
            }
        }
        DocCmd::Refresh { json } => {
            let rep = db.refresh_documents()?;
            if json {
                print_json(&rep);
            } else {
                println!(
                    "Checked {} document(s): {} re-indexed, {} newly missing.",
                    rep.checked, rep.reindexed, rep.missing
                );
            }
        }
        DocCmd::Stats { json } => {
            let _ = db.refresh_documents()?;
            let by_kind = db.doc_counts()?;
            let total_docs: i64 = by_kind.iter().map(|(_, c)| c).sum();
            let (sessions, messages) = match db.global_counts() {
                Ok(c) => (c.sessions, c.messages),
                Err(e) => return Err(e.into()),
            };
            if json {
                print_json(&serde_json::json!({
                    "documents": {
                        "total": total_docs,
                        "by_kind": by_kind.iter().map(|(k, c)| serde_json::json!({ "kind": k, "count": c })).collect::<Vec<_>>(),
                    },
                    "sessions": sessions,
                    "messages": messages,
                }));
            } else {
                println!("Machie stats");
                println!("  documents : {total_docs}");
                for (k, c) in &by_kind {
                    println!("    - {k:<8} {c}");
                }
                println!("  sessions  : {sessions}");
                println!("  messages  : {messages}");
            }
        }
    }
    Ok(())
}

pub fn run_search(action: SearchCmd, db: &Db) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        SearchCmd::Query { query, limit, kinds, in_paths, json } => {
            let _ = db.refresh_documents()?;
            let kind_refs: Vec<&str> = kinds.iter().map(|s| s.as_str()).collect();
            for k in &kind_refs {
                if *k != "text" && *k != "binary" {
                    return Err(format!(
                        "unknown document type '{k}' (known: text, binary — see `machie doc list`)"
                    )
                    .into());
                }
            }
            let prefixes: Vec<String> = in_paths
                .as_deref()
                .map(|s| {
                    s.split(',')
                        .map(|p| p.trim().to_string())
                        .filter(|p| !p.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            let mut hits = db.search_documents_filtered(&query, limit * 2 + 10, &kind_refs)?;
            if !prefixes.is_empty() {
                hits.retain(|h| {
                    prefixes
                        .iter()
                        .any(|p| h.doc.path.starts_with(p) || h.doc.title.contains(p))
                });
            }
            hits.truncate(limit);
            if json {
                let out: Vec<HitJson> = hits
                    .iter()
                    .map(|h| HitJson {
                        id: h.doc.id,
                        path: h.doc.path.clone(),
                        title: h.doc.title.clone(),
                        score: h.fts_rank,
                        name_match: h.name_match,
                        tag_match: h.tag_match,
                        snippet: h.snippet.clone(),
                    })
                    .collect();
                print_json(&serde_json::json!({ "query": query, "hits": out }));
            } else if hits.is_empty() {
                println!("No matches for \"{query}\".");
                println!("Hints: fewer words, try a prefix (e.g. 'panca' finds 'pancakes'),");
                println!("       check coverage with `machie doc list`, or add files first.");
                if !kinds.is_empty() || !prefixes.is_empty() {
                    println!("       note: your --type/--in-paths filters may have excluded everything.");
                }
            } else {
                for (i, h) in hits.iter().enumerate() {
                    let flags = match (h.name_match, h.tag_match) {
                        (true, true) => "name+tag",
                        (true, false) => "name",
                        (false, true) => "tag",
                        _ => "text",
                    };
                    println!("{:>2}. #{} {} ({})", i + 1, h.doc.id, h.doc.title, flags);
                    if !h.snippet.trim().is_empty() {
                        println!("     …{}…", h.snippet.trim());
                    }
                    println!("     {}", h.doc.path);
                }
                println!("({} result(s) for \"{query}\")", hits.len());
            }
        }
    }
    Ok(())
}

/// Shallow directory walk without extra deps: regular files only, hidden dirs skipped,
/// bounded depth. Good enough for v0.2; revisit if it ever needs to be smarter.
fn walkdir_shallow(root: &std::path::Path, max_depth: usize) -> Vec<PathBuf> {
    fn go(dir: &std::path::Path, depth: usize, out: &mut Vec<PathBuf>) {
        if depth == 0 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if p.is_dir() {
                if !name.starts_with('.') {
                    go(&p, depth - 1, out);
                }
            } else if p.is_file() && !name.starts_with('.') {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    go(root, max_depth, &mut out);
    out.sort();
    out
}
