//! BM25 Semantic Search Implementation
//!
//! This module provides BM25 (Best Match 25) ranking for semantic code search.
//! Unlike exact symbol name matching, BM25 enables loose term queries like
//! "authentication", "error handling", or "database connection" that find
//! conceptually related code.
//!
//! # Architecture
//!
//! The index is built from symbol data during shard generation:
//! - Terms are extracted from symbol names, file paths, and TOON summaries
//! - An inverted index maps terms to documents (symbols)
//! - At query time, BM25 scoring ranks results by relevance
//!
//! # BM25 Parameters
//!
//! - k1 = 1.2 (term frequency saturation)
//! - b = 0.75 (document length normalization)

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// BM25 parameters
const K1: f64 = 1.2;
const B: f64 = 0.75;

/// A document in the BM25 index (represents a symbol)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bm25Document {
    /// Symbol hash (for lookup)
    pub hash: String,
    /// Symbol name
    pub symbol: String,
    /// File path
    pub file: String,
    /// Line range
    pub lines: String,
    /// Symbol kind (fn, struct, etc.)
    pub kind: String,
    /// Module name
    pub module: String,
    /// Risk level
    pub risk: String,
    /// Document length (total term count)
    pub doc_length: u32,
}

/// Term frequency entry in the inverted index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TermEntry {
    /// Document ID (symbol hash)
    pub doc_id: String,
    /// Term frequency in this document
    pub tf: u32,
}

/// BM25 Index for semantic search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bm25Index {
    /// Schema version for compatibility
    pub schema_version: u32,
    /// Inverted index: term -> list of (doc_id, term_freq)
    pub inverted_index: HashMap<String, Vec<TermEntry>>,
    /// Document metadata: doc_id -> document info
    pub documents: HashMap<String, Bm25Document>,
    /// Total number of documents
    pub total_docs: u32,
    /// Average document length
    pub avg_doc_length: f64,
}

/// Search result with BM25 score
#[derive(Debug, Clone)]
pub struct Bm25SearchResult {
    /// Symbol hash
    pub hash: String,
    /// Symbol name
    pub symbol: String,
    /// File path
    pub file: String,
    /// Line range
    pub lines: String,
    /// Symbol kind
    pub kind: String,
    /// Module name
    pub module: String,
    /// Risk level
    pub risk: String,
    /// BM25 relevance score
    pub score: f64,
    /// Terms that matched in this result
    pub matched_terms: Vec<String>,
}

impl Default for Bm25Index {
    fn default() -> Self {
        Self::new()
    }
}

impl Bm25Index {
    /// Create a new empty BM25 index
    pub fn new() -> Self {
        Self {
            schema_version: 1,
            inverted_index: HashMap::new(),
            documents: HashMap::new(),
            total_docs: 0,
            avg_doc_length: 0.0,
        }
    }

    /// Add a document to the index
    pub fn add_document(&mut self, doc: Bm25Document, terms: Vec<String>) {
        let doc_id = doc.hash.clone();
        let doc_length = terms.len() as u32;

        // Update document with actual length
        let mut doc = doc;
        doc.doc_length = doc_length;

        // Count term frequencies
        let mut term_freqs: HashMap<String, u32> = HashMap::new();
        for term in &terms {
            *term_freqs.entry(term.clone()).or_insert(0) += 1;
        }

        // Add to inverted index
        for (term, freq) in term_freqs {
            self.inverted_index
                .entry(term)
                .or_default()
                .push(TermEntry {
                    doc_id: doc_id.clone(),
                    tf: freq,
                });
        }

        // Store document
        self.documents.insert(doc_id, doc);
    }

    /// Add a document when terms are already unique (tf = 1 for all terms).
    pub fn add_document_unique_terms(&mut self, doc: Bm25Document, terms: Vec<String>) {
        let doc_id = doc.hash.clone();
        let doc_length = terms.len() as u32;

        let mut doc = doc;
        doc.doc_length = doc_length;

        for term in terms {
            self.inverted_index
                .entry(term)
                .or_default()
                .push(TermEntry {
                    doc_id: doc_id.clone(),
                    tf: 1,
                });
        }

        self.documents.insert(doc_id, doc);
    }

    /// Finalize the index (compute averages)
    pub fn finalize(&mut self) {
        self.total_docs = self.documents.len() as u32;
        if self.total_docs > 0 {
            let total_length: u64 = self.documents.values().map(|d| d.doc_length as u64).sum();
            self.avg_doc_length = total_length as f64 / self.total_docs as f64;
        }
    }

    /// Search the index with BM25 ranking
    pub fn search(&self, query: &str, limit: usize) -> Vec<Bm25SearchResult> {
        let query_terms = tokenize(query);
        if query_terms.is_empty() {
            return Vec::new();
        }

        // Calculate BM25 scores for each document
        let mut scores: HashMap<String, (f64, Vec<String>)> = HashMap::new();

        for term in &query_terms {
            // Get documents containing this term
            if let Some(postings) = self.inverted_index.get(term) {
                // Calculate IDF for this term
                let df = postings.len() as f64;
                let idf = ((self.total_docs as f64 - df + 0.5) / (df + 0.5) + 1.0).ln();

                for entry in postings {
                    if let Some(doc) = self.documents.get(&entry.doc_id) {
                        // BM25 term score
                        let tf = entry.tf as f64;
                        let doc_len = doc.doc_length as f64;
                        let numerator = tf * (K1 + 1.0);
                        let denominator = tf + K1 * (1.0 - B + B * doc_len / self.avg_doc_length);
                        let term_score = idf * numerator / denominator;

                        let (score, matched) = scores
                            .entry(entry.doc_id.clone())
                            .or_insert((0.0, Vec::new()));
                        *score += term_score;
                        if !matched.contains(term) {
                            matched.push(term.clone());
                        }
                    }
                }
            }
        }

        // Convert to results and sort by score
        let mut results: Vec<Bm25SearchResult> = scores
            .into_iter()
            .filter_map(|(doc_id, (score, matched_terms))| {
                self.documents.get(&doc_id).map(|doc| Bm25SearchResult {
                    hash: doc.hash.clone(),
                    symbol: doc.symbol.clone(),
                    file: doc.file.clone(),
                    lines: doc.lines.clone(),
                    kind: doc.kind.clone(),
                    module: doc.module.clone(),
                    risk: doc.risk.clone(),
                    score,
                    matched_terms,
                })
            })
            .collect();

        // Sort by score descending
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Limit results
        results.truncate(limit);
        results
    }

    /// Get related query suggestions based on co-occurring terms
    pub fn suggest_related_terms(&self, query: &str, limit: usize) -> Vec<String> {
        let query_terms: std::collections::HashSet<String> = tokenize(query).into_iter().collect();
        if query_terms.is_empty() {
            return Vec::new();
        }

        // Find documents matching query terms
        let mut matching_docs: std::collections::HashSet<String> = std::collections::HashSet::new();
        for term in &query_terms {
            if let Some(postings) = self.inverted_index.get(term) {
                for entry in postings {
                    matching_docs.insert(entry.doc_id.clone());
                }
            }
        }

        // Count co-occurring terms
        let mut term_counts: HashMap<String, usize> = HashMap::new();
        for (term, postings) in &self.inverted_index {
            if query_terms.contains(term) {
                continue; // Skip query terms themselves
            }
            let count = postings
                .iter()
                .filter(|e| matching_docs.contains(&e.doc_id))
                .count();
            if count > 0 {
                term_counts.insert(term.clone(), count);
            }
        }

        // Sort by frequency and return top N
        let mut terms: Vec<(String, usize)> = term_counts.into_iter().collect();
        terms.sort_by(|a, b| b.1.cmp(&a.1));
        terms.truncate(limit);
        terms.into_iter().map(|(t, _)| t).collect()
    }

    /// Save index to a file
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let file = std::fs::File::create(path)?;
        let writer = std::io::BufWriter::new(file);
        serde_json::to_writer(writer, self)?;
        Ok(())
    }

    /// Load index from a file
    pub fn load(path: &Path) -> std::io::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let index: Self = serde_json::from_str(&content)?;
        Ok(index)
    }
}

pub fn init_bm25_sqlite(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS bm25_documents (
            doc_id TEXT PRIMARY KEY,
            symbol TEXT,
            file TEXT,
            lines TEXT,
            kind TEXT,
            module TEXT,
            risk TEXT,
            doc_length INTEGER
        );
        CREATE TABLE IF NOT EXISTS bm25_terms (
            term TEXT,
            doc_id TEXT,
            tf INTEGER
        );
        CREATE TABLE IF NOT EXISTS bm25_meta (
            total_docs INTEGER,
            avg_doc_length REAL
        );
        CREATE INDEX IF NOT EXISTS idx_bm25_terms_term ON bm25_terms(term);
        CREATE INDEX IF NOT EXISTS idx_bm25_terms_doc ON bm25_terms(doc_id);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_bm25_terms_unique ON bm25_terms(term, doc_id);
        "#,
    )?;
    Ok(())
}

pub fn clear_bm25_sqlite(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        r#"
        DELETE FROM bm25_terms;
        DELETE FROM bm25_documents;
        DELETE FROM bm25_meta;
        "#,
    )?;
    Ok(())
}

pub fn write_bm25_meta(
    conn: &Connection,
    total_docs: u32,
    avg_doc_length: f64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO bm25_meta (total_docs, avg_doc_length) VALUES (?, ?)",
        params![total_docs as i64, avg_doc_length],
    )?;
    Ok(())
}

pub fn search_sqlite(
    path: &Path,
    query: &str,
    limit: usize,
) -> std::io::Result<Vec<Bm25SearchResult>> {
    let conn =
        Connection::open(path).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    let (total_docs, avg_doc_length): (f64, f64) = conn
        .query_row(
            "SELECT total_docs, avg_doc_length FROM bm25_meta LIMIT 1",
            [],
            |row| Ok((row.get::<_, i64>(0)? as f64, row.get::<_, f64>(1)?)),
        )
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    let query_terms = tokenize(query);
    if query_terms.is_empty() || total_docs == 0.0 {
        return Ok(Vec::new());
    }

    let mut scores: HashMap<String, (f64, Vec<String>, Bm25Document)> = HashMap::new();

    for term in &query_terms {
        let mut stmt = conn
            .prepare(
                r#"
                SELECT t.doc_id, t.tf, d.symbol, d.file, d.lines, d.kind, d.module, d.risk, d.doc_length
                FROM bm25_terms t
                JOIN bm25_documents d ON d.doc_id = t.doc_id
                WHERE t.term = ?
                "#,
            )
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        let postings = stmt
            .query_map([term], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)? as f64,
                    Bm25Document {
                        hash: row.get::<_, String>(0)?,
                        symbol: row.get::<_, String>(2)?,
                        file: row.get::<_, String>(3)?,
                        lines: row.get::<_, String>(4)?,
                        kind: row.get::<_, String>(5)?,
                        module: row.get::<_, String>(6)?,
                        risk: row.get::<_, String>(7)?,
                        doc_length: row.get::<_, i64>(8)? as u32,
                    },
                ))
            })
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        let mut posting_vec = Vec::new();
        for row in postings {
            posting_vec.push(row.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?);
        }

        let df = posting_vec.len() as f64;
        if df == 0.0 {
            continue;
        }
        let idf = ((total_docs - df + 0.5) / (df + 0.5) + 1.0).ln();

        for (doc_id, tf, doc) in posting_vec {
            let doc_len = doc.doc_length as f64;
            let numerator = tf * (K1 + 1.0);
            let denominator = tf + K1 * (1.0 - B + B * doc_len / avg_doc_length);
            let term_score = idf * numerator / denominator;

            let entry = scores.entry(doc_id).or_insert((0.0, Vec::new(), doc));
            entry.0 += term_score;
            if !entry.1.contains(term) {
                entry.1.push(term.clone());
            }
        }
    }

    let mut results: Vec<Bm25SearchResult> = scores
        .into_iter()
        .map(|(_doc_id, (score, matched_terms, doc))| Bm25SearchResult {
            hash: doc.hash,
            symbol: doc.symbol,
            file: doc.file,
            lines: doc.lines,
            kind: doc.kind,
            module: doc.module,
            risk: doc.risk,
            score,
            matched_terms,
        })
        .collect();

    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(limit);
    Ok(results)
}

/// Tokenize text into searchable terms
///
/// This function:
/// - Converts to lowercase
/// - Splits on camelCase and snake_case boundaries
/// - Removes very short terms (< 2 chars)
/// - Removes common stop words
pub fn tokenize(text: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Split on whitespace, punctuation, and underscores
    for word in text.split(|c: char| !c.is_alphanumeric()) {
        if word.is_empty() {
            continue;
        }

        // Handle camelCase
        let mut current = String::new();
        let chars = word.chars();

        for c in chars {
            if c.is_uppercase() && !current.is_empty() {
                // Start of new word
                if current.len() >= 2 && !is_stop_word(&current) {
                    let lower = current.to_lowercase();
                    if seen.insert(lower.clone()) {
                        terms.push(lower);
                    }
                }
                current = String::new();
            }
            current.push(c);
        }

        // Don't forget the last segment
        if current.len() >= 2 && !is_stop_word(&current) {
            let lower = current.to_lowercase();
            if seen.insert(lower.clone()) {
                terms.push(lower);
            }
        }

        // Also add the full word if it's different from segments
        let lower_word = word.to_lowercase();
        if lower_word.len() >= 2 && !is_stop_word(&lower_word) {
            if seen.insert(lower_word.clone()) {
                terms.push(lower_word);
            }
        }
    }

    terms
}

pub fn extract_terms_from_file_path(file_path: &str) -> Vec<String> {
    let mut terms = Vec::new();

    if let Some(filename) = Path::new(file_path).file_stem() {
        terms.extend(tokenize(&filename.to_string_lossy()));
    }
    if let Some(parent) = Path::new(file_path).parent() {
        if let Some(dir_name) = parent.file_name() {
            terms.extend(tokenize(&dir_name.to_string_lossy()));
        }
    }

    terms
}

pub fn extract_terms_from_toon(content: &str) -> Vec<String> {
    let mut terms = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();

        // Extract called function names
        if trimmed.starts_with("calls") || trimmed.contains("->") {
            for part in trimmed.split(|c: char| !c.is_alphanumeric() && c != '_') {
                if part.len() >= 3 {
                    terms.extend(tokenize(part));
                }
            }
        }

        // Extract from state changes
        if trimmed.starts_with("state:") || trimmed.contains("let ") || trimmed.contains("var ") {
            for part in trimmed.split(|c: char| !c.is_alphanumeric() && c != '_') {
                if part.len() >= 3 {
                    terms.extend(tokenize(part));
                }
            }
        }

        // Extract from control flow keywords
        if trimmed.starts_with("if")
            || trimmed.starts_with("for")
            || trimmed.starts_with("while")
            || trimmed.starts_with("match")
            || trimmed.starts_with("try")
        {
            terms.push(
                trimmed
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_lowercase(),
            );
        }
    }

    terms
}

/// Extract searchable terms from a symbol for indexing
pub fn extract_terms_from_symbol(
    symbol_name: &str,
    file_path: &str,
    kind: &str,
    toon_content: Option<&str>,
) -> Vec<String> {
    let mut terms = Vec::new();

    // Extract from symbol name
    terms.extend(tokenize(symbol_name));

    // Extract from file path (just filename and parent dir)
    terms.extend(extract_terms_from_file_path(file_path));

    // Add kind as a term
    terms.push(kind.to_lowercase());

    // Extract from TOON content if available
    if let Some(content) = toon_content {
        terms.extend(extract_terms_from_toon(content));
    }

    // Deduplicate while preserving order
    let mut seen = std::collections::HashSet::new();
    terms.retain(|t| seen.insert(t.clone()));

    terms
}

/// Check if a word is a stop word (common words with little semantic value)
fn is_stop_word(word: &str) -> bool {
    matches!(
        word.to_lowercase().as_str(),
        // Common programming terms that don't add search value
        "the" | "a" | "an" | "is" | "are" | "was" | "be" | "to" | "of" | "and" |
        "in" | "it" | "for" | "on" | "with" | "as" | "at" | "by" | "or" | "if" |
        // Very common code terms
        "fn" | "let" | "var" | "const" | "mut" | "pub" | "self" | "impl" |
        // Single characters
        "i" | "j" | "k" | "n" | "x" | "y" | "e" | "t" | "s"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_camel_case() {
        let terms = tokenize("handleUserAuthentication");
        assert!(terms.contains(&"handle".to_string()));
        assert!(terms.contains(&"user".to_string()));
        assert!(terms.contains(&"authentication".to_string()));
    }

    #[test]
    fn test_tokenize_snake_case() {
        let terms = tokenize("handle_user_auth");
        assert!(terms.contains(&"handle".to_string()));
        assert!(terms.contains(&"user".to_string()));
        assert!(terms.contains(&"auth".to_string()));
    }

    #[test]
    fn test_stop_words_filtered() {
        let terms = tokenize("the function is a test");
        assert!(!terms.contains(&"the".to_string()));
        assert!(!terms.contains(&"is".to_string()));
        assert!(!terms.contains(&"a".to_string()));
        assert!(terms.contains(&"function".to_string()));
        assert!(terms.contains(&"test".to_string()));
    }

    #[test]
    fn test_bm25_search() {
        let mut index = Bm25Index::new();

        index.add_document(
            Bm25Document {
                hash: "hash1".to_string(),
                symbol: "authenticate_user".to_string(),
                file: "src/auth.rs".to_string(),
                lines: "10-50".to_string(),
                kind: "fn".to_string(),
                module: "auth".to_string(),
                risk: "low".to_string(),
                doc_length: 0,
            },
            vec![
                "authenticate".to_string(),
                "user".to_string(),
                "login".to_string(),
            ],
        );

        index.add_document(
            Bm25Document {
                hash: "hash2".to_string(),
                symbol: "format_output".to_string(),
                file: "src/format.rs".to_string(),
                lines: "20-40".to_string(),
                kind: "fn".to_string(),
                module: "format".to_string(),
                risk: "low".to_string(),
                doc_length: 0,
            },
            vec![
                "format".to_string(),
                "output".to_string(),
                "display".to_string(),
            ],
        );

        index.finalize();

        let results = index.search("authentication login", 10);
        assert!(!results.is_empty());
        assert_eq!(results[0].symbol, "authenticate_user");
    }
}
