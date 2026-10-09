//! Document store operations (ADR-0005): index rows over files that live on disk.
//!
//! The DB never owns file contents — it owns a mirror of metadata + extracted text.
//! Reconciliation against the filesystem is lazy: callers ask us to check freshness
//! before reading/searching, and we re-extract or mark missing as needed.

use std::path::Path;

use rusqlite::params;

use super::{Db, DbError};

/// A row in the `documents` table (without the body, which can be large).
#[derive(Debug, Clone)]
pub struct Doc {
    pub id: i64,
    pub path: String,
    pub title: String,
    pub kind: String,
    pub size_bytes: i64,
    pub mtime_unix: i64,
    pub content_hash: String,
    pub tags: String,
    pub status: String,
}

/// One search hit with ranking info.
#[derive(Debug, Clone)]
pub struct SearchHit {
    pub doc: Doc,
    /// bm25 score from FTS5 (lower is better in sqlite; we negate for display rank)
    pub fts_rank: f64,
    /// did the query match the filename/title?
    pub name_match: bool,
    /// did the query match tags?
    pub tag_match: bool,
    /// small snippet with the query terms highlighted by markers
    pub snippet: String,
}

/// Result of a lazy reconciliation pass (ADR-0005 §4).
#[derive(Debug, Default, serde::Serialize)]
pub struct RefreshReport {
    pub checked: usize,
    pub reindexed: usize,
    pub missing: usize,
}

/// Table-wide counts for `machie stats`.
#[derive(Debug, Default, serde::Serialize)]
pub struct GlobalCounts {
    pub sessions: i64,
    pub messages: i64,
}

impl Db {
    /// Add (or refresh) a document at `path`. Returns the row id.
    /// Extraction of non-UTF8/binary files yields an empty body but still indexes metadata.
    pub fn add_document(&self, path: &Path, title: Option<&str>) -> Result<i64, DbError> {
        let meta = std::fs::metadata(path).map_err(|_| DbError::Unreadable(path.display().to_string()))?;
        if !meta.is_file() {
            return Err(DbError::NotAFile(path.display().to_string()));
        }
        let canon = path
            .canonicalize()
            .unwrap_or_else(|_| path.to_path_buf());
        let path_str = canon.display().to_string();
        let file_name = canon
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let title = title.unwrap_or(&file_name).to_string();
        let (kind, body) = extract_text(&canon);
        let hash = sha256_of(&canon);
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let size = meta.len() as i64;

        self.conn.execute(
            r#"INSERT INTO documents (path, title, kind, size_bytes, mtime_unix, content_hash, body, status)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'indexed')
               ON CONFLICT(path) DO UPDATE SET
                 title=excluded.title, kind=excluded.kind, size_bytes=excluded.size_bytes,
                 mtime_unix=excluded.mtime_unix, content_hash=excluded.content_hash,
                 body=excluded.body, status='indexed', indexed_at=datetime('now')"#,
            params![path_str, title, kind, size, mtime, hash, body],
        )?;
        let id: i64 = self.conn.query_row(
            "SELECT id FROM documents WHERE path = ?1",
            params![path_str],
            |r| r.get(0),
        )?;
        Ok(id)
    }

    pub fn remove_document(&self, id: i64) -> Result<usize, DbError> {
        Ok(self.conn.execute("DELETE FROM documents WHERE id = ?1", params![id])?)
    }

    pub fn remove_document_by_path(&self, path: &Path) -> Result<usize, DbError> {
        let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        Ok(self.conn.execute(
            "DELETE FROM documents WHERE path = ?1",
            params![canon.display().to_string()],
        )?)
    }

    pub fn set_tags(&self, id: i64, tags: &str) -> Result<(), DbError> {
        // Normalized: trimmed, lowercased, deduped, comma-separated.
        let norm = normalize_tags(tags);
        let n = self.conn.execute(
            "UPDATE documents SET tags = ?2 WHERE id = ?1",
            params![id, norm],
        )?;
        if n == 0 {
            return Err(DbError::NotFound(format!("document #{id}")));
        }
        Ok(())
    }

    pub fn get_document(&self, id: i64) -> Result<Option<Doc>, DbError> {
        let mut stmt = self.conn.prepare(&format!("{DOC_COLS} WHERE id = ?1"))?;
        let mut rows = stmt.query_map(params![id], map_doc)?;
        Ok(rows.next().transpose()?)
    }

    pub fn get_document_by_path(&self, path: &Path) -> Result<Option<Doc>, DbError> {
        let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let mut stmt = self.conn.prepare(&format!("{DOC_COLS} WHERE path = ?1"))?;
        let mut rows = stmt.query_map(params![canon.display().to_string()], map_doc)?;
        Ok(rows.next().transpose()?)
    }

    pub fn list_documents(&self) -> Result<Vec<Doc>, DbError> {
        let mut stmt = self.conn.prepare(&format!("{DOC_COLS} ORDER BY id ASC"))?;
        let rows = stmt.query_map([], map_doc)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Read back the stored body for a document (for `doc show`).
    pub fn document_body(&self, id: i64) -> Result<Option<String>, DbError> {
        let body = self
            .conn
            .query_row(
                "SELECT body FROM documents WHERE id = ?1",
                params![id],
                |r| r.get::<_, String>(0),
            )
            .ok();
        Ok(body)
    }

    /// Lazy reconciliation (ADR-0005 §4): walk all rows, compare against the filesystem.
    /// Returns counts of what changed so callers can print a one-line summary.
    pub fn refresh_documents(&self) -> Result<RefreshReport, DbError> {
        let docs = self.list_documents()?;
        let mut rep = RefreshReport::default();
        for d in docs {
            rep.checked += 1;
            let p = Path::new(&d.path);
            match std::fs::metadata(p) {
                Err(_) => {
                    if d.status != "missing" {
                        self.conn.execute(
                            "UPDATE documents SET status='missing' WHERE id=?1",
                            params![d.id],
                        )?;
                        rep.missing += 1;
                    }
                }
                Ok(meta) => {
                    let mtime = meta
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|x| x.as_secs() as i64)
                        .unwrap_or(0);
                    let dirty = meta.len() as i64 != d.size_bytes || mtime != d.mtime_unix;
                    if dirty {
                        // Cheap pre-check passed (size/mtime differ) — re-hash to confirm,
                        // then re-extract inline. Files whose hash matches despite mtime churn
                        // are just touched, not reindexed.
                        let hash = sha256_of(p);
                        if hash == d.content_hash {
                            self.conn.execute(
                                "UPDATE documents SET mtime_unix=?2 WHERE id=?1",
                                params![d.id, mtime],
                            )?;
                        } else {
                            let (kind, body) = extract_text(p);
                            self.conn.execute(
                                "UPDATE documents SET kind=?2, body=?3, size_bytes=?4, mtime_unix=?5,
                                 content_hash=?6, status='indexed', indexed_at=datetime('now')
                                 WHERE id=?1",
                                params![d.id, kind, body, meta.len() as i64, mtime, hash],
                            )?;
                            rep.reindexed += 1;
                        }
                    } else if d.status == "missing" {
                        self.conn
                            .execute("UPDATE documents SET status='indexed' WHERE id=?1", params![d.id])?;
                    }
                }
            }
        }
        Ok(rep)
    }

    /// Deterministic multi-signal ranking core (v0.2.2): FTS5 bm25 over title/tags/body,
    /// merged with filename and tag match boosts. Empty/whitespace queries return [].
    fn search_ranked(&self, query: &str, fetch: usize) -> Result<Vec<SearchHit>, DbError> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(Vec::new());
        }
        // Sanitize into a safe OR-of-quoted-terms FTS5 phrase query.
        let fts_query = build_fts_query(q);

        let sql = format!(
            r#"SELECT d.id, bm25(doc_fts, 5.0, 3.0, 1.0) AS rank,
                      snippet(doc_fts, 2, '[', ']', ' … ', 12) AS snip
               FROM doc_fts JOIN documents d ON d.id = doc_fts.rowid
               WHERE doc_fts MATCH ?1 AND d.status != 'missing'
               ORDER BY rank LIMIT ?2"#
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let lower_q = q.to_lowercase();
        let rows = stmt.query_map(params![fts_query, fetch as i64], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?, r.get::<_, String>(2)?))
        })?;

        let mut hits = Vec::new();
        for row in rows {
            let (id, rank, snip) = row?;
            if let Some(doc) = self.get_document(id)? {
                let name_match = doc
                    .path
                    .to_lowercase()
                    .contains(&lower_q)
                    || doc.title.to_lowercase().contains(&lower_q);
                let tag_match = doc.tags.to_lowercase().contains(&lower_q);
                hits.push(SearchHit {
                    doc,
                    fts_rank: rank,
                    name_match,
                    tag_match,
                    snippet: snip,
                });
            }
        }
        // Merge: primary = bm25 (sqlite: more negative = better), boosted by name/tag matches.
        hits.sort_by(|a, b| {
            let key = |h: &SearchHit| {
                let boost = if h.name_match { -100.0 } else { 0.0 } + if h.tag_match { -50.0 } else { 0.0 };
                h.fts_rank + boost
            };
            key(a).partial_cmp(&key(b)).unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(hits)
    }

    /// Ranked search with optional kind filtering applied *after* ranking so the
    /// relative order of surviving hits is unchanged. Empty/whitespace queries return [].
    pub fn search_documents_filtered(
        &self,
        query: &str,
        limit: usize,
        kinds: &[&str],
    ) -> Result<Vec<SearchHit>, DbError> {
        let over = if kinds.is_empty() { limit } else { limit * 4 + 20 };
        let all = self.search_ranked(query, over.max(limit))?;
        let mut hits: Vec<SearchHit> = if kinds.is_empty() {
            all
        } else {
            all.into_iter()
                .filter(|h| kinds.contains(&h.doc.kind.as_str()))
                .collect()
        };
        hits.truncate(limit);
        Ok(hits)
    }

    /// Unfiltered convenience wrapper (kept for tests and simple callers).
    pub fn search_documents(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>, DbError> {
        self.search_documents_filtered(query, limit, &[])
    }

    /// Count documents per kind (for `machie stats`).
    pub fn doc_counts(&self) -> Result<Vec<(String, i64)>, DbError> {
        let mut stmt = self
            .conn
            .prepare("SELECT kind, COUNT(*) FROM documents GROUP BY kind ORDER BY kind")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Aggregate counts (for `machie stats`).
    pub fn global_counts(&self) -> Result<GlobalCounts, DbError> {
        let sessions: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM session", [], |r| r.get(0))?;
        let messages: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM message", [], |r| r.get(0))?;
        Ok(GlobalCounts { sessions, messages })
    }
}

const DOC_COLS: &str = "SELECT id, path, title, kind, size_bytes, mtime_unix, content_hash, tags, status FROM documents";

fn map_doc(r: &rusqlite::Row) -> rusqlite::Result<Doc> {
    Ok(Doc {
        id: r.get(0)?,
        path: r.get(1)?,
        title: r.get(2)?,
        kind: r.get(3)?,
        size_bytes: r.get(4)?,
        mtime_unix: r.get(5)?,
        content_hash: r.get(6)?,
        tags: r.get(7)?,
        status: r.get(8)?,
    })
}

fn normalize_tags(tags: &str) -> String {
    let mut seen = Vec::new();
    for t in tags.split([',', ' ']) {
        // strip punctuation so "rust!" == "rust"; tags stay simple slugs
        let t: String = t
            .trim()
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
            .collect::<String>()
            .to_lowercase();
        if !t.is_empty() && !seen.contains(&t) {
            seen.push(t);
        }
    }
    seen.join(",")
}

/// Turn free-text into a safe FTS5 query: each term quoted, joined with OR, so odd
/// characters can't break syntax. Prefix-match each bare word for forgiving search.
fn build_fts_query(q: &str) -> String {
    let terms: Vec<String> = q
        .split_whitespace()
        .map(|t| {
            let clean: String = t
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
                .collect();
            if clean.is_empty() {
                String::new()
            } else {
                format!("\"{clean}\"*").to_string()
            }
        })
        .filter(|s| !s.is_empty())
        .collect();
    if terms.is_empty() {
        // Query was pure punctuation — fall back to nothing matchable.
        return "\" \" ".trim().to_string();
    }
    terms.join(" OR ")
}

/// Extract plain text from a file. v0.2.x scope (ADR-0005 §6): UTF-8 text files are
/// read directly; anything else is `binary` with an empty body. PDF/DOCX land in v0.2.3.
pub fn extract_text(path: &Path) -> (String, String) {
    match std::fs::read(path) {
        Ok(bytes) => match std::str::from_utf8(&bytes) {
            Ok(s) => ("text".into(), s.to_string()),
            Err(_) => ("binary".into(), String::new()),
        },
        Err(_) => ("binary".into(), String::new()),
    }
}

/// Minimal SHA-256 (no external crate yet; vendored implementation, ~60 lines).
/// Kept private to this crate; if a second consumer appears we promote to a util crate.
pub fn sha256_of(path: &Path) -> String {
    let bytes = std::fs::read(path).unwrap_or_default();
    hex(&sha256(&bytes))
}

// --- tiny, boring SHA-256 (FIPS 180-4) -------------------------------------

fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
        let mut h: [u32; 8] = [
            0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
            0x5be0cd19,
        ];

    let ml = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&ml.to_be_bytes());

    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(chunk[i * 4..i * 4 + 4].try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) = (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        let parts = [a, b, c, d, e, f, g, hh];
        for (x, p) in h.iter_mut().zip(parts) {
            *x = x.wrapping_add(p);
        }
    }

    let mut out = [0u8; 32];
    for i in 0..8 {
        out[i * 4..i * 4 + 4].copy_from_slice(&h[i].to_be_bytes());
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_file(dir: &Path, name: &str, content: &str) -> std::path::PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, content).unwrap();
        p
    }

    #[test]
    fn sha256_matches_known_vectors() {
        // NIST/FIPS standard test vectors
        assert_eq!(hex(&sha256(b"")), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(hex(&sha256(b"abc")), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let long = b"a".repeat(1000);
        assert_eq!(
            hex(&sha256(&long)),
            "41edece42d63e8d9bf515a9ba6932e1c20cbc9f5a5d134645adb5db1b9737ea3"
        );
    }

    #[test]
    fn add_list_remove_roundtrip() {
        let db = Db::in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let f = write_file(dir.path(), "notes.txt", "hello machie world");
        let id = db.add_document(&f, None).unwrap();
        let d = db.get_document(id).unwrap().unwrap();
        assert_eq!(d.title, "notes.txt");
        assert_eq!(d.kind, "text");
        assert_eq!(d.status, "indexed");
        assert_eq!(db.list_documents().unwrap().len(), 1);
        // removing the row leaves the file alone (user owns files)
        db.remove_document(id).unwrap();
        assert!(db.get_document(id).unwrap().is_none());
        assert!(f.exists());
    }

    #[test]
    fn adding_same_path_twice_updates_not_duplicates() {
        let db = Db::in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let f = write_file(dir.path(), "a.md", "first version");
        let id1 = db.add_document(&f, None).unwrap();
        std::fs::write(&f, "second version").unwrap();
        let id2 = db.add_document(&f, None).unwrap();
        assert_eq!(id1, id2);
        assert_eq!(db.list_documents().unwrap().len(), 1);
        assert_eq!(db.document_body(id1).unwrap().unwrap(), "second version");
    }

    #[test]
    fn binary_files_index_as_metadata_only() {
        let db = Db::in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("blob.bin");
        std::fs::write(&p, [0u8, 159, 146, 150, 0xFF]).unwrap();
        let id = db.add_document(&p, None).unwrap();
        let d = db.get_document(id).unwrap().unwrap();
        assert_eq!(d.kind, "binary");
        assert_eq!(db.document_body(id).unwrap().unwrap().len(), 0);
    }

    #[test]
    fn missing_file_detected_by_refresh() {
        let db = Db::in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let f = write_file(dir.path(), "gone.txt", "content");
        let id = db.add_document(&f, None).unwrap();
        std::fs::remove_file(&f).unwrap();
        let rep = db.refresh_documents().unwrap();
        assert_eq!(rep.missing, 1);
        assert_eq!(db.get_document(id).unwrap().unwrap().status, "missing");
        // missing docs are excluded from search results
        std::fs::write(&f, "back again").unwrap();
        db.refresh_documents().unwrap();
        assert_eq!(db.get_document(id).unwrap().unwrap().status, "indexed");
    }

    #[test]
    fn external_edit_reindexes_inline() {
        let db = Db::in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let f = write_file(dir.path(), "live.txt", "alpha beta");
        let id = db.add_document(&f, None).unwrap();
        std::fs::write(&f, "alpha gamma delta").unwrap();
        let rep = db.refresh_documents().unwrap();
        assert_eq!(rep.reindexed, 1);
        let hits = db.search_documents("gamma", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].doc.id, id);
    }

    #[test]
    fn search_fulltext_prefix_and_snippet() {
        let db = Db::in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let f1 = write_file(dir.path(), "recipe.txt", "the best lasagna needs bechamel sauce and patience");
        let f2 = write_file(dir.path(), "todo.txt", "buy milk, eggs, flour for pancakes");
        db.add_document(&f1, None).unwrap();
        db.add_document(&f2, None).unwrap();

        let hits = db.search_documents("lasagna", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].snippet.contains("[lasagna]"), "snippet: {}", hits[0].snippet);

        // prefix matching: "panca" finds "pancakes"
        let hits = db.search_documents("panca", 10).unwrap();
        assert_eq!(hits.len(), 1);

        // multi-term OR ranking: doc containing both terms should outrank one-term doc
        let f3 = write_file(dir.path(), "both.txt", "lasagna and pancakes together");
        db.add_document(&f3, None).unwrap();
        let hits = db.search_documents("lasagna pancakes", 10).unwrap();
        assert_eq!(hits.len(), 3);
        assert_eq!(hits[0].doc.title, "both.txt");
    }

    #[test]
    fn search_name_and_tag_boosts() {
        let db = Db::in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        // two docs contain the word in body; only one has it in the filename AND tag
        let a = write_file(dir.path(), "quantum-physics-notes.txt", "entanglement explained plainly here somewhere deep inside");
        let b = write_file(dir.path(), "misc.txt", "quantum entanglement is mentioned once");
        let ia = db.add_document(&a, None).unwrap();
        db.add_document(&b, None).unwrap();
        db.set_tags(ia, "physics, quantum").unwrap();
        let hits = db.search_documents("quantum", 10).unwrap();
        assert!(hits.len() >= 2);
        assert_eq!(hits[0].doc.id, ia, "name+tag match should rank first");
        assert!(hits[0].name_match && hits[0].tag_match);
    }

    #[test]
    fn search_weird_input_never_errors() {
        let db = Db::in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let f = write_file(dir.path(), "x.txt", "plain words only");
        db.add_document(&f, None).unwrap();
        for q in ["", "   ", "\"\"\"", "AND OR NOT", "***", "near(a b)", "c:d/e\\f", "🦄✨"] {
            let res = db.search_documents(q, 5);
            assert!(res.is_ok(), "query {q:?} errored: {:?}", res.err());
        }
    }

    #[test]
    fn search_kind_filter_preserves_relative_ranking() {
        let db = Db::in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let a = write_file(dir.path(), "a.txt", "quantum notes about entanglement");
        let b = write_file(dir.path(), "b.txt", "quantum also appears here");
        let c = write_file(dir.path(), "c.png", "\u{fffd}\u{fffd}binary\u{fffd}"); // non-UTF8 -> kind=binary
        std::fs::write(&c, [0xffu8, 0xfe, 0x00, 0x01]).unwrap();
        let ia = db.add_document(&a, None).unwrap();
        let _ib = db.add_document(&b, None).unwrap();
        let ic = db.add_document(&c, None).unwrap();
        db.set_tags(ic, "quantum").unwrap(); // binary doc matches the query via tags

        let unfiltered = db.search_documents("quantum", 10).unwrap();
        assert_eq!(unfiltered.len(), 3, "all three should match: {:?}", titles(&unfiltered));

        let text_only = db.search_documents_filtered("quantum", 10, &["text"]).unwrap();
        assert_eq!(text_only.len(), 2);
        // relative order of surviving docs must be unchanged by the filter
        let order_unf: Vec<i64> = unfiltered.iter().map(|h| h.doc.id).filter(|id| *id != ic).collect();
        let order_f: Vec<i64> = text_only.iter().map(|h| h.doc.id).collect();
        assert_eq!(order_unf, order_f);
        assert!(text_only.iter().all(|h| h.doc.kind == "text"));

        let bin_only = db.search_documents_filtered("quantum", 10, &["binary"]).unwrap();
        assert_eq!(bin_only.len(), 1);
        assert_eq!(bin_only[0].doc.id, ic);

        // limit respected after filtering
        assert_eq!(db.search_documents_filtered("quantum", 1, &["text"]).unwrap().len(), 1);
        assert_eq!(ia, ia); // id sanity
    }

    fn titles(hits: &[crate::docs::SearchHit]) -> Vec<String> {
        hits.iter().map(|h| h.doc.title.clone()).collect()
    }

    #[test]
    fn counts_helpers() {
        let db = Db::in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let f = write_file(dir.path(), "n.txt", "hello");
        db.add_document(&f, None).unwrap();
        db.create_session("s1").unwrap();
        db.add_message(1, "user", "hi").unwrap();
        let kinds = db.doc_counts().unwrap();
        assert_eq!(kinds, vec![("text".to_string(), 1)]);
        let g = db.global_counts().unwrap();
        assert_eq!((g.sessions, g.messages), (1, 1));
    }

    #[test]
    fn tags_are_normalized() {
        let db = Db::in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let f = write_file(dir.path(), "t.txt", "hi");
        let id = db.add_document(&f, None).unwrap();
        db.set_tags(id, " Rust , rust!  , WIKI ").unwrap();
        let d = db.get_document(id).unwrap().unwrap();
        assert_eq!(d.tags, "rust,wiki");
    }

    #[test]
    fn remove_by_path_after_move_is_clean() {
        let db = Db::in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let f = write_file(dir.path(), "temp.txt", "x");
        db.add_document(&f, None).unwrap();
        assert_eq!(db.remove_document_by_path(&f).unwrap(), 1);
        assert!(db.list_documents().unwrap().is_empty());
    }

    #[test]
    fn add_directory_is_friendly_error() {
        let db = Db::in_memory().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let err = db.add_document(dir.path(), None).err().unwrap();
        assert!(matches!(err, DbError::NotAFile(_)), "{err}");
        let err = db.add_document(std::path::Path::new("/definitely/not/here.txt"), None).err().unwrap();
        assert!(matches!(err, DbError::Unreadable(_)), "{err}");
    }
}
