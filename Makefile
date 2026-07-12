SHELL := sh
.DEFAULT_GOAL := help

COMPOSE      := docker compose
COMPOSE_PROD := docker compose -f docker-compose.prod.yml

# --- General ---

.PHONY: help
help: ## Show available commands
	@grep -E '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  %-12s %s\n", $$1, $$2}'

.PHONY: setup
setup: ## Create .env from the template (won't overwrite an existing one)
	cp -n .env.example .env || true

# --- App (native) ---

.PHONY: run
run: ## Run the API natively (expects the dev database, see db-up)
	cargo run

.PHONY: dev
dev: db-up run ## Start the dev database, then run the API natively

.PHONY: test
test: ## Run all workspace tests
	cargo test

.PHONY: fmt
fmt: ## Format all code
	cargo fmt --all

.PHONY: lint
lint: ## Run clippy with warnings as errors
	cargo clippy --all-targets -- -D warnings

.PHONY: check
check: ## fmt check + clippy + tests (what CI would run)
	cargo fmt --all --check
	cargo clippy --all-targets -- -D warnings
	cargo test

# --- Dev infrastructure (Docker) ---

.PHONY: db-up
db-up: ## Start the dev database in the background
	$(COMPOSE) up -d

.PHONY: db-down
db-down: ## Stop the dev database
	$(COMPOSE) down

.PHONY: db-reset
db-reset: ## Stop the dev database and wipe its data, then start fresh
	$(COMPOSE) down -v
	$(COMPOSE) up -d

.PHONY: db-logs
db-logs: ## Tail dev database logs
	$(COMPOSE) logs -f postgres

# --- Prod stack (Docker) ---

.PHONY: prod-up
prod-up: ## Build and start the full prod stack
	$(COMPOSE_PROD) up -d --build

.PHONY: prod-down
prod-down: ## Stop the prod stack
	$(COMPOSE_PROD) down

.PHONY: prod-logs
prod-logs: ## Tail prod API logs
	$(COMPOSE_PROD) logs -f api
