# Engagement-File Structure + Operational Discipline Checklist

**PLSQL-CVT-004** — public specification for the **private**
engagement-file structure plsql-intelligence operators maintain
per Customer. This document describes the **shape**; the
**filled-in instances** live in a separate private repository
and are explicitly NOT committed to this public-doc tree.

> ⚠️ **DO NOT COMMIT** filled-in engagement files (with Customer
> names, schema names, or signed PDFs) anywhere in this repo. The
> repo's `.gitignore` MUST contain `engagements/` as a guard;
> never use that directory in this tree for actual engagement
> data.

---

## A. Directory layout

The private engagement file lives at
`<private-ops-repo>/engagements/<engagement-id>/`. Required
contents:

```text
engagements/<engagement-id>/
├── README.md                 # Engagement summary, status, dates
├── 01-contract/
│   ├── signed-contract.pdf   # PDF of CVT-001 contract (PRIVATE)
│   ├── signatures.txt        # Names + dates of signatures
│   └── change-orders/        # Any subsequent change orders
├── 02-intake/
│   ├── intake-checklist.md   # Filled-in copy of CVT-003 (PRIVATE)
│   └── icp-qualifier.md      # Sign-off on the four ICP gates
├── 03-corpus/
│   ├── inventory.md          # B.2 counts: pkgs/views/triggers/types
│   ├── manifest.toml         # `plsql-project.toml` contents
│   ├── redaction-manifest.toml # Customer's redaction rule source
│   └── wrapped-list.txt      # Output of `looks_wrapped` scan
├── 04-run/
│   ├── engine-version.txt    # x.y.z + commit SHA (CVT-001 §10)
│   ├── run.log               # Engine stdout/stderr
│   └── timestamps.txt        # start / end UTC, duration
├── 05-deliverables/
│   ├── change-impact.md      # Rendered CVT-002
│   ├── change-impact.html    # Same, HTML form
│   ├── lineage.json          # robot-JSON envelope
│   ├── bindings.tar.gz       # plsql-bindgen output
│   └── docs.tar.gz           # plsql-doc output
├── 06-acceptance/
│   ├── acceptance.md         # CVT-001 §11 acceptance record
│   └── sha256.txt            # Appendix A hashes
└── 07-support/
    ├── bundles/              # Any plsql-support export-bundle outputs
    └── incidents.md          # Any post-delivery issue tickets
```

## B. Operational discipline checklist

Run through these at each engagement phase. **Every** unchecked
item blocks transition to the next phase.

### B.1. Pre-kickoff (after CVT-003 intake)

* [ ] Engagement ID assigned (`<customer>-<YYYY-MM>-<seq>`).
* [ ] `engagements/<id>/` private directory created.
* [ ] `01-contract/signed-contract.pdf` present.
* [ ] `02-intake/intake-checklist.md` complete and signed off.

### B.2. Kickoff

* [ ] `04-run/engine-version.txt` pinned with the exact
      `plsql-intelligence x.y.z` + commit SHA.
* [ ] `03-corpus/redaction-manifest.toml` reviewed with
      Customer; Customer signs off on its rules.
* [ ] Output channel confirmed (email / Slack / git push /
      ticket).
* [ ] Engine run scheduled with start time agreed.

### B.3. During run

* [ ] Engine runs on Customer infrastructure (per CVT-001 §3).
* [ ] Provider does NOT exfiltrate Customer source.
* [ ] All artefacts emitted into `05-deliverables/`.
* [ ] Run log captured into `04-run/run.log`.

### B.4. Pre-delivery

* [ ] All five artefacts of CVT-001 §1 generated.
* [ ] `06-acceptance/sha256.txt` computed for each deliverable
      (`sha256sum *` form).
* [ ] Each deliverable opened and visually sanity-checked by
      the operator before sending.
* [ ] No customer-specific identifiers in this public-doc tree
      (`git grep -i '<customer-name>' docs/` returns nothing).

### B.5. Delivery

* [ ] Deliverables sent through the agreed output channel.
* [ ] `06-acceptance/acceptance.md` opens with the delivery
      timestamp and the 10-business-day clock per CVT-001 §11.

### B.6. Acceptance window

* [ ] Any non-conformance objection captured under
      `06-acceptance/`.
* [ ] If accepted, mark `acceptance.md` with the acceptance
      timestamp.
* [ ] If silently accepted (10 business days elapsed without
      objection), mark `acceptance.md` accordingly.

### B.7. Post-engagement

* [ ] Any `plsql support export-bundle` artefacts filed in
      `07-support/bundles/`.
* [ ] Retention horizon noted (CVT-001 §4: 90 days after
      closure unless otherwise agreed).
* [ ] Engagement summary updated in `README.md` with final
      status: Accepted / Rejected / Cancelled.

## C. Discipline rules

These are non-negotiable; every operator running an engagement
must honour them:

1. **No Customer code leaves Customer infrastructure** except
   through the agreed output channel and only for artefacts in
   CVT-001 §1. Source code is never transmitted to the
   Provider.
2. **Engagement files never enter the public repo.** The
   engagement-id directory lives only in the private ops repo.
   This public-doc spec is the only artefact that may reference
   the structure.
3. **Engine version is pinned at kickoff and not changed**
   without a change order (CVT-001 §10). If a patch-level
   upgrade is requested mid-run, document the upgrade in
   `04-run/engine-version.txt` with the change-order reference.
4. **Customer redaction manifest is authoritative.** The
   operator does not relax the rules.
5. **Support bundles** that the Customer sends per CVT-001 §4
   are kept in `07-support/bundles/` for at most 90 days after
   issue closure, then deleted with a record of deletion in
   `incidents.md`.

## D. Audit trail

Each entry in `04-run/` and `06-acceptance/` should include an
ISO-8601 UTC timestamp. The audit trail is what an external
reviewer (Customer auditor / regulator) walks if they ask "what
exactly happened on this engagement".

---

*Spec version 1.0. Update via the same review process as the
engine code. /oracle anchors: this is operational discipline,
not an /oracle topic. The Oracle behaviour the spec defers to
is anchored in DATABASE-REFERENCE.md (PL/SQL Language Reference
target version) and LOW-LEVEL-CATALOGS.md
(ALL_OBJECTS / ALL_PROCEDURES / ALL_TRIGGERS surface counts).*
