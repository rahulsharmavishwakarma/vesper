SHELL := /bin/sh
.DEFAULT_GOAL := help

# Every recipe uses the same optional local Rust/CMake environment as development.
RUN = . ./scripts/dev-env.sh &&
UV ?= uv
UV_RUN = $(UV) run --locked
PYTHON = $(UV_RUN) python
PRE_COMMIT = $(UV_RUN) pre-commit
RUFF = $(UV_RUN) ruff
SHELLCHECK = $(UV_RUN) shellcheck
SHFMT = $(UV_RUN) shfmt
UV_CACHE_DIR ?= $(CURDIR)/.cache/uv
export UV_CACHE_DIR
HASH_PROBE := benchmarks/asr/hash-probe/Cargo.toml
PLAYWRIGHT_BROWSERS_PATH ?= $(CURDIR)/.cache/playwright
export PLAYWRIGHT_BROWSERS_PATH
MODEL_DIR ?= models/downloads
export MODEL_DIR
HF_REPO ?=
HF_WEIGHTS ?=
HF_REVISION ?= main
HF_OUTPUT ?= models/custom-profile.json
export HF_REPO HF_WEIGHTS HF_REVISION HF_OUTPUT

.PHONY: help setup setup-checks hooks precommit prepush workflows dev web model-profile build build-debug build-web check check-fast check-web test-tools test-python test-browser benchmark-contract version-check version-set container-build container-smoke fmt fmt-check lint test smoke-native models sherpa-runtime test-native-report
.PHONY: check-portable check-rust fmt-rust fmt-rust-check fmt-web fmt-web-check fmt-python fmt-python-check fmt-shell fmt-shell-check lint-rust lint-web lint-python lint-shell test-rust doc-rust
.PHONY: python-lock-check audit-python audit-python-audio-frontend audit-python-vibevoice audit-rust test-glib-backport

help: ## Show available commands
	@awk 'BEGIN { FS = ":.*## "; printf "Usage: make <target>\n\n" } /^[a-zA-Z_-]+:.*## / { printf "  %-14s %s\n", $$1, $$2 }' Makefile
	@printf '\nModel directory: make models MODEL_DIR="/path/to/models"\n'

setup: ## Install locked npm dependencies and fetch Rust dependencies
	$(RUN) npm ci
	$(RUN) cargo fetch --locked
	$(MAKE) sherpa-runtime

sherpa-runtime: ## Build and verify the pinned ASR runtime with completion reporting
	$(RUN) node scripts/sherpa-runtime.mjs

test-native-report: ## Test native completion, reset, cutoff and numerical-error reporting without weights
	$(RUN) sh scripts/test-native-report.sh

setup-checks: ## Install isolated Python tooling and the Chromium test browser
	$(UV) sync --locked
	npm exec playwright install chromium

python-lock-check: ## Verify Python manifests, lockfiles and compatibility exports
	$(UV) lock --check
	$(UV) lock --check --project benchmarks/asr/python/audio-frontend
	$(UV) lock --check --project benchmarks/asr/python/vibevoice
	@mkdir -p .cache
	$(UV) export --locked --project benchmarks/asr/python/audio-frontend --no-dev --no-emit-project --output-file .cache/audio-frontend-requirements.txt > /dev/null
	cmp benchmarks/asr/audio-frontend-requirements.txt .cache/audio-frontend-requirements.txt
	$(UV) export --locked --project benchmarks/asr/python/vibevoice --no-dev --no-emit-project --output-file .cache/vibevoice-reference-requirements.txt > /dev/null
	cmp benchmarks/asr/vibevoice-reference-requirements.txt .cache/vibevoice-reference-requirements.txt

audit-python: ## Audit every locked contributor dependency group without re-resolving it
	@mkdir -p .cache
	$(UV) export --locked --all-groups --no-emit-project --output-file .cache/python-audit-requirements.txt > /dev/null
	$(UV_RUN) pip-audit --strict --require-hashes --disable-pip -r .cache/python-audit-requirements.txt

audit-rust: ## Verify the GLib backport and run the strict RustSec audit
	$(RUN) $(PYTHON) -m unittest discover -s scripts -p test_rust_audit.py
	$(RUN) $(PYTHON) scripts/audit-rust.py

test-glib-backport: ## Run optimized GLib iterator regressions (requires system GLib)
	$(RUN) cargo test --release --locked --manifest-path tests/glib-backport/Cargo.toml

audit-python-audio-frontend: PYTHON_AUDIT_PROJECT = audio-frontend
audit-python-vibevoice: PYTHON_AUDIT_PROJECT = vibevoice
audit-python-audio-frontend: ## Audit the locked audio frontend environment on Apple-silicon macOS
audit-python-vibevoice: ## Audit the locked VibeVoice environment on Apple-silicon macOS
audit-python-audio-frontend audit-python-vibevoice:
	@$(PYTHON) -c 'import platform, sys; sys.exit(0 if (platform.system(), platform.machine()) == ("Darwin", "arm64") else "Benchmark audits require Apple-silicon macOS; other hosts skip their dependency markers.")'
	@mkdir -p .cache
	$(UV) export --locked --project benchmarks/asr/python/$(PYTHON_AUDIT_PROJECT) --no-dev --no-emit-project --output-file .cache/$(PYTHON_AUDIT_PROJECT)-audit-requirements.txt > /dev/null
	$(UV_RUN) pip-audit --strict --require-hashes --disable-pip -r .cache/$(PYTHON_AUDIT_PROJECT)-audit-requirements.txt

hooks: ## Install the pre-commit and pre-push Git hooks
	$(PRE_COMMIT) install --install-hooks

precommit: ## Run all pre-commit checks against the repository
	$(PRE_COMMIT) run --all-files --show-diff-on-failure

prepush: ## Run lightweight push checks; use make check for the full local suite
	$(PRE_COMMIT) run --hook-stage pre-push --all-files

workflows: ## Lint Actions syntax, embedded shell scripts, and workflow security
	$(PRE_COMMIT) run actionlint --files .github/workflows/*.yml
	$(PRE_COMMIT) run zizmor --files .github/workflows/*.yml .github/dependabot.yml .pre-commit-config.yaml

dev: ## Start the native desktop app
	$(RUN) npm run desktop:dev

web: ## Start the browser preview (native features disabled)
	$(RUN) npm run dev

model-profile: ## Generate a GGUF profile: HF_REPO=owner/repo HF_WEIGHTS=file.gguf
	node scripts/huggingface-profile.mjs --repo "$$HF_REPO" --revision "$$HF_REVISION" --runtime llama_cpp --weights "$$HF_WEIGHTS" --out "$$HF_OUTPUT"

build: ## Build the release desktop app
	$(RUN) npm run desktop:build

build-debug: ## Build the debug desktop app
	$(RUN) npm exec --workspace @transcribe/desktop -- tauri build --debug --bundles app -- --locked

build-web: ## Build frontend assets
	$(RUN) npm run build

check: check-fast test-python test-browser test-native-report check-rust lint-rust test-rust doc-rust ## Run all deterministic language, native and browser checks

check-fast: python-lock-check fmt-check lint-web lint-python lint-shell check-web test-tools ## Run formatting, portable lints/types and Node tests before each commit

check-portable: python-lock-check fmt-web-check fmt-python-check fmt-shell-check lint-web lint-python lint-shell check-web test-tools test-python build-web ## Run portable checks shared with the frontend CI job

test-tools: version-check ## Test frontend data helpers and repository tooling
	node --test apps/desktop/src/lib/*.test.mjs scripts/*.test.mjs

test-python: ## Run deterministic Python tooling tests without models or datasets
	$(PYTHON) scripts/run-python-tests.py

test-browser: ## Run the existing mocked-native UI regressions in Chromium
	npm exec playwright test

benchmark-contract: ## Test benchmark preparation, scoring, manifests, and recommendation rules
	node --test scripts/prepare-fleurs-benchmark.test.mjs scripts/prepare-mucs-benchmark.test.mjs scripts/summarize-asr-benchmarks.test.mjs scripts/summarize-personalization-benchmark.test.mjs
	$(RUN) cargo test -p transcribe-eval --locked

version-check: ## Verify Cargo, npm, lockfile, Tauri, and optional TAG versions agree
	node scripts/version.mjs check $(if $(strip $(TAG)),--tag=$(TAG),)

version-set: ## Set every application version declaration: make version-set VERSION=X.Y.Z
	@test -n "$(VERSION)" || { printf '%s\n' 'VERSION is required, for example: make version-set VERSION=0.2.0'; exit 1; }
	node scripts/version.mjs set "$(VERSION)"

container-build: ## Build the optional browser-preview OCI image (not the Mac app)
	docker build --build-arg APP_VERSION="$$(node scripts/version.mjs show)" --tag transcribe-browser-preview:local .

container-smoke: container-build ## Manually health-check the optional browser-preview image
	@container_id=$$(docker run --detach transcribe-browser-preview:local); \
	trap 'docker rm --force "$$container_id" >/dev/null 2>&1 || true' EXIT; \
	attempt=0; \
	while [ "$$attempt" -lt 30 ]; do \
		status=$$(docker inspect --format '{{.State.Health.Status}}' "$$container_id"); \
		[ "$$status" = healthy ] && break; \
		[ "$$status" = unhealthy ] && { docker logs "$$container_id"; exit 1; }; \
		attempt=$$((attempt + 1)); sleep 1; \
	done; \
	[ "$$(docker inspect --format '{{.State.Health.Status}}' "$$container_id")" = healthy ]; \
	docker exec "$$container_id" wget -qO- http://127.0.0.1:8080/healthz | grep -qx ok

check-web: ## Check Svelte and TypeScript
	$(RUN) npm run check

fmt: fmt-rust fmt-web fmt-python fmt-shell ## Format all maintained source languages

fmt-check: fmt-rust-check fmt-web-check fmt-python-check fmt-shell-check ## Check source formatting without changing files

fmt-rust: ## Format the Rust workspace and isolated integrity probe
	$(RUN) cargo fmt --all
	$(RUN) cargo fmt --manifest-path $(HASH_PROBE)
	$(RUN) cargo fmt --manifest-path tests/glib-backport/Cargo.toml

fmt-rust-check: ## Check Rust workspace and integrity-probe formatting
	$(RUN) cargo fmt --all -- --check
	$(RUN) cargo fmt --manifest-path $(HASH_PROBE) -- --check
	$(RUN) cargo fmt --manifest-path tests/glib-backport/Cargo.toml -- --check

fmt-web: ## Format TypeScript, JavaScript, Svelte, CSS, HTML and root JSON
	npm run format

fmt-web-check: ## Check frontend and Node source formatting with Prettier
	npm run format:check

fmt-python: ## Format Python tools and tests with Ruff
	$(RUFF) format scripts

fmt-python-check: ## Check Python formatting with Ruff
	$(RUFF) format --check scripts

fmt-shell: ## Format maintained shell scripts with shfmt
	$(SHFMT) -ln posix -i 2 -w scripts/*.sh

fmt-shell-check: ## Check shell formatting and POSIX syntax
	$(SHFMT) -ln posix -i 2 -d scripts/*.sh

lint: lint-web lint-python lint-shell lint-rust ## Run all language linters

lint-web: ## Lint JavaScript, TypeScript and Svelte with no warnings allowed
	npm run lint

lint-python: ## Lint every Python tool and test, including import order
	$(RUFF) check scripts

lint-shell: ## Lint POSIX shell scripts and their sourced files
	$(SHELLCHECK) --external-sources --shell=sh scripts/*.sh

# Tauri's compile-time context needs the generated frontend assets.
check-rust: build-web ## Compile-check every workspace target with the locked dependencies
	$(RUN) cargo check --workspace --all-targets --locked
	$(RUN) cargo check --manifest-path $(HASH_PROBE) --all-targets --all-features --locked

lint-rust: build-web ## Run strict Clippy for the workspace and isolated probe
	$(RUN) cargo clippy --workspace --all-targets --locked -- -D warnings
	$(RUN) cargo clippy --manifest-path $(HASH_PROBE) --all-targets --all-features --locked -- -D warnings

test: test-rust ## Run all Rust targets and documentation tests

test-rust: build-web ## Run workspace targets, doctests and the isolated probe without models
	$(RUN) cargo test --workspace --all-targets --locked
	$(RUN) cargo test --workspace --doc --locked
	$(RUN) cargo test --manifest-path $(HASH_PROBE) --all-targets --all-features --locked

doc-rust: build-web ## Check public Rust documentation with warnings denied
	$(RUN) RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked

smoke-native: build-web ## Run three explicit installed-model tests with the pinned public fixture (no recording/downloads)
	sh scripts/smoke-native.sh

models: ## Download or verify pinned Nemotron + Qwen weights (about 1.31 GB plus runtime)
	$(RUN) cargo run --locked -p transcribe-models --example install -- "$$MODEL_DIR"
