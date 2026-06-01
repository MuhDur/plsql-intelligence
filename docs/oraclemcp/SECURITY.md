# Security Policy — oraclemcp

`oraclemcp` is a safe-by-default Oracle Database MCP server. This document
states the security model, what it does and does **not** guarantee, and how to
report a vulnerability.

## Reporting a vulnerability

Report privately to the maintainer (do **not** open a public issue for a
suspected vulnerability). Include a minimal reproduction and the affected
version/commit. You will get an acknowledgement; fixes are coordinated before
public disclosure.

## The security model (what protects your database)

oraclemcp is **read-only by default** and uses defense-in-depth. Effective
capability = **DB-privilege ceiling ∩ session operating level**. The boundaries,
strongest first:

1. **DB-level privilege ceiling (the only hard boundary).** A statement can
   never exceed the connecting Oracle user's actual grants. For shared/untrusted
   targets, connect with a least-privilege (read-only) user — then write/DDL/admin
   are impossible at the engine regardless of any server-side state.
2. **Per-target operating-level ceiling (`max_level`).** A `protected`
   (production) profile pins `max_level = READ_ONLY`, immutable for the life of
   the process — no token, confirmation, OAuth scope, or config reload can raise
   it. Over HTTP an OAuth scope can only *lower* the effective ceiling.
3. **Fail-closed statement classifier.** Every statement is classified before
   dispatch. Anything not provably read-only — any PL/SQL block, any statement
   that does not parse, any multi-statement desync, any user-defined function a
   `SELECT` calls that the engine cannot prove `ProvenReadOnly` — is treated as
   side-effecting (≥ Guarded). Dynamic SQL / `UTL_FILE` / outbound network /
   unconditional DDL inside PL/SQL is `Forbidden`.
4. **`SET TRANSACTION READ ONLY`** while the session level is `READ_ONLY`, so a
   misclassified *direct* DML still raises `ORA-01456` at the engine.
5. **Human step-up confirmation** for every level escalation (in-band MCP
   elicitation selector; **not** device 2FA).
6. **Durable, out-of-band, fsync-before-execute audit** with a tamper-evident
   hash chain for Guarded/Destructive/escalation calls.

## Honest caveats (what oraclemcp is NOT)

- **Not a sandbox.** The classifier reduces risk; it does not make destructive
  SQL impossible. The DB-level privilege model (boundary 1) is the real wall.
- **All-levels mode is weaker than a least-privilege user.** When you connect
  with a privileged account, the *server* (boundaries 2–6) is the boundary,
  which is weaker than boundary 1. Use a least-privilege user for prod.
- **The allow-once / preview token is friction, not a control.** The agent is
  the untrusted party and can self-issue it; it is UX + an audit artifact, never
  a security boundary. The real boundaries are the DB privilege ceiling and the
  human step-up confirmation.
- **`SET TRANSACTION READ ONLY` does not stop `AUTONOMOUS_TRANSACTION`
  side-effects** fired by triggers/VPD functions (they commit independently).
  The classifier's trigger/VPD walk is the defense; on a `protected` target the
  least-privilege user is the real boundary.

## Secrets

Credentials are referenced (`credential_ref`), never materialized into
`list_profiles` metadata or audit records. Audit entries store the SQL SHA-256 +
a truncated preview, never bind values or secrets. The production profile
default-denies plaintext passwords.

## Reproducible / auditable builds

The whole workspace is `#![forbid(unsafe_code)]`. CI runs `cargo clippy
-D warnings`, `cargo deny` (advisories/licenses/sources), and `cargo audit`. The
classifier ships with a differential adversarial corpus and a cargo-fuzz target.
