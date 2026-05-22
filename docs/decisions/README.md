# Decisions — plsql-intelligence

This directory tracks D-decisions from `plan.md` §23 as they are operationalized.
Each open decision blocks at least one implementation bead; closing a decision
unblocks that work and is recorded as a separate file here.

## Index

| D# | Title | Status | Filename |
|----|-------|--------|----------|
| D1 | Parser backend selection (antlr-rust vs Java ANTLR worker) | Closed | [D1-backend-tournament-result.md](D1-backend-tournament-result.md) — GO `antlr4rust`; `java-antlr` fallback NO-GO until flip-criteria met (PLSQL-PARSE-000C) |
| D8 | License model for paid tiers (FSL vs BSL vs permissive-plus-services) | Open | — |
| D11 | Customer-defined SAST rule SDK exposure timing | Closed | "Not in first release" (plan §2.3) |
| D19 | Pre-GA `0.x.y` release cadence | Closed | "GA is 1.0, 0.x permitted for foundation-OSS adoption" (plan §1.4) |
| D20 | LSP / IDE integration | Open (deferred) | "`plsql-output` schemas don't preclude it; no LSP server in first release" |

## Authoring conventions

- One file per decision, slug `D<n>-<short-kebab-slug>.md`.
- Each file documents: options considered, tradeoffs, evidence, recommendation, and (when adopted) the closing rationale.
- Status states: `Open`, `Adopted`, `Rejected`, `Superseded by D<m>`.
- A decision file MUST cite the plan section that introduced or amended it.

## Operationalization

Decisions are tracked as `area:decision-log` beads in Beads (see `PLSQL-DECISION-LOG-001`).
A decision moves from `Open` → `Adopted/Rejected` only via a written decision file in
this directory + a commit that closes the matching bead. Silent decisions are forbidden
(R13 corollary — uncertainty applies to design as much as to runtime analysis).

## Cross-references

- `plan.md` §23 — full D-decision list with current status flags.
- `docs/architecture.md` — layer-wise architectural overview that references the
  adopted decisions.
- `AGENTS.md` — repo operating rules; agents must not silently change a decision.
