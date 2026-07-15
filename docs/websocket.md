# WebSocket API (`/ws`)

Swagger/OpenAPI does not describe WebSocket upgrades and frame contracts well,
so this document describes the platform real-time API.

## Endpoint and authentication

- **Endpoint:** `GET /ws?token=<access_token>`
- **Auth:** access token is passed via query parameter `token`
- **Transport:** one multiplexed WebSocket connection for all real-time streams

## Channel names

- `global_chat` — global chat room
- `market_chat:<market_uuid>` — chat room for a specific market
- `market:<market_uuid>` — live price feed for a specific market
- `market_bets:<market_uuid>` — live feed of placed bets for a specific market

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

Notes:

- `chat_message` is valid only for chat channels (`global_chat`, `market_chat:<id>`)
- posting does **not** require prior `subscribe`

## Server → Client frames

All server frames are JSON with `type` in `snake_case`.

### History (on chat subscribe)

Sent once after successful subscribe to a chat channel.
(A `market_bets:<id>` subscribe is also answered with a `history` frame —
see [Bet placed](#bet-placed) below.)

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
      "posted_at": "2026-07-15T19:00:00Z"
    }
  ]
}
```

### Chat message (live)

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
    "body": "...",
    "posted_at": "2026-07-15T19:01:00Z"
  }
}
```

### Price update

On market-feed subscribe, server first sends a snapshot (one frame per outcome),
then sends live updates.

```json
{
  "type": "price_update",
  "channel": "market:7f6ec8be-e0dc-4fd8-b00c-c8c3850f4e91",
  "data": {
    "outcome_id": "7f6ec8be-e0dc-4fd8-b00c-c8c3850f4e92",
    "price": "0.42",
    "recorded_at": "2026-07-15T19:02:00Z"
  }
}
```

### Bet placed

On `market_bets:<market_id>` subscribe, the server first answers with a
`history` frame carrying the market's recent bets (oldest first, same shape
as the `data` object below), then streams newly committed bets. Clients
should deduplicate by bet `id` in case a live frame overlaps the history.

```json
{
  "type": "history",
  "channel": "market_bets:7f6ec8be-e0dc-4fd8-b00c-c8c3850f4e91",
  "data": [
    {
      "id": "7f6ec8be-e0dc-4fd8-b00c-c8c3850f4e93",
      "user_id": "7f6ec8be-e0dc-4fd8-b00c-c8c3850f4e94",
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
  "channel": "market_bets:7f6ec8be-e0dc-4fd8-b00c-c8c3850f4e91",
  "data": {
    "id": "7f6ec8be-e0dc-4fd8-b00c-c8c3850f4e93",
    "user_id": "7f6ec8be-e0dc-4fd8-b00c-c8c3850f4e94",
    "outcome_id": "7f6ec8be-e0dc-4fd8-b00c-c8c3850f4e92",
    "amount": 1000,
    "price": 0.5,
    "created_at": "2026-07-15T19:03:00Z"
  }
}
```

### Error

Connection stays open; client can send another valid frame.

```json
{
  "type": "error",
  "message": "unknown channel \"foo\""
}
```

## Behavioral details

- Duplicate subscribe to the same channel returns `error` (`already subscribed ...`)
- Unsubscribe from non-subscribed channel returns `error` (`not subscribed ...`)
- Unknown market in `market:<id>` subscribe returns `error`
- Invalid JSON/shape returns `error` with a format hint
- On disconnect, all channel forwarding tasks are aborted