//! Build script for `plsql-parser-antlr`.
//!
//! When `PLSQL_ANTLR_REGEN=1` is set, this script:
//!
//! 1. Locates the vendored `antlr4-4.13.3-SNAPSHOT-complete.jar` codegen tool, or the
//!    `PLSQL_ANTLR_TOOL_JAR` override when set. Relative overrides resolve
//!    from the workspace root when it can be found.
//! 2. Runs `java -jar <jar> -Dlanguage=Rust` on `PlSqlLexer.g4` and
//!    `PlSqlParser.g4`
//! 3. Applies post-processing fixes for known antlr4rust blockers:
//!    - Replaces `fn` keyword collisions with `r#fn`
//!    - Replaces Java-style `this.` with Rust-style `recog.` in embedded
//!      actions
//! 4. Writes the generated Rust source into `src/generated/`, or into
//!    `PLSQL_ANTLR_REGEN_DIR` when that environment variable is set. Relative
//!    override directories resolve from the workspace root when it can be found.
//!
//! Normal builds never require Java. They compile the committed generated
//! source under `src/generated/`.

use std::env;
use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

type BuildResult<T> = Result<T, Box<dyn Error>>;

fn main() -> BuildResult<()> {
    println!("cargo:rerun-if-env-changed=PLSQL_ANTLR_REGEN");
    println!("cargo:rerun-if-env-changed=PLSQL_ANTLR_REGEN_DIR");
    println!("cargo:rerun-if-env-changed=PLSQL_ANTLR_TOOL_JAR");

    // Only validate or regenerate generated code when the feature is enabled.
    if env::var("CARGO_FEATURE_ANTLR_CODEGEN").is_err() {
        println!("cargo:warning=antlr-codegen feature not enabled, skipping ANTLR codegen");
        return Ok(());
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").map_err(|e| {
        build_error(format!(
            "CARGO_MANIFEST_DIR must be set by Cargo for ANTLR codegen: {e}"
        ))
    })?);
    let workspace_root = find_workspace_root(&manifest_dir);
    let committed_generated_dir = manifest_dir.join("src/generated");

    let grammar_dir = manifest_dir.join("grammars");
    let jar_path = env::var_os("PLSQL_ANTLR_TOOL_JAR")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest_dir.join("tools/antlr4-4.13.3-SNAPSHOT-complete.jar"));
    let jar_path = if jar_path.is_absolute() {
        jar_path
    } else {
        workspace_root.join(jar_path)
    };
    let lexer_grammar = grammar_dir.join("PlSqlLexer.g4");
    let parser_grammar = grammar_dir.join("PlSqlParser.g4");

    // Re-run if inputs or committed generated files change.
    println!("cargo:rerun-if-changed={}", lexer_grammar.display());
    println!("cargo:rerun-if-changed={}", parser_grammar.display());
    println!("cargo:rerun-if-changed={}", jar_path.display());
    for name in generated_file_names() {
        println!(
            "cargo:rerun-if-changed={}",
            committed_generated_dir.join(name).display()
        );
    }

    if !regen_requested() {
        verify_generated_files(&committed_generated_dir)?;
        println!(
            "cargo:warning=Using committed ANTLR generated source from {}",
            committed_generated_dir.display()
        );
        return Ok(());
    }

    // Verify generator inputs exist only for explicit regeneration.
    require_existing_path(&jar_path, "antlr4-4.13.3-SNAPSHOT-complete.jar")?;
    require_existing_path(&lexer_grammar, "PlSqlLexer.g4")?;
    require_existing_path(&parser_grammar, "PlSqlParser.g4")?;

    let out_dir = env::var_os("PLSQL_ANTLR_REGEN_DIR")
        .map(PathBuf::from)
        .unwrap_or(committed_generated_dir);
    let out_dir = if out_dir.is_absolute() {
        out_dir
    } else {
        workspace_root.join(out_dir)
    };
    fs::create_dir_all(&out_dir).map_err(|e| {
        build_error(format!(
            "failed to create generated output dir {}: {e}",
            out_dir.display()
        ))
    })?;

    // --- Generate lexer ---
    println!("cargo:warning=Generating Rust lexer from PlSqlLexer.g4...");
    run_antlr(&jar_path, &grammar_dir, "PlSqlLexer.g4", &out_dir, false)?;

    // --- Generate parser + listener ---
    // NOTE: The parser grammar has known non-fatal errors (fn keyword collision).
    // antlr4rust still generates valid output despite reporting these errors.
    // We pass `allow_errors=true` to tolerate the non-zero exit code.
    println!("cargo:warning=Generating Rust parser from PlSqlParser.g4...");
    run_antlr_with_listener(&jar_path, &grammar_dir, "PlSqlParser.g4", &out_dir, true)?;

    // Verify output files were generated.
    for name in generated_file_names() {
        let path = out_dir.join(name);
        require_existing_path(&path, &format!("generated file {name}"))?;
        let size = fs::metadata(&path)
            .map_err(|e| build_error(format!("failed to stat {}: {e}", path.display())))?
            .len();
        println!("cargo:warning=Generated {name}: {size} bytes");
    }

    // --- Post-process generated code ---
    post_process(&out_dir.join("plsqllexer.rs"), "lexer")?;
    post_process(&out_dir.join("plsqlparser.rs"), "parser")?;
    post_process(&out_dir.join("plsqlparserlistener.rs"), "listener")?;
    post_process(&out_dir.join("plsqlparserbaselistener.rs"), "base_listener")?;

    println!(
        "cargo:warning=ANTLR codegen complete. Output in {}",
        out_dir.display()
    );
    Ok(())
}

fn build_error(message: impl Into<String>) -> Box<dyn Error> {
    Box::new(io::Error::other(message.into()))
}

fn require_existing_path(path: &Path, description: &str) -> BuildResult<()> {
    if path.exists() {
        return Ok(());
    }

    Err(build_error(format!(
        "{description} not found at {}",
        path.display()
    )))
}

fn find_workspace_root(manifest_dir: &Path) -> PathBuf {
    let mut current = Some(manifest_dir);
    while let Some(dir) = current {
        if dir.join("Cargo.lock").exists() && dir.join("Cargo.toml").exists() {
            return dir.to_path_buf();
        }
        current = dir.parent();
    }
    manifest_dir.to_path_buf()
}

fn regen_requested() -> bool {
    env::var("PLSQL_ANTLR_REGEN")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn generated_file_names() -> [&'static str; 4] {
    [
        "plsqllexer.rs",
        "plsqlparser.rs",
        "plsqlparserlistener.rs",
        "plsqlparserbaselistener.rs",
    ]
}

fn verify_generated_files(dir: &Path) -> BuildResult<()> {
    for name in generated_file_names() {
        let path = dir.join(name);
        if !path.exists() {
            return Err(build_error(format!(
                "committed ANTLR generated file missing at {}. \
                 Restore it from git, or run `PLSQL_ANTLR_REGEN=1 cargo build -p plsql-parser-antlr --features antlr-codegen` to regenerate it.",
                path.display()
            )));
        }
    }
    Ok(())
}

/// Run ANTLR codegen for a lexer grammar.
fn run_antlr(
    jar: &Path,
    grammar_dir: &Path,
    grammar_name: &str,
    out_dir: &Path,
    allow_errors: bool,
) -> BuildResult<()> {
    let output = Command::new("java")
        .current_dir(grammar_dir)
        .args([
            "-jar",
            &jar.to_string_lossy(),
            "-Dlanguage=Rust",
            "-o",
            &out_dir.to_string_lossy(),
            "-no-listener",
            "-no-visitor",
            grammar_name,
        ])
        .output()
        .map_err(|e| build_error(format!("failed to run java; is Java 11+ on PATH? {e}")))?;

    // Print stderr warnings/errors from ANTLR.
    let stderr = String::from_utf8_lossy(&output.stderr);
    for line in stderr.lines() {
        println!("cargo:warning=  antlr: {line}");
    }

    if !output.status.success() && !allow_errors {
        return Err(build_error(format!(
            "ANTLR codegen failed for {}",
            grammar_dir.join(grammar_name).display()
        )));
    }

    Ok(())
}

/// Run ANTLR codegen for a parser grammar (with listener).
fn run_antlr_with_listener(
    jar: &Path,
    grammar_dir: &Path,
    grammar_name: &str,
    out_dir: &Path,
    allow_errors: bool,
) -> BuildResult<()> {
    let output = Command::new("java")
        .current_dir(grammar_dir)
        .args([
            "-jar",
            &jar.to_string_lossy(),
            "-Dlanguage=Rust",
            "-o",
            &out_dir.to_string_lossy(),
            "-lib",
            &out_dir.to_string_lossy(),
            "-listener",
            "-no-visitor",
            grammar_name,
        ])
        .output()
        .map_err(|e| build_error(format!("failed to run java; is Java 11+ on PATH? {e}")))?;

    // Print stderr warnings/errors from ANTLR.
    let stderr = String::from_utf8_lossy(&output.stderr);
    for line in stderr.lines() {
        println!("cargo:warning=  antlr: {line}");
    }

    if !output.status.success() && !allow_errors {
        return Err(build_error(format!(
            "ANTLR codegen failed for {}",
            grammar_dir.join(grammar_name).display()
        )));
    }

    Ok(())
}

/// Apply post-processing fixes for known antlr4rust blockers.
///
/// These are mechanical transformations that fix issues in the generated
/// code caused by the grammar using Java-specific conventions or antlr4rust
/// codegen quirks:
///
/// 1. `fn` keyword collision — grammar labels named `fn` conflict with
///    Rust's `fn` keyword. Replace with `r#fn` (raw identifier).
///
/// 2. `this.` references — embedded actions use Java's `this.MethodName()`
///    instead of antlr4rust's `recog.MethodName()`.
///
/// 3. Inner `#![allow(...)]` attributes — generated files emit these at the
///    top, but they are included via `include!()` inside `mod` blocks, where
///    inner attributes on the crate root are invalid.  Convert to outer `#[allow(...)]`.
///
/// 4. Doubled `Parser` in `PlSqlParserParserContext` — a codegen template
///    bug for labeled-alternative rules.  Replace with the correct name.
///
/// 5. Missing user-defined semantic predicate methods — the grammar embeds
///    predicates that call `recog.isVersion12()`, `recog.isVersion11()`,
///    `recog.isVersion10()`, `recog.IsNotNumericFunction()` (parser) and
///    `recog.IsNewlineAtPos()` (lexer).  Inject extension traits providing
///    permissive defaults.
///
/// 6. Absolute-path generated headers — ANTLR writes the local checkout path
///    into the first comment. Normalize it so CI and developer workstations
///    produce byte-identical generated files.
///
/// 7. Trailing newline count — codegen can leave an extra blank line at EOF.
///    Normalize to exactly one final newline for byte-stable drift checks.
fn post_process(path: &Path, label: &str) -> BuildResult<()> {
    let content = fs::read_to_string(path).map_err(|e| {
        build_error(format!(
            "failed to read generated {label} at {}: {e}",
            path.display()
        ))
    })?;

    let original_len = content.len();

    // Class E: normalize local checkout paths in ANTLR's generated header.
    let content = normalize_generated_header(&content, label)?;
    // Class A: inner attributes → outer attributes.
    let content = fix_inner_attributes(&content);
    // Class B: `fn` keyword collisions (field access + definitions).
    let content = fix_fn_keyword_collisions(&content);
    // Class C: doubled `Parser` in generated trait name.
    let content = fix_doubled_parser_context(&content);
    // Existing: `this.` → `recog.`
    let content = fix_this_references(&content);
    // Class D: inject missing semantic-predicate extension traits.
    let content = inject_predicate_traits(&content, label);
    // Class F: keep EOF newline normalization stable across platforms.
    let content = normalize_trailing_newline(&content);

    let changes = original_len != content.len();

    fs::write(path, &content).map_err(|e| {
        build_error(format!(
            "failed to write post-processed {label} at {}: {e}",
            path.display()
        ))
    })?;

    if changes {
        println!("cargo:warning=Post-processed {label}: applied antlr4rust compatibility patches");
    }

    Ok(())
}

fn normalize_trailing_newline(content: &str) -> String {
    let mut normalized = content.trim_end_matches('\n').to_owned();
    normalized.push('\n');
    normalized
}

fn normalize_generated_header(content: &str, label: &str) -> BuildResult<String> {
    let grammar = match label {
        "lexer" => "PlSqlLexer.g4",
        "parser" | "listener" | "base_listener" => "PlSqlParser.g4",
        other => {
            return Err(build_error(format!(
                "unknown generated ANTLR label: {other}"
            )));
        }
    };
    let marker = format!("{grammar} by ANTLR ");

    let mut normalized = String::with_capacity(content.len());
    for line in content.lines() {
        if line.starts_with("// Generated from ") && line.contains(&marker) {
            let version = line
                .rsplit_once(" by ANTLR ")
                .map(|(_, version)| version)
                .unwrap_or("unknown");
            let canonical = format!("// Generated from grammars/{grammar} by ANTLR {version}");
            normalized.push_str(&canonical);
        } else {
            normalized.push_str(line);
        }
        normalized.push('\n');
    }
    if !content.ends_with('\n') {
        normalized.pop();
    }
    Ok(normalized)
}

/// Class A fix: convert inner `#![allow(...)]` to outer `#[allow(...)]`.
///
/// The ANTLR Rust codegen emits `#![allow(...)]` at the top of each
/// generated file.  These files are included via `include!()` inside a `mod`
/// block, where inner crate-level attributes are not permitted.  Converting
/// them to outer attributes (`#[allow(...)]`) makes them annotate the next
/// item instead, which is valid and produces the same suppression effect
/// because `lib.rs` already applies `#![allow(warnings)]` at the module level.
fn fix_inner_attributes(content: &str) -> String {
    content.replace("#![allow(", "#[allow(")
}

/// Class B fix: `fn` keyword collisions in generated Rust code.
///
/// Replaces bare `fn` when used as a struct field or method target with
/// `r#fn` (Rust raw identifier syntax).
///
/// IMPORTANT: field-assignment sites like `ctx.fn = val` must become
/// `ctx.r#fn = val` (keeping the dot), not `ctxr#fn = val`.
fn fix_fn_keyword_collisions(content: &str) -> String {
    // Field assignment: `.fn =` → `.r#fn =` (keep the dot!)
    let content = content.replace(".fn =", ".r#fn =");
    // Struct field in pattern/expression: `.fn,` → `.r#fn,`
    let content = content.replace(".fn,", ".r#fn,");
    // Struct field definition: `pub fn:` → `pub r#fn:`
    let content = content.replace("pub fn:", "pub r#fn:");
    // Field in initializer: `fn: None` → `r#fn: None`
    content.replace("fn: None", "r#fn: None")
}

/// Class C fix: doubled `Parser` in generated trait name.
///
/// The codegen template for labeled-alternative rules (`*ContextAll` enums)
/// emits `PlSqlParserParserContext` instead of `PlSqlParserContext`.
/// All 14,013 other trait-impl sites in the file use the correct name.
fn fix_doubled_parser_context(content: &str) -> String {
    content.replace("PlSqlParserParserContext", "PlSqlParserContext")
}

/// Fix `this.` references in embedded actions.
///
/// The grammar uses Java-style `this.MethodName()` which is invalid in
/// Rust. antlr4rust expects `recog.MethodName()` or `self.MethodName()`.
fn fix_this_references(content: &str) -> String {
    // Replace `this.` with `recog.` in embedded action contexts.
    // This is a broad replacement but safe because `this` is not a valid
    // Rust identifier and only appears in generated embedded actions.
    content.replace("this.", "recog.")
}

/// Class D fix: inject missing semantic-predicate extension traits.
///
/// The grammars-v4 PL/SQL grammar embeds semantic predicates that call
/// user-defined methods on the parser (`isVersion12`, `isVersion11`,
/// `isVersion10`, `IsNotNumericFunction`) and the lexer (`IsNewlineAtPos`).
/// These are Java-convention methods expected on a subclass; no `@members`
/// block was ported.  We inject extension traits with permissive defaults:
///
/// - `isVersion12/11/10` → `true` (accept maximum syntax; version-gating
///   can be wired later via a runtime flag)
/// - `IsNotNumericFunction` → `false` (conservative: treat as numeric by
///   default, which is the safe fallback in the grammars-v4 semantics)
/// - `IsNewlineAtPos` → `false` (conservative: no special newline handling)
///
/// The traits are injected at the end of the respective generated file so
/// they are in scope for all `*_sempred` functions in that same module.
///
/// The exact type signatures are derived from the generated code:
///   - `BaseParserType<'input, I>` = `BaseParser<'input, PlSqlParserExt<'input>, I,
///     PlSqlParserContextType, dyn PlSqlParserListener<'input> + 'input>`
///   - `LocalTokenFactory<'input>` = `CommonTokenFactory`
///   - `From<'a>` = `<CommonTokenFactory as TokenFactory<'a>>::From` = `Cow<'a, str>`
///   - Lexer impl needs `Input: CharStream<From<'input>>` = `CharStream<Cow<'input, str>>`
fn inject_predicate_traits(content: &str, label: &str) -> String {
    match label {
        "parser" => {
            // There are two call-site contexts for predicate methods:
            //
            // 1. In `PlSqlParserExt::sempred()`: `recog: &mut BaseParserType<'input, I>`
            //    BaseParserType = BaseParser<'input, PlSqlParserExt<'input>, I,
            //                               PlSqlParserContextType,
            //                               dyn PlSqlParserListener<'input> + 'input>
            //
            // 2. In individual parser rule methods: `let mut recog = self` where
            //    `self: &mut PlSqlParser<'input, I>`.
            //
            // We must implement the trait for both types.  Rust does not propagate
            // trait method lookups through `Deref` when calling via `recog.method()`.
            let trait_code = r#"

// ---------------------------------------------------------------------------
// Class D patch (injected by build.rs post_process): semantic-predicate stubs
//
// The grammar embeds semantic predicates like `{recog.isVersion12()}?` that
// call user-defined methods on the parser.  These are not defined in
// ANTLR Rust runtime's BaseParser; they were expected from a Java subclass.
// We provide permissive defaults so the parser accepts the maximum set of
// PL/SQL syntax regardless of database version.
//
// Two impl blocks are required:
//   - BaseParserType: for calls inside PlSqlParserExt::sempred()
//   - PlSqlParser:    for calls inside individual grammar rule methods
// ---------------------------------------------------------------------------
#[allow(non_snake_case)]
trait PlSqlParserPredicates {
    fn isVersion12(&mut self) -> bool;
    fn isVersion11(&mut self) -> bool;
    fn isVersion10(&mut self) -> bool;
    fn IsNotNumericFunction(&mut self) -> bool;
    fn isNotStartOfJoin(&mut self) -> bool;
}

#[allow(non_snake_case)]
impl<'input, I> PlSqlParserPredicates for BaseParserType<'input, I>
where
    I: __ANTLR_RUNTIME__::token_stream::TokenStream<'input, TF = LocalTokenFactory<'input>>
        + __ANTLR_RUNTIME__::TidAble<'input>,
{
    fn isVersion12(&mut self) -> bool { true }
    fn isVersion11(&mut self) -> bool { true }
    fn isVersion10(&mut self) -> bool { true }
    fn IsNotNumericFunction(&mut self) -> bool { false }
    /// Return false to signal "this is the start of a JOIN clause" (permissive default).
    /// grammars-v4 semantics: `isNotStartOfJoin` guards alias consumption to avoid
    /// ambiguity between `tbl alias` and `tbl JOIN`.  Returning false means the parser
    /// will not consume the next token as an alias when it could be a JOIN keyword.
    fn isNotStartOfJoin(&mut self) -> bool { false }
}

#[allow(non_snake_case)]
impl<'input, I> PlSqlParserPredicates for PlSqlParser<'input, I>
where
    I: __ANTLR_RUNTIME__::token_stream::TokenStream<'input, TF = LocalTokenFactory<'input>>
        + __ANTLR_RUNTIME__::TidAble<'input>,
{
    fn isVersion12(&mut self) -> bool { true }
    fn isVersion11(&mut self) -> bool { true }
    fn isVersion10(&mut self) -> bool { true }
    fn IsNotNumericFunction(&mut self) -> bool { false }
    fn isNotStartOfJoin(&mut self) -> bool { false }
}
"#;
            format!(
                "{content}{}",
                trait_code.replace("__ANTLR_RUNTIME__", "antlr4rust")
            )
        }
        "lexer" => {
            // The sempred functions receive `recog: &mut BaseLexer<'input, PlSqlLexerActions, Input, LocalTokenFactory<'input>>`.
            // LocalTokenFactory<'input> = CommonTokenFactory
            // From<'a> = <CommonTokenFactory as TokenFactory<'a>>::From = Cow<'a, str>
            // So the bound on Input is: CharStream<From<'input>> = CharStream<Cow<'input, str>>
            let trait_code = r#"

// ---------------------------------------------------------------------------
// Class D patch (injected by build.rs post_process): semantic-predicate stubs
//
// The grammar embeds `{recog.IsNewlineAtPos(-4)}?`-style predicates that
// call a method on the BaseLexer.  We provide a conservative default.
// ---------------------------------------------------------------------------
#[allow(non_snake_case)]
trait PlSqlLexerPredicates {
    fn IsNewlineAtPos(&mut self, _pos: isize) -> bool;
}

#[allow(non_snake_case)]
impl<'input, Input> PlSqlLexerPredicates
    for __ANTLR_RUNTIME__::lexer::BaseLexer<
        'input,
        PlSqlLexerActions,
        Input,
        LocalTokenFactory<'input>,
    >
where
    Input: __ANTLR_RUNTIME__::char_stream::CharStream<From<'input>>,
{
    fn IsNewlineAtPos(&mut self, _pos: isize) -> bool { false }
}
"#;
            format!(
                "{content}{}",
                trait_code.replace("__ANTLR_RUNTIME__", "antlr4rust")
            )
        }
        _ => content.to_string(),
    }
}
