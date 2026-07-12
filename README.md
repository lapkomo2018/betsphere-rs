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
│   └── persistence/      # Grouped by backend: in_memory/, postgres/, redis/
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

The database schema lives in `migrations/` (plain SQL, sqlx format). The files
are embedded into the binary at compile time and applied automatically on
startup, so a fresh database needs no manual steps. To add a migration, drop a
new `NNNN_description.sql` file in `migrations/` — applied ones are tracked in
the `_sqlx_migrations` table and never re-run.

Dev runs only the infrastructure (Postgres + Redis) in Docker
(`docker-compose.yml`) while the app runs natively for the fastest feedback
loop. Prod (`make prod-up`, `docker-compose.prod.yml`) runs the full stack:
the API is compiled as a release binary in a multi-stage image (`Dockerfile`),
the database and cache publish no host ports, and all services restart
automatically.

User lookups go through a Redis read-through cache (`CachedUserRepository`)
in front of Postgres. Entries expire after `CACHE_TTL_SECS` and the cache
fails open: if Redis is down, reads fall through to Postgres.

## API

| Method | Path                 | Description                                              |
|--------|----------------------|----------------------------------------------------------|
| GET    | `/health`            | Liveness check                                           |
| POST   | `/api/auth/register` | Register `{username, email, password}` → tokens          |
| POST   | `/api/auth/login`    | Login `{email, password}` → access token + refresh cookie |
| POST   | `/api/auth/refresh`  | Rotate the refresh cookie, get a new access token        |
| POST   | `/api/auth/logout`   | Invalidate the refresh token                             |
| GET    | `/api/users/me`      | Current user (requires `Authorization: Bearer <token>`)  |
| GET    | `/api/users/{id}`    | Public profile (no email/balance)                        |

Auth follows the access/refresh JWT scheme: the short-lived access token is
returned in the response body, the long-lived refresh token travels in an
httpOnly cookie scoped to `/api/auth` and is rotated on every refresh.
Passwords are hashed with Argon2id; refresh tokens are stored hashed (SHA-256).

Errors return `{"error": "..."}` with 401 (unauthorized), 404 (not found),
409 (conflict), 422 (validation), or 500.

Interactive docs: **`/docs`** (OpenAPI spec at `/api-docs/openapi.json`),
generated from the handler annotations in `routes/` via utoipa.

## Adding a feature

1. **Domain**: add/extend entities, value objects, and repository traits in `crates/domain`.
2. **Application**: add a use case in `crates/application/src/use_cases/`.
3. **Infrastructure**: implement any new ports in `crates/infrastructure`.
4. **API**: add `routes/<resource>.rs` (router + handlers + DTOs), merge it in `routes.rs`, wire the use case in `state.rs`.
