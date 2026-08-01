# Betsphere

Rust backend structured as a Cargo workspace following clean architecture. Dependencies point strictly inward, and the crate boundaries make the compiler enforce that rule.

## Layers

```
crates/
├── domain/           # Innermost circle — no framework dependencies
│   ├── entities/         # User, Market, Bet, ChatMessage, ...
│   ├── value_objects/    # Grouped by entity: user/{email,username}.rs
│   ├── repositories/     # Ports (traits) implemented by outer layers
│   ├── services/         # Pure domain logic: pricing, authorization
│   └── events/           # Facts about committed changes, one struct per event
├── application/      # Use cases orchestrating domain objects
│   ├── use_cases/        # Grouped by entity: user/{create,get,list}_user.rs
│   ├── ports/            # Non-persistence ports: auth, storage, broker, events
│   ├── realtime/         # Payloads crossing the broker, shared with the API
│   └── broadcasters/     # Event handlers that fan changes out to the broker
├── infrastructure/   # Adapters implementing domain and application ports
│   ├── persistence/      # Grouped by backend: in_memory/, postgres/, redis/
│   ├── events/           # Transactional outbox + in-memory bus
│   ├── messaging/        # Broker: Redis Pub/Sub, in-memory
│   ├── auth/             # Argon2 hashing, JWT access tokens
│   └── storage/          # File storage
├── demo/             # Driving adapter: bot accounts that simulate activity
│   ├── cast.rs           # The bot accounts, provisioned idempotently
│   ├── content.rs        # Market templates, chat lines, reactions
│   └── {market_maker,bettor,chatter}.rs  # One activity loop each
└── api/              # Delivery mechanism (axum) + composition root
    ├── main.rs           # Wires repositories into use cases, starts server
    ├── config.rs         # Typed env configuration (loads .env via dotenvy)
    ├── routes.rs         # Merges per-resource routers
    ├── routes/           # One file per resource: router + handlers + DTOs
    │   ├── health.rs
    │   ├── users.rs
    │   └── ws/           # The multiplexed WebSocket endpoint
    ├── state/            # Per-resource state: the use cases a router needs
    ├── extract.rs        # Auth extractors
    └── error.rs          # ApplicationError → HTTP status mapping
```

Dependency rule: `api → application → domain` and `infrastructure → domain`. The domain crate depends on nothing but std-adjacent utility crates. `demo` is a second driving adapter alongside `api`, on the same `→ application → domain` path.

Each layer is swappable at the composition root: `main.rs` builds the Postgres,
Redis and Redis-broker adapters, while the API's own tests build the same router
over the in-memory ones, so the full request path is exercised without Docker.

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
loop. A local model server for [demo mode](#demo-mode) sits in the same file
behind the `llm` profile, so it is opt-in rather than something every
`make db-up` starts. Prod (`make prod-up`, `docker-compose.prod.yml`) runs the full stack:
the API is compiled as a release binary in a multi-stage image (`Dockerfile`),
the database and cache publish no host ports, and all services restart
automatically.

User lookups go through a Redis read-through cache (`CachedUserRepository`)
in front of Postgres. Entries expire after `CACHE_TTL_SECS` and the cache
fails open: if Redis is down, reads fall through to Postgres.

## API

`Auth` is what a request must present: **—** public, **user** an
`Authorization: Bearer <access token>` header, **admin** the same but from an
account with the `admin` role, **key** the `X-Internal-Key` header.

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET    | `/health` | — | Liveness check |
| POST   | `/api/auth/register` | — | Register `{username, email, password}` → tokens |
| POST   | `/api/auth/login` | — | Login `{email, password}` → access token + refresh cookie |
| POST   | `/api/auth/refresh` | cookie | Rotate the refresh cookie, get a new access token |
| POST   | `/api/auth/logout` | cookie | Invalidate the refresh token |
| GET    | `/api/users/me` | user | Own profile: balance, email, stats |
| GET    | `/api/users/{id}` | — | Public profile (no email/balance) |
| GET    | `/api/users/{id}/bets` | — | A user's bet history (`sort`, `status`, `page`, `limit`) |
| POST   | `/api/users/me/avatar` | user | Upload an avatar (multipart `file`) |
| GET    | `/api/markets` | — | List markets (`sort`, `category`, `status`, `search`, `page`, `limit`) |
| GET    | `/api/markets/featured` | — | The most popular market right now |
| GET    | `/api/markets/{id}` | — | One market with its outcomes |
| GET    | `/api/markets/{id}/price-history` | — | Price points per outcome (`interval`, `from`, `to`) |
| GET    | `/api/markets/{id}/bets` | — | Bets placed on one market |
| POST   | `/api/markets` | admin | Create a market with its outcomes |
| POST   | `/api/markets/{id}/resolve` | admin | Settle a market on its winning outcome |
| POST   | `/api/markets/{id}/thumbnail` | admin | Upload a market thumbnail (multipart `file`) |
| POST   | `/api/markets/{id}/outcomes/{outcome_id}/thumbnail` | admin | Upload an outcome thumbnail |
| POST   | `/api/bets` | user | Place a bet `{outcome_id, amount}` |
| GET    | `/api/bets/feed` | — | Cross-market bet feed |
| GET    | `/api/chat/messages` | user | One chat room's history (`market_id`, `before_uuid`, `after_uuid`) |
| GET    | `/api/files/{folder}/{name}` | — | Serve an uploaded file |
| PATCH  | `/api/internal/users/{id}` | key | Update a user's role |
| GET    | `/ws?token=<access token>` | user | Multiplexed real-time socket — see [docs/websocket.md](docs/websocket.md) |

Auth follows the access/refresh JWT scheme: the short-lived access token is
returned in the response body, the long-lived refresh token travels in an
httpOnly cookie scoped to `/api/auth` and is rotated on every refresh.
Passwords are hashed with Argon2id; refresh tokens are stored hashed (SHA-256).
The WebSocket takes its token as a query parameter because browsers cannot set
headers on a WS handshake.

Errors return `{"error": "..."}` with 401 (unauthorized), 403 (forbidden),
404 (not found), 409 (conflict), 422 (validation), or 500.

Interactive docs: **`/docs`** (OpenAPI spec at `/api-docs/openapi.json`),
generated from the handler annotations in `routes/` via utoipa. WebSocket frames
are not expressible in OpenAPI and live in
[docs/websocket.md](docs/websocket.md) instead.

## Real-time

Chat rooms, market price feeds and bet feeds are all multiplexed over the single
`/ws` endpoint. Nothing is broadcast directly from the code that made a change:

1. A repository writes the change and records a domain event in the **outbox**,
   in the same transaction — so the event exists if and only if the change
   committed.
2. The `OutboxProcessor` (woken by `LISTEN`/`NOTIFY`, with a periodic sweep as a
   fallback) delivers each event to its handlers, at least once.
3. A **broadcaster** re-reads the current state the event names and publishes it
   to a **broker** channel (Redis Pub/Sub in production).
4. The WebSocket endpoint forwards that channel to every subscribed socket.

Keeping fan-out in the broker is what makes the API stateless: no messages are
buffered in any one process, so instances can be scaled out, restarted or
load-balanced freely. Because delivery is at-least-once, handlers are idempotent
and payloads carry absolute state rather than deltas.

## Demo mode

`DEMO_MODE=true` (or `make demo`) starts a simulation of a busy platform: a cast
of bot accounts where one opens and settles markets and the rest bet, chat,
reply and react, on their own jittered schedules.

The bots are not a fixture — they are users. They call the same use cases the
HTTP handlers call, so their stakes move real prices, their payouts move real
balances, and everything they do travels the outbox → broker → WebSocket path
described above and arrives at a connected client as ordinary live traffic.
That also makes the simulation a decent smoke test of the whole write path.

Accounts live under `@bots.betsphere.invalid` and are created once, then reused:
restarting the server continues the demo rather than restarting it. They all
share `DEMO_BOT_PASSWORD`, so you can log in as one to see the platform from the
inside — and the market maker (`the_oracle`) has the admin role, which is
exactly why this must never be pointed at a real database.

Cadences and cast size are configurable; see the demo section of `.env.example`.

### Letting a local model do the talking

Canned chat lines repeat after a while. Point `DEMO_LLM_URL` at a local model
server and the bots compose their own messages, replies and reactions instead.

The dev compose file ships one, behind the `llm` profile so it starts only when
asked for:

```sh
make demo-llm    # dev database + model server + the API with the bots on
```

That is `make llm-up` (start `ollama`, download `DEMO_LLM_MODEL` into a volume —
the first run fetches a few hundred MB) followed by the API with
`DEMO_LLM_URL` pointed at `localhost:11434`. The full stack runs it too:

```sh
docker compose -f docker-compose.prod.yml --profile llm up -d --build
```

There the API reaches the server by service name; compose rewrites whatever
`DEMO_LLM_URL` holds to the in-network URL, so any non-empty value means "on".

The model is whatever `DEMO_LLM_MODEL` names, as an
[Ollama library](https://ollama.com/library) tag — `qwen2.5:0.5b` (~0.4 GB) by
default, `gemma3:270m` if that is still too much, `llama3.2:1b` for better
banter. Nothing ties this to Ollama: any server speaking the OpenAI
chat-completions shape works, so llama.cpp's `llama-server` or LM Studio on the
host do just as well — set `DEMO_LLM_URL` to their endpoint and skip the
profile. The URL must be `http://`, since this is meant for a server on the
same machine and the client is built without TLS.

Each bot is prompted with the room it is standing in — the market, its current
prices, and the last few messages — so the talk follows the board and answers
whoever spoke last, real users included.

A 0.5B model is wrong a lot, which is fine here: the answer is stripped of
reasoning blocks, quotes and self-narration, cut to a word boundary, and
validated by the same value objects a user's message is. Anything left
unusable — as well as a server that is down, slow, or answering nonsense —
falls back to a canned line, so the demo never stalls or goes quiet waiting on
a model.

## Adding a feature

1. **Domain**: add/extend entities, value objects, and repository traits in `crates/domain`.
2. **Application**: add a use case in `crates/application/src/use_cases/`.
3. **Infrastructure**: implement any new ports in `crates/infrastructure` — both
   the real backend and the in-memory one the tests run against.
4. **API**: add `routes/<resource>.rs` (router + handlers + DTOs), merge it in `routes.rs`, wire the use case in `state.rs`.
5. **Migration**: if the schema changed, add `migrations/NNNN_description.sql`.

If the feature is real-time, two more steps sit between 1 and 3: define the event
in `domain/src/events/`, and add a broadcaster in
`application/src/broadcasters/` plus its payload in `application/src/realtime/`.
Register the broadcaster on the `OutboxProcessor` in `main.rs` **and** on the
in-memory bus in `api/src/tests.rs`, or it will work in production and be
invisible to the tests.

Run `make check` (fmt + clippy + tests) before pushing.
