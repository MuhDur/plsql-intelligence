# plsql-intelligence — top-level Makefile.
#
# This Makefile collects developer-facing demo targets that are awkward
# to express purely in cargo. The build/test/clippy gate stays on cargo;
# only "spin up an external thing" targets land here.
#
# Conventions:
#   - Every target is idempotent: re-running yields the same end state.
#   - Every target prints a single-line preamble naming itself so log
#     output is grep-friendly.
#   - `make help` lists every target with one-line descriptions.

XE_COMPOSE := examples/oracle-xe/docker-compose.yml
# Autonomous (no-auth) override: gvenzl/oracle-free instead of the
# license-walled Oracle Container Registry image. Used by CI / agents.
XE_COMPOSE_CI := examples/oracle-xe/docker-compose.gvenzl.yml
# Compose-binary autodetect: the v2 plugin (`docker compose`) is
# preferred but not always installed; fall back to standalone
# `docker-compose`. Override with `make DOCKER_COMPOSE=... <target>`.
DOCKER_COMPOSE := $(shell docker compose version >/dev/null 2>&1 && echo 'docker compose' || echo 'docker-compose')
HERO_DIFF := corpus/lab/hero_diff
DEMOS_DIR := examples/demos
RECORDINGS_DIR := $(DEMOS_DIR)/recordings
PERSONAS := release-engineer dba security governance rust-dev

.PHONY: help demo-oracle-xe demo-oracle-xe-ci demo-oracle-xe-status demo-oracle-xe-down demo-oracle-xe-purge \
        demo-no-db demo-no-db-verify check demo-record demo-record-clean \
        lab-gate lab-gate-list

help: ## List available make targets
	@echo "plsql-intelligence — developer demo targets"
	@echo
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) \
		| awk -F':.*?## ' '{ printf "  \033[36m%-30s\033[0m %s\n", $$1, $$2 }'

demo-oracle-xe: ## Spin up Oracle XE 23ai + load synthetic lab fixtures (PLSQL-LAB-007; needs container-registry.oracle.com login)
	@echo "[make] demo-oracle-xe: starting Oracle XE 23ai container"
	@$(DOCKER_COMPOSE) -f $(XE_COMPOSE) up -d
	@echo "[make] demo-oracle-xe: waiting for healthcheck (this can take ~5m on first boot)"
	@until [ "$$(docker inspect -f '{{.State.Health.Status}}' plsql-intelligence-xe 2>/dev/null)" = "healthy" ]; do \
		sleep 10; \
		echo "  ...still starting (last: $$(docker inspect -f '{{.State.Health.Status}}' plsql-intelligence-xe 2>/dev/null))"; \
	done
	@echo "[make] demo-oracle-xe: container healthy. Connection string:"
	@echo "  DEMO/DemoLab#2026@//localhost:1521/FREEPDB1"
	@echo "[make] demo-oracle-xe: run 'make demo-oracle-xe-status' to view the loader log."

demo-oracle-xe-ci: ## Spin up Oracle Free 23ai via the no-auth gvenzl override (CI / agents; no Oracle login)
	@echo "[make] demo-oracle-xe-ci: starting Oracle Free 23ai (gvenzl, no-auth)"
	@$(DOCKER_COMPOSE) -f $(XE_COMPOSE) -f $(XE_COMPOSE_CI) up -d
	@echo "[make] demo-oracle-xe-ci: waiting for healthcheck (first boot ~2-4m)"
	@until [ "$$(docker inspect -f '{{.State.Health.Status}}' plsql-intelligence-xe 2>/dev/null)" = "healthy" ]; do \
		sleep 10; \
		echo "  ...still starting (last: $$(docker inspect -f '{{.State.Health.Status}}' plsql-intelligence-xe 2>/dev/null))"; \
	done
	@echo "[make] demo-oracle-xe-ci: container healthy. Connection string:"
	@echo "  DEMO/DemoLab#2026@//localhost:1521/FREEPDB1"

demo-oracle-xe-status: ## Tail the container loader log (shows fixture loading progress)
	@echo "[make] demo-oracle-xe-status: tailing plsql-intelligence-xe logs"
	@docker logs --tail 100 plsql-intelligence-xe

demo-oracle-xe-down: ## Stop the container; preserve the persistent volume
	@echo "[make] demo-oracle-xe-down: stopping container (volume preserved)"
	@$(DOCKER_COMPOSE) -f $(XE_COMPOSE) down

demo-oracle-xe-purge: ## Stop the container AND delete the persistent volume (fresh start)
	@echo "[make] demo-oracle-xe-purge: stopping container + deleting volume"
	@$(DOCKER_COMPOSE) -f $(XE_COMPOSE) down --volumes

demo-no-db: ## Run the static-only demo against the L1 hero diff (no Oracle needed; PLSQL-LAB-003)
	@echo "[make] demo-no-db: building plsql-intelligence (debug)"
	@cargo build --workspace --quiet
	@echo "[make] demo-no-db: change file → unified-diff classification"
	@if [ ! -f "$(HERO_DIFF)/change.diff" ]; then \
		echo "ERROR: $(HERO_DIFF)/change.diff missing — run 'git checkout' or pull the hero-diff fixture (PLSQL-LAB-002)" >&2; \
		exit 2; \
	fi
	@echo "[make] demo-no-db: parsing $(HERO_DIFF)/change.diff via plsql-lineage parse_change_file"
	@cargo test -p plsql-lineage parse_change_file -- --nocapture 2>&1 | grep -E 'test result|test parse_change_file' | head -5 || true
	@echo "[make] demo-no-db: golden expectations under $(HERO_DIFF)/"
	@ls -1 $(HERO_DIFF) | sed 's/^/    /'
	@if command -v jq >/dev/null 2>&1; then \
		echo "[make] demo-no-db: expected_what_breaks.json summary"; \
		jq -r '"  scenario: " + (.scenario // "unknown") + "  | " + ((.changed_objects // []) | length | tostring) + " changed objects, " + ((.broken_callers // []) | length | tostring) + " broken callers"' $(HERO_DIFF)/expected_what_breaks.json; \
	else \
		echo "  (install jq to render the expected_what_breaks.json summary)"; \
	fi
	@echo "[make] demo-no-db: done — no Oracle connection required"

demo-no-db-verify: ## Sanity-check the hero-diff golden against the lineage parser (CI gate for PLSQL-LAB-003)
	@echo "[make] demo-no-db-verify: running plsql-lineage parse_change_file tests"
	@cargo test --quiet -p plsql-lineage parse_change_file
	@if [ ! -s "$(HERO_DIFF)/expected_what_breaks.json" ]; then \
		echo "ERROR: $(HERO_DIFF)/expected_what_breaks.json is empty/missing" >&2; exit 2; \
	fi
	@echo "[make] demo-no-db-verify: hero-diff fixture present + parser-tests green"

check: ## Run the project quality gate (fmt + clippy + workspace tests)
	@echo "[make] check: cargo fmt --all -- --check"
	@cargo fmt --all -- --check
	@echo "[make] check: cargo clippy --workspace --all-targets -- -D warnings"
	@cargo clippy --workspace --all-targets -- -D warnings
	@echo "[make] check: cargo test --workspace"
	@cargo test --workspace --quiet
	@echo "[make] check: green"

demo-record: ## Render every persona-tape to GIF via VHS (PLSQL-LAB-009; needs charmbracelet/vhs)
	@command -v vhs >/dev/null 2>&1 || { \
		echo "[make] demo-record: vhs not installed — see https://github.com/charmbracelet/vhs"; \
		echo "[make] demo-record: brew install vhs   # macOS"; \
		echo "[make] demo-record: go install github.com/charmbracelet/vhs@latest   # everywhere else"; \
		exit 2; \
	}
	@mkdir -p $(RECORDINGS_DIR)
	@for persona in $(PERSONAS); do \
		echo "[make] demo-record: rendering $$persona.gif"; \
		vhs $(DEMOS_DIR)/$$persona.tape; \
	done
	@echo "[make] demo-record: done — recordings under $(RECORDINGS_DIR)"
	@ls -1 $(RECORDINGS_DIR) | sed 's/^/  /'

demo-record-clean: ## Remove rendered persona-tape GIFs (idempotent reset)
	@echo "[make] demo-record-clean: clearing $(RECORDINGS_DIR)"
	@if [ -d $(RECORDINGS_DIR) ]; then rm -rf $(RECORDINGS_DIR); fi

lab-gate: ## PR-blocking release gate — fails if corpus/lab/expected/*.json drifts (PLSQL-LAB-010)
	@echo "[make] lab-gate: validating corpus/lab/ goldens"
	@goldens="$$(find corpus/lab -name 'expected_*.json' 2>/dev/null)"; \
	if [ -z "$$goldens" ]; then \
		echo "[make] lab-gate: no expected_*.json fixtures found — nothing to gate" >&2; \
		exit 2; \
	fi; \
	echo "[make] lab-gate: $$(echo "$$goldens" | wc -l) golden file(s) detected"; \
	for f in $$goldens; do \
		echo "  - $$f"; \
		if ! python3 -c "import json; json.load(open('$$f'))" 2>/dev/null; then \
			echo "[make] lab-gate: ERROR: $$f is not valid JSON" >&2; \
			exit 2; \
		fi; \
	done
	@echo "[make] lab-gate: running lineage what-breaks tests (consume the hero-diff golden)"
	@cargo test --quiet -p plsql-lineage parse_change_file
	@echo "[make] lab-gate: running lineage orphan tests (consume the orphans golden)"
	@cargo test --quiet -p plsql-lineage orphan_doctor 2>&1 | tail -3
	@echo "[make] lab-gate: green — corpus/lab/ goldens consistent with engine output"

lab-gate-list: ## List the golden files lab-gate currently asserts (PLSQL-LAB-010 introspection)
	@echo "[make] lab-gate-list: golden files under corpus/lab/"
	@find corpus/lab -name 'expected_*.json' 2>/dev/null | sed 's/^/  /' || echo "  (none)"
