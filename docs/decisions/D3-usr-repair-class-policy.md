# D3: USR repair-class policy & the gate honesty manifest

> **Decision:** A USR candidate diff carries a mandatory, machine-checked
> **honesty manifest**. The §3 gate's G7 stage rejects any candidate
> whose manifest is absent, malformed, or claims a coverage gain not
> matched by a measured extraction rise. Repair-class `d` is the last
> resort and must stay honest.
>
> **Date:** 2026-05-19 · **Ref:** `PLSQL-USR-001` P4 · **Spec:** §2.1, §3.G7, §9

## Why this exists

Spec §3.G7 ("Anti-gaming + honesty") is the spine invariant I-NO-GAMING
made executable: *a coverage gain is valid only if accompanied by a
commensurate, measured rise in extracted semantics for the targeted gap
signature.* That inequality cannot be inferred from a raw diff; the
candidate must **declare** what it claims to resolve, and the gate
enforces the inequality against that declaration. An undeclared or
inconsistent claim is suppression by omission and is auto-rejected.

## The honesty manifest (candidate-diff directive lines)

A candidate diff is a unified diff. Lines beginning `# usr-gate:` are
the manifest. G7 requires exactly these keys:

| Key | Meaning | G7 rule |
|-----|---------|---------|
| `repair-class=` | `g` \| `l` \| `d` | must be one of the four spec tags |
| `signature=` | the targeted frozen gap signature | non-empty |
| `diagnostics-resolved=` | count of occurrences this patch removes | integer ≥ 0 |
| `extracted-semantics-delta=` | measured rise in extracted semantics (edges+facts) | integer |
| `posture=` | `preserved` \| `improved` | never `weakened`; `Clean`-where-uncertain ⇒ REJECT |
| `unknown-reason=` | (class `d` only) the *typed* `UnknownReason` variant the Unknown becomes | non-empty for class `d` |
| `golden-delta=` | (optional) a justified golden churn (consumed by G4) | free text |

### The enforced inequality (G7)

```
REJECT iff  diagnostics_resolved > 0  AND  extracted_semantics_delta < diagnostics_resolved
REJECT iff  posture == weakened
REJECT iff  repair_class == d  AND  unknown_reason is empty   (Unknown silenced, not typed)
```

A patch that drops diagnostics (`diagnostics_resolved > 0`) without a
commensurate extraction rise (`extracted_semantics_delta` below the
resolved count) is **suppression**, the exact `oracle-bh4p` dishonesty,
and dies at G7. A patch that resolves nothing (`diagnostics_resolved
== 0`, e.g. a pure refactor) is permitted iff posture is not weakened.

## Repair-class policy (when each lane is permitted)

- **`g` (grammar `.g4`):** real grammar work. Permitted when the gap is
  a genuine parser reach gap (`PARSE-ANTLR4RUST-001`). Slowest, soundest.
- **`l` (lowering/dispatch):** the grammar already parses the form but
  lowering does not classify/dispatch it (`IR_UNCLASSIFIED_DECL`,
  `IR_DDL_NOT_LOWERED`). Permitted when a tree-lower/lower extension
  raises extraction.
- **`d` (typed degradation), LAST RESORT:** only when the form is
  genuinely ambiguous Oracle dialect that we *choose* not to deep-parse.
  The Unknown MUST be replaced by a **typed, still-surfaced**
  `UnknownReason` (honest "we recognise this and choose not to
  deep-parse it, with the reason"), NEVER silenced. `d` may not be used to
  make a diagnostic disappear; it converts an untyped Unknown into a
  typed-known one. This is coverage *of honesty*, never of fabrication
  (spec §9).

## Immutability

The gate script is content-pinned (`crates/plsql-accretion/gate.sha256`);
`plsql-accretion::gate` aborts on sha mismatch. Changing the gate or this
policy requires a deliberate, human-reviewed commit + sha bump (mirrors
compliance `☖ STAKE-RUBRIC`). The bar does not move silently.
