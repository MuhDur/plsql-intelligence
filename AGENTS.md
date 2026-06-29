# AGENTS.md — plsql-intelligence

Operating rules for agents working in this repository.

**This is a greenfield Rust project now in active implementation.** The working plan lives in [`plan.md`](plan.md); read it before doing anything substantive. If code and plan disagree, fix one or the other deliberately, never silently.

- **Project key** (for beads, agent mail, cass-memory): `plsql-intelligence`
- **Repo name on disk:** `oracle/` (not the same as the project key)
- **Language:** Rust (Cargo workspace). No JS/TS/Python in this repo.
- **Status:** Pre-GA implementation. Foundational crates exist; higher layers are still in progress.
- **Web access:** allowed. Use `WebFetch` / `WebSearch` for OSS docs, Oracle reference material, crate research, etc.

## Founder constraints (plan.md §3, non-negotiable)

- **C1/C2:** No ML training on private data; no private code through founder-owned inference infra.
- **C5/C6 + private estate:** Private PL/SQL test material lives in the directory named by the `PLSQL_PRIVATE_ESTATE` environment variable. Strictly local-only. Never publish, copy, or `git add` anything from that path into this repo or any other git repository. Treat it as pattern reference only; test cases mirroring its patterns are re-synthesized from grammar + description, never copied.
- **C7 (operational, supersedes plan.md §3 literal text):** Agents may commit and push via the git skills below. Never run destructive ops (`git reset --hard`, `git clean -fd`, `git push --force`, branch deletion, `git stash drop`, `rm -rf` on tracked paths) without explicit in-session approval.

If unsure whether an action would violate one of these, stop and ask.

## Plan rules to internalize (plan.md §4)

- **R10 / R11:** Every CLI ships `--robot-json` and a `doctor` subcommand.
- **R13:** No uncertainty is silently dropped; every blind spot becomes a typed `UnknownReason` with provenance.
- **R17:** No telemetry by default.
- **R20:** Parser backend isolation is mandatory; no downstream crate depends on ANTLR-generated types.

Full R-rule list and rationale: `plan.md` §4. The plan governs; this file is a pointer.

---

## RULE 1 – ABSOLUTE (DO NOT EVER VIOLATE THIS)

You may NOT delete any file or directory unless I explicitly give the exact command **in this session**.

- This includes files you just created (tests, tmp files, scripts, etc.).
- You do not get to decide that something is "safe" to remove.
- If you think something should be removed, stop and ask. You must receive clear written approval **before** any deletion command is even proposed.

Treat "never delete files without permission" as a hard invariant.

---

## IRREVERSIBLE GIT & FILESYSTEM ACTIONS

Absolutely forbidden unless I give the **exact command and explicit approval** in the same message:

- `git reset --hard`
- `git clean -fd`
- `rm -rf`
- Any command that can delete or overwrite code/data

Rules:

1. If you are not 100% sure what a command will delete, do not propose or run it. Ask first.
2. Prefer safe tools: `git status`, `git diff`, `git stash`, copying to backups, etc.
3. After approval, restate the command verbatim, list what it will affect, and wait for confirmation.
4. When a destructive command is run, record in your response:
   - The exact user text authorizing it
   - The command run
   - When you ran it

If that audit trail is missing, then you must act as if the operation never happened.

---

## Rust toolchain

- **Build system:** Cargo workspace. One crate per component (plan.md §6.2.1).
- **Toolchain:** `rust-toolchain.toml` uses `stable`; `Cargo.toml`
  declares the workspace `rust-version` floor. Default workspace
  builds, tests, clippy, and release binaries must stay stable-only.
  Any remaining `cargo +nightly-2026-05-11` command is transitional
  retirement work for still-unremoved live/MCP/codegen surfaces, not a
  precedent for new engine code.
- **Style:** `cargo fmt` (default config) + `cargo clippy -- -D warnings`. No exceptions.
- **Errors:** `miette` for human diagnostics, `thiserror` for library errors. No `anyhow` except `main()`.
- **Observability:** `tracing` with structured fields. Spans on every public API call.
- **Async/runtime model:** The offline engine is sync-first. Public
  APIs for parse, IR, depgraph, lineage, SAST, docs, bindgen, CI/CD
  prediction, catalog snapshot ingestion, and local analysis should be
  synchronous by default. Do not introduce `asupersync`, `oraclemcp-db`,
  live Oracle sockets, MCP serving, telemetry, or daemon/network runtime
  dependencies in this repo. Live Oracle I/O and MCP serving belong in
  the separate `oraclemcp` repository; this repo exposes stable library
  crates and CLIs for that consumer.
- **Parser codegen / Java:** Normal builds use committed ANTLR Rust
  sources and must not require Java. Java is needed only when
  intentionally regenerating parser output with `PLSQL_ANTLR_REGEN=1`
  or running the CI drift-check path.

---

## Git workflow

Use these skills when the situation matches their trigger conditions; for routine commits and pushes, plain git via Bash is fine.

- `gh-cli` — repos, issues, PRs, actions, releases
- `git-stash-janitor` — mine accumulated stashes for content worth landing before cleanup
- `git-worktree-branch-rationalization` — collapse sprawling worktrees and branches back to a canonical line
- `git-repo-janitor` — general repo hygiene

Standard flow: `git pull --rebase`, stage explicit paths, run quality gates, commit, push. Forbidden ops are listed under C7.

---

## Project Architecture

See `plan.md` §5 for the layer-based dependency graph (Layer 0 → Layer 5). Per-component design is in `plan.md` §6–§15. Do not re-document architecture here; update the plan when it changes.

---

## Repo Layout

Current high-level layout:

```
oracle/
├── Cargo.toml       # workspace manifest
├── plan.md          # authoritative specification
├── README.md        # public-facing landing
├── AGENTS.md        # this file
├── crates/          # implemented Rust workspace crates
├── corpus/          # corpus manifest + future fixtures
├── .beads/          # br issue tracker
├── .claude/         # Claude Code settings
└── .ntm/            # NTM swarm state
```

Longer-term target layout (`docs/`, `tools/`, additional crates, fuller corpus): `plan.md` §6.2.1.

---

## Generated Files — NEVER Edit Manually

<!-- CUSTOMIZE: If you have generated files, document them here -->

**Current state:** `crates/plsql-parser-antlr/src/generated/` contains
committed ANTLR outputs used by the normal `antlr-codegen` feature path so
engine builds do not require Java. Do not hand-edit files in that directory.
Regenerate them only with:

```bash
PLSQL_ANTLR_REGEN=1 cargo build -p plsql-parser-antlr --features antlr-codegen
```

For CI drift checks, regenerate to a scratch directory and compare against the
committed copy:

```bash
PLSQL_ANTLR_REGEN=1 \
PLSQL_ANTLR_REGEN_DIR=/tmp/plsql-parser-antlr-generated \
cargo build -p plsql-parser-antlr --features antlr-codegen
diff -ru crates/plsql-parser-antlr/src/generated /tmp/plsql-parser-antlr-generated
```

If/when you add generated artifacts:
- **Rule:** Never hand-edit generated outputs.
- **Convention:** Put generated outputs in a clearly labeled directory and document the generator command.

---

## Code Editing Discipline

- Do **not** run scripts that bulk-modify code (codemods, invented one-off scripts, giant `sed`/regex refactors).
- Large mechanical changes: break into smaller, explicit edits and review diffs.
- Subtle/complex changes: edit by hand, file-by-file, with careful reasoning.

---

## Backwards Compatibility & File Sprawl

We optimize for a clean architecture now, not backwards compatibility.

- No "compat shims" or "v2" file clones.
- When changing behavior, migrate callers and remove old code.
- New files are only for genuinely new domains that don't fit existing modules.
- The bar for adding files is very high.

---

## Console Output

- Prefer **structured, minimal logs** (avoid spammy debug output).
- Treat user-facing UX as UI-first; logs are for operators/debugging.

---

## MCP Agent Mail — Multi-Agent Coordination

Agent Mail is available as an MCP server for coordinating work across agents.

What Agent Mail gives:
- Identities, inbox/outbox, searchable threads.
- Advisory file reservations (leases) to avoid agents clobbering each other.
- Persistent artifacts in git (human-auditable).

Core patterns:

1. **Same repo**
   - Register identity:
     - `ensure_project` then `register_agent` with the repo's absolute path as `project_key`.
   - Reserve files before editing:
     - `file_reservation_paths(project_key, agent_name, ["src/**"], ttl_seconds=3600, exclusive=true)`.
   - Communicate:
     - `send_message(..., thread_id="FEAT-123")`.
     - `fetch_inbox`, then `acknowledge_message`.
   - Fast reads:
     - `resource://inbox/{Agent}?project=<abs-path>&limit=20`.
     - `resource://thread/{id}?project=<abs-path>&include_bodies=true`.

2. **Macros vs granular:**
   - Prefer macros when speed is more important than fine-grained control:
     - `macro_start_session`, `macro_prepare_thread`, `macro_file_reservation_cycle`, `macro_contact_handshake`.
   - Use granular tools when you need explicit behavior.

Common pitfalls:
- "from_agent not registered" → call `register_agent` with correct `project_key`.
- `FILE_RESERVATION_CONFLICT` → adjust patterns, wait for expiry, or use non-exclusive reservation.

---

## Landing the plane (session hand-off)

When ending a work session:

1. **Pull-rebase first**: `git pull --rebase`.
2. **Stage** relevant paths (`git add <paths>`, never `git add -A`, which risks pulling in private estate material if anything was symlinked).
3. **Run quality gates** if code changed: `cargo check`, `cargo clippy -- -D warnings`, `cargo test`, `ubs <changed files>`.
4. **Update beads**: close finished work, mark in-progress items, file new beads for follow-ups.
5. **Flush beads to JSONL** with `br sync --flush-only`, then stage `.beads/` alongside the code.
6. **Commit** with a concise message; **push** to the appropriate branch (never force-push to `main` or `master`).
7. **Summarize for the founder**: what changed, why, what was pushed, what's blocked.

Destructive ops listed in C7 stay forbidden without explicit approval.

---

## Issue Tracking with br (Beads)

All issue tracking goes through **Beads**. No other TODO systems.

Key invariants:

- `.beads/` is authoritative state and **must always be committed** with code changes.
- Do not edit `.beads/*.jsonl` directly; only via `br`.

### Basics

Check ready work:

```bash
br ready --json
```

Create issues:

```bash
br create "Issue title" -t bug|feature|task -p 0-4 --json
br create "Issue title" -p 1 --deps discovered-from:br-123 --json
```

Update:

```bash
br update br-42 --status in_progress --json
br update br-42 --priority 1 --json
```

Complete:

```bash
br close br-42 --reason "Completed" --json
```

Types: `bug`, `feature`, `task`, `epic`, `chore`

Priorities: `0` critical, `1` high, `2` medium (default), `3` low, `4` backlog

Agent workflow:

1. `br ready` to find unblocked work.
2. Claim: `br update <id> --status in_progress`.
3. Implement + test.
4. If you discover new work, create a new bead with `discovered-from:<parent-id>`.
5. Close when done.
6. Commit `.beads/` in the same commit as code changes.

### Plan-to-bead conversion

Before converting fresh bead seeds out of `plan.md` (or before merging any
plan.md change that adds or relocates bead-seed rows), run:

```bash
cargo run -p plan-lint -- --doctor
```

Resolve any new findings (or escalate to PLSQL-PLAN-003 if the drift is
already tracked) before invoking `br create`. CI also runs plan-lint on
every PR, but the in-the-loop check during conversion catches mistakes
before they bleed into the issue tracker (`PLSQL-PLAN-002`).

### Recurring /oracle skill audits

Before closing any catalog / parser-dialect / depgraph-edge / SAST-rule
work, run a `/oracle`-skill check against the matching reference in
your local Oracle reference skill (`~/.claude/skills/oracle/`, when
present):

- catalog work → `LOW-LEVEL-CATALOGS.md` + `DATABASE-REFERENCE.md`
- parser dialect features → `DATABASE-REFERENCE.md` + `SUPPORT-RELEASE-MATRIX.md`
- depgraph edge kinds → `LOW-LEVEL-CATALOGS.md` (ALL_DEPENDENCIES)
- SAST rules → `SECURITY-OPTIONS-REFERENCE.md` + `DATABASE-REFERENCE.md`
- bindings type map → `DATABASE-REFERENCE.md` + `OBJECT-TYPES-REFERENCE.md`

Cite the source line + reference section in the closure rationale. Six
substantive /oracle audits landed during the 2026-05-15 session
(catalog, L2 corpus, depgraph taxonomy, parser version coverage, MCP
surface, bindgen type map). That cadence is the floor, not a ceiling.

Never:
- Use markdown TODO lists.
- Use other trackers.
- Duplicate tracking.

---

## Using bv as an AI sidecar

bv is a graph-aware triage engine for Beads projects. Use robot flags for deterministic outputs.

**⚠️ CRITICAL: Use ONLY `--robot-*` flags. Bare `bv` launches an interactive TUI that blocks your session.**

```bash
bv --robot-triage        # THE MEGA-COMMAND: start here
bv --robot-next          # Just the single top pick + claim command
bv --robot-plan          # Parallel execution tracks
bv --robot-insights      # Full graph metrics
```

Use bv instead of parsing beads.jsonl; it computes PageRank, critical paths, cycles, and parallel tracks deterministically.

---

## cass — Cross-Agent Search

`cass` indexes prior agent conversations so we can reuse solved problems.

**Rules:** Never run bare `cass` (TUI). Always use `--robot` or `--json`.

```bash
cass health
cass search "authentication error" --robot --limit 5
cass view /path/to/session.jsonl -n 42 --json
```

Treat cass as a way to avoid re-solving problems other agents already handled.

---

## Memory System: cass-memory

Before starting complex tasks, retrieve relevant context:

```bash
cm context "<task description>" --json
```

This returns:
- **relevantBullets**: Rules that may help with your task
- **antiPatterns**: Pitfalls to avoid
- **historySnippets**: Past sessions that solved similar problems

Protocol:
1. **START**: Run `cm context "<task>" --json` before non-trivial work
2. **WORK**: Reference rule IDs when following them
3. **END**: Just finish your work. Learning happens automatically.

---

## UBS Quick Reference

**Golden Rule:** `ubs <changed-files>` before every commit. Exit 0 = safe. Exit >0 = fix & re-run.

```bash
ubs file.ts file2.py                    # Specific files (< 1s) — USE THIS
ubs $(git diff --name-only --cached)    # Staged files — before commit
ubs .                                   # Whole project
```

**Speed Critical:** Scope to changed files. `ubs src/file.ts` (< 1s) vs `ubs .` (30s).

**Bug Severity:**
- **Critical** (always fix): Null safety, XSS/injection, async/await, memory leaks
- **Important** (production): Type narrowing, division-by-zero, resource leaks
- **Contextual** (judgment): TODO/FIXME, console logs
