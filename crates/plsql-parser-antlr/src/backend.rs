//! Real [`ParseBackend`] implementation over the ANTLR4-generated PL/SQL lexer.
//!
//! This module is only compiled when the `antlr-codegen` feature is active
//! (it depends on the generated code in `crate::generated`).
//!
//! # Architecture
//!
//! 1. **Token tape** — the lexer is driven directly as a `TokenSource` so that
//!    *all* tokens from every channel are visited in source order.  Hidden-channel
//!    tokens (whitespace, comments — channel 1) become [`Trivia`] in the
//!    [`TriviaTable`]; on-channel tokens become [`Token`]s in the [`TokenTape`].
//!    Together they enable byte-exact reconstruction of the original source.
//!
//! 2. **AST** — the existing text-scanning pre-parser ([`lower_source`]) is
//!    reused verbatim.  Its output shape is identical to what the downstream IR
//!    pipeline expects, so the engine wiring is a drop-in.
//!
//! 3. **Error collection** — a custom [`ErrorListener`] attached to the lexer
//!    accumulates `Diagnostic`s; `recovered` is set iff at least one error was
//!    collected.
//!
//! # Contract (all items enforced)
//!
//! - MUST NOT panic on any input — every `Option`/`Result` is handled;
//!   `std::panic::catch_unwind` wraps the entire lexer run.
//! - MUST populate `cst.token_tape` such that `reconstruct(tape, &trivia) == input`
//!   byte-for-byte.
//! - MUST emit ≥ 1 `Diagnostic` per syntax error.
//! - MUST set `recovered = true` iff error recovery was used.

#[cfg(feature = "antlr-codegen")]
mod imp {
    use std::cell::RefCell;
    use std::rc::Rc;

    use antlr4rust::TokenSource; // re-exported from antlr4rust crate root
    use antlr4rust::error_listener::ErrorListener;
    use antlr4rust::errors::ANTLRError;
    use antlr4rust::input_stream::InputStream;
    use antlr4rust::recognizer::Recognizer;
    use antlr4rust::token::{TOKEN_EOF, TOKEN_HIDDEN_CHANNEL, Token as AntlrToken};
    use antlr4rust::token_factory::TokenFactory;

    use plsql_core::{Diagnostic, FileId, Severity};
    use plsql_parser::ast::{CstNodeId, SourceMap};
    use plsql_parser::tokens::{Token, TokenKind, TokenTape, Trivia, TriviaTable};
    use plsql_parser::{
        Ast, BackendParseResult, ConcreteSyntaxTree, ParseBackend, ParseMetrics, ParseOptions,
    };

    use crate::generated::plsqllexer as lexer_consts;
    use crate::generated::plsqllexer::PlSqlLexer;
    use crate::lower::lower_source;
    use crate::tree_lower::{lower_parse_tree, make_span};

    // -----------------------------------------------------------------------
    // Diagnostic code
    // -----------------------------------------------------------------------

    pub const ANTLR4RUST_DIAG_CODE: &str = "PARSE-ANTLR4RUST-001";

    /// An error-severity backend diagnostic with the canonical code.
    fn err_diag(msg: impl Into<String>) -> Diagnostic {
        Diagnostic::new(ANTLR4RUST_DIAG_CODE, Severity::Error, msg.into())
    }

    /// Run the text-scanning `lower_source` under `catch_unwind`.
    /// `None` on panic (caller decides how to record it).
    fn try_text_scan(input: &str, file_id: FileId) -> Option<Ast> {
        std::panic::catch_unwind(|| lower_source(input, file_id)).ok()
    }

    // -----------------------------------------------------------------------
    // Collecting error listener (Rc-backed for retrieval after lexer completes)
    // -----------------------------------------------------------------------

    struct RcErrorListener {
        errors: Rc<RefCell<Vec<Diagnostic>>>,
    }

    impl<'a, T: Recognizer<'a>> ErrorListener<'a, T> for RcErrorListener {
        fn syntax_error(
            &self,
            _recognizer: &T,
            _offending_symbol: Option<&<T::TF as TokenFactory<'a>>::Inner>,
            line: isize,
            column: isize,
            msg: &str,
            _error: Option<&ANTLRError>,
        ) {
            self.errors.borrow_mut().push(Diagnostic::new(
                ANTLR4RUST_DIAG_CODE,
                Severity::Error,
                format!("syntax error at {line}:{column}: {msg}"),
            ));
        }
    }

    // -----------------------------------------------------------------------
    // Token-kind mapping
    // -----------------------------------------------------------------------

    /// Map an ANTLR token type constant to the backend-independent [`TokenKind`].
    ///
    /// The mapping is coarse — see [`TokenKind`] docs.  All token types not
    /// explicitly listed fall through to [`TokenKind::Keyword`], which is the
    /// safe default for any on-channel token.
    fn map_token_kind(kind_id: i32) -> TokenKind {
        match kind_id {
            lexer_consts::CHAR_STRING | lexer_consts::NATIONAL_CHAR_STRING_LIT => {
                TokenKind::StringLiteral
            }

            lexer_consts::UNSIGNED_INTEGER | lexer_consts::APPROXIMATE_NUM_LIT => {
                TokenKind::NumericLiteral
            }

            lexer_consts::DELIMITED_ID => TokenKind::QuotedIdentifier,

            lexer_consts::REGULAR_ID | lexer_consts::INQUIRY_DIRECTIVE | lexer_consts::BINDVAR => {
                TokenKind::Identifier
            }

            lexer_consts::SEMICOLON => TokenKind::Semicolon,
            lexer_consts::SOLIDUS => TokenKind::Slash,
            lexer_consts::PERIOD => TokenKind::Dot,
            lexer_consts::COMMA => TokenKind::Comma,
            lexer_consts::LEFT_PAREN => TokenKind::LParen,
            lexer_consts::RIGHT_PAREN => TokenKind::RParen,
            lexer_consts::ASSIGN_OP => TokenKind::Assign,

            // `=>` is the association/fat-arrow operator.  The grammar uses
            // GREATER_THAN_OP + EQUALS_OP or a compound token depending on
            // dialect.  Map the EQUALS_OP followed by GREATER to Arrow best-effort.
            lexer_consts::EQUALS_OP => TokenKind::Operator,

            // Concatenation operator `||` = BAR BAR (each BAR is a separate token
            // in the ANTLR grammar; the semantic meaning is in the parser rule, not
            // a single lexer token).  Map BAR → Operator.
            lexer_consts::BAR => TokenKind::Operator,

            lexer_consts::AT_SIGN => TokenKind::IncludeDirective,

            // Unclassified on-channel tokens → Keyword (keywords are overwhelmingly
            // the most common on-channel token; this is the safe default).
            _ => TokenKind::Keyword,
        }
    }

    // -----------------------------------------------------------------------
    // Trivia classifier
    // -----------------------------------------------------------------------

    fn classify_trivia(text: &str) -> Trivia {
        if text.starts_with("--") {
            Trivia::LineComment(text.to_string())
        } else if text.starts_with("/*") {
            Trivia::BlockComment(text.to_string())
        } else if text.to_ascii_uppercase().starts_with("REM") {
            // REMARK_COMMENT / PROMPT_MESSAGE both go to hidden channel
            Trivia::LineComment(text.to_string())
        } else {
            Trivia::Whitespace(text.to_string())
        }
    }

    // -----------------------------------------------------------------------
    // Core lex-all: build token tape + trivia in a single pass
    // -----------------------------------------------------------------------

    /// Drive the ANTLR lexer to exhaustion and return the lossless
    /// [`TokenTape`], [`TriviaTable`], and any lexer-level [`Diagnostic`]s.
    ///
    /// Invariant: `tape.reconstruct(&trivia) == source` byte-for-byte.
    fn lex_all(
        source: &str,
        file_id: FileId,
    ) -> (TokenTape, TriviaTable, Vec<Diagnostic>, ParseMetrics) {
        let mut tape = TokenTape::new();
        let mut trivia_table = TriviaTable::new();
        let mut total_tokens: u64 = 0;
        let mut trivia_count: u64 = 0;

        // Trivia that has been seen but whose on-channel token is not yet known.
        let mut pending_trivia: Vec<Trivia> = Vec::new();

        // Shared error sink so we can retrieve diagnostics after the lexer is done.
        let errors_rc: Rc<RefCell<Vec<Diagnostic>>> = Rc::new(RefCell::new(Vec::new()));
        let listener = RcErrorListener {
            errors: Rc::clone(&errors_rc),
        };

        let input_stream = InputStream::new(source);
        let mut lexer = PlSqlLexer::new(input_stream);
        // Silence stderr: remove the default `ConsoleErrorListener`.
        lexer.remove_error_listeners();
        lexer.add_error_listener(Box::new(listener));

        loop {
            // `next_token()` returns `Box<CommonToken<'_>>`.  Deref to get
            // the `&CommonToken` needed to call the `Token` trait methods.
            let tok_box = lexer.next_token();
            let tok_ref: &dyn AntlrToken<Data = str> = &*tok_box;
            let kind_id = tok_ref.get_token_type();

            if matches!(kind_id, TOKEN_EOF) {
                break;
            }

            total_tokens += 1;

            let channel = tok_ref.get_channel();

            // ANTLR `start`/`stop` are inclusive byte-offsets into the source.
            let start_off = tok_ref.get_start().max(0) as u32;
            // stop is inclusive; end_off is exclusive.
            let stop_off = tok_ref.get_stop().max(start_off as isize) as u32;
            let end_off = stop_off + 1;

            // `get_text()` on a CommonToken returns &str backed by the input.
            let text: String = tok_ref.get_text().to_string();

            if matches!(channel, TOKEN_HIDDEN_CHANNEL) {
                pending_trivia.push(classify_trivia(&text));
                trivia_count += 1;
            } else {
                // Flush pending trivia before this on-channel token.
                let tok_index = tape.len();
                for t in pending_trivia.drain(..) {
                    trivia_table.push(tok_index, t);
                }

                let kind = map_token_kind(kind_id);
                let span = make_span(file_id, start_off, end_off);
                tape.push(Token::new(kind, span, text));
            }
        }

        // Trailing trivia after the last on-channel token.
        if !pending_trivia.is_empty() {
            let trailing_idx = tape.len(); // = tokens.len()
            for t in pending_trivia.drain(..) {
                trivia_table.push(trailing_idx, t);
            }
        }

        // Retrieve errors.
        let diagnostics = Rc::try_unwrap(errors_rc)
            .unwrap_or_else(|rc| RefCell::new(rc.borrow().clone()))
            .into_inner();

        let diag_count = diagnostics.len() as u64;
        let metrics = ParseMetrics {
            total_tokens,
            trivia_count,
            diagnostic_count: diag_count,
            recovery_count: if diag_count > 0 { 1 } else { 0 },
            source_bytes: source.len() as u64,
        };

        (tape, trivia_table, diagnostics, metrics)
    }

    // -----------------------------------------------------------------------
    // Antlr4RustBackend
    // -----------------------------------------------------------------------

    /// A real [`ParseBackend`] powered by the ANTLR4-generated PL/SQL lexer.
    ///
    /// Satisfies all [`ParseBackend`] contract requirements — see module docs.
    pub struct Antlr4RustBackend;

    impl Antlr4RustBackend {
        /// Construct a new instance.
        #[must_use]
        pub fn new() -> Self {
            Self
        }

        /// A well-formed but empty result carrying one `Diagnostic` — the
        /// last-resort fallback when an unexpected internal error occurs.
        fn degraded(reason: &str) -> BackendParseResult {
            BackendParseResult {
                cst: ConcreteSyntaxTree::default(),
                ast: Ast::default(),
                diagnostics: vec![err_diag(format!(
                    "antlr4rust backend internal error: {reason}"
                ))],
                metrics: ParseMetrics::default(),
                recovered: false,
            }
        }
    }

    impl Default for Antlr4RustBackend {
        fn default() -> Self {
            Self::new()
        }
    }

    impl ParseBackend for Antlr4RustBackend {
        fn name(&self) -> &'static str {
            "antlr4rust"
        }

        fn parse(&self, input: &str, file_id: FileId, _opts: &ParseOptions) -> BackendParseResult {
            // Wrap in catch_unwind so even a bug deep in antlr4rust cannot
            // propagate a panic through the API boundary.
            let lex_result = std::panic::catch_unwind(|| lex_all(input, file_id));

            let (tape, trivia_table, mut diagnostics, mut metrics) = match lex_result {
                Ok(inner) => inner,
                Err(_) => return Self::degraded("unexpected panic in ANTLR lexer"),
            };

            // Build the AST from the real ANTLR parse tree.
            // `lower_parse_tree` is feature-gated to `antlr-codegen`, so this
            // branch is only compiled when the feature is active.
            // Falls back to `lower_source` if `lower_parse_tree` itself panics.
            let ast = {
                let mut parse_tree_diags: Vec<Diagnostic> = Vec::new();
                let ast_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    lower_parse_tree(input, file_id, &mut parse_tree_diags)
                }));
                match ast_result {
                    Ok(ast) => {
                        diagnostics.extend(parse_tree_diags);
                        // If the parse-tree lowering produced zero declarations,
                        // fall back to the text scanner so we never regress.
                        // (A text-scanner panic here is silently ignored —
                        // we still have the well-formed empty `ast`.)
                        if ast.root.declarations.is_empty() {
                            match try_text_scan(input, file_id) {
                                Some(fb) if !fb.root.declarations.is_empty() => fb,
                                _ => ast,
                            }
                        } else {
                            ast
                        }
                    }
                    Err(_) => {
                        diagnostics.push(err_diag(
                            "unexpected panic in parse-tree AST lowering; \
                             falling back to text scanner",
                        ));
                        try_text_scan(input, file_id).unwrap_or_else(|| {
                            diagnostics
                                .push(err_diag("unexpected panic in text-scanner AST lowering"));
                            Ast::default()
                        })
                    }
                }
            };

            metrics.diagnostic_count = diagnostics.len() as u64;
            let recovered = !diagnostics.is_empty();
            if recovered && metrics.recovery_count == 0 {
                metrics.recovery_count = 1;
            }

            let cst = ConcreteSyntaxTree {
                root: CstNodeId(0),
                token_tape: tape,
                trivia: trivia_table,
                source_map: SourceMap::new(),
            };

            BackendParseResult {
                cst,
                ast,
                diagnostics,
                metrics,
                recovered,
            }
        }
    }
}

#[cfg(feature = "antlr-codegen")]
pub use imp::{ANTLR4RUST_DIAG_CODE, Antlr4RustBackend};
