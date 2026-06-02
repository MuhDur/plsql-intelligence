//! Top-level declaration lowering.
//!
//! This module scans PL/SQL source text and produces [`plsql_parser::Ast`]
//! nodes for top-level declarations: packages, procedures, functions,
//! triggers, views, and types.
//!
//! # Architecture
//!
//! This is a **text-scanning pre-parser** that recognizes the `CREATE [OR
//! REPLACE]` header of each top-level construct and extracts the
//! declaration name and span.  It does NOT parse statement bodies,
//! expressions, or types — that work belongs to PARSE-005/006/007.
//!
//! Once the ANTLR backend compiles cleanly (PARSE-002 blockers resolved),
//! this module will be superseded by proper ANTLR parse-tree lowering.
//! The `Ast` output shape is identical either way.
//!
//! # R13 compliance
//!
//! Text that cannot be classified as a known declaration kind is lowered
//! to `AstDecl::Unknown` — never silently dropped.

use plsql_core::{FileId, Position, Span};
use plsql_parser::ast::{Ast, AstDecl, AstExpr, AstStatement, AstTypeDecl, SourceFile};

/// Lower a source file's text into an [`Ast`].
///
/// Scans for top-level `CREATE [OR REPLACE]` declarations and produces
/// one [`AstDecl`] per declaration found.  Statements that are not
/// preceded by `CREATE` are not yet recognized (that's PARSE-005+).
pub fn lower_source(source: &str, file_id: FileId) -> Ast {
    let declarations = scan_declarations(source, file_id);
    // Saturating cast (oracle-kxb3 sibling): the legacy `as u32`
    // wraps for a >u32::MAX source, producing a tiny span that
    // overlaps every diagnostic. Saturate to `u32::MAX`.
    let total_len = u32::try_from(source.len()).unwrap_or(u32::MAX);

    let root = SourceFile {
        span: Span::new(
            file_id,
            Position::new(1, 1, 0),
            Position::new(1, 1, total_len),
        ),
        declarations,
    };

    Ast {
        root,
        source_map: plsql_parser::ast::SourceMap::new(),
        body_statements: Vec::new(),
    }
}

/// Scan source text for top-level declarations.
fn scan_declarations(source: &str, file_id: FileId) -> Vec<AstDecl> {
    let mut decls = Vec::new();
    let bytes = source.as_bytes();
    let len = bytes.len();
    let mut pos = 0;

    while pos < len {
        // Skip whitespace and comments
        let skip = skip_whitespace_and_comments(bytes, pos);
        if skip > 0 {
            pos += skip;
            continue;
        }

        // Look for a top-level DDL keyword. CREATE has the richest
        // sub-keyword vocabulary (PACKAGE / PROCEDURE / FUNCTION / …) so
        // it gets a specialised path. ALTER / DROP / GRANT / REVOKE /
        // COMMENT funnel through `lower_simple_ddl` because dependency
        // analysis only needs to know the kind + target shape, not the
        // full statement body. (PLSQL-PARSE-008)
        if matches_keyword_ignore_case(bytes, pos, b"ALTER") {
            let decl = lower_simple_ddl(bytes, file_id, pos, "ALTER", 5);
            decls.push(decl);
            pos = advance_past_statement_end(bytes, pos + 5);
            continue;
        }
        if matches_keyword_ignore_case(bytes, pos, b"DROP") {
            let decl = lower_simple_ddl(bytes, file_id, pos, "DROP", 4);
            decls.push(decl);
            pos = advance_past_statement_end(bytes, pos + 4);
            continue;
        }
        if matches_keyword_ignore_case(bytes, pos, b"GRANT") {
            let decl = lower_simple_ddl(bytes, file_id, pos, "GRANT", 5);
            decls.push(decl);
            pos = advance_past_statement_end(bytes, pos + 5);
            continue;
        }
        if matches_keyword_ignore_case(bytes, pos, b"REVOKE") {
            let decl = lower_simple_ddl(bytes, file_id, pos, "REVOKE", 6);
            decls.push(decl);
            pos = advance_past_statement_end(bytes, pos + 6);
            continue;
        }
        if matches_keyword_ignore_case(bytes, pos, b"COMMENT") {
            let decl = lower_simple_ddl(bytes, file_id, pos, "COMMENT", 7);
            decls.push(decl);
            pos = advance_past_statement_end(bytes, pos + 7);
            continue;
        }

        if !matches_keyword_ignore_case(bytes, pos, b"CREATE") {
            pos += 1;
            continue;
        }

        let create_start = pos;
        pos += 6; // skip "CREATE"

        // Skip whitespace
        let ws = skip_whitespace(bytes, pos);
        pos += ws;

        // Optional OR REPLACE
        if matches_keyword_ignore_case(bytes, pos, b"OR") {
            let after_or = pos + 2;
            let ws2 = skip_whitespace(bytes, after_or);
            if matches_keyword_ignore_case(bytes, after_or + ws2, b"REPLACE") {
                pos = after_or + ws2 + 7;
                let ws3 = skip_whitespace(bytes, pos);
                pos += ws3;
            }
        }

        // Skip whitespace
        let ws = skip_whitespace(bytes, pos);
        pos += ws;

        // Now classify the declaration kind
        let decl = if matches_keyword_ignore_case(bytes, pos, b"PACKAGE") {
            lower_package(bytes, source, file_id, create_start, pos + 7)
        } else if matches_keyword_ignore_case(bytes, pos, b"PROCEDURE") {
            lower_procedure(bytes, source, file_id, create_start, pos + 9)
        } else if matches_keyword_ignore_case(bytes, pos, b"FUNCTION") {
            lower_function(bytes, source, file_id, create_start, pos + 8)
        } else if matches_keyword_ignore_case(bytes, pos, b"TRIGGER") {
            lower_trigger(bytes, source, file_id, create_start, pos + 7)
        } else if matches_keyword_ignore_case(bytes, pos, b"VIEW") {
            lower_view(bytes, source, file_id, create_start, pos + 4)
        } else if matches_keyword_ignore_case(bytes, pos, b"TYPE") {
            lower_type(bytes, source, file_id, create_start, pos + 4)
        } else {
            // Unknown CREATE statement — record as DDL
            lower_unknown_create(bytes, source, file_id, create_start, pos)
        };

        decls.push(decl);

        // Advance past this declaration to avoid re-matching
        pos = advance_past_statement_end(bytes, pos);
    }

    decls
}

// ---------------------------------------------------------------------------
// Per-kind lowering
// ---------------------------------------------------------------------------

/// Lower a `CREATE [OR REPLACE] PACKAGE [BODY] <name>` declaration.
fn lower_package(
    bytes: &[u8],
    source: &str,
    file_id: FileId,
    create_start: usize,
    after_keyword: usize,
) -> AstDecl {
    let mut pos = after_keyword;
    pos += skip_whitespace(bytes, pos);

    let is_body = matches_keyword_ignore_case(bytes, pos, b"BODY");
    if is_body {
        pos += 4;
        pos += skip_whitespace(bytes, pos);
    }

    let name = extract_identifier(source, pos);
    let end = advance_to_decl_end(bytes, create_start);
    let span = make_span(file_id, create_start as u32, end as u32);

    if is_body {
        AstDecl::PackageBody { name, span }
    } else {
        AstDecl::PackageSpec { name, span }
    }
}

/// Lower a `CREATE [OR REPLACE] PROCEDURE <name>` declaration.
fn lower_procedure(
    bytes: &[u8],
    source: &str,
    file_id: FileId,
    create_start: usize,
    after_keyword: usize,
) -> AstDecl {
    let mut pos = after_keyword;
    pos += skip_whitespace(bytes, pos);

    let name = extract_identifier(source, pos);
    let end = advance_to_decl_end(bytes, create_start);
    let span = make_span(file_id, create_start as u32, end as u32);

    AstDecl::Procedure { name, span }
}

/// Lower a `CREATE [OR REPLACE] FUNCTION <name>` declaration.
fn lower_function(
    bytes: &[u8],
    source: &str,
    file_id: FileId,
    create_start: usize,
    after_keyword: usize,
) -> AstDecl {
    let mut pos = after_keyword;
    pos += skip_whitespace(bytes, pos);

    let name = extract_identifier(source, pos);
    let end = advance_to_decl_end(bytes, create_start);
    let span = make_span(file_id, create_start as u32, end as u32);

    AstDecl::Function { name, span }
}

/// Lower a `CREATE [OR REPLACE] TRIGGER <name>` declaration.
fn lower_trigger(
    bytes: &[u8],
    source: &str,
    file_id: FileId,
    create_start: usize,
    after_keyword: usize,
) -> AstDecl {
    let mut pos = after_keyword;
    pos += skip_whitespace(bytes, pos);

    let name = extract_identifier(source, pos);
    let end = advance_to_decl_end(bytes, create_start);
    let span = make_span(file_id, create_start as u32, end as u32);

    AstDecl::Trigger { name, span }
}

/// Lower a `CREATE [OR REPLACE] VIEW <name>` declaration.
fn lower_view(
    bytes: &[u8],
    source: &str,
    file_id: FileId,
    create_start: usize,
    after_keyword: usize,
) -> AstDecl {
    let mut pos = after_keyword;
    pos += skip_whitespace(bytes, pos);

    let name = extract_identifier(source, pos);
    let end = advance_to_decl_end(bytes, create_start);
    let span = make_span(file_id, create_start as u32, end as u32);

    AstDecl::View { name, span }
}

/// Lower a `CREATE [OR REPLACE] TYPE [BODY] <name>` declaration.
fn lower_type(
    bytes: &[u8],
    source: &str,
    file_id: FileId,
    create_start: usize,
    after_keyword: usize,
) -> AstDecl {
    let mut pos = after_keyword;
    pos += skip_whitespace(bytes, pos);

    let is_body = matches_keyword_ignore_case(bytes, pos, b"BODY");
    if is_body {
        pos += 4;
        pos += skip_whitespace(bytes, pos);
    }

    let name = extract_identifier(source, pos);
    let end = advance_to_decl_end(bytes, create_start);
    let span = make_span(file_id, create_start as u32, end as u32);

    if is_body {
        AstDecl::TypeBody { name, span }
    } else {
        AstDecl::TypeSpec { name, span }
    }
}

/// The fixed allowlist of top-level DDL *object* grammar keywords
/// the text scanner recognises. Every entry is a SQL **grammar
/// keyword constant** (never estate data). Used only to *classify
/// by keyword comparison* — the scanned bytes are matched against
/// this list, never echoed into the result. The returned name
/// mirrors the ANTLR `create_<obj>` rule-name shape so a
/// text-scanner-path gap and a parse-tree-path gap for the *same*
/// DDL class share the same `antlr_rule_path` (maximises dedup;
/// §2.1`, I-PRIVACY).
const DDL_OBJECT_KEYWORDS: &[&str] = &[
    "TABLE",
    "INDEX",
    "SEQUENCE",
    "SYNONYM",
    "VIEW",
    "TYPE",
    "TRIGGER",
    "PACKAGE",
    "PROCEDURE",
    "FUNCTION",
    "MATERIALIZED",
    "DATABASE",
    "TABLESPACE",
    "USER",
    "ROLE",
    "DIRECTORY",
    "CONTEXT",
    "CLUSTER",
    "LIBRARY",
    "OUTLINE",
    "DIMENSION",
    "PROFILE",
];

/// Fixed allowlist of DDL *modifier* keywords that may sit between
/// the verb and the object keyword (`CREATE UNIQUE INDEX`,
/// `CREATE GLOBAL TEMPORARY TABLE`, `CREATE OR REPLACE FORCE
/// EDITIONABLE VIEW`, `DROP PUBLIC SYNONYM`, …). All grammar
/// keyword constants — skipped by *comparison*, never echoed
/// (I-PRIVACY).
const DDL_MODIFIER_KEYWORDS: &[&str] = &[
    "OR",
    "REPLACE",
    "UNIQUE",
    "BITMAP",
    "GLOBAL",
    "PRIVATE",
    "TEMPORARY",
    "SHARDED",
    "DUPLICATED",
    "PUBLIC",
    "FORCE",
    "NO",
    "EDITIONABLE",
    "NONEDITIONABLE",
    "MULTITENANT",
];

/// Classify the keyword starting at `pos` against
/// [`DDL_OBJECT_KEYWORDS`] (whole-word, case-insensitive), first
/// skipping any leading [`DDL_MODIFIER_KEYWORDS`], and return its
/// lowercased grammar-keyword form, or `None`. **Only allowlist
/// constants are matched/returned — the source bytes are compared,
/// never echoed** (I-PRIVACY: nothing estate-derived can escape).
fn ddl_object_keyword(bytes: &[u8], pos: usize) -> Option<&'static str> {
    // Skip a bounded run of modifier keywords (bounded so a
    // pathological input cannot loop; real Oracle DDL never stacks
    // more than ~4 modifiers).
    let mut p = pos;
    for _ in 0..6 {
        let Some(modifier) = DDL_MODIFIER_KEYWORDS
            .iter()
            .find(|kw| matches_keyword_ignore_case(bytes, p, kw.as_bytes()))
        else {
            break;
        };
        p += modifier.len();
        p += skip_whitespace(bytes, p);
    }
    DDL_OBJECT_KEYWORDS
        .iter()
        .find(|kw| matches_keyword_ignore_case(bytes, p, kw.as_bytes()))
        .map(|kw| match *kw {
            "TABLE" => "table",
            "INDEX" => "index",
            "SEQUENCE" => "sequence",
            "SYNONYM" => "synonym",
            "VIEW" => "view",
            "TYPE" => "type",
            "TRIGGER" => "trigger",
            "PACKAGE" => "package",
            "PROCEDURE" => "procedure",
            "FUNCTION" => "function",
            "MATERIALIZED" => "materialized_view",
            "DATABASE" => "database",
            "TABLESPACE" => "tablespace",
            "USER" => "user",
            "ROLE" => "role",
            "DIRECTORY" => "directory",
            "CONTEXT" => "context",
            "CLUSTER" => "cluster",
            "LIBRARY" => "library",
            "OUTLINE" => "outline",
            "DIMENSION" => "dimension",
            "PROFILE" => "profile",
            _ => "object",
        })
}

/// Synthesise a grammar-keyword-shaped `antlr_rule_path` for a
/// text-scanner DDL whose file ANTLR could not build a parse tree
/// for (the `backend.rs` whole-file fallback). The path is
/// `text_scan>create_<obj>` / `text_scan>alter` / … — its
/// components are **only** the literal verb passed by the
/// dispatcher (a hardcoded keyword constant) and an allowlisted
/// object keyword. No scanned identifier/literal byte is ever
/// included (I-PRIVACY); the value is a `String` (R20). The
/// `text_scan>` prefix honestly records that this is the
/// no-parse-tree provenance, distinct from a real ANTLR position.
fn text_scan_ddl_rule_path(verb: &str, object: Option<&str>) -> String {
    let verb_lc = verb.to_ascii_lowercase();
    match object {
        Some(obj) => format!("text_scan>{verb_lc}_{obj}"),
        None => format!("text_scan>{verb_lc}"),
    }
}

/// Lower an unrecognized `CREATE ...` statement.
fn lower_unknown_create(
    bytes: &[u8],
    _source: &str,
    file_id: FileId,
    create_start: usize,
    after_create: usize,
) -> AstDecl {
    // Try to extract the DDL kind (first keyword after CREATE [OR REPLACE])
    let kind_end = scan_to_whitespace(bytes, after_create);
    let kind = String::from_utf8_lossy(&bytes[after_create..kind_end]).to_string();
    let end = advance_to_decl_end(bytes, create_start);
    let span = make_span(file_id, create_start as u32, end as u32);

    // USR-loop §2.1: fine-grained, privacy-safe gap signature even
    // on the no-parse-tree path. The object keyword is classified
    // against a fixed grammar-keyword allowlist (matched, never
    // echoed) so two CREATE TABLE gaps cluster, while CREATE
    // SEQUENCE vs CREATE SYNONYM stay distinct.
    let object = ddl_object_keyword(bytes, after_create);
    AstDecl::Ddl {
        kind,
        span,
        antlr_rule_path: Some(text_scan_ddl_rule_path("create", object)),
    }
}

/// Lower an `ALTER` / `DROP` / `GRANT` / `REVOKE` / `COMMENT` statement.
///
/// Dependency analysis only needs the leading verb and the next
/// keyword (e.g. `ALTER TABLE`, `DROP INDEX`, `GRANT SELECT`); the
/// statement body itself is consumed by `advance_past_statement_end`.
///
/// `verb` is the leading word, `verb_len` its byte length.
fn lower_simple_ddl(
    bytes: &[u8],
    file_id: FileId,
    statement_start: usize,
    verb: &str,
    verb_len: usize,
) -> AstDecl {
    let after_verb = statement_start + verb_len;
    let ws = skip_whitespace(bytes, after_verb);
    let target_start = after_verb + ws;
    let target_end = scan_to_whitespace(bytes, target_start);

    let kind = if target_end > target_start {
        let target = String::from_utf8_lossy(&bytes[target_start..target_end]).to_uppercase();
        format!("{verb} {target}")
    } else {
        verb.to_owned()
    };

    let end = advance_past_statement_end(bytes, after_verb);
    let span = make_span(file_id, statement_start as u32, end as u32);

    // USR-loop §2.1: privacy-safe fine-grained rule path. For
    // `ALTER`/`DROP` the word after the verb is itself a grammar
    // *object keyword* (`ALTER TABLE`, `DROP INDEX`) — classify it
    // against the fixed allowlist (matched, never echoed) so e.g.
    // `alter_table` clusters with the ANTLR-path `alter_table`. For
    // `GRANT`/`REVOKE`/`COMMENT` the next token is a
    // privilege/identifier (estate-derived) — so verb-only, never
    // the scanned target.
    let object = match verb {
        "ALTER" | "DROP" => ddl_object_keyword(bytes, target_start),
        _ => None,
    };
    AstDecl::Ddl {
        kind,
        span,
        antlr_rule_path: Some(text_scan_ddl_rule_path(verb, object)),
    }
}

// ---------------------------------------------------------------------------
// Statement-body lowering (PLSQL-PARSE-005)
// ---------------------------------------------------------------------------

/// Lower a routine / anonymous-block body source slice into the
/// syntactic [`AstStatement`] sequence.
///
/// `body` is the text between `BEGIN` and the matching `END;` of
/// a routine (the caller is responsible for extracting it).
/// `file_id` + `base_offset` let the produced spans point back
/// into the original file: each statement's span is offset by
/// `base_offset` so it stays consistent with the file-level AST.
///
/// This is the parser-layer (syntactic) counterpart to the
/// semantic statement IR in `plsql_ir::stmt`. It recognises the
/// common shapes the lab corpus exercises — assignment, control
/// flow, RAISE / RETURN, EXECUTE IMMEDIATE, embedded SQL,
/// statement-level calls — and emits [`AstStatement::Unknown`]
/// (R13) for anything it cannot classify rather than dropping it.
///
/// The `;`-splitter depth-tracks every block opener — `BEGIN`,
/// `IF`, `LOOP`, `CASE` — and the matching terminators (bare `END`
/// plus `END IF` / `END LOOP` / `END CASE`). An inner semicolon
/// inside *any* of those blocks therefore does not split the
/// statement: the whole control-flow body stays one chunk so its
/// nested DML is recovered intact.
#[must_use]
pub fn lower_statement_body(body: &str, file_id: FileId, base_offset: usize) -> Vec<AstStatement> {
    let mut out: Vec<AstStatement> = Vec::new();
    let bytes = body.as_bytes();
    let mut depth: i32 = 0;
    let mut chunk_start = 0usize;
    let mut i = 0usize;
    // Byte-index walk; track block depth so an inner `;` inside a
    // nested BEGIN…END / IF…END IF / LOOP…END LOOP / CASE…END CASE
    // does not split the statement.
    while i < bytes.len() {
        // Skip comments and string/q-quote literals first so a `;`, `END`,
        // `BEGIN`, `IF`, `LOOP`, or `CASE` embedded in a literal (common in
        // dynamic SQL builders and dbms_output messages) does not mis-split
        // the body or skew the block-depth bookkeeping.
        if let Some(next) = crate::recover::skip_opaque_span(bytes, i, 0) {
            i = next;
            continue;
        }
        // `END IF` / `END LOOP` / `END CASE` must be matched before a
        // bare `END`, otherwise the bare-`END` arm would consume the
        // `END` and the depth bookkeeping would double-count.
        if let Some(consumed) = end_keyword_len(body, i) {
            depth = (depth - 1).max(0);
            i += consumed;
            continue;
        }
        if keyword_at(body, i, "BEGIN") {
            depth += 1;
            i += 5;
            continue;
        }
        if keyword_at(body, i, "IF") {
            depth += 1;
            i += 2;
            continue;
        }
        if keyword_at(body, i, "LOOP") {
            depth += 1;
            i += 4;
            continue;
        }
        if keyword_at(body, i, "CASE") {
            depth += 1;
            i += 4;
            continue;
        }
        if bytes[i] == b';' && depth == 0 {
            let raw = body[chunk_start..=i].trim().to_string();
            if !raw.is_empty() {
                out.push(classify_statement(
                    &raw,
                    file_id,
                    base_offset + chunk_start,
                    base_offset + i + 1,
                ));
            }
            chunk_start = i + 1;
        }
        i += 1;
    }
    let tail = body[chunk_start..].trim();
    if !tail.is_empty() {
        out.push(classify_statement(
            tail,
            file_id,
            base_offset + chunk_start,
            base_offset + body.len(),
        ));
    }
    out
}

/// If a block terminator starts at byte `pos`, return its length in
/// bytes (covering `END`, any whitespace, and the optional
/// `IF`/`LOOP`/`CASE` sub-keyword). Returns `None` when there is no
/// `END` at `pos`.
fn end_keyword_len(body: &str, pos: usize) -> Option<usize> {
    if !keyword_at(body, pos, "END") {
        return None;
    }
    let bytes = body.as_bytes();
    // Skip `END` and any run of ASCII whitespace.
    let mut j = pos + 3;
    while j < bytes.len() && bytes[j].is_ascii_whitespace() {
        j += 1;
    }
    for sub in ["IF", "LOOP", "CASE"] {
        if keyword_at(body, j, sub) {
            return Some(j + sub.len() - pos);
        }
    }
    // Bare `END` (terminates a BEGIN…END block).
    Some(3)
}

/// Whole-word case-insensitive keyword match at byte `pos`.
///
/// `pos` is a **byte** index (the outer loop steps by 1 byte), so we
/// must stay in byte-land throughout. Using `s[pos..]` would panic on
/// any `pos` that falls inside a multi-byte UTF-8 code-point (e.g.
/// when the source contains a Greek or Cyrillic character). The slice
/// `s.as_bytes()[pos..pos+kw.len()]` never panics because it operates
/// on the raw byte array, and `eq_ignore_ascii_case` on `&[u8]` only
/// fires for ASCII letters — exactly the right semantics for SQL keywords.
fn keyword_at(s: &str, pos: usize, kw: &str) -> bool {
    let b = s.as_bytes();
    if pos + kw.len() > b.len() {
        return false;
    }
    // Use byte-level comparison to avoid a char-boundary panic when `pos`
    // is not on a char boundary (happens with multi-byte UTF-8 chars).
    if !b[pos..pos + kw.len()].eq_ignore_ascii_case(kw.as_bytes()) {
        return false;
    }
    let prev_ok = pos == 0 || !is_ident_byte(b[pos - 1]);
    let next = pos + kw.len();
    let next_ok = next >= b.len() || !is_ident_byte(b[next]);
    prev_ok && next_ok
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b == b'#'
}

fn classify_statement(raw: &str, file_id: FileId, start: usize, end: usize) -> AstStatement {
    let span = make_span(file_id, start as u32, end as u32);
    // Strip a leading line comment run.
    let text = raw.trim();
    let upper = text.to_ascii_uppercase();
    let u = upper.trim();

    if u.starts_with("NULL") {
        return AstStatement::Null { span };
    }
    if u.starts_with("RAISE") {
        let rest = text[5..].trim().trim_end_matches(';').trim();
        return AstStatement::Raise {
            exception: (!rest.is_empty()).then(|| rest.to_string()),
            span,
        };
    }
    if u.starts_with("RETURN") {
        let rest = text[6..].trim().trim_end_matches(';').trim();
        return AstStatement::Return {
            value_text: (!rest.is_empty()).then(|| rest.to_string()),
            span,
        };
    }
    if u.starts_with("EXECUTE IMMEDIATE") {
        let after = &text[17..];
        let sql_text = extract_first_quoted(after).unwrap_or_default();
        let has_using = after.to_ascii_uppercase().contains(" USING ");
        return AstStatement::ExecuteImmediate {
            sql_text,
            has_using,
            span,
        };
    }
    for verb in ["SELECT", "INSERT", "UPDATE", "DELETE", "MERGE"] {
        if u.starts_with(verb) {
            return AstStatement::Sql {
                verb: verb.to_string(),
                raw_text: text.trim().trim_end_matches(';').trim().to_string(),
                span,
            };
        }
    }
    if u.starts_with("IF ") {
        let then_pos = upper.find("THEN").unwrap_or(text.len());
        let cond = text[3..then_pos.min(text.len())].trim().to_string();
        return AstStatement::If {
            cond_text: cond,
            span,
        };
    }
    if u.starts_with("LOOP") || u.starts_with("FOR ") || u.starts_with("WHILE ") {
        let header_end = upper.find("LOOP").map_or(text.len(), |p| p + 4);
        return AstStatement::Loop {
            header_text: text[..header_end.min(text.len())].trim().to_string(),
            span,
        };
    }
    if let Some((lhs, rhs)) = text.split_once(":=") {
        return AstStatement::Assignment {
            target: lhs.trim().to_string(),
            rhs_text: rhs.trim().trim_end_matches(';').trim().to_string(),
            span,
        };
    }
    // `pkg.proc(args);` — a statement-level call.
    let head: String = text
        .chars()
        .take_while(|c| {
            c.is_ascii_alphanumeric() || *c == '_' || *c == '.' || *c == '$' || *c == '#'
        })
        .collect();
    if !head.is_empty()
        && text[head.len()..].trim_start().starts_with('(')
        && head.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
    {
        return AstStatement::Call { callee: head, span };
    }
    AstStatement::Unknown { span }
}

fn extract_first_quoted(s: &str) -> Option<String> {
    let mut it = s.char_indices();
    for (_, c) in it.by_ref() {
        if c == '\'' {
            let mut buf = String::new();
            for (_, nc) in it.by_ref() {
                if nc == '\'' {
                    return Some(buf);
                }
                buf.push(nc);
            }
            return Some(buf);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Expression lowering (PLSQL-PARSE-006)
// ---------------------------------------------------------------------------

/// Lower an expression source slice into the syntactic
/// [`AstExpr`]. Heuristic, precedence-aware top-level binary
/// split; the parser layer only recognises shape, the semantic
/// `plsql_ir::Expr` (IR-005) does narrowing. `Unknown` (R13)
/// for anything unclassifiable — never dropped.
#[must_use]
pub fn lower_expression_text(expr: &str, file_id: FileId, base_offset: usize) -> AstExpr {
    let text = expr.trim().trim_end_matches(';').trim();
    let span = make_span(
        file_id,
        base_offset as u32,
        (base_offset + expr.len()) as u32,
    );
    if text.is_empty() {
        return AstExpr::Literal {
            text: "NULL".to_string(),
            span,
        };
    }
    let upper = text.to_ascii_uppercase();

    // Literals.
    if matches!(upper.as_str(), "NULL" | "TRUE" | "FALSE")
        || text.starts_with('\'')
        || text.as_bytes()[0].is_ascii_digit()
    {
        return AstExpr::Literal {
            text: text.to_string(),
            span,
        };
    }
    // Bind / substitution.
    if let Some(rest) = text.strip_prefix("&&") {
        return AstExpr::Substitution {
            name: rest.to_string(),
            sticky: true,
            span,
        };
    }
    if let Some(rest) = text.strip_prefix('&') {
        return AstExpr::Substitution {
            name: rest.to_string(),
            sticky: false,
            span,
        };
    }
    if let Some(rest) = text.strip_prefix(':') {
        return AstExpr::Bind {
            name: rest.to_string(),
            span,
        };
    }
    // Top-level binary (lowest precedence first).
    let tiers: &[&[&str]] = &[
        &[" OR "],
        &[" AND "],
        &["="],
        &["<>", "!=", "<=", ">=", "<", ">"],
        &["||"],
        &["+", "-"],
        &["*", "/"],
    ];
    for tier in tiers {
        if let Some((l, op, r)) = split_top_level_bin(text, tier) {
            return AstExpr::Binary {
                op: op.trim().to_string(),
                lhs_text: l.trim().to_string(),
                rhs_text: r.trim().to_string(),
                span,
            };
        }
    }
    // Unary.
    if upper.starts_with("NOT ") {
        return AstExpr::Unary {
            op: "NOT".to_string(),
            operand_text: text[4..].trim().to_string(),
            span,
        };
    }
    // Call `name(args)`.
    if let Some(open) = text.find('(')
        && text.ends_with(')')
    {
        let callee = text[..open].trim();
        if !callee.is_empty()
            && callee
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "._$#".contains(c))
        {
            return AstExpr::Call {
                callee: callee.to_string(),
                args_text: text[open + 1..text.len() - 1].to_string(),
                span,
            };
        }
    }
    // Dotted name (allow `%TYPE`/`%ROWTYPE` attribute suffix).
    if text
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "._$#%:".contains(c))
        && text
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == ':')
    {
        return AstExpr::Name {
            path: text.to_string(),
            span,
        };
    }
    AstExpr::Unknown {
        text: text.to_string(),
        span,
    }
}

/// Find the leftmost top-level (paren/quote depth 0) operator
/// from `ops`. Alpha ops require word boundaries.
fn split_top_level_bin<'a, 'b>(
    text: &'a str,
    ops: &'b [&'b str],
) -> Option<(&'a str, &'b str, &'a str)> {
    let b = text.as_bytes();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut i = 0usize;
    while i < b.len() {
        let c = b[i];
        if c == b'\'' {
            in_str = !in_str;
            i += 1;
            continue;
        }
        if in_str {
            i += 1;
            continue;
        }
        if c == b'(' {
            depth += 1;
            i += 1;
            continue;
        }
        if c == b')' {
            depth -= 1;
            i += 1;
            continue;
        }
        if depth == 0 {
            for op in ops {
                let ob = op.as_bytes();
                // Use byte-level comparison to avoid a char-boundary panic
                // when `i` is not on a char boundary (multi-byte UTF-8 chars).
                // The slice `&text[..i]` / `&text[i + ob.len()..]` are safe
                // because we only split at byte positions that came from a
                // byte-walk, but the *comparison* `text[i..]` would panic —
                // use `b[i..]` for the eq test instead.
                if i + ob.len() <= b.len() && b[i..i + ob.len()].eq_ignore_ascii_case(ob) {
                    // The split points `i` and `i + ob.len()` are both
                    // char-boundary-safe only if the matched bytes are all
                    // ASCII (which they must be, since SQL operators are ASCII).
                    // Verify before slicing into `text`.
                    if text.is_char_boundary(i) && text.is_char_boundary(i + ob.len()) {
                        let l = &text[..i];
                        let r = &text[i + ob.len()..];
                        if !l.trim().is_empty() && !r.trim().is_empty() {
                            return Some((l, op, r));
                        }
                    }
                }
            }
        }
        i += 1;
    }
    None
}

// ---------------------------------------------------------------------------
// Type-declaration lowering (PLSQL-PARSE-007)
// ---------------------------------------------------------------------------

/// Lower a `CREATE TYPE` / collection / `TYPE … IS RECORD`
/// source slice into the syntactic [`AstTypeDecl`]. Attribute /
/// element / field text is kept raw for the bindgen layer to
/// resolve. `Unknown` (R13) for anything unclassifiable.
#[must_use]
pub fn lower_type_decl(decl: &str, file_id: FileId, base_offset: usize) -> AstTypeDecl {
    let text = decl.trim();
    let span = make_span(
        file_id,
        base_offset as u32,
        (base_offset + decl.len()) as u32,
    );
    let upper = text.to_ascii_uppercase();

    // PL/SQL record: `TYPE <name> IS RECORD ( … )`.
    if let Some(after_type) = strip_kw_prefix(&upper, text, "TYPE")
        && let Some(is_pos) = after_type.0.to_ascii_uppercase().find(" IS RECORD")
    {
        let name = after_type.0[..is_pos].trim().to_string();
        let fields = paren_body(&after_type.0[is_pos..]).unwrap_or_default();
        return AstTypeDecl::Record {
            name,
            fields_text: fields,
            span,
        };
    }

    // Object / collection: `CREATE [OR REPLACE] TYPE <name> AS …`.
    let name = extract_type_name(text, &upper);
    if upper.contains(" AS OBJECT") || upper.contains(" AS  OBJECT") {
        let attrs = paren_body(text).unwrap_or_default();
        return AstTypeDecl::Object {
            name,
            attributes_text: attrs,
            span,
        };
    }
    if let Some(of_pos) = upper.find(" TABLE OF ") {
        let elem = text[of_pos + 10..]
            .trim()
            .trim_end_matches(';')
            .trim()
            .to_string();
        return AstTypeDecl::Collection {
            name,
            element_text: elem,
            is_varray: false,
            span,
        };
    }
    if let Some(varray_pos) = upper.find("VARRAY") {
        let after = &text[varray_pos..];
        let elem = after
            .to_ascii_uppercase()
            .find(" OF ")
            .map(|p| {
                after[p + 4..]
                    .trim()
                    .trim_end_matches(';')
                    .trim()
                    .to_string()
            })
            .unwrap_or_default();
        return AstTypeDecl::Collection {
            name,
            element_text: elem,
            is_varray: true,
            span,
        };
    }
    AstTypeDecl::Unknown {
        text: text.to_string(),
        span,
    }
}

/// Strip a leading whole-word keyword; return the remainder of
/// the original (case-preserving) text after it.
fn strip_kw_prefix<'a>(upper: &str, text: &'a str, kw: &str) -> Option<(&'a str, ())> {
    let t = upper.trim_start();
    if t.starts_with(kw)
        && t.as_bytes()
            .get(kw.len())
            .is_some_and(|b| b.is_ascii_whitespace())
    {
        let lead = upper.len() - t.len();
        Some((text[lead + kw.len()..].trim_start(), ()))
    } else {
        None
    }
}

fn extract_type_name(text: &str, upper: &str) -> String {
    // After `TYPE`, before `AS` / `IS` / `(`.
    let Some(type_kw) = upper.find("TYPE") else {
        return String::new();
    };
    let rest = text[type_kw + 4..].trim_start();
    let rest_upper = rest.to_ascii_uppercase();
    let end = [" AS ", " IS ", "("]
        .iter()
        .filter_map(|m| rest_upper.find(m))
        .min()
        .unwrap_or(rest.len());
    rest[..end].trim().to_string()
}

/// Return the text inside the first balanced `( … )`.
fn paren_body(s: &str) -> Option<String> {
    let open = s.find('(')?;
    let mut depth = 0i32;
    for (idx, ch) in s[open..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(s[open + 1..open + idx].trim().to_string());
                }
            }
            _ => {}
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Scanning utilities
// ---------------------------------------------------------------------------

/// Skip whitespace characters. Returns number of bytes skipped.
fn skip_whitespace(bytes: &[u8], pos: usize) -> usize {
    let mut i = pos;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    i - pos
}

/// Skip whitespace and single-line/block comments. Returns bytes skipped.
fn skip_whitespace_and_comments(bytes: &[u8], pos: usize) -> usize {
    let mut i = pos;
    loop {
        let ws = skip_whitespace(bytes, i);
        i += ws;

        // Single-line comment: --
        if i + 1 < bytes.len() && bytes[i] == b'-' && bytes[i + 1] == b'-' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }

        // Block comment: /* ... */
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() {
                if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                    i += 2;
                    break;
                }
                i += 1;
            }
            continue;
        }

        break;
    }
    i - pos
}

/// Case-insensitive keyword match at a position.
fn matches_keyword_ignore_case(bytes: &[u8], pos: usize, keyword: &[u8]) -> bool {
    let end = pos + keyword.len();
    if end > bytes.len() {
        return false;
    }
    let candidate = &bytes[pos..end];
    // Trailing word boundary: the keyword must not be the prefix of a
    // longer identifier. Oracle identifiers continue with letters,
    // digits, `_`, `$`, or `#` — so `create_page_plug` must NOT match
    // the `CREATE` keyword (regression: APEX `wwv_flow*.create_*(…)`
    // call floods misread as CREATE DDL).
    let is_ident_cont = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'$' || c == b'#';
    if end < bytes.len() && is_ident_cont(bytes[end]) {
        return false;
    }
    // Leading word boundary: the byte before `pos` must not be an
    // identifier char either (e.g. `xcreate` / `re_create`).
    if pos > 0 && is_ident_cont(bytes[pos - 1]) {
        return false;
    }
    candidate.eq_ignore_ascii_case(keyword)
}

/// Scan forward to the next whitespace character.
fn scan_to_whitespace(bytes: &[u8], pos: usize) -> usize {
    let mut i = pos;
    while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

/// Extract an identifier from the source at the given byte offset.
///
/// Handles simple identifiers and quoted identifiers ("name").
fn extract_identifier(source: &str, pos: usize) -> String {
    let bytes = source.as_bytes();
    if pos >= bytes.len() {
        return String::new();
    }

    // Quoted identifier
    if bytes[pos] == b'"' {
        let start = pos + 1;
        let mut end = start;
        while end < bytes.len() {
            if bytes[end] == b'"' {
                // Check for escaped quote ""
                if end + 1 < bytes.len() && bytes[end + 1] == b'"' {
                    end += 2;
                } else {
                    break;
                }
            } else {
                end += 1;
            }
        }
        return source[start..end].to_string();
    }

    // Simple identifier
    let start = pos;
    let mut end = start;
    while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
        end += 1;
    }
    source[start..end].to_string()
}

/// Advance past the end of the current statement.
///
/// PL/SQL statements end at `;` (most statements) or `/` on its own line
/// (SQL*Plus terminator, e.g. after type bodies).
fn advance_to_decl_end(bytes: &[u8], start: usize) -> usize {
    let len = bytes.len();
    let mut i = start;
    let mut depth = 0; // track BEGIN...END nesting

    while i < len {
        // Skip comments and string/q-quote literals via the shared scanner
        // so an embedded `END`/`;` inside a literal (e.g. a dynamic-SQL or
        // dbms_output message) cannot truncate the declaration span.
        if let Some(next) = crate::recover::skip_opaque_span(bytes, i, start) {
            i = next;
            continue;
        }

        // Track BEGIN/END nesting for proper statement boundary detection
        if matches_keyword_ignore_case(bytes, i, b"BEGIN") {
            depth += 1;
            i += 5;
            continue;
        }
        if matches_keyword_ignore_case(bytes, i, b"END") {
            if depth > 0 {
                depth -= 1;
            }
            i += 3;
            continue;
        }

        // Statement terminator: ;
        if bytes[i] == b';' {
            if depth == 0 {
                return i + 1;
            }
            i += 1;
            continue;
        }

        // SQL*Plus / terminator (newline + / + newline or EOF)
        if bytes[i] == b'/' {
            // Check it's on its own line
            let is_sol = i == 0 || bytes[i - 1] == b'\n';
            let is_eol = i + 1 >= len || bytes[i + 1] == b'\n' || bytes[i + 1] == b'\r';
            if is_sol && is_eol && depth == 0 {
                return i + 1;
            }
        }

        i += 1;
    }

    len
}

/// Advance past the current position to avoid re-matching.
fn advance_past_statement_end(bytes: &[u8], pos: usize) -> usize {
    let end = advance_to_decl_end(bytes, pos);
    // Also skip whitespace after the statement end
    end + skip_whitespace_and_comments(bytes, end)
}

/// Create a span from byte offsets.
fn make_span(file_id: FileId, start: u32, end: u32) -> Span {
    Span::new(
        file_id,
        Position::new(1, start + 1, start),
        Position::new(1, end + 1, end),
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use plsql_core::FileId;

    fn fid() -> FileId {
        FileId::new(0)
    }

    fn lower_and_collect(source: &str) -> Vec<(String, u32, u32)> {
        let ast = lower_source(source, fid());
        ast.root
            .declarations
            .iter()
            .map(|d| {
                let (name, span) = match d {
                    AstDecl::PackageSpec { name, span } => (name.clone(), span),
                    AstDecl::PackageBody { name, span } => (name.clone(), span),
                    AstDecl::Procedure { name, span } => (name.clone(), span),
                    AstDecl::Function { name, span } => (name.clone(), span),
                    AstDecl::Trigger { name, span } => (name.clone(), span),
                    AstDecl::View { name, span } => (name.clone(), span),
                    AstDecl::TypeSpec { name, span } => (name.clone(), span),
                    AstDecl::TypeBody { name, span } => (name.clone(), span),
                    AstDecl::Ddl { kind, span, .. } => (kind.clone(), span),
                    AstDecl::Unknown { span, .. } => ("?".into(), span),
                };
                (name, span.start.offset, span.end.offset)
            })
            .collect()
    }

    #[test]
    fn empty_source_produces_empty_ast() {
        let ast = lower_source("", fid());
        assert!(ast.root.declarations.is_empty());
    }

    #[test]
    fn lower_package_spec() {
        let src = "CREATE OR REPLACE PACKAGE employee_mgmt\nAS\n  PROCEDURE hire(p_name VARCHAR2);\nEND employee_mgmt;\n";
        let decls = lower_and_collect(src);
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].0, "employee_mgmt");
        // Verify it's a PackageSpec
        let ast = lower_source(src, fid());
        assert!(matches!(
            ast.root.declarations[0],
            AstDecl::PackageSpec { .. }
        ));
    }

    #[test]
    fn lower_package_body() {
        let src = "CREATE OR REPLACE PACKAGE BODY employee_mgmt\nAS\n  PROCEDURE hire(p_name VARCHAR2) IS BEGIN NULL; END;\nEND employee_mgmt;\n";
        let decls = lower_and_collect(src);
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].0, "employee_mgmt");
        let ast = lower_source(src, fid());
        assert!(matches!(
            ast.root.declarations[0],
            AstDecl::PackageBody { .. }
        ));
    }

    #[test]
    fn lower_procedure() {
        let src = "CREATE PROCEDURE do_something\nIS\nBEGIN\n  NULL;\nEND;\n";
        let decls = lower_and_collect(src);
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].0, "do_something");
        let ast = lower_source(src, fid());
        assert!(matches!(
            ast.root.declarations[0],
            AstDecl::Procedure { .. }
        ));
    }

    #[test]
    fn lower_function() {
        let src = "CREATE OR REPLACE FUNCTION get_name(p_id NUMBER) RETURN VARCHAR2\nIS\nBEGIN\n  RETURN NULL;\nEND;\n";
        let decls = lower_and_collect(src);
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].0, "get_name");
        let ast = lower_source(src, fid());
        assert!(matches!(ast.root.declarations[0], AstDecl::Function { .. }));
    }

    #[test]
    fn lower_view() {
        let src = "CREATE OR REPLACE VIEW active_employees AS\nSELECT emp_id, emp_name FROM employees WHERE active = 'Y';\n";
        let decls = lower_and_collect(src);
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].0, "active_employees");
        let ast = lower_source(src, fid());
        assert!(matches!(ast.root.declarations[0], AstDecl::View { .. }));
    }

    #[test]
    fn lower_trigger() {
        let src = "CREATE OR REPLACE TRIGGER trg_audit\nBEFORE INSERT ON employees\nFOR EACH ROW\nBEGIN\n  :new.created := SYSDATE;\nEND;\n";
        let decls = lower_and_collect(src);
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].0, "trg_audit");
        let ast = lower_source(src, fid());
        assert!(matches!(ast.root.declarations[0], AstDecl::Trigger { .. }));
    }

    #[test]
    fn lower_type_spec() {
        let src = "CREATE OR REPLACE TYPE t_address AS OBJECT (\n  street VARCHAR2(200),\n  city VARCHAR2(100)\n);\n";
        let decls = lower_and_collect(src);
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].0, "t_address");
        let ast = lower_source(src, fid());
        assert!(matches!(ast.root.declarations[0], AstDecl::TypeSpec { .. }));
    }

    #[test]
    fn lower_type_body() {
        let src = "CREATE OR REPLACE TYPE BODY t_address AS\n  MEMBER FUNCTION full RETURN VARCHAR2 IS BEGIN RETURN 'x'; END;\nEND;\n/\n";
        let decls = lower_and_collect(src);
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].0, "t_address");
        let ast = lower_source(src, fid());
        assert!(matches!(ast.root.declarations[0], AstDecl::TypeBody { .. }));
    }

    #[test]
    fn lower_multiple_declarations() {
        let src = "\
CREATE OR REPLACE PACKAGE pkg_a AS
  PROCEDURE p;
END pkg_a;

CREATE OR REPLACE FUNCTION f1 RETURN NUMBER IS BEGIN RETURN 1; END;

CREATE VIEW v1 AS SELECT 1 FROM dual;
";
        let decls = lower_and_collect(src);
        assert_eq!(decls.len(), 3);
        assert_eq!(decls[0].0, "pkg_a");
        assert_eq!(decls[1].0, "f1");
        assert_eq!(decls[2].0, "v1");
    }

    #[test]
    fn lower_quoted_identifier() {
        let src = "CREATE OR REPLACE PACKAGE \"My_Package\" AS\nEND \"My_Package\";\n";
        let decls = lower_and_collect(src);
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].0, "My_Package");
    }

    #[test]
    fn lower_ddl_kind_recorded() {
        let src = "CREATE SEQUENCE seq_emp START WITH 1;\n";
        let ast = lower_source(src, fid());
        assert_eq!(ast.root.declarations.len(), 1);
        assert!(matches!(
            ast.root.declarations[0],
            AstDecl::Ddl { kind: ref k, .. } if k == "SEQUENCE"
        ));
    }

    #[test]
    fn case_insensitive_matching() {
        let src = "create or replace procedure do_it\nis begin null; end;\n";
        let decls = lower_and_collect(src);
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].0, "do_it");
    }

    #[test]
    fn comments_skipped() {
        let src =
            "-- This is a comment\n/* Block comment */\nCREATE PROCEDURE p IS BEGIN NULL; END;\n";
        let decls = lower_and_collect(src);
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].0, "p");
    }

    #[test]
    fn span_offsets_are_correct() {
        let src = "CREATE PROCEDURE hello IS BEGIN NULL; END;";
        let decls = lower_and_collect(src);
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].1, 0); // starts at byte 0
        assert_eq!(decls[0].2, src.len() as u32); // ends at full length
    }

    #[test]
    fn synthetic_corpus_pkg_employee_mgmt() {
        let spec = include_str!("../../../../corpus/synthetic/l1/pkg_employee_mgmt.pks");
        let body = include_str!("../../../../corpus/synthetic/l1/pkg_employee_mgmt.pkb");

        let spec_ast = lower_source(spec, fid());
        assert_eq!(spec_ast.root.declarations.len(), 1);
        assert!(matches!(
            spec_ast.root.declarations[0],
            AstDecl::PackageSpec { ref name, .. } if name == "employee_mgmt"
        ));

        let body_ast = lower_source(body, fid());
        assert_eq!(body_ast.root.declarations.len(), 1);
        assert!(matches!(
            body_ast.root.declarations[0],
            AstDecl::PackageBody { ref name, .. } if name == "employee_mgmt"
        ));
    }

    #[test]
    fn synthetic_corpus_all_packages_parse() {
        // Verify all 10 synthetic packages produce valid ASTs
        let files = [
            ("pkg_employee_mgmt", true),
            ("pkg_cursor_demo", true),
            ("pkg_bulk_ops", true),
            ("pkg_error_handling", true),
            ("pkg_collections", true),
            ("pkg_dynamic_sql", true),
            ("pkg_overload", true),
            ("pkg_type_demo", false), // TYPE, not PACKAGE
            ("pkg_security", true),
            ("pkg_conditional", true),
        ];

        for (name, _is_package) in &files {
            let spec_path = format!("../../corpus/synthetic/l1/{name}.pks");
            let spec = std::fs::read_to_string(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(&spec_path),
            )
            .unwrap_or_else(|e| panic!("Failed to read {spec_path}: {e}"));

            let ast = lower_source(&spec, fid());
            assert!(
                !ast.root.declarations.is_empty(),
                "No declarations found in {name}.pks"
            );
        }
    }

    #[test]
    fn synthetic_corpus_views_parse() {
        let views = [
            "vw_active_employees",
            "vw_dept_summary",
            "vw_high_earners",
            "vw_audit_report",
            "vw_unresolved_deps",
        ];

        for name in &views {
            let path = format!("../../corpus/synthetic/l1/{name}.sql");
            let source = std::fs::read_to_string(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(&path),
            )
            .unwrap_or_else(|e| panic!("Failed to read {path}: {e}"));

            let ast = lower_source(&source, fid());
            assert_eq!(ast.root.declarations.len(), 1, "Expected 1 decl in {name}");
            assert!(
                matches!(ast.root.declarations[0], AstDecl::View { .. }),
                "Expected View for {name}, got {:?}",
                ast.root.declarations[0]
            );
        }
    }

    // -----------------------------------------------------------------
    // PLSQL-PARSE-008 — ALTER / DROP / GRANT / REVOKE / COMMENT
    // -----------------------------------------------------------------

    #[test]
    fn alter_table_is_ddl() {
        let src = "ALTER TABLE employees ADD (start_date DATE);";
        let ast = lower_source(src, fid());
        assert_eq!(ast.root.declarations.len(), 1);
        match &ast.root.declarations[0] {
            AstDecl::Ddl { kind, .. } => assert_eq!(kind, "ALTER TABLE"),
            other => panic!("expected Ddl, got {other:?}"),
        }
    }

    #[test]
    fn drop_index_is_ddl() {
        let src = "DROP INDEX ix_emp_dept;";
        let ast = lower_source(src, fid());
        assert_eq!(ast.root.declarations.len(), 1);
        match &ast.root.declarations[0] {
            AstDecl::Ddl { kind, .. } => assert_eq!(kind, "DROP INDEX"),
            other => panic!("expected Ddl, got {other:?}"),
        }
    }

    #[test]
    fn grant_select_is_ddl() {
        let src = "GRANT SELECT ON employees TO reader;";
        let ast = lower_source(src, fid());
        assert_eq!(ast.root.declarations.len(), 1);
        match &ast.root.declarations[0] {
            AstDecl::Ddl { kind, .. } => assert_eq!(kind, "GRANT SELECT"),
            other => panic!("expected Ddl, got {other:?}"),
        }
    }

    #[test]
    fn revoke_select_is_ddl() {
        let src = "REVOKE SELECT ON employees FROM reader;";
        let ast = lower_source(src, fid());
        assert_eq!(ast.root.declarations.len(), 1);
        match &ast.root.declarations[0] {
            AstDecl::Ddl { kind, .. } => assert_eq!(kind, "REVOKE SELECT"),
            other => panic!("expected Ddl, got {other:?}"),
        }
    }

    #[test]
    fn comment_on_table_is_ddl() {
        let src = "COMMENT ON TABLE employees IS 'Employee roster';";
        let ast = lower_source(src, fid());
        assert_eq!(ast.root.declarations.len(), 1);
        match &ast.root.declarations[0] {
            AstDecl::Ddl { kind, .. } => assert_eq!(kind, "COMMENT ON"),
            other => panic!("expected Ddl, got {other:?}"),
        }
    }

    #[test]
    fn mixed_ddl_and_create_lowers_all() {
        let src = "
            CREATE TABLE employees (id NUMBER);
            ALTER TABLE employees ADD (name VARCHAR2(50));
            CREATE OR REPLACE PROCEDURE p IS BEGIN NULL; END;
            DROP INDEX ix_old;
            GRANT SELECT ON employees TO reader;
        ";
        let ast = lower_source(src, fid());
        let kinds: Vec<String> = ast
            .root
            .declarations
            .iter()
            .map(|d| match d {
                AstDecl::Ddl { kind, .. } => format!("Ddl:{kind}"),
                AstDecl::Procedure { name, .. } => format!("Procedure:{name}"),
                AstDecl::PackageSpec { .. } => "PackageSpec".into(),
                AstDecl::Unknown { .. } => "Unknown".into(),
                other => format!("{other:?}"),
            })
            .collect();
        // Must surface: CREATE TABLE (as a Ddl), ALTER TABLE, Procedure p, DROP INDEX, GRANT SELECT.
        assert!(kinds.iter().any(|k| k == "Ddl:TABLE"), "got {kinds:?}");
        assert!(
            kinds.contains(&"Ddl:ALTER TABLE".to_string()),
            "got {kinds:?}"
        );
        assert!(kinds.contains(&"Procedure:p".to_string()), "got {kinds:?}");
        assert!(
            kinds.contains(&"Ddl:DROP INDEX".to_string()),
            "got {kinds:?}"
        );
        assert!(
            kinds.contains(&"Ddl:GRANT SELECT".to_string()),
            "got {kinds:?}"
        );
    }

    #[test]
    fn ddl_keyword_inside_string_is_not_promoted() {
        // ALTER appears inside a string literal of a CREATE PROCEDURE body —
        // the scanner currently treats top-level keyword position as start
        // of a declaration only when at statement scope. This test pins the
        // current behaviour so future stricter parsing is detectable.
        let src = "CREATE PROCEDURE p IS BEGIN dbms_output.put_line('not really ALTER'); END;";
        let ast = lower_source(src, fid());
        // We accept that the scanner is a pre-parser; it may over-classify.
        // The test guards against a regression where we'd somehow miss the
        // procedure entirely.
        assert!(
            ast.root
                .declarations
                .iter()
                .any(|d| matches!(d, AstDecl::Procedure { .. })),
            "procedure missing from {:?}",
            ast.root.declarations
        );
    }

    #[test]
    fn end_and_semicolon_inside_string_do_not_truncate_decl_span() {
        // oracle-qm3q.12: a string literal embedding `END;` (common in
        // dynamic-SQL / dbms_output builders) must NOT terminate the
        // declaration early. Before the fix, `advance_to_decl_end` saw the
        // in-string `END` (depth 1 -> 0) and the in-string `;` and recorded
        // a span truncated mid-literal at offset 64; the real trailing `END;`
        // ends at offset 78 (== src.len()).
        let src = "CREATE PROCEDURE p IS BEGIN dbms_output.put_line('msg with END; inside'); END;";
        assert_eq!(src.len(), 78);
        let ast = lower_source(src, fid());
        let proc_span = ast
            .root
            .declarations
            .iter()
            .find_map(|d| match d {
                AstDecl::Procedure { name, span } if name == "p" => Some(*span),
                _ => None,
            })
            .unwrap_or_else(|| panic!("procedure p missing from {:?}", ast.root.declarations));
        assert_eq!(
            proc_span.end.offset, 78,
            "procedure span must run to the real trailing END; (offset 78), not the \
             in-string END;/; — got {}",
            proc_span.end.offset
        );
        // And there is exactly one declaration: the in-string text was not
        // mis-promoted into a spurious second decl.
        assert_eq!(ast.root.declarations.len(), 1, "{:?}", ast.root.declarations);
    }

    #[test]
    fn semicolon_inside_string_does_not_split_statement_body() {
        // oracle-qm3q.12: the `;` inside 'a ; b' must not split the call into
        // extra statements. Before the fix this produced 3 statements with a
        // truncated Call; after the fix it is 2 statements.
        let s = stmts("dbms_output.put_line('a ; b'); v := 1;");
        assert_eq!(s.len(), 2, "got {s:?}");
        assert!(
            matches!(&s[0], AstStatement::Call { .. } | AstStatement::Unknown { .. }),
            "first statement should be the whole put_line call, got {:?}",
            s[0]
        );
        match &s[1] {
            AstStatement::Assignment { target, .. } => assert_eq!(target, "v"),
            other => panic!("expected assignment, got {other:?}"),
        }
    }

    #[test]
    fn block_keyword_inside_string_does_not_skew_body_depth() {
        // oracle-qm3q.12: an `END IF;` embedded in a string literal inside a
        // real IF…END IF block must NOT prematurely close the block. Before
        // the fix, the in-string `END IF` decremented depth to 0 and the
        // in-string `;` split the statement, fragmenting the IF body into
        // multiple chunks. After the fix, the literal is skipped, the block
        // stays one chunk, and the trailing assignment is its own statement.
        let s = stmts("IF x THEN msg := 'END IF; oops'; END IF; v := 1;");
        assert_eq!(s.len(), 2, "IF body must stay one chunk; got {s:?}");
        assert!(
            matches!(&s[0], AstStatement::If { .. }),
            "first statement should be the whole IF block, got {:?}",
            s[0]
        );
        match &s[1] {
            AstStatement::Assignment { target, .. } => assert_eq!(target, "v"),
            other => panic!("expected trailing assignment, got {other:?}"),
        }
    }

    #[test]
    fn synthetic_corpus_triggers_parse() {
        let triggers = ["trg_employees_audit", "trg_check_salary"];

        for name in &triggers {
            let path = format!("../../corpus/synthetic/l1/{name}.sql");
            let source = std::fs::read_to_string(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(&path),
            )
            .unwrap_or_else(|e| panic!("Failed to read {path}: {e}"));

            let ast = lower_source(&source, fid());
            assert_eq!(ast.root.declarations.len(), 1, "Expected 1 decl in {name}");
            assert!(
                matches!(ast.root.declarations[0], AstDecl::Trigger { .. }),
                "Expected Trigger for {name}, got {:?}",
                ast.root.declarations[0]
            );
        }
    }

    // -- PLSQL-PARSE-005: statement-body lowering --

    fn stmts(body: &str) -> Vec<AstStatement> {
        lower_statement_body(body, fid(), 0)
    }

    #[test]
    fn parse005_null_statement() {
        let s = stmts("NULL;");
        assert_eq!(s.len(), 1);
        assert!(matches!(s[0], AstStatement::Null { .. }));
    }

    #[test]
    fn parse005_assignment_captures_target_rhs() {
        let s = stmts("v_x := 42;");
        match &s[0] {
            AstStatement::Assignment {
                target, rhs_text, ..
            } => {
                assert_eq!(target, "v_x");
                assert_eq!(rhs_text, "42");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn parse005_execute_immediate_with_using() {
        let s = stmts("EXECUTE IMMEDIATE 'UPDATE t SET a = :1' USING v_a;");
        match &s[0] {
            AstStatement::ExecuteImmediate {
                sql_text,
                has_using,
                ..
            } => {
                assert_eq!(sql_text, "UPDATE t SET a = :1");
                assert!(*has_using);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn parse005_raise_and_return() {
        let s = stmts("RAISE no_data_found; RETURN v_sum;");
        assert_eq!(s.len(), 2);
        assert!(matches!(&s[0], AstStatement::Raise { exception, .. }
            if exception.as_deref() == Some("no_data_found")));
        assert!(matches!(&s[1], AstStatement::Return { value_text, .. }
            if value_text.as_deref() == Some("v_sum")));
    }

    #[test]
    fn parse005_sql_verbs_classified() {
        for (src, _v) in [
            ("SELECT 1 INTO x FROM dual;", "SELECT"),
            ("INSERT INTO t VALUES (1);", "INSERT"),
            ("UPDATE t SET a = 1;", "UPDATE"),
            ("DELETE FROM t;", "DELETE"),
        ] {
            let s = stmts(src);
            assert!(matches!(s[0], AstStatement::Sql { .. }), "{src}");
        }
    }

    #[test]
    fn parse005_nested_block_does_not_split_on_inner_semicolons() {
        let s = stmts("BEGIN INSERT INTO x VALUES (1); UPDATE x SET a = 2; END;");
        // Whole nested block is one chunk → not split into 2 SQL
        // statements; classified by its leading token.
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn parse005_statement_level_call() {
        let s = stmts("billing_pkg.post_invoice(p_id, p_amt);");
        match &s[0] {
            AstStatement::Call { callee, .. } => {
                assert_eq!(callee, "billing_pkg.post_invoice");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn parse005_if_captures_condition() {
        let s = stmts("IF v_x > 0 THEN NULL; END IF;");
        assert!(matches!(&s[0], AstStatement::If { cond_text, .. }
            if cond_text == "v_x > 0"));
    }

    // oracle-hbhm: the body-splitter must depth-track IF…END IF so a
    // multi-statement IF body with internal `;` is NOT torn into
    // separate top-level statements (which silently lost the IF guard
    // on the leaked DML).
    #[test]
    fn parse005_if_body_not_split_on_inner_semicolons() {
        let s = stmts(
            "IF p_flag = 1 THEN \
             INSERT INTO audit_log VALUES (1); \
             UPDATE accounts SET bal = 0; \
             END IF;",
        );
        assert_eq!(s.len(), 1, "IF body must stay one statement: {s:?}");
        assert!(matches!(&s[0], AstStatement::If { cond_text, .. }
            if cond_text == "p_flag = 1"));
    }

    // oracle-hbhm: the body-splitter must depth-track LOOP…END LOOP
    // so a multi-statement loop body is not torn apart.
    #[test]
    fn parse005_loop_body_not_split_on_inner_semicolons() {
        let s = stmts(
            "FOR r IN 1..10 LOOP \
             INSERT INTO dst VALUES (r); \
             DELETE FROM stale WHERE id = r; \
             END LOOP;",
        );
        assert_eq!(s.len(), 1, "LOOP body must stay one statement: {s:?}");
        assert!(matches!(s[0], AstStatement::Loop { .. }));
    }

    // oracle-hbhm: a nested IF inside a LOOP — both opener families
    // tracked together — stays one statement.
    #[test]
    fn parse005_nested_if_inside_loop_not_split() {
        let s = stmts(
            "FOR i IN 1..3 LOOP \
             IF i > 1 THEN do_a(i); ELSE do_b(i); END IF; \
             log_iter(i); \
             END LOOP;",
        );
        assert_eq!(s.len(), 1, "nested IF/LOOP must stay one statement: {s:?}");
        assert!(matches!(s[0], AstStatement::Loop { .. }));
    }

    #[test]
    fn parse005_unrecognised_is_unknown_not_dropped() {
        let s = stmts("@@@garbage;");
        assert_eq!(s.len(), 1);
        assert!(matches!(s[0], AstStatement::Unknown { .. }));
    }

    #[test]
    fn parse005_spans_offset_by_base() {
        use plsql_parser::Spanned;
        let s = lower_statement_body("NULL;", fid(), 100);
        let span = s[0].span();
        assert!(span.start.offset >= 100);
    }

    // -- PLSQL-PARSE-006: expression lowering --

    fn ex(s: &str) -> AstExpr {
        lower_expression_text(s, fid(), 0)
    }

    #[test]
    fn parse006_literals() {
        assert!(matches!(ex("42"), AstExpr::Literal { .. }));
        assert!(matches!(ex("'hi'"), AstExpr::Literal { .. }));
        assert!(matches!(ex("NULL"), AstExpr::Literal { .. }));
        assert!(matches!(ex("TRUE"), AstExpr::Literal { .. }));
    }

    #[test]
    fn parse006_bind_and_substitution() {
        assert!(matches!(ex(":1"), AstExpr::Bind { .. }));
        assert!(matches!(ex(":emp_id"), AstExpr::Bind { .. }));
        assert!(matches!(
            ex("&v"),
            AstExpr::Substitution { sticky: false, .. }
        ));
        assert!(matches!(
            ex("&&v"),
            AstExpr::Substitution { sticky: true, .. }
        ));
    }

    #[test]
    fn parse006_dotted_name_and_attribute() {
        assert!(matches!(ex("hr.employees.emp_id"), AstExpr::Name { .. }));
        match ex("v_sal%TYPE") {
            AstExpr::Name { path, .. } => assert_eq!(path, "v_sal%TYPE"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn parse006_function_call() {
        match ex("nvl(v_x, 0)") {
            AstExpr::Call {
                callee, args_text, ..
            } => {
                assert_eq!(callee, "nvl");
                assert_eq!(args_text, "v_x, 0");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn parse006_binary_precedence_or_lowest() {
        match ex("a AND b OR c") {
            AstExpr::Binary { op, .. } => assert_eq!(op, "OR"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn parse006_concat_is_binary() {
        match ex("first || ' ' || last") {
            AstExpr::Binary { op, .. } => assert_eq!(op, "||"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn parse006_paren_protects_inner_op() {
        match ex("(a OR b) AND c") {
            AstExpr::Binary { op, .. } => assert_eq!(op, "AND"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn parse006_unary_not() {
        match ex("NOT v_flag") {
            AstExpr::Unary {
                op, operand_text, ..
            } => {
                assert_eq!(op, "NOT");
                assert_eq!(operand_text, "v_flag");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn parse006_string_with_operator_inside_stays_literal() {
        assert!(matches!(ex("'a + b'"), AstExpr::Literal { .. }));
    }

    #[test]
    fn parse006_garbage_is_unknown() {
        assert!(matches!(ex("@@@"), AstExpr::Unknown { .. }));
    }

    #[test]
    fn parse006_empty_is_null_literal() {
        match ex("  ; ") {
            AstExpr::Literal { text, .. } => assert_eq!(text, "NULL"),
            other => panic!("{other:?}"),
        }
    }

    // -- PLSQL-PARSE-007: type declaration lowering --

    fn ty(s: &str) -> AstTypeDecl {
        lower_type_decl(s, fid(), 0)
    }

    #[test]
    fn parse007_object_type() {
        match ty("CREATE OR REPLACE TYPE employee_t AS OBJECT (id NUMBER, name VARCHAR2(100))") {
            AstTypeDecl::Object {
                name,
                attributes_text,
                ..
            } => {
                assert_eq!(name, "employee_t");
                assert!(attributes_text.contains("id NUMBER"));
                assert!(attributes_text.contains("name VARCHAR2(100)"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn parse007_nested_table_collection() {
        match ty("CREATE TYPE id_list AS TABLE OF NUMBER") {
            AstTypeDecl::Collection {
                name,
                element_text,
                is_varray,
                ..
            } => {
                assert_eq!(name, "id_list");
                assert_eq!(element_text, "NUMBER");
                assert!(!is_varray);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn parse007_varray_collection() {
        match ty("CREATE TYPE phone_arr AS VARRAY(5) OF VARCHAR2(20)") {
            AstTypeDecl::Collection {
                name,
                element_text,
                is_varray,
                ..
            } => {
                assert_eq!(name, "phone_arr");
                assert_eq!(element_text, "VARCHAR2(20)");
                assert!(is_varray);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn parse007_plsql_record() {
        match ty("TYPE emp_rec IS RECORD (id NUMBER, sal NUMBER(12,2))") {
            AstTypeDecl::Record {
                name, fields_text, ..
            } => {
                assert_eq!(name, "emp_rec");
                assert!(fields_text.contains("id NUMBER"));
                assert!(fields_text.contains("sal NUMBER(12,2)"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn parse007_unrecognised_is_unknown() {
        assert!(matches!(
            ty("CREATE SEQUENCE s START WITH 1"),
            AstTypeDecl::Unknown { .. }
        ));
    }

    #[test]
    fn parse007_object_with_no_attributes() {
        match ty("CREATE TYPE marker_t AS OBJECT ()") {
            AstTypeDecl::Object {
                name,
                attributes_text,
                ..
            } => {
                assert_eq!(name, "marker_t");
                assert!(attributes_text.is_empty());
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn parse007_span_offset_applied() {
        use plsql_parser::Spanned;
        let t = lower_type_decl("CREATE TYPE x AS TABLE OF NUMBER", fid(), 50);
        assert!(t.span().start.offset >= 50);
    }
}
