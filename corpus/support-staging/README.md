# Support-engagement staging directory

> **Internal-only.** Fixtures that arrive from a support engagement
> land here for **human review** before any move to `corpus/public/`
> or `corpus/synthetic/`. (`PLSQL-SUPPORT-015` / oracle-w7bs.)

## What goes here

A subdirectory per support engagement, named
`<YYYY-MM-DD>-<short-slug>/`:

```
corpus/support-staging/
├── 2026-05-15-acme-corp/
│   ├── README.md                # one paragraph: what broke, who reported
│   ├── original.sha256          # hash-only of the original input
│   ├── redacted/                # post-redaction sources
│   │   ├── pkg_billing.pks
│   │   └── pkg_billing.pkb
│   └── redaction_delta.json     # PLSQL-SUPPORT-014 manifest
└── …
```

## Required files

Every engagement directory under `corpus/support-staging/` MUST carry:

| File | Purpose |
|------|---------|
| `README.md` | Human-readable note: what the engagement was, who reviewed |
| `redaction_delta.json` | `RedactionDeltaManifest` proving every transformation applied (SUPPORT-014) |
| `redacted/` | Directory of post-redaction sources — same shape as the original tree |

The `corpus-license-check` tool fails the build if any subdirectory of
`corpus/support-staging/` is missing **either** `redaction_delta.json`
or `redacted/`. This is the CI gate the bead requires.

## What does NOT belong here

- Pre-redaction source from the customer (carry the `original.sha256`
  hash file only).
- PII, credentials, hostnames, customer schema names, real identifiers.
  The redaction pipeline (rename + scrub + rule-list) must have run
  first.
- Anything from the private estate directory named by
  `$PLSQL_PRIVATE_ESTATE` (AGENTS.md C5/C6 — strictly local, never
  published).

## Promotion path

```
support engagement → corpus/support-staging/<dated-slug>/  (CI-gated)
                  → human review (manual)
                  → corpus/public/ (with manifest entry) OR corpus/synthetic/
```

Files do NOT auto-promote. A human reviewer flags the engagement entry
in the corpus manifest with `provenance = "support-engagement"` when
promoting.

## Pointers

- `crates/plsql-support/src/redaction_delta.rs` — the manifest type.
- `crates/plsql-support/src/rename.rs` — token-level identifier rename.
- `crates/plsql-support/src/scrub_literals.rs` — literal scrubbing.
- `tools/corpus-license-check/src/main.rs` — enforces this directory
  layout in CI.
- `plan.md` §16 — Support workflow.
- `AGENTS.md` C5/C6 — private estate data is strictly local.
