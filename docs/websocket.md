# WebSocket API (`/ws`)

Swagger/OpenAPI does not describe WebSocket upgrades and frame contracts well,
so this document describes the platform real-time API.

## Endpoint and authentication

- **Endpoint:** `GET /ws?token=<access_token>`
- **Auth:** access token is passed via query parameter `token` — browsers cannot
  set the `Authorization` header on a WS handshake. An invalid token fails the
  upgrade itself, so there is no unauthenticated socket to speak on.
- **Transport:** one multiplexed WebSocket connection for all real-time streams

## Channel names

| Channel | Carries | Direction |
|---------|---------|-----------|
| `global_chat` | global chat room | both ways |
| `market_chat:<market_uuid>` | one market's chat room | both ways |
| `market:<market_uuid>` | one market's price feed | server → client |
| `market_bets:<market_uuid>` | one market's placed bets | server → client |
| `global_bets` | every market's placed bets, in one feed | server → client |

Subscribing to a market-scoped channel whose market does not exist returns
`error` rather than a silent, permanently empty stream.

## Client → Server frames

All frames are JSON objects with `type` in `snake_case`.

### Subscribe

```json
{
  "type": "subscribe",
  "channel": "global_chat"
}
```

```json
{
  "type": "subscribe",
  "channel": "market_chat:7f6ec8be-e0dc-4fd8-b00c-c8c3850f4e91"
}
```

```json
{
  "type": "subscribe",
  "channel": "market:7f6ec8be-e0dc-4fd8-b00c-c8c3850f4e91"
}
```

```json
{ "type": "subscribe", "channel": "market_bets:7f6ec8be-e0dc-4fd8-b00c-c8c3850f4e91" }
```

```json
{ "type": "subscribe", "channel": "global_bets" }
```

Chat and bet channels answer a subscribe with a `history` frame; a market feed
answers with one `price_update` per outcome. The server subscribes to the live
stream *before* replaying that state, so nothing published in between is lost —
which means the replay and the first live frames may overlap. Deduplicate chat
messages and bets by `id`; `price_update` and `reaction_update` are idempotent
and need no deduplication.

### Unsubscribe

```json
{
  "type": "unsubscribe",
  "channel": "global_chat"
}
```

### Post chat message

```json
{
  "type": "chat_message",
  "channel": "global_chat",
  "body": "Hello"
}
```

Optionally a reply, quoting an earlier message of the **same** room:

```json
{
  "type": "chat_message",
  "channel": "global_chat",
  "body": "Agreed",
  "reply_to": "7f6ec8be-e0dc-4fd8-b00c-c8c3850f4e95"
}
```

Notes:

- `chat_message` is valid only for chat channels (`global_chat`, `market_chat:<id>`);
  sending it to a market feed or bet feed returns `error`
- posting does **not** require prior `subscribe`
- there is no direct acknowledgement: the sender learns the server-assigned `id`
  and `created_at` from the `chat_message` frame broadcast to the room, which it
  receives like any other subscriber — so a client that wants its own message
  echoed back must be subscribed to the room it posts to
- `body` is trimmed, must be non-empty, and is capped at 2000 characters
- `reply_to` naming a message of another room (or no message at all) returns `error`
- replies are one level deep on the wire: a reply carries the message it quotes,
  but not what *that* one quoted

### Add / remove reaction

```json
{
  "type": "add_reaction",
  "message_id": "7f6ec8be-e0dc-4fd8-b00c-c8c3850f4e95",
  "emoji": "🔥"
}
```

```json
{
  "type": "remove_reaction",
  "message_id": "7f6ec8be-e0dc-4fd8-b00c-c8c3850f4e95",
  "emoji": "🔥"
}
```

Notes:

- no `channel`: the message id already says which room the message is in
- both are idempotent — reacting with an emoji already held, or removing one that
  isn't, succeeds and broadcasts nothing
- `emoji` must be a single emoji sequence (pictographs, keycaps, flags, ZWJ
  sequences and skin-tone modifiers); plain text returns `error`
- a user holds each emoji on a message at most once; reacting does **not**
  require prior `subscribe`

## Server → Client frames

All server frames are JSON with `type` in `snake_case`.

### History (on chat subscribe)

Sent once after a successful subscribe to a chat channel: the most recent 50
messages, oldest first. Page further back over REST
(`GET /api/chat/messages?before_uuid=<oldest id you hold>`).

The bet feeds are answered with a `history` frame too — same `type`, different
payload shape. The `channel` tells the two apart; see [Bet placed](#bet-placed)
below.

```json
{
  "type": "history",
  "channel": "global_chat",
  "data": [
    {
      "id": "...",
      "author": {
        "id": "...",
        "username": "...",
        "avatar_url": null
      },
      "body": "...",
      "reply_to": null,
      "reactions": [{ "emoji": "🔥", "count": 3, "reacted": true }],
      "created_at": "2026-07-15T19:00:00Z"
    }
  ]
}
```

`reactions` is one entry per distinct emoji, in the order each emoji first
appeared on the message. `reacted` says whether **the account this socket is
authenticated as** is among the reactors, so a history page is only correct for
the client that asked for it.

### Chat message (live)

`reply_to` is set when the message is a reply, and is the quoted message
flattened to what a quote line renders. `reactions` is always empty here — a
message this fresh has none; the ones that follow arrive as `reaction_update`.

```json
{
  "type": "chat_message",
  "channel": "market_chat:7f6ec8be-e0dc-4fd8-b00c-c8c3850f4e91",
  "data": {
    "id": "...",
    "author": {
      "id": "...",
      "username": "...",
      "avatar_url": null
    },
    "body": "Agreed",
    "reply_to": {
      "id": "7f6ec8be-e0dc-4fd8-b00c-c8c3850f4e95",
      "author": { "id": "...", "username": "...", "avatar_url": null },
      "body": "The original"
    },
    "reactions": [],
    "created_at": "2026-07-15T19:01:00Z"
  }
}
```

### Reaction update

Sent to every subscriber of the room the reacted-to message is in, whenever a
reaction is added or taken back.

```json
{
  "type": "reaction_update",
  "channel": "global_chat",
  "data": {
    "message_id": "7f6ec8be-e0dc-4fd8-b00c-c8c3850f4e95",
    "emoji": "🔥",
    "count": 3,
    "user_id": "7f6ec8be-e0dc-4fd8-b00c-c8c3850f4e94",
    "added": true
  }
}
```

Notes:

- `count` is the total as it now stands, **not** a delta, so applying the same
  frame twice lands in the same place; `0` means the emoji should disappear from
  the message's reaction row
- `user_id` and `added` are how a client keeps the per-reader `reacted` flag it
  got with the history in step, without refetching the page

### Price update

On market-feed subscribe the server first sends a snapshot — one frame per
outcome, all carrying the same `recorded_at` — then streams the moves as bets
shift the prices.

```json
{
  "type": "price_update",
  "channel": "market:7f6ec8be-e0dc-4fd8-b00c-c8c3850f4e91",
  "data": {
    "outcome_id": "7f6ec8be-e0dc-4fd8-b00c-c8c3850f4e92",
    "price": 0.42,
    "recorded_at": "2026-07-15T19:02:00Z"
  }
}
```

`price` is a number in `0.0..=1.0` with four decimals of resolution: the
outcome's share of the market's total staked volume, split evenly before any
volume exists. A market's prices always sum to exactly `1.0`, so one price move
produces one frame per outcome of that market — every share shifts together.

### Bet placed

On `market_bets:<market_id>` or `global_bets` subscribe, the server first
answers with a `history` frame carrying that feed's recent bets (oldest first,
same shape as the `data` object below), then streams newly committed bets.
Deduplicate by bet `id` in case a live frame overlaps the history.

```json
{
  "type": "history",
  "channel": "market_bets:7f6ec8be-e0dc-4fd8-b00c-c8c3850f4e91",
  "data": [
    {
      "id": "7f6ec8be-e0dc-4fd8-b00c-c8c3850f4e93",
      "user_id": "7f6ec8be-e0dc-4fd8-b00c-c8c3850f4e94",
      "market_id": "7f6ec8be-e0dc-4fd8-b00c-c8c3850f4e91",
      "outcome_id": "7f6ec8be-e0dc-4fd8-b00c-c8c3850f4e92",
      "amount": 1000,
      "price": 0.5,
      "created_at": "2026-07-15T19:03:00Z"
    }
  ]
}
```

```json
{
  "type": "bet_placed",
  "channel": "global_bets",
  "data": {
    "id": "7f6ec8be-e0dc-4fd8-b00c-c8c3850f4e93",
    "user_id": "7f6ec8be-e0dc-4fd8-b00c-c8c3850f4e94",
    "market_id": "7f6ec8be-e0dc-4fd8-b00c-c8c3850f4e91",
    "outcome_id": "7f6ec8be-e0dc-4fd8-b00c-c8c3850f4e92",
    "amount": 1000,
    "price": 0.5,
    "created_at": "2026-07-15T19:03:00Z"
  }
}
```

Notes:

- `market_id` is present on both feeds, redundantly on `market_bets:<id>`, so one
  client-side handler serves both
- `price` is the price the stake was struck at, fixed at placement time
- `amount` is in minimal currency units
- every bet goes out on both feeds: its own market's and the cross-market one

### Error

Connection stays open; the client can send another valid frame. Errors are
returned to the sender only, never to the room, and carry no channel — match
them to what you last sent.

```json
{
  "type": "error",
  "message": "unknown channel \"foo\""
}
```

## Behavioral details

- Duplicate subscribe to the same channel returns `error` (`already subscribed ...`)
- Unsubscribe from non-subscribed channel returns `error` (`not subscribed ...`)
- Unknown market in `market:<id>`, `market_chat:<id>` or `market_bets:<id>`
  returns `error`
- Unknown message in `add_reaction` / `remove_reaction` returns `error`
- A reaction that changes nothing broadcasts nothing, so a retried frame is safe
- Deleting a message takes its reactions with it, and clears the `reply_to` of
  every reply that quoted it — those replies stay, unquoted
- Invalid JSON/shape returns `error` with a format hint
- Nothing is delivered on a channel you are not subscribed to, including your own
  messages and reactions
- On disconnect, all channel forwarding tasks are aborted

## Delivery guarantees

Frames reach a client through a transactional outbox and a shared broker (Redis
Pub/Sub in production), not straight from the request that caused them. Three
consequences worth designing for:

- **Committed, then delivered.** A frame exists only if the change it describes
  committed, so nothing is ever broadcast for a write that was rolled back.
- **At least once.** A frame may arrive twice. Chat messages and bets carry a
  stable `id`; `price_update` and `reaction_update` carry absolute state rather
  than deltas, so re-applying either is a no-op.
- **Not instant, and not ordered across channels.** Delivery is asynchronous, so
  a client can observe its own write over the socket slightly after the HTTP
  response (or frame) that triggered it. Ordering holds within one channel.

Because fan-out lives in the broker rather than in any one process, no server
holds session state: any instance can serve any socket, and a reconnect can land
anywhere behind the load balancer.

## Related

- REST chat history and paging: `GET /api/chat/messages` — see `/docs`
- Everything else the API exposes: [README](../README.md#api)