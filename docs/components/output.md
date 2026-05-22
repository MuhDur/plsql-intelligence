# plsql-output

Versioned wire envelopes for every analysis result the workspace emits.
Layer 0.

## Purpose

Every CLI in the workspace exposes a `--robot-json` flag (R10). That flag
flips the human-formatted text output to a stable JSON envelope shape that
downstream tools, MCP servers, and CI gates can parse without screen-scraping.
`plsql-output` is where the envelope shapes live.

## Surface

| Type | Purpose |
|------|---------|
| `SchemaDescriptor` | `{ id, version, description }` for a single payload schema |
| `SchemaVersion` | SemVer triple (`major.minor.patch`) for compatibility checks |
| `RobotJsonEnvelope<T>` | `{ schema_id, schema_version, payload: T }` |
| `DiagnosticEnvelope` | Standard wrapper for `Vec<Diagnostic>` (lints, parse errors) |
| `EvidenceEnvelope` | Standard wrapper for evidence records (UnknownReason instances) |
| `RedactionPolicy` | Sanitisation rules for support bundles |

## Conventions

- **Schema ID:** `plsql.<component>.<operation>` (e.g. `plsql.lineage.impact`).
- **Version bumps:** breaking changes increment `major`; additive fields
  increment `minor`. Consumers verify via `envelope.matches_schema(SCHEMA)`.
- **Each component owns its descriptors.** `plsql-lineage` defines
  `IMPACT_SCHEMA`, `DEPENDENCIES_SCHEMA`, …; `plsql-output` only owns the
  generic wrapper machinery.
- **All envelopes are `Serialize + Deserialize` and pin the schema ID +
  version inline** so the JSON is self-describing.

## Pointers

- Source: `crates/plsql-output/src/`
- Plan: `plan.md` §6.2, §4 (R10 — `--robot-json` mandate), §18 (cross-cutting
  concerns), §22 (verification)
- Consumers: every CLI surface
