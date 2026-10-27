# Common development tasks. Run `make` or `make help` to list them.

CARGO ?= cargo
# Extra arguments for the app, e.g. `make run ARGS="--host tcp://host:2375"`.
ARGS ?=
RELEASE_BIN := target/release/docker-tui

.DEFAULT_GOAL := help
.PHONY: help build release run run-release test test-unit test-integration \
        test-all snapshots fmt fmt-check clippy doc doc-open check ci clean

help: ## List available targets
	@awk 'BEGIN {FS = ":.*## "} \
		/^## / {printf "\n%s\n", substr($$0, 4)} \
		/^[a-zA-Z_-]+:.*## / {printf "  \033[36m%-17s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

## Building

build: ## Debug build
	$(CARGO) build

release: ## Optimized release build
	$(CARGO) build --release
	@echo "Built $(RELEASE_BIN)"

run: ## Run the debug build (pass flags with ARGS="...")
	$(CARGO) run -- $(ARGS)

run-release: ## Run the release build (pass flags with ARGS="...")
	$(CARGO) run --release -- $(ARGS)

## Testing

test: test-unit ## Unit and snapshot tests (no Docker needed)

test-unit: ## Unit and snapshot tests in tests/unit
	$(CARGO) test --test unit

test-integration: ## Integration tests against the local Docker daemon
	$(CARGO) test --test docker_integration -- --ignored

test-all: test-unit test-integration ## Unit and integration tests

snapshots: ## Re-record UI snapshots after an intentional change (review with git diff)
	INSTA_UPDATE=always $(CARGO) test --test unit

## Code quality

fmt: ## Format the code
	$(CARGO) fmt

fmt-check: ## Fail if the code is not formatted
	$(CARGO) fmt --check

clippy: ## Lint, treating warnings as errors
	$(CARGO) clippy --all-targets -- -D warnings

## Documentation

doc: ## Generate API docs into target/doc
	RUSTDOCFLAGS="-D warnings" $(CARGO) doc --no-deps --document-private-items

doc-open: ## Generate API docs and open them in a browser
	RUSTDOCFLAGS="-D warnings" $(CARGO) doc --no-deps --document-private-items --open

## Aggregates

check: fmt-check clippy test ## Quick pre-commit check: format, lint, unit tests

ci: fmt-check clippy test-all doc ## Everything CI runs, including Docker integration tests

clean: ## Remove build artifacts
	$(CARGO) clean
