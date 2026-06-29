#![forbid(unsafe_code)]

//! `plsql` CLI surface for CI/CD change-impact prediction.
//!
//! The binary is intentionally a thin adapter: changeset construction,
//! direct prediction, lineage-fed transitive expansion, and the stable
//! robot-JSON payload all remain in `plsql-cicd`.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use plsql_cicd::{
    CHANGE_IMPACT_SCHEMA, ChangeImpactCompileErrorFlag, ChangeImpactEnvelope, ChangeSet, CicdError,
    LineageObjectMetadata, PredictMode, change_impact_envelope, doctor_report, predict,
    predict_with_lineage,
};
use plsql_core::{ObjectName, SchemaName, SymbolId, SymbolInterner};
use plsql_lineage::LineageResult;
use plsql_output::{RobotJsonEnvelope, SchemaDescriptor, SchemaVersion};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

const ERROR_ENVELOPE_SCHEMA: SchemaDescriptor = SchemaDescriptor {
    id: "plsql.cicd.error_envelope",
    version: SchemaVersion::new(1, 0, 0),
    description: "plsql CLI runtime error envelope",
};

const DOC_RENDER_SCHEMA: SchemaDescriptor = SchemaDescriptor {
    id: "plsql.doc.render",
    version: SchemaVersion::new(1, 0, 0),
    description: "plsql CLI documentation render payload",
};

const SAST_RENDER_SCHEMA: SchemaDescriptor = SchemaDescriptor {
    id: "plsql.sast.report",
    version: SchemaVersion::new(1, 0, 0),
    description: "plsql CLI SAST report render payload",
};

const CAPABILITIES_CONTRACT_VERSION: u32 = 1;

#[derive(Debug, Parser)]
#[command(name = "plsql")]
#[command(version)]
#[command(about = "PL/SQL Intelligence release-assurance CLI")]
#[command(
    after_help = "DISCOVERY:\n  plsql capabilities       machine-readable agent contract (JSON)\n  plsql robot-docs         agent handbook\n  plsql --robot-triage     one-shot bootstrap"
)]
struct Cli {
    /// Emit a single-line machine-readable JSON object on stdout.
    #[arg(long, global = true)]
    robot_json: bool,
    /// Emit {capabilities, health, quick_ref} and exit.
    #[arg(long, global = true)]
    robot_triage: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Predict invalidations for a proposed PL/SQL changeset.
    Predict(PredictArgs),
    /// Render or inspect a plsql-doc DocSet JSON artifact.
    Doc(DocArgs),
    /// Render or inspect a plsql-sast ScanReport JSON artifact.
    Sast(SastArgs),
    /// Print a diagnostic report. With a changeset, includes changeset health.
    Doctor(DoctorArgs),
    /// Print the machine-readable agent contract as JSON.
    Capabilities,
    /// Print a concise agent handbook.
    RobotDocs,
}

#[derive(Debug, Args)]
struct PredictArgs {
    /// Changeset source. Auto-detected as directory, unified diff, SQL
    /// script, or serialized ChangeSet JSON unless --source-kind is supplied.
    #[arg(value_name = "CHANGESET_SOURCE")]
    changeset_source: Option<PathBuf>,
    /// Source type for CHANGESET_SOURCE.
    #[arg(long, value_enum)]
    source_kind: Option<SourceKindArg>,
    /// Prediction mode.
    #[arg(long, value_enum, default_value_t = PredictModeArg::CatalogAware)]
    mode: PredictModeArg,
    /// Before directory for before/after directory changeset construction.
    #[arg(long, value_name = "DIR")]
    before: Option<PathBuf>,
    /// After directory for before/after directory changeset construction.
    #[arg(long, value_name = "DIR")]
    after: Option<PathBuf>,
    /// Git range in the form FROM..TO. Uses --repo, defaulting to cwd.
    #[arg(long, value_name = "FROM..TO")]
    git_range: Option<String>,
    /// Repository path used with --git-range.
    #[arg(long, value_name = "DIR", default_value = ".")]
    repo: PathBuf,
    /// Read one offline plsql.lineage.impact LineageResult JSON document.
    /// May be supplied more than once.
    #[arg(long, value_name = "PATH")]
    lineage_impact: Vec<PathBuf>,
    /// JSON metadata map used to lower LineageResult logical IDs into
    /// CI/CD object metadata. Required when --lineage-impact is used.
    #[arg(long, value_name = "PATH")]
    lineage_metadata: Option<PathBuf>,
    /// Read compile-error flags to include in the robot-json payload. Accepts
    /// either an array of flag rows or an object with compile_error_flags.
    #[arg(long, value_name = "PATH")]
    compile_error_flags: Option<PathBuf>,
}

#[derive(Debug, Args, Default)]
struct DoctorArgs {
    /// Optional changeset source to diagnose.
    #[arg(value_name = "CHANGESET_SOURCE")]
    changeset_source: Option<PathBuf>,
    /// Source type for CHANGESET_SOURCE.
    #[arg(long, value_enum)]
    source_kind: Option<SourceKindArg>,
    /// Prediction mode used when a changeset is supplied.
    #[arg(long, value_enum, default_value_t = PredictModeArg::CatalogAware)]
    mode: PredictModeArg,
    /// Before directory for before/after directory changeset construction.
    #[arg(long, value_name = "DIR")]
    before: Option<PathBuf>,
    /// After directory for before/after directory changeset construction.
    #[arg(long, value_name = "DIR")]
    after: Option<PathBuf>,
    /// Git range in the form FROM..TO. Uses --repo, defaulting to cwd.
    #[arg(long, value_name = "FROM..TO")]
    git_range: Option<String>,
    /// Repository path used with --git-range.
    #[arg(long, value_name = "DIR", default_value = ".")]
    repo: PathBuf,
}

#[derive(Debug, Args)]
struct DocArgs {
    /// DocSet source JSON, either raw DocSet or a robot-json envelope whose
    /// payload is a DocSet.
    #[arg(value_name = "DOCSET_JSON")]
    docset_source: PathBuf,
    /// Render format.
    #[arg(long, value_enum, default_value_t = DocFormatArg::Markdown)]
    format: DocFormatArg,
    /// Label used in generated index headings.
    #[arg(long, value_name = "LABEL", default_value = "plsql")]
    project_label: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
enum DocFormatArg {
    #[default]
    Markdown,
    Html,
    Json,
    Doctor,
}

impl DocFormatArg {
    const fn wire_name(self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
            Self::Html => "html",
            Self::Json => "json",
            Self::Doctor => "doctor",
        }
    }

    const fn media_type(self) -> &'static str {
        match self {
            Self::Markdown => "text/markdown",
            Self::Html => "text/html",
            Self::Json | Self::Doctor => "application/json",
        }
    }
}

#[derive(Debug, Args)]
struct SastArgs {
    /// ScanReport source JSON, either raw ScanReport or a robot-json envelope
    /// whose payload is a ScanReport.
    #[arg(value_name = "SCAN_REPORT_JSON")]
    scan_report_source: PathBuf,
    /// Render format.
    #[arg(long, value_enum, default_value_t = SastFormatArg::Sarif)]
    format: SastFormatArg,
    /// SARIF tool name.
    #[arg(long, value_name = "NAME", default_value = "plsql-sast")]
    tool_name: String,
    /// SARIF semantic version. Defaults to the plsql CLI crate version.
    #[arg(long, value_name = "VERSION")]
    tool_version: Option<String>,
    /// JUnit test suite name.
    #[arg(long, value_name = "NAME", default_value = "plsql-sast")]
    suite_name: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
enum SastFormatArg {
    #[default]
    Sarif,
    Json,
    Junit,
    Histogram,
}

impl SastFormatArg {
    const fn wire_name(self) -> &'static str {
        match self {
            Self::Sarif => "sarif",
            Self::Json => "json",
            Self::Junit => "junit",
            Self::Histogram => "histogram",
        }
    }

    const fn media_type(self) -> &'static str {
        match self {
            Self::Sarif | Self::Json | Self::Histogram => "application/json",
            Self::Junit => "application/junit+xml",
        }
    }
}

#[derive(Debug, Serialize)]
struct DocRenderPayload {
    command: &'static str,
    format: &'static str,
    media_type: &'static str,
    project_label: String,
    object_count: usize,
    coverage: plsql_doc::DocCoverageReport,
    artifact: Value,
}

#[derive(Debug, Serialize)]
struct SastRenderPayload {
    command: &'static str,
    format: &'static str,
    media_type: &'static str,
    finding_count: usize,
    skipped_count: usize,
    rules_run: usize,
    rules_gated: usize,
    max_severity: Option<plsql_sast::Severity>,
    histogram: plsql_sast::RuleFiringHistogram,
    artifact: Value,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, ValueEnum)]
enum PredictModeArg {
    SourceOnly,
    #[default]
    CatalogAware,
    LiveSnapshot,
}

impl From<PredictModeArg> for PredictMode {
    fn from(value: PredictModeArg) -> Self {
        match value {
            PredictModeArg::SourceOnly => Self::SourceOnly,
            PredictModeArg::CatalogAware => Self::CatalogAware,
            PredictModeArg::LiveSnapshot => Self::LiveSnapshot,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum SourceKindArg {
    Auto,
    Directory,
    Diff,
    Script,
    ChangesetJson,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ResolvedSourceKind {
    Directory,
    Diff,
    Script,
    ChangesetJson,
}

#[derive(Debug)]
struct CliError {
    code: &'static str,
    message: String,
    path: Option<PathBuf>,
    remediation: Option<String>,
    exit_code: u8,
}

impl CliError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            path: None,
            remediation: None,
            exit_code: 1,
        }
    }

    fn with_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path = Some(path.into());
        self
    }

    fn with_remediation(mut self, remediation: impl Into<String>) -> Self {
        self.remediation = Some(remediation.into());
        self
    }

    fn with_exit_code(mut self, exit_code: u8) -> Self {
        self.exit_code = exit_code;
        self
    }
}

impl From<CicdError> for CliError {
    fn from(value: CicdError) -> Self {
        Self::new("changeset_load_failed", value.to_string())
    }
}

impl From<serde_json::Error> for CliError {
    fn from(value: serde_json::Error) -> Self {
        Self::new("json_failed", value.to_string())
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let robot_json = cli.robot_json;

    let result = if cli.robot_triage {
        run_robot_triage(robot_json)
    } else {
        match cli.command {
            Some(Command::Predict(args)) => run_predict(args, robot_json),
            Some(Command::Doc(args)) => run_doc(args, robot_json),
            Some(Command::Sast(args)) => run_sast(args, robot_json),
            Some(Command::Doctor(args)) => run_doctor(args, robot_json),
            Some(Command::Capabilities) => run_capabilities(robot_json),
            Some(Command::RobotDocs) => {
                print!("{}", robot_docs_text());
                Ok(ExitCode::SUCCESS)
            }
            None => {
                let mut cmd = Cli::command();
                let _ = cmd.write_long_help(&mut std::io::stderr());
                let _ = writeln!(std::io::stderr());
                let _ = writeln!(
                    std::io::stderr(),
                    "no subcommand given - try `plsql predict --robot-json <changeset>`, `plsql doc --robot-json <docset>`, `plsql sast --robot-json <scan-report>`, `plsql doctor`, or `plsql --robot-triage`."
                );
                Ok(ExitCode::from(2))
            }
        }
    };

    match result {
        Ok(code) => code,
        Err(err) => {
            emit_error(robot_json, &err);
            ExitCode::from(err.exit_code)
        }
    }
}

fn run_predict(args: PredictArgs, robot_json: bool) -> Result<ExitCode, CliError> {
    let changeset = load_changeset(PredictSource {
        changeset_source: args.changeset_source,
        source_kind: args.source_kind,
        before: args.before,
        after: args.after,
        git_range: args.git_range,
        repo: args.repo,
    })?;
    let mode = PredictMode::from(args.mode);
    let prediction = if args.lineage_impact.is_empty() {
        predict(&changeset, mode)
    } else {
        let metadata_path = args.lineage_metadata.ok_or_else(|| {
            CliError::new(
                "lineage_metadata_required",
                "--lineage-metadata is required when --lineage-impact is supplied",
            )
            .with_remediation(
                "provide a JSON document with an `objects` array keyed by lineage logical_id",
            )
            .with_exit_code(2)
        })?;
        let metadata = load_lineage_metadata(&metadata_path, &changeset)?;
        let impacts = load_lineage_impacts(&args.lineage_impact)?;
        predict_with_lineage(&changeset, mode, &impacts, |logical_id| {
            metadata.get(logical_id).cloned()
        })
    };
    let compile_error_flags = match args.compile_error_flags {
        Some(path) => load_compile_error_flags(&path)?,
        None => Vec::new(),
    };
    let envelope = change_impact_envelope(&prediction, compile_error_flags);
    if robot_json {
        println!("{}", serialize_compact(&envelope)?);
    } else {
        print_predict_human(&envelope);
    }
    Ok(ExitCode::SUCCESS)
}

fn run_doc(args: DocArgs, robot_json: bool) -> Result<ExitCode, CliError> {
    let docset = load_doc_set(&args.docset_source)?;
    let coverage = plsql_doc::doctor_report(&docset);
    let artifact = match args.format {
        DocFormatArg::Markdown => Value::String(plsql_doc::render_full_markdown_bundle(
            &docset,
            &args.project_label,
        )),
        DocFormatArg::Html => Value::String(plsql_doc::render_full_html_bundle(
            &docset,
            &args.project_label,
        )),
        DocFormatArg::Json => serde_json::to_value(&docset)?,
        DocFormatArg::Doctor => serde_json::to_value(&coverage)?,
    };
    let payload = DocRenderPayload {
        command: "doc",
        format: args.format.wire_name(),
        media_type: args.format.media_type(),
        project_label: args.project_label,
        object_count: docset.objects.len(),
        coverage,
        artifact,
    };
    if robot_json {
        let envelope = RobotJsonEnvelope::new(DOC_RENDER_SCHEMA, payload);
        println!("{}", serialize_compact(&envelope)?);
    } else {
        print_artifact(&payload.artifact)?;
    }
    Ok(ExitCode::SUCCESS)
}

fn run_sast(args: SastArgs, robot_json: bool) -> Result<ExitCode, CliError> {
    let report = load_scan_report(&args.scan_report_source)?;
    let histogram = plsql_sast::rule_firing_histogram(&report);
    let tool_version = args
        .tool_version
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());
    let artifact = match args.format {
        SastFormatArg::Sarif => serde_json::to_value(plsql_sast::to_sarif(
            &report,
            &args.tool_name,
            &tool_version,
        ))?,
        SastFormatArg::Json => serde_json::to_value(&report)?,
        SastFormatArg::Junit => Value::String(plsql_sast::to_junit_xml(&report, &args.suite_name)),
        SastFormatArg::Histogram => serde_json::to_value(&histogram)?,
    };
    let payload = SastRenderPayload {
        command: "sast",
        format: args.format.wire_name(),
        media_type: args.format.media_type(),
        finding_count: report.findings.len(),
        skipped_count: report.skipped.len(),
        rules_run: report.rules_run,
        rules_gated: report.rules_gated,
        max_severity: report.max_severity(),
        histogram,
        artifact,
    };
    if robot_json {
        let envelope = RobotJsonEnvelope::new(SAST_RENDER_SCHEMA, payload);
        println!("{}", serialize_compact(&envelope)?);
    } else {
        print_artifact(&payload.artifact)?;
    }
    Ok(ExitCode::SUCCESS)
}

fn run_doctor(args: DoctorArgs, robot_json: bool) -> Result<ExitCode, CliError> {
    let changeset_report = match args.changeset_source {
        Some(changeset_source) => {
            let changeset = load_changeset(PredictSource {
                changeset_source: Some(changeset_source),
                source_kind: args.source_kind,
                before: args.before,
                after: args.after,
                git_range: args.git_range,
                repo: args.repo,
            })?;
            let prediction = predict(&changeset, PredictMode::from(args.mode));
            Some(doctor_report(&changeset, Some(&prediction)))
        }
        None => None,
    };
    let report = serde_json::json!({
        "binary": "plsql",
        "version": env!("CARGO_PKG_VERSION"),
        "status": "ok",
        "blockers": [],
        "schemas": {
            "change_impact": {
                "id": CHANGE_IMPACT_SCHEMA.id,
                "version": CHANGE_IMPACT_SCHEMA.version.to_string()
            },
            "error_envelope": {
                "id": ERROR_ENVELOPE_SCHEMA.id,
                "version": ERROR_ENVELOPE_SCHEMA.version.to_string()
            },
            "doc_render": {
                "id": DOC_RENDER_SCHEMA.id,
                "version": DOC_RENDER_SCHEMA.version.to_string()
            },
            "sast_report": {
                "id": SAST_RENDER_SCHEMA.id,
                "version": SAST_RENDER_SCHEMA.version.to_string()
            }
        },
        "commands": ["predict", "doc", "sast", "doctor", "capabilities", "robot-docs"],
        "changeset": changeset_report,
    });
    if robot_json {
        println!("{}", serialize_compact(&report)?);
    } else {
        eprintln!(
            "plsql {} (plsql-cicd release-assurance CLI)",
            env!("CARGO_PKG_VERSION")
        );
        println!(
            "doctor: blockers=0 schemas=change_impact:{}",
            CHANGE_IMPACT_SCHEMA.version
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn run_capabilities(robot_json: bool) -> Result<ExitCode, CliError> {
    let doc = capabilities_json();
    if robot_json {
        println!("{}", serialize_compact(&doc)?);
    } else {
        println!("{}", serde_json::to_string_pretty(&doc)?);
    }
    Ok(ExitCode::SUCCESS)
}

fn run_robot_triage(robot_json: bool) -> Result<ExitCode, CliError> {
    let health = serde_json::json!({
        "binary": "plsql",
        "version": env!("CARGO_PKG_VERSION"),
        "status": "ok",
        "blockers": [],
        "schemas": {
            "change_impact": {
                "id": CHANGE_IMPACT_SCHEMA.id,
                "version": CHANGE_IMPACT_SCHEMA.version.to_string()
            },
            "doc_render": {
                "id": DOC_RENDER_SCHEMA.id,
                "version": DOC_RENDER_SCHEMA.version.to_string()
            },
            "sast_report": {
                "id": SAST_RENDER_SCHEMA.id,
                "version": SAST_RENDER_SCHEMA.version.to_string()
            }
        }
    });
    let quick_ref = serde_json::json!([
        {
            "description": "predict change impact from a changeset source",
            "invocation": "plsql predict --robot-json <changeset-source>"
        },
        {
            "description": "predict with offline lineage impact JSON",
            "invocation": "plsql predict --robot-json --lineage-impact impact.json --lineage-metadata metadata.json --compile-error-flags compile-errors.json --source-kind changeset-json changeset.json"
        },
        {
            "description": "render a plsql-doc DocSet artifact",
            "invocation": "plsql doc --robot-json --format markdown docset.json"
        },
        {
            "description": "render a plsql-sast ScanReport as SARIF",
            "invocation": "plsql sast --robot-json --format sarif scan-report.json"
        },
        {
            "description": "machine-readable health check",
            "invocation": "plsql doctor --robot-json"
        },
        {
            "description": "full versioned agent contract",
            "invocation": "plsql capabilities"
        }
    ]);
    let mega = serde_json::json!({
        "capabilities": capabilities_json(),
        "health": health,
        "quick_ref": quick_ref,
    });
    if robot_json {
        println!("{}", serialize_compact(&mega)?);
    } else {
        println!("{}", serde_json::to_string_pretty(&mega)?);
    }
    Ok(ExitCode::SUCCESS)
}

fn capabilities_json() -> Value {
    serde_json::json!({
        "binary": "plsql",
        "contract_version": CAPABILITIES_CONTRACT_VERSION,
        "version": env!("CARGO_PKG_VERSION"),
        "global_flags": {
            "--robot-json": "emit compact single-line JSON on stdout; diagnostics always go to stderr",
            "--robot-triage": "one-shot bootstrap: emit {capabilities, health, quick_ref} on stdout and exit"
        },
        "commands": {
            "predict": "wrap ChangeSet construction, predict or predict_with_lineage, and emit plsql.cicd.change_impact",
            "doc": "render or inspect an offline plsql-doc DocSet artifact as markdown, html, json, or doctor coverage",
            "sast": "render or inspect an offline plsql-sast ScanReport artifact as SARIF, JUnit, JSON, or a rule histogram",
            "doctor": "report binary/schema health and optional changeset health",
            "capabilities": "print this machine-readable agent contract",
            "robot-docs": "print a concise agent handbook"
        },
        "changeset_sources": {
            "directory": "recursive .sql/.pls/.plsql/.pks/.pkb staged source directory",
            "diff": "unified diff file from git diff or diff -u",
            "script": "standalone SQL script retained as an unclassified changeset file",
            "changeset-json": "serialized plsql_cicd::ChangeSet JSON",
            "before_after": "use --before DIR --after DIR",
            "git_range": "use --git-range FROM..TO [--repo DIR]"
        },
        "predict_inputs": {
            "--lineage-impact PATH": "optional plsql.lineage.impact LineageResult JSON; repeatable",
            "--lineage-metadata PATH": "required when --lineage-impact is supplied; maps logical IDs to object symbols and object types",
            "--compile-error-flags PATH": "optional JSON array, or object with compile_error_flags, copied into payload.compile_error_flags"
        },
        "schemas": {
            "change_impact": {
                "id": CHANGE_IMPACT_SCHEMA.id,
                "version": CHANGE_IMPACT_SCHEMA.version.to_string()
            },
            "doc_render": {
                "id": DOC_RENDER_SCHEMA.id,
                "version": DOC_RENDER_SCHEMA.version.to_string()
            },
            "sast_report": {
                "id": SAST_RENDER_SCHEMA.id,
                "version": SAST_RENDER_SCHEMA.version.to_string()
            },
            "error_envelope": {
                "id": ERROR_ENVELOPE_SCHEMA.id,
                "version": ERROR_ENVELOPE_SCHEMA.version.to_string()
            }
        },
        "exit_codes": {
            "0": "success",
            "1": "runtime failure",
            "2": "invocation failure"
        },
        "stdout_contract": "stdout is data only; all diagnostics go to stderr"
    })
}

fn robot_docs_text() -> String {
    format!(
        r#"plsql agent handbook
====================

WHAT IT DOES
  plsql is the release-assurance CLI for plsql-intelligence. The current
  shipped surface is `predict`: construct a ChangeSet, run the CI/CD
  invalidation predictor, optionally compose offline lineage impact
  results, and emit the stable plsql.cicd.change_impact payload.
  `doc` renders offline plsql-doc DocSet artifacts. `sast` renders
  offline plsql-sast ScanReport artifacts.

CANONICAL INVOCATIONS
  plsql predict --robot-json <changeset-source>
  plsql predict --robot-json --before before_dir --after after_dir
  plsql predict --robot-json --git-range main..HEAD --repo .
  plsql predict --robot-json --source-kind changeset-json changeset.json \
      --lineage-impact impact.json --lineage-metadata metadata.json
  plsql predict --robot-json --source-kind changeset-json changeset.json \
      --lineage-impact impact.json --lineage-metadata metadata.json \
      --compile-error-flags compile-errors.json
  plsql doc --robot-json --format markdown docset.json
  plsql sast --robot-json --format sarif scan-report.json
  plsql doctor --robot-json
  plsql --robot-triage

ROBOT-JSON
  predict emits:
    format:         plsql-robot-json
    schema_id:      {schema_id}
    schema_version: {schema_version}
  doc emits schema_id:  {doc_schema_id}
  sast emits schema_id: {sast_schema_id}

EXIT CODES
  0 success
  1 runtime failure
  2 invocation failure

DISCOVERY
  plsql capabilities
  plsql robot-docs
"#,
        schema_id = CHANGE_IMPACT_SCHEMA.id,
        schema_version = CHANGE_IMPACT_SCHEMA.version,
        doc_schema_id = DOC_RENDER_SCHEMA.id,
        sast_schema_id = SAST_RENDER_SCHEMA.id,
    )
}

struct PredictSource {
    changeset_source: Option<PathBuf>,
    source_kind: Option<SourceKindArg>,
    before: Option<PathBuf>,
    after: Option<PathBuf>,
    git_range: Option<String>,
    repo: PathBuf,
}

fn load_changeset(source: PredictSource) -> Result<ChangeSet, CliError> {
    if let Some(range) = source.git_range {
        let (from, to) = range.split_once("..").ok_or_else(|| {
            CliError::new(
                "invalid_git_range",
                "--git-range must use the form FROM..TO",
            )
            .with_exit_code(2)
        })?;
        return ChangeSet::from_git_diff(&source.repo, from, to).map_err(Into::into);
    }

    match (source.before, source.after) {
        (Some(before), Some(after)) => {
            validate_dir(&before)?;
            validate_dir(&after)?;
            return ChangeSet::from_before_after_dirs(&before, &after).map_err(Into::into);
        }
        (Some(_), None) | (None, Some(_)) => {
            return Err(CliError::new(
                "before_after_required",
                "--before and --after must be supplied together",
            )
            .with_exit_code(2));
        }
        (None, None) => {}
    }

    let path = source.changeset_source.ok_or_else(|| {
        CliError::new(
            "changeset_source_required",
            "predict requires a changeset source, --before/--after, or --git-range",
        )
        .with_exit_code(2)
    })?;
    if !path.exists() {
        return Err(CliError::new(
            "changeset_source_missing",
            format!("changeset source does not exist: {}", path.display()),
        )
        .with_path(path)
        .with_exit_code(2));
    }

    let source_kind = resolve_source_kind(source.source_kind, &path);
    match source_kind {
        ResolvedSourceKind::Directory => {
            validate_dir(&path)?;
            ChangeSet::from_directory(&path).map_err(Into::into)
        }
        ResolvedSourceKind::Diff => load_unified_diff(&path),
        ResolvedSourceKind::Script => ChangeSet::from_ddl_script(&path).map_err(Into::into),
        ResolvedSourceKind::ChangesetJson => load_changeset_json(&path),
    }
}

fn resolve_source_kind(source_kind: Option<SourceKindArg>, path: &Path) -> ResolvedSourceKind {
    match source_kind.unwrap_or(SourceKindArg::Auto) {
        SourceKindArg::Auto => infer_source_kind(path),
        SourceKindArg::Directory => ResolvedSourceKind::Directory,
        SourceKindArg::Diff => ResolvedSourceKind::Diff,
        SourceKindArg::Script => ResolvedSourceKind::Script,
        SourceKindArg::ChangesetJson => ResolvedSourceKind::ChangesetJson,
    }
}

fn infer_source_kind(path: &Path) -> ResolvedSourceKind {
    if path.is_dir() {
        return ResolvedSourceKind::Directory;
    }
    match path.extension().and_then(|ext| ext.to_str()) {
        Some(ext)
            if matches!(
                ext.to_ascii_lowercase().as_str(),
                "diff" | "patch" | "udiff"
            ) =>
        {
            ResolvedSourceKind::Diff
        }
        Some(ext) if ext.eq_ignore_ascii_case("json") => ResolvedSourceKind::ChangesetJson,
        _ => ResolvedSourceKind::Script,
    }
}

fn load_unified_diff(path: &Path) -> Result<ChangeSet, CliError> {
    let raw = std::fs::read_to_string(path).map_err(|err| {
        CliError::new("changeset_read_failed", err.to_string())
            .with_path(path)
            .with_exit_code(2)
    })?;
    ChangeSet::from_unified_diff(path.display().to_string(), &raw).map_err(Into::into)
}

fn load_changeset_json(path: &Path) -> Result<ChangeSet, CliError> {
    let raw = std::fs::read_to_string(path).map_err(|err| {
        CliError::new("changeset_read_failed", err.to_string())
            .with_path(path)
            .with_exit_code(2)
    })?;
    serde_json::from_str(&raw).map_err(|err| {
        CliError::new("changeset_json_invalid", err.to_string())
            .with_path(path)
            .with_exit_code(2)
    })
}

fn load_doc_set(path: &Path) -> Result<plsql_doc::DocSet, CliError> {
    let raw = read_input_file(path, "docset_read_failed")?;
    parse_json_or_envelope(&raw).map_err(|err| {
        CliError::new("docset_json_invalid", err.to_string())
            .with_path(path)
            .with_exit_code(2)
    })
}

fn load_scan_report(path: &Path) -> Result<plsql_sast::ScanReport, CliError> {
    let raw = read_input_file(path, "scan_report_read_failed")?;
    parse_json_or_envelope(&raw).map_err(|err| {
        CliError::new("scan_report_json_invalid", err.to_string())
            .with_path(path)
            .with_exit_code(2)
    })
}

fn read_input_file(path: &Path, code: &'static str) -> Result<String, CliError> {
    std::fs::read_to_string(path).map_err(|err| {
        CliError::new(code, err.to_string())
            .with_path(path)
            .with_exit_code(2)
    })
}

fn parse_json_or_envelope<T>(raw: &str) -> Result<T, serde_json::Error>
where
    T: DeserializeOwned,
{
    if let Ok(envelope) = serde_json::from_str::<RobotJsonEnvelope<T>>(raw) {
        return Ok(envelope.payload);
    }
    serde_json::from_str(raw)
}

fn validate_dir(path: &Path) -> Result<(), CliError> {
    if path.is_dir() {
        Ok(())
    } else {
        Err(CliError::new(
            "directory_missing",
            format!("directory does not exist: {}", path.display()),
        )
        .with_path(path)
        .with_exit_code(2))
    }
}

fn load_lineage_impacts(paths: &[PathBuf]) -> Result<Vec<LineageResult>, CliError> {
    paths
        .iter()
        .map(|path| {
            let raw = std::fs::read_to_string(path).map_err(|err| {
                CliError::new("lineage_impact_read_failed", err.to_string())
                    .with_path(path)
                    .with_exit_code(2)
            })?;
            parse_lineage_result(&raw).map_err(|err| {
                CliError::new("lineage_impact_json_invalid", err.to_string())
                    .with_path(path)
                    .with_exit_code(2)
            })
        })
        .collect()
}

fn parse_lineage_result(raw: &str) -> Result<LineageResult, serde_json::Error> {
    if let Ok(envelope) = serde_json::from_str::<RobotJsonEnvelope<LineageResult>>(raw) {
        return Ok(envelope.payload);
    }
    serde_json::from_str(raw)
}

#[derive(Debug, Deserialize)]
struct CompileErrorFlagsDocument {
    compile_error_flags: Vec<ChangeImpactCompileErrorFlag>,
}

#[derive(Debug, Deserialize)]
struct CompileErrorFlagsEnvelope {
    payload: CompileErrorFlagsDocument,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum CompileErrorFlagsInput {
    Rows(Vec<ChangeImpactCompileErrorFlag>),
    Document(CompileErrorFlagsDocument),
    Envelope(CompileErrorFlagsEnvelope),
}

fn load_compile_error_flags(path: &Path) -> Result<Vec<ChangeImpactCompileErrorFlag>, CliError> {
    let raw = std::fs::read_to_string(path).map_err(|err| {
        CliError::new("compile_error_flags_read_failed", err.to_string())
            .with_path(path)
            .with_exit_code(2)
    })?;
    let input: CompileErrorFlagsInput = serde_json::from_str(&raw).map_err(|err| {
        CliError::new("compile_error_flags_json_invalid", err.to_string())
            .with_path(path)
            .with_exit_code(2)
    })?;
    Ok(match input {
        CompileErrorFlagsInput::Rows(rows) => rows,
        CompileErrorFlagsInput::Document(document) => document.compile_error_flags,
        CompileErrorFlagsInput::Envelope(envelope) => envelope.payload.compile_error_flags,
    })
}

#[derive(Debug, Deserialize)]
struct LineageMetadataDocument {
    objects: Vec<LineageMetadataRow>,
}

#[derive(Debug, Deserialize)]
struct LineageMetadataRow {
    logical_id: String,
    owner_symbol: Option<u64>,
    name_symbol: Option<u64>,
    owner: Option<String>,
    name: Option<String>,
    object_type: String,
    #[serde(default = "default_force_compile")]
    force_compile: bool,
}

fn default_force_compile() -> bool {
    true
}

fn load_lineage_metadata(
    path: &Path,
    changeset: &ChangeSet,
) -> Result<BTreeMap<String, LineageObjectMetadata>, CliError> {
    let raw = std::fs::read_to_string(path).map_err(|err| {
        CliError::new("lineage_metadata_read_failed", err.to_string())
            .with_path(path)
            .with_exit_code(2)
    })?;
    let doc: LineageMetadataDocument = serde_json::from_str(&raw).map_err(|err| {
        CliError::new("lineage_metadata_json_invalid", err.to_string())
            .with_path(path)
            .with_exit_code(2)
    })?;
    let mut interner = reserved_interner(changeset);
    let mut rows = doc.objects;
    rows.sort_by(|left, right| left.logical_id.cmp(&right.logical_id));
    let mut out = BTreeMap::new();
    for row in rows {
        let (owner, name) = row_to_object_symbols(&mut interner, &row).map_err(|err| {
            CliError::new("lineage_metadata_invalid", err)
                .with_path(path)
                .with_exit_code(2)
        })?;
        let logical_id = row.logical_id;
        out.insert(
            logical_id,
            LineageObjectMetadata::new(owner, name, row.object_type, row.force_compile),
        );
    }
    Ok(out)
}

fn row_to_object_symbols(
    interner: &mut SymbolInterner,
    row: &LineageMetadataRow,
) -> Result<(SchemaName, ObjectName), String> {
    match (row.owner_symbol, row.name_symbol) {
        (Some(owner), Some(name)) => Ok((
            SchemaName::new(SymbolId::new(owner)),
            ObjectName::new(SymbolId::new(name)),
        )),
        (None, None) => {
            let (owner_text, name_text) = owner_name_text(row)?;
            let owner = interner
                .intern_schema_name(owner_text)
                .ok_or_else(|| "symbol table overflow while interning owner".to_string())?;
            let name = interner
                .intern(name_text)
                .map(ObjectName::from)
                .ok_or_else(|| "symbol table overflow while interning object".to_string())?;
            Ok((owner, name))
        }
        _ => Err(
            "owner_symbol and name_symbol must either both be present or both be omitted"
                .to_string(),
        ),
    }
}

fn owner_name_text(row: &LineageMetadataRow) -> Result<(String, String), String> {
    if let (Some(owner), Some(name)) = (&row.owner, &row.name) {
        return Ok((owner.to_ascii_uppercase(), name.to_ascii_uppercase()));
    }
    let mut parts = row
        .logical_id
        .split('.')
        .map(str::trim)
        .filter(|part| !part.is_empty());
    let owner = parts
        .next()
        .ok_or_else(|| "logical_id is missing owner".to_string())?;
    let name = parts
        .next()
        .ok_or_else(|| "logical_id is missing object name".to_string())?;
    Ok((owner.to_ascii_uppercase(), name.to_ascii_uppercase()))
}

fn reserved_interner(changeset: &ChangeSet) -> SymbolInterner {
    let max_symbol = changeset
        .objects
        .iter()
        .flat_map(|object| [object.owner.symbol().get(), object.name.symbol().get()])
        .max()
        .unwrap_or(0);
    let mut interner = SymbolInterner::new();
    for index in 0..=max_symbol {
        let _ = interner.intern(format!("__reserved_{index}"));
    }
    interner
}

fn print_predict_human(envelope: &ChangeImpactEnvelope) {
    let summary = &envelope.payload.summary;
    println!(
        "predict: invalidations={} recompile={} uncertainties={} max_distance={}",
        summary.invalidation_count,
        summary.recompile_count,
        summary.uncertainty_count,
        summary.max_distance
    );
}

fn print_artifact(artifact: &Value) -> Result<(), CliError> {
    if let Some(text) = artifact.as_str() {
        print!("{text}");
        if !text.ends_with('\n') {
            println!();
        }
    } else {
        println!("{}", serde_json::to_string_pretty(artifact)?);
    }
    Ok(())
}

fn serialize_compact<T: serde::Serialize>(value: &T) -> Result<String, CliError> {
    serde_json::to_string(value).map_err(|err| {
        CliError::new("serialize_failed", err.to_string()).with_remediation(
            "file an issue with the command, schema id, and input shape that failed",
        )
    })
}

fn emit_error(robot_json: bool, err: &CliError) {
    eprintln!("plsql: {}", err.message);
    if !robot_json {
        return;
    }
    let payload = serde_json::json!({
        "kind": "error",
        "code": err.code,
        "message": err.message,
        "path": err.path.as_ref().map(|path| path.display().to_string()),
        "remediation": err.remediation,
    });
    let envelope = RobotJsonEnvelope::new(ERROR_ENVELOPE_SCHEMA, payload);
    if let Ok(json) = serde_json::to_string(&envelope) {
        println!("{json}");
    }
}
