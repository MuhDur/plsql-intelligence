# Commercial Validation Track (CVT)

This directory holds the **plsql-intelligence** commercial-track
artefact templates. Each Markdown file is the authoritative source;
PDF / Word renderings are derived via the project's standard
Pandoc-based pipeline. The Markdown form is what reviewers diff in
PRs.

## Templates

| File | Bead | Purpose |
|---|---|---|
| [`engagement-contract-template.md`](./engagement-contract-template.md) | `PLSQL-CVT-001` | Standard engagement agreement — fixed scope, no-custom-feature clause, on-prem-only tooling clause, redacted-repro consent. |
| [`change-impact-report-template.md`](./change-impact-report-template.md) | `PLSQL-CVT-002` | Change Impact Assessment shape spec — Cover / Trust Block / Changeset / Predicted Invalidations / Deployment Plan / Uncertainties / Recompile / Call-Graph + Table Usage / Lineage Verdict / AUDIT block. |
| [`intake-checklist.md`](./intake-checklist.md) | `PLSQL-CVT-003` | Pre-engagement one-pager: 4-question ICP qualifier + 5-section intake checklist (Authority / Corpus / Operational / Risk / Commercial) + GO/NOGO decision + engagement-file artefact list. |
| [`engagement-file-spec.md`](./engagement-file-spec.md) | `PLSQL-CVT-004` | **Public spec for the private engagement-file structure.** Directory layout (7 sub-folders), 7-phase operational discipline checklist, 5 non-negotiable rules. Filled-in instances stay in the private ops repo, NEVER in this tree. |

## Rendering to PDF

```sh
pandoc \
  --from gfm \
  --to latex \
  --output engagement-contract.pdf \
  --metadata title="plsql-intelligence Engagement Agreement" \
  engagement-contract-template.md
```

The project's LaTeX template (when shipped) lands in the same
directory under `templates/`; for now any Pandoc default template
will produce a serviceable PDF.

## Update Policy

* Each template carries a version footer (`Template version x.y`).
* Material changes (scope, clauses) bump the **major** version.
* Typo fixes and clarifications bump the **minor**.
* Every change goes through the same PR review process as engine
  code.

## /oracle anchor

Routing for the commercial track itself is **not** an /oracle topic
— there is no Oracle reference doc that governs engagement legal
terms. The engine's behaviour the contract refers to (the support
bundle, redaction, encryption, version stamp) is anchored in:

* `~/.claude/skills/oracle/DATABASE-REFERENCE.md` for the PL/SQL
  Language Reference baseline.
* `~/.claude/skills/oracle/LOW-LEVEL-CATALOGS.md` for the
  `ALL_*` / `USER_*` view families the analysis consumes.
* `~/.claude/skills/oracle/SUPPORT-RELEASE-MATRIX.md` for the
  release-family lens the §10 Engine Version clause defers to.
