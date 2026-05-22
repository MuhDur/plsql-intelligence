# Standard Engagement Contract Template

**PLSQL-CVT-001** — canonical Markdown source for the plsql-intelligence
engagement contract. PDF exports are rendered from this file via the
project's standard Markdown-to-PDF pipeline (Pandoc + the project's
LaTeX template); the Markdown source is the authoritative artefact and
the PDF is treated as a derived deliverable.

---

## Parties

This Engagement Agreement (the "**Agreement**") is entered into between:

* **Provider**: \<Provider Legal Name\>, a \<state/country\>-registered
  entity (the "**Provider**").
* **Customer**: \<Customer Legal Name\>, a \<state/country\>-registered
  entity (the "**Customer**").

Effective on the date of the last signature below (the "**Effective
Date**").

## 1. Scope

The Provider will deliver the **plsql-intelligence** static analysis
engagement, comprising:

1. One bounded analysis run against the Customer's PL/SQL corpus.
2. Delivery of the generated artefacts:
   * Change Impact Assessment report (Markdown + HTML).
   * Lineage report (Markdown + HTML + JSON envelope).
   * Bindings package (Rust source + auto-generated `Cargo.toml`).
   * Documentation site (Markdown + Docusaurus-compatible MDX).
3. Up to **one (1) follow-up working session** to walk through findings.

The scope above is **fixed**. New analyses, additional corpora, repeat
runs after corpus edits, and on-site work each require a separate
written change order.

## 2. No-Custom-Feature Clause

The Customer acknowledges that the engagement runs the **stock**
plsql-intelligence engine. The Provider:

* Will **not** author Customer-specific parser dialects, lineage
  heuristics, bindings layouts, or report formats.
* Will **not** modify the engine's source code as part of this
  engagement.
* Will accept feature requests as **product feedback** routed through
  the public issue tracker, not as deliverables under this Agreement.

The Provider commits to the stock engine's behaviour as documented in
the released `plsql-intelligence` package corresponding to the version
stamp recorded in §10 below.

## 3. On-Prem-Only Tooling Clause

All analysis runs against Customer source code execute **on Customer
infrastructure**.

* The Provider will not transmit any Customer source, schema metadata,
  bind values, or stack traces to third-party services.
* The plsql-intelligence engine ships as a **self-contained binary**
  with no required network egress for analysis (the optional
  `live-db` feature only contacts the Customer's own Oracle instance).
* Any optional cloud-hosted tooling the Provider offers separately is
  explicitly **outside** the scope of this engagement.

## 4. Redacted Repro Consent

The Provider may request a **support bundle** — a JSON artefact
produced by `plsql support export-bundle` — if a bug is found during
the engagement.

The Customer **consents in advance** to share such a bundle, subject to
all of the following:

* The bundle is generated **on-prem** by the Customer through the
  declared `RedactionManifest`. The Customer is solely responsible for
  the rules in that manifest.
* Every blob in the bundle ships with a
  `redactions_applied: <integer>` counter and a `sha256` of the
  *post-redaction* bytes. The Customer may verify the redaction
  count and content hash before sending.
* The bundle MAY be encrypted to a Provider-supplied age / PGP
  recipient (`plsql support encrypt-bundle`). Encryption is at the
  Customer's discretion.
* The Provider will use the bundle **solely** for diagnosing the
  reported issue and will retain it for no longer than ninety (90)
  days after issue closure.

## 5. Authority

The Customer represents and warrants that the individual signing this
Agreement has the authority to grant the access and disclose the
artefacts described above. The Provider operates entirely on
Customer-supplied inputs.

## 6. Fees

* **Fixed fee**: \<USD ###\>, payable in full within thirty (30) days
  of the Effective Date.
* **Travel and incidentals**: none. The engagement is delivered
  remotely. Optional on-site work is a separate engagement.
* **Re-runs**: any analysis run performed against a Customer corpus
  **after** the initial deliverables of §1 have been accepted requires
  a separate change order at the rate stated above.

## 7. Term and Termination

* **Term**: the Agreement begins on the Effective Date and terminates
  upon Customer acceptance of the artefacts described in §1, or
  ninety (90) days after the Effective Date, whichever is earlier.
* **Termination for convenience**: either party may terminate this
  Agreement with thirty (30) days written notice. Fees already
  invoiced are non-refundable.
* **Termination for cause**: either party may terminate immediately
  on a material breach that remains uncured for fourteen (14) days
  after written notice.

## 8. Confidentiality

Each party will hold the other party's confidential information in
strict confidence for two (2) years following termination. Customer
source code, schema names, and any support bundles shared under §4
constitute Customer Confidential Information.

## 9. Limitation of Liability

To the maximum extent permitted by applicable law, the Provider's
aggregate liability under this Agreement is capped at the total fees
paid by the Customer under §6. Neither party is liable for indirect,
incidental, or consequential damages.

## 10. Engine Version

The engagement is executed using the plsql-intelligence release
recorded below. The Provider will run the engagement against this
version (or, with the Customer's written agreement, a later
patch-level release). Major / minor upgrades require a new
engagement.

* **Engine version**: \<plsql-intelligence x.y.z\>
* **Engine SHA**: \<commit hash\>
* **Engine licence**: Apache-2.0 OR MIT (dual-licensed).

## 11. Acceptance Criteria

Deliverables are deemed accepted when, for each artefact in §1:

* The artefact has been delivered to the Customer's designated
  contact, AND
* The Customer has not raised a written non-conformance objection
  within ten (10) business days of delivery.

A non-conformance objection MUST identify the specific deliverable
and the specific clause of this Agreement the Customer believes is
not met.

## 12. Governing Law

This Agreement is governed by the laws of \<jurisdiction\>, without
regard to its conflict-of-laws principles.

---

## Signatures

**Provider** — \<Provider Legal Name\>

* Name: \_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_
* Title: \_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_
* Date: \_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_
* Signature: \_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_

**Customer** — \<Customer Legal Name\>

* Name: \_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_
* Title: \_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_
* Date: \_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_
* Signature: \_\_\_\_\_\_\_\_\_\_\_\_\_\_\_\_

---

## Appendix A — Deliverable Hashes

To be completed on delivery. Each artefact lists its SHA-256 so the
Customer can verify the artefact under §11 acceptance.

| Artefact | SHA-256 |
|---|---|
| Change Impact Assessment (Markdown) | `sha256:_______________________________` |
| Change Impact Assessment (HTML) | `sha256:_______________________________` |
| Lineage report (JSON envelope) | `sha256:_______________________________` |
| Bindings package (.tar.gz) | `sha256:_______________________________` |
| Documentation site (.tar.gz) | `sha256:_______________________________` |

---

*Template version 1.0 — owned by the plsql-intelligence Commercial
Validation Track (CVT). Update via the same review process as the
engine itself.*
