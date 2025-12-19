//! SQL Injection (CWE-89) patterns
//!
//! Generic patterns for SQL injection vulnerabilities across languages

use crate::lang::Lang;
use crate::security::compiler::fingerprinter::fingerprint_from_source;
use crate::security::{CVEPattern, PatternSource};

/// SQL Injection vulnerable patterns
pub fn patterns() -> Vec<CVEPattern> {
    vec![
        // JavaScript/TypeScript patterns
        js_string_concat_sql(),
        js_template_literal_sql(),
        // Python patterns
        python_format_string_sql(),
        python_percent_format_sql(),
        // Java patterns
        java_string_concat_sql(),
        // Rust patterns
        rust_format_sql(),
        // C# patterns
        csharp_string_concat_sql(),
    ]
}

/// JavaScript string concatenation SQL injection
fn js_string_concat_sql() -> CVEPattern {
    let source = r#"
        const query = "SELECT * FROM users WHERE id = " + userId;
        db.query(query);
        connection.execute("SELECT * FROM orders WHERE user_id = " + req.params.id);
    "#;

    let fp = fingerprint_from_source(source, Lang::JavaScript);

    CVEPattern::new("CWE-89-JS-CONCAT", vec!["CWE-89".into()], 0)
        .with_fingerprints(
            fp.fingerprints.call,
            fp.fingerprints.control_flow,
            fp.fingerprints.state,
        )
        .with_vulnerable_calls(vec![
            "query".into(),
            "execute".into(),
            "raw".into(),
            "exec".into(),
            "all".into(),
            "get".into(),
            "run".into(),
        ])
        .with_cvss(8.6)
        .with_description("SQL injection via string concatenation in JavaScript")
        .with_languages(vec![Lang::JavaScript, Lang::TypeScript])
        .with_source(PatternSource::ManualCuration {
            author: "Semfora Security Team".into(),
            date: "2024-01-01".into(),
        })
        .with_confidence(0.85)
}

/// JavaScript template literal SQL injection
fn js_template_literal_sql() -> CVEPattern {
    let source = r#"
        const query = `SELECT * FROM users WHERE name = '${userName}'`;
        await db.query(`DELETE FROM posts WHERE id = ${postId}`);
    "#;

    let fp = fingerprint_from_source(source, Lang::JavaScript);

    CVEPattern::new("CWE-89-JS-TEMPLATE", vec!["CWE-89".into()], 1)
        .with_fingerprints(
            fp.fingerprints.call,
            fp.fingerprints.control_flow,
            fp.fingerprints.state,
        )
        .with_vulnerable_calls(vec!["query".into(), "execute".into(), "raw".into()])
        .with_cvss(8.6)
        .with_description("SQL injection via template literals in JavaScript")
        .with_languages(vec![Lang::JavaScript, Lang::TypeScript])
        .with_source(PatternSource::ManualCuration {
            author: "Semfora Security Team".into(),
            date: "2024-01-01".into(),
        })
        .with_confidence(0.85)
}

/// Python f-string SQL injection
fn python_format_string_sql() -> CVEPattern {
    let source = r#"
        query = f"SELECT * FROM users WHERE id = {user_id}"
        cursor.execute(query)
        db.execute(f"DELETE FROM posts WHERE author = '{author}'")
    "#;

    let fp = fingerprint_from_source(source, Lang::Python);

    CVEPattern::new("CWE-89-PY-FSTRING", vec!["CWE-89".into()], 2)
        .with_fingerprints(
            fp.fingerprints.call,
            fp.fingerprints.control_flow,
            fp.fingerprints.state,
        )
        .with_vulnerable_calls(vec!["execute".into(), "executemany".into(), "raw".into()])
        .with_cvss(8.6)
        .with_description("SQL injection via f-strings in Python")
        .with_languages(vec![Lang::Python])
        .with_source(PatternSource::ManualCuration {
            author: "Semfora Security Team".into(),
            date: "2024-01-01".into(),
        })
        .with_confidence(0.85)
}

/// Python percent format SQL injection
fn python_percent_format_sql() -> CVEPattern {
    let source = r#"
        query = "SELECT * FROM users WHERE id = %s" % user_id
        cursor.execute(query)
        db.execute("SELECT * FROM posts WHERE id = %d" % post_id)
    "#;

    let fp = fingerprint_from_source(source, Lang::Python);

    CVEPattern::new("CWE-89-PY-PERCENT", vec!["CWE-89".into()], 3)
        .with_fingerprints(
            fp.fingerprints.call,
            fp.fingerprints.control_flow,
            fp.fingerprints.state,
        )
        .with_vulnerable_calls(vec!["execute".into(), "executemany".into()])
        .with_cvss(8.6)
        .with_description("SQL injection via percent formatting in Python")
        .with_languages(vec![Lang::Python])
        .with_source(PatternSource::ManualCuration {
            author: "Semfora Security Team".into(),
            date: "2024-01-01".into(),
        })
        .with_confidence(0.80)
}

/// Java string concatenation SQL injection
fn java_string_concat_sql() -> CVEPattern {
    let source = r#"
        String query = "SELECT * FROM users WHERE id = " + userId;
        Statement stmt = connection.createStatement();
        ResultSet rs = stmt.executeQuery(query);
        connection.prepareStatement("SELECT * FROM " + tableName);
    "#;

    let fp = fingerprint_from_source(source, Lang::Java);

    CVEPattern::new("CWE-89-JAVA-CONCAT", vec!["CWE-89".into()], 4)
        .with_fingerprints(
            fp.fingerprints.call,
            fp.fingerprints.control_flow,
            fp.fingerprints.state,
        )
        .with_vulnerable_calls(vec![
            "executeQuery".into(),
            "executeUpdate".into(),
            "execute".into(),
            "createStatement".into(),
            "prepareStatement".into(),
        ])
        .with_cvss(8.6)
        .with_description("SQL injection via string concatenation in Java")
        .with_languages(vec![Lang::Java])
        .with_source(PatternSource::ManualCuration {
            author: "Semfora Security Team".into(),
            date: "2024-01-01".into(),
        })
        .with_confidence(0.85)
}

/// Rust format! SQL injection
fn rust_format_sql() -> CVEPattern {
    let source = r#"
        let query = format!("SELECT * FROM users WHERE id = {}", user_id);
        sqlx::query(&query).fetch_all(&pool).await?;
        conn.execute(&format!("DELETE FROM posts WHERE id = {}", id))?;
    "#;

    let fp = fingerprint_from_source(source, Lang::Rust);

    CVEPattern::new("CWE-89-RUST-FORMAT", vec!["CWE-89".into()], 5)
        .with_fingerprints(
            fp.fingerprints.call,
            fp.fingerprints.control_flow,
            fp.fingerprints.state,
        )
        .with_vulnerable_calls(vec![
            "query".into(),
            "execute".into(),
            "fetch_all".into(),
            "fetch_one".into(),
            "fetch_optional".into(),
        ])
        .with_cvss(8.6)
        .with_description("SQL injection via format! macro in Rust")
        .with_languages(vec![Lang::Rust])
        .with_source(PatternSource::ManualCuration {
            author: "Semfora Security Team".into(),
            date: "2024-01-01".into(),
        })
        .with_confidence(0.80)
}

/// C# string concatenation SQL injection
fn csharp_string_concat_sql() -> CVEPattern {
    let source = r#"
        string query = "SELECT * FROM Users WHERE Id = " + userId;
        SqlCommand cmd = new SqlCommand(query, connection);
        cmd.ExecuteReader();
        connection.Execute("SELECT * FROM Posts WHERE Author = '" + author + "'");
    "#;

    let fp = fingerprint_from_source(source, Lang::CSharp);

    CVEPattern::new("CWE-89-CSHARP-CONCAT", vec!["CWE-89".into()], 6)
        .with_fingerprints(
            fp.fingerprints.call,
            fp.fingerprints.control_flow,
            fp.fingerprints.state,
        )
        .with_vulnerable_calls(vec![
            "SqlCommand".into(),
            "ExecuteReader".into(),
            "ExecuteNonQuery".into(),
            "ExecuteScalar".into(),
            "Execute".into(),
        ])
        .with_cvss(8.6)
        .with_description("SQL injection via string concatenation in C#")
        .with_languages(vec![Lang::CSharp])
        .with_source(PatternSource::ManualCuration {
            author: "Semfora Security Team".into(),
            date: "2024-01-01".into(),
        })
        .with_confidence(0.85)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::Severity;

    #[test]
    fn test_sql_injection_patterns() {
        let patterns = patterns();
        assert!(!patterns.is_empty());

        for pattern in &patterns {
            assert!(pattern.cwe_ids.contains(&"CWE-89".to_string()));
            assert!(pattern.severity == Severity::High);
        }
    }
}
