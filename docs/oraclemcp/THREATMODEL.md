# Threat Model — oraclemcp

Scope: the `oraclemcp` server mediating between an AI agent (untrusted) and an
Oracle database. Framed around the plan's hard problems (§5) and risk register
(§16).

## Assets

- **The database** (integrity + confidentiality of customer data).
- **Credentials** (Oracle passwords / wallets / IAM tokens).
- **The audit trail** (must be durable + tamper-evident for the calls it covers).

## Trust boundaries

```
[ AI agent: UNTRUSTED ] --MCP(stdio|HTTPS)--> [ oraclemcp: semi-trusted ] --OracleNet--> [ Oracle DB: the real boundary ]
```

The **agent is the untrusted party** — its SQL is effectively attacker-controlled
input. The server is only as strong as the DB grants behind it.

## Adversaries & mitigations

| Adversary / threat | Mitigation |
|---|---|
| **Agent runs destructive SQL** (intentional or hallucinated) | Read-only default; fail-closed classifier; `max_level` ceiling; least-privilege DB user (the hard boundary). |
| **Obfuscated DML evades the classifier** (comment/CTE/`q'[]'`-hidden, side-effecting UDF in a SELECT, multi-statement desync) | Lexer-based literal/quote-aware splitter (desync → `Forbidden`); engine-aware purity consult (clear to `Safe` only on `ProvenReadOnly`); differential adversarial corpus + cargo-fuzz; never claimed a sandbox (R1, R15). |
| **Silent data corruption via wrong numbers** | NUMBER→string by default; canonical ISO/period serializer; type-fidelity golden tests (R11). |
| **Session-state bleed across pooled connections** | Session-lease primitive pins one physical session per unit of work; stateful ops without a lease are a structured error (R2). |
| **Privileged escalation without authorization** | Step-up human confirmation for every level change; `protected` profiles hard-reject escalation; OAuth scope can only lower the ceiling (R13). |
| **Allow-once token mistaken for a control** | Documented as friction-only; never a boundary (R3). |
| **Transport attacker (HTTP)** | TLS/HTTPS-only, reject non-loopback `http`; OAuth 2.1 resource-server validation (RFC 9728/8707/9207, PKCE); mTLS; origin checks / DNS-rebinding guard. |
| **Clock manipulation extends a token/window** | All TTLs on a monotonic clock; deserialized / prior-process-generation tokens are fail-closed expired (R: §5.10). |
| **Audit tampering / loss** | Out-of-band append-only sink (never the audited Oracle session); fsync-before-execute for Guarded+; monotonic-seq hash chain (R9). |
| **Standby/replica write attempt** | Standby auto-detection forces `READ_ONLY` and disables EXPLAIN-into-PLAN_TABLE. |
| **Malicious operator config** (virtual tools, login scripts) | Classified fail-closed at load; bind-only params; HMAC-signed on `protected`; refuse-to-load on `Forbidden` / over-ceiling. |
| **Third-party plugin = arbitrary code in-process** | No in-process `.so`; virtual tools then out-of-process/WASM only (R7). |
| **Pool exhaustion / DoS against shared prod** | Admission control + per-agent caps + fair queue + structured `BUSY` (R: §5.6). |

## Out of scope

- Device-based out-of-band 2FA (the gate is in-band human confirmation).
- Protecting against a fully-privileged DB user the operator deliberately
  configured in all-levels mode (the server boundary is weaker than DB grants;
  documented).
- Network-level attacks the OS/transport layer already addresses.
