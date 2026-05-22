# Support → minimize-repro → corpus pipeline

> **Internal-only.** This doc describes how a customer-supplied repro
> moves from a support inbox into the redacted-and-vetted corpus. It
> is the runbook for `plsql support` operators. (`PLSQL-SUPPORT-016`
> / oracle-5c0q.)

## Why we minimize-and-redact

Customer-supplied PL/SQL almost always contains content we can't keep:
PII in literals, real schema names, table contents, customer-specific
package bodies that fall under their license. We need:

1. A **deterministic** redaction so two runs over the same input
   produce identical output — auditors can verify reproducibility.
2. A **provable** redaction so we can show what was removed without
   needing the original.
3. A **minimized** repro so the corpus stays small and CI-fast.

The pipeline below covers all three. Every step ships as code in
`crates/plsql-support/` and `tools/corpus-license-check/`.

## Stage 0 — intake

When a support engagement arrives:

1. Operator clones the customer's repro into a **local-only** scratch
   directory **outside the repo** — typically
   `~/scratch/<YYYY-MM-DD>-<customer-slug>/`.
2. **DO NOT** copy customer PL/SQL into the repo at any point under
   any name. AGENTS.md C5/C6: private estate data is strictly local, and
   the same policy applies to any customer-supplied source.
3. Hash the original input with `sha256sum`; record the hash for the
   `RedactionDeltaManifest::original_sha256` field later.

## Stage 1 — minimize the repro

Run the parser-level shrinker against the customer's repro:

```rust
use plsql_support::{Granularity, ReproOracle, shrink_with_chunks};

let result = shrink_with_chunks(&original_source, Granularity::Statement, |candidate| {
    // The ReproOracle returns true iff `candidate` still reproduces
    // the bug the customer is reporting. Provide whatever check
    // mirrors the engagement (parser panic, missing diagnostic,
    // wrong invalidation prediction, etc.).
    candidate_still_reproduces(candidate)
});
```

`shrink_with_chunks` and `shrink_lines` (in `plsql-support::shrink`)
are deterministic — same `(source, oracle)` always shrinks to the
same minimal output. For a parser panic, the typical sequence is:

1. Run `shrink_with_chunks` at `Granularity::Statement`.
2. Run `shrink_lines` over the remaining buffer for line-level
   minimization.
3. (Optional) Run `shrink_with_chunks` at `Granularity::Token` for the
   tightest possible repro — usually only worth it for parser fuzzes.

## Stage 2 — apply the redaction pipeline

Three passes run in canonical order. The `RedactionDeltaManifest`
records every step so the redaction is auditable:

```rust
use plsql_support::{
    DeltaConfig, RedactionManifest, ScrubThresholds, record_redaction_delta,
};

let manifest = record_redaction_delta(
    &minimized_source,
    &DeltaConfig {
        fixture_id: "2026-05-15-acme/pkg_billing.sql".into(),
        bundle_salt: "engagement-2026-05-15-acme".into(),
        rule_manifest: RedactionManifest {
            version: 1,
            rules: vec![/* customer-specific substring rules */],
        },
        scrub_thresholds: ScrubThresholds::default_thresholds(),
        reserved_identifiers: None,
    },
);
```

The three passes in order:

| # | Pass | Reads | Writes |
|---|------|-------|--------|
| 1 | `apply_rules` (RedactionManifest) | substring rule-list (customer names, hostnames, etc.) | rewrites literal hits |
| 2 | `scrub_literals` | `ScrubThresholds` | replaces long strings / numbers / date literals |
| 3 | `rename_identifiers` | `bundle_salt` (per-engagement) | rewrites every identifier to `id_<hex12>` |

Stricter posture: pass `ScrubThresholds { string_min_len: 0,
numeric_min_digits: 0, date_literals_scrubbed: true }` to scrub every
literal regardless of length. Use this when the bundle ships to an
external auditor.

## Stage 3 — land in `corpus/support-staging/`

The redacted output goes into the human-review staging area, NOT
directly into `corpus/public/` or `corpus/synthetic/`:

```
corpus/support-staging/
└── 2026-05-15-acme/
    ├── README.md                # ~3 lines: what broke, who reviewed
    ├── original.sha256          # the hash from Stage 0
    ├── redacted/
    │   └── pkg_billing.sql
    └── redaction_delta.json     # the Stage 2 manifest
```

The CI gate (`corpus-license-check`, PLSQL-SUPPORT-015) fails the
build if either `redaction_delta.json` or `redacted/` is missing from
any engagement subdirectory. This forces every support engagement
through the redaction pipeline.

## Stage 4 — human review

Manual gate. A second pair of eyes verifies:

1. The `redaction_delta.json` chain reproduces the redacted output
   bytewise (run `verify_redaction_delta`).
2. The redacted bytes contain no PII, no customer schema names, no
   hostnames, no credentials, no private estate IDs.
3. The `original.sha256` hash matches what the customer sent.
4. The minimization is tight — no extraneous packages / statements
   beyond what's needed to reproduce.

## Stage 5 — promote (or reject)

If review passes:

- Move the redacted files into `corpus/public/support-engagement/<dated-slug>/`.
- Add `[[file]]` entries to `corpus/manifest.toml` with
  `provenance = "support-engagement"` and the engagement-dated path.
- Reference the redaction delta manifest in the manifest entry's
  `notes` field.
- Open a follow-up bead if the engagement surfaced a new bug class.

If review fails:

- Move the engagement directory to `~/scratch/` for further redaction
  iteration.
- Document the failure mode in the engagement's README so future
  operators avoid it.

## Pointers

- `crates/plsql-support/src/shrink.rs` — minimization.
- `crates/plsql-support/src/rename.rs` — identifier rename.
- `crates/plsql-support/src/scrub_literals.rs` — literal scrubbing.
- `crates/plsql-support/src/redaction_delta.rs` — manifest.
- `tools/corpus-license-check/src/main.rs` — CI gate.
- `corpus/support-staging/README.md` — staging directory layout.
- `plan.md` §16 — Support workflow plan.
- `AGENTS.md` C5/C6 — strictly-local data policy.

## What can go wrong

- **Premature commit.** Operator stages customer PL/SQL into
  `corpus/` before running the redaction pipeline. The
  `corpus-license-check` CI gate catches this for files under
  `corpus/support-staging/` (it requires `redaction_delta.json`), but
  files staged elsewhere slip through. Operators MUST run the
  redaction pipeline before any `git add`.
- **Salt reuse across engagements.** If two engagements share a
  `bundle_salt`, an attacker who acquires both redacted bundles can
  correlate identifier aliases across them. Always use a per-engagement
  salt (the engagement directory's dated slug is a fine default).
- **Manifest pruning during minimize.** The minimizer can drop a
  statement that the redaction manifest's rule list assumed was
  present. After minimization, re-run the redaction pipeline against
  the new buffer — never carry over a stale `redaction_delta.json`.
