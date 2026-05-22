# Intake Checklist + ICP Qualifier — One-Pager

**PLSQL-CVT-003** — pre-engagement screen for the plsql-intelligence
engagement (PLSQL-CVT-001). Run through this checklist with the
prospective Customer before drafting the contract. Items in **bold**
are hard gates; items in *italics* are advisory.

The checklist is intentionally a one-pager so it fits on a printed
sheet during a 30-minute intake call.

---

## A. Ideal Customer Profile (ICP) qualifier

A prospective Customer is **in profile** when **all four** of the
following are true:

* [ ] **PL/SQL surface area is non-trivial.** The Customer owns at
      least one Oracle PL/SQL codebase with ≥ 50 packages **or** ≥
      5,000 lines of stored PL/SQL.
* [ ] **Change cadence is real.** The Customer ships PL/SQL changes
      at least monthly (or has an active migration project that will
      ship multiple changesets in the next quarter).
* [ ] **Static analysis is the right tool.** The Customer's question
      is "what breaks if I change X", "what does X depend on", or
      "give me Rust bindings to call X". If the question is "make
      my queries faster" or "rewrite my schema", redirect to other
      offerings.
* [ ] **They can run a binary on their own infrastructure.** No
      cloud-only deployment requirement; no air-gap that would
      block downloading a self-contained binary; no policy that
      blocks running compiled code outside the DB.

*Soft signals worth recording but not gating:*

* *Customer has CI/CD pipeline for PL/SQL.*
* *Customer has an Oracle 19c-or-later target (parser dialect
  coverage is best from 19c onward — see
  `~/.claude/skills/oracle/SUPPORT-RELEASE-MATRIX.md`).*
* *Customer has previous experience with Oracle ADTs, packages w/
  ACCESSIBLE BY, or PL/Scope.*

If **any** hard gate is `No`, route the prospect to the public-doc
self-serve path. Do **not** open an engagement file.

---

## B. Intake checklist

Run sequentially. Stop at the first **Fail** and resolve before
proceeding.

### B.1. Authority

* [ ] **Identified the signing party** with authority to grant the
      access described in PLSQL-CVT-001 §5.
* [ ] **Confirmed the data-handling policy** allows on-prem
      analysis of PL/SQL source.
* [ ] **Captured the engagement primary contact** + escalation
      contact.

### B.2. Corpus

* [ ] **Confirmed the corpus is checked into the Customer's VCS**
      (git / Perforce / Subversion).
* [ ] **Counted the surface**: packages, procedures, functions,
      views, triggers, types — record the totals in the engagement
      file.
* [ ] **Identified wrapped packages** (`looks_wrapped` per
      PLSQL-WS-009). The engagement will surface these as
      `UnknownReason::WrappedSource` (R13).
* [ ] **Recorded the Oracle target version** (11g / 12c / 19c /
      21c / 23ai / 26ai). The parser dialect routing depends on
      this.

### B.3. Operational

* [ ] **Workspace access**: ssh / VPN / Citrix as required.
* [ ] **Engine version pinned** — record the exact
      `plsql-intelligence x.y.z` + commit SHA the engagement will
      run.
* [ ] **Output channel agreed**: email / Slack / git push /
      Customer issue tracker.
* [ ] **Retention horizon agreed** — defaults to 90 days post
      acceptance (per PLSQL-CVT-001 §4).

### B.4. Risk

* [ ] **Reviewed the engagement risk tier** (Safe / Caution /
      Destructive). Destructive engagements (touching production
      DDL) require a written escalation acknowledgement from the
      Customer.
* [ ] **Reviewed the redaction posture** — Customer is responsible
      for the rules in their `RedactionManifest`.
* [ ] **Confirmed there is no private corpus**
      being passed through this engagement (the engine's CI keeps a
      hard rule against publishing private corpora into the public
      tree).

### B.5. Commercial

* [ ] **Fixed fee agreed in writing.** Record on the engagement file.
* [ ] **Acceptance criteria copy-pasted from PLSQL-CVT-001 §11**.
      No bespoke acceptance language.
* [ ] **No-custom-feature clause read aloud.** Customer
      acknowledged that feature requests route through the public
      issue tracker, not the engagement.

---

## C. Decision

After all of A and B pass:

* [ ] **GO** — draft the engagement contract from
      `engagement-contract-template.md`, render the PDF, schedule
      kickoff.
* [ ] **NOGO** — record the failing item(s); route the prospect to
      the public-doc self-serve path; close the intake.

---

## D. Engagement file artefacts

When the answer is **GO**, the engagement file (private,
operational) must contain:

* [ ] Signed PDF of `engagement-contract-template.md`.
* [ ] Engine version stamp (x.y.z + SHA).
* [ ] Corpus inventory (B.2 totals).
* [ ] Risk tier (B.4) + escalation acknowledgement if Destructive.
* [ ] Redaction manifest source-of-truth pointer (Customer-owned).
* [ ] Output channel + retention horizon (B.3).

A separate **operational discipline checklist**
(PLSQL-CVT-004 — referenced for completeness) lives in the private
operational repo and is **not** committed to this public-doc tree.

---

*One-pager version 1.0. Update via the same review process as the
engine code. /oracle anchors:*

* *`DATABASE-REFERENCE.md` for the Oracle-release-family lens
  applied at B.2 "target version".*
* *`SUPPORT-RELEASE-MATRIX.md` for the 19c-baseline reasoning
  behind the soft-signal in section A.*
* *`LOW-LEVEL-CATALOGS.md` Data Dictionary View Families — the
  corpus-counting step in B.2 maps directly to `ALL_OBJECTS`
  / `ALL_PROCEDURES` / `ALL_TRIGGERS` if the Customer wants to
  verify the totals server-side.*
