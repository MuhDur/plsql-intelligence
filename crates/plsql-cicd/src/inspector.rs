//! Offline SQL classifier helpers for CI/CD planning.
//!
//! Earlier versions hosted a live Oracle inspector here. That live I/O path
//! now belongs in `oraclemcp`; this module keeps only the stable, offline
//! statement classifier used by planning and safety diagnostics.

/// Statically classify `sql` as a read-only statement (`SELECT` or `WITH`
/// CTE). Strips leading whitespace + block comments before classifying.
#[must_use]
pub fn is_read_only_sql(sql: &str) -> bool {
    let mut remainder = sql.trim_start();
    // Strip leading SQL block comments to reach the verb token.
    while remainder.starts_with("/*") {
        if let Some(end) = remainder.find("*/") {
            remainder = remainder[end + 2..].trim_start();
        } else {
            return false;
        }
    }
    let token = remainder
        .split(|c: char| c.is_whitespace() || c == '(')
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    matches!(token.as_str(), "SELECT" | "WITH")
}

#[must_use]
pub fn preview_sql(sql: &str) -> String {
    let trimmed = sql.trim();
    let mut preview: String = trimmed.chars().take(72).collect();
    if trimmed.len() > 72 {
        preview.push('…');
    }
    preview
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_read_only_sql_accepts_select_and_with() {
        assert!(is_read_only_sql("SELECT 1 FROM DUAL"));
        assert!(is_read_only_sql("  select 1 from dual"));
        assert!(is_read_only_sql(
            "WITH cte AS (SELECT 1 FROM DUAL) SELECT * FROM cte"
        ));
        assert!(is_read_only_sql("/* hint */ select 1 from dual"));
    }

    #[test]
    fn is_read_only_sql_rejects_ddl_dml_and_anonymous_blocks() {
        assert!(!is_read_only_sql("INSERT INTO FOO VALUES (1)"));
        assert!(!is_read_only_sql("UPDATE FOO SET A = 1"));
        assert!(!is_read_only_sql("DELETE FROM FOO"));
        assert!(!is_read_only_sql("CREATE TABLE FOO (A NUMBER)"));
        assert!(!is_read_only_sql("ALTER TABLE FOO ADD B NUMBER"));
        assert!(!is_read_only_sql("DROP TABLE FOO"));
        assert!(!is_read_only_sql("GRANT SELECT ON FOO TO PUBLIC"));
        assert!(!is_read_only_sql("BEGIN proc; END;"));
        assert!(!is_read_only_sql("/* unterminated comment"));
    }

    #[test]
    fn preview_sql_truncates_long_statements() {
        let sql = format!("select {} from dual", "x".repeat(100));
        let preview = preview_sql(&sql);
        assert!(preview.len() < sql.len());
        assert!(preview.ends_with('…'));
    }
}
