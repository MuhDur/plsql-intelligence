# `corpus/lab/orphans/` — orphan-vs-not-orphan fixture

L2 lab fixture covering `PLSQL-LIN-019` / `PLSQL-LIN-022`. Used by
end-to-end tests to validate `plsql_lineage::detect_orphans` produces
the right tier classification.

## Packages

| File pair | Role | Expected tier |
|---|---|---|
| `pkg_actively_used.{pks,pkb}` | Called by `pkg_caller` | **NOT an orphan** |
| `pkg_caller.{pks,pkb}` | References `pkg_actively_used` | Root caller (test-harness choice) |
| `pkg_legacy_purge.{pks,pkb}` | Standalone, references nothing | `HighConfidenceUnused` |
| `pkg_likely_orphan.{pks,pkb}` | Reads `event_log` but nothing calls it | `LikelyUnused` |

## Golden expected report

`expected_orphans.json` carries the canonical `OrphanCandidatesReport`
shape that `detect_orphans` should produce against a depgraph built
from this fixture. Schema id `plsql.lineage.orphan_candidates` v1.0.0.

## Notes

- All files are synthetic, authored fresh for this project — no private
  estate reference (C5/C6).
- Patterns derived from grammar + plan §13.8 orphan-candidates spec.
- Manifest entries are not required: `corpus/lab/` is excluded from
  `corpus-license-check` enforcement (only `corpus/public/` is gated).
