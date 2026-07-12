# Betsphere

Rust backend structured as a Cargo workspace following clean architecture. Dependencies point strictly inward, and the crate boundaries make the compiler enforce that rule.

## Layers

```
crates/
├── domain/           # Innermost circle — no framework dependencies
│   ├── entities/         # User, UserId
│   ├── value_objects/    # Grouped by entity: user/{email,username}.rs
│   └── repositories/     # Ports (traits) implemented by outer layers
├── application/      # Use cases orchestrating domain objects
│   └── use_cases/        # Grouped by entity: user/{create,get,list}_user.rs
├── infrastructure/   # Adapters implementing domain ports
│   └── persistence/      # Grouped by backend: in_memory/, postgres/ (later)
└── api/              # Delivery mechanism (axum) + composition root
    ├── main.rs           # Wires repositories into use cases, starts server
    ├── config.rs         # Typed env configuration (loads .env via dotenvy)
    ├── routes.rs         # Merges per-resource routers
    ├── routes/           # One file per resource: router + handlers + DTOs
    │   ├── health.rs
    │   └── users.rs
    └── error.rs          # ApplicationError → HTTP status mapping
```

Dependency rule: `api → application → domain` and `infrastructure → domain`. The domain crate depends on nothing but std-adjacent utility crates.

## Running

All commands live in the Makefile — run `make help` for the full list.

```sh
make setup   # create .env from the template
make dev     # start the dev database (Docker) + run the API natively
make check   # fmt check + clippy + tests (what CI would run)
```

Configuration is read from the environment (and `.env` if present) into a typed
`Config` struct at startup — see `.env.example` for available variables.

Dev runs only Postgres in Docker (`docker-compose.yml`) while the app runs
natively for the fastest feedback loop. Prod (`make prod-up`,
`docker-compose.prod.yml`) runs the full stack: the API is compiled as a
release binary in a multi-stage image (`Dockerfile`), the database publishes
no host port, and both services restart automatically.

## API

| Method | Path             | Description                          |
|--------|------------------|--------------------------------------|
| GET    | `/health`        | Liveness check                       |
| POST   | `/api/users`     | Create user `{username, email}`      |
| GET    | `/api/users`     | List users                           |
| GET    | `/api/users/{id}`| Get user by UUID                     |

Errors return `{"error": "..."}` with 422 (validation), 404 (not found), 409 (conflict), or 500.

Interactive docs: **`/docs`** (OpenAPI spec at `/api-docs/openapi.json`),
generated from the handler annotations in `routes/` via utoipa.

## Adding a feature

1. **Domain**: add/extend entities, value objects, and repository traits in `crates/domain`.
2. **Application**: add a use case in `crates/application/src/use_cases/`.
3. **Infrastructure**: implement any new ports in `crates/infrastructure`.
4. **API**: add `routes/<resource>.rs` (router + handlers + DTOs), merge it in `routes.rs`, wire the use case in `state.rs`.
