# WebSocket transaction tracking protocol

This document specifies the **WebSocket message protocol** used by the API server for **transaction (tx_id) status tracking**.

The protocol is implemented by `api-server/src/websocket.rs` and uses the following Rust types from `api-server/src/types.rs`:

- `SubscribeMessage`
- `TransactionStatusEvent`

Endpoint: `GET /ws`

---

## Transport and framing

- The server expects **JSON text frames** (`Message::Text`).
- Each frame is a single JSON object.
- The server sends JSON text frames back.

---

## Message envelope format

All messages sent by the server use the following JSON envelope:

```json
{
  "msg_type": "<string>",
  "data": { /* type-specific payload */ }
}
```

All messages sent by the client are parsed as:

```json
{
  "action": "subscribe" | "unsubscribe",
  "tx_id": "<tx_id>"
}
```

Notes:
- The server does not require a top-level `version` or `request_id`.
- If the server cannot parse the client message, it logs a warning and ignores it.

---

## Client → Server: subscribe / unsubscribe

### Subscribe

Client sends:

```json
{
  "action": "subscribe",
  "tx_id": "<tx_id>"
}
```

`tx_id` is treated as an opaque string by the server.

On subscription, the server also registers the subscriber in `AppState` and begins forwarding status events as they are broadcast.

### Unsubscribe

Client sends:

```json
{
  "action": "unsubscribe",
  "tx_id": "<tx_id>"
}
```

Unsubscription removes the tx_id subscription from the local list and calls `state.remove_subscriber`.

### WebSocket disconnect behavior

When the connection closes (client sends `Close` or the socket ends), the server removes all tx_id subscriptions associated with that connection.

---

## Server → Client: message types

### `subscribed`

Sent immediately after a successful `subscribe` request.

Envelope:

```json
{
  "msg_type": "subscribed",
  "data": {
    "tx_id": "<tx_id>",
    "status": "subscribed"
  }
}
```

### `status_update`

Sent whenever the server receives a `TransactionStatusEvent` for a subscribed `tx_id`.

Envelope:

```json
{
  "msg_type": "status_update",
  "data": {
    "tx_id": "<tx_id>",
    "status": "<status>",
    "timestamp": "<timestamp>",
    "message": "<optional message or null>"
  }
}
```

#### `status` values

The server uses the `TransactionStatus` enum serialized with `UPPERCASE` names.

Possible values:

- `PENDING`
- `SUBMITTED`
- `CONFIRMED`
- `FAILED`

#### `timestamp`

`timestamp` is a string (`TransactionStatusEvent.timestamp: String`).

The server forwards it as-is.

#### `message`

`message` is optional (`TransactionStatusEvent.message: Option<String>`). 
- If present: a string.
- If absent: `null` (because the server includes `event.message` directly in the JSON payload).

---

## Client example

```js
const ws = new WebSocket("ws://localhost:8080/ws");

ws.onopen = () => {
  ws.send(JSON.stringify({ action: "subscribe", tx_id: "tx_123" }));
};

ws.onmessage = (ev) => {
  const msg = JSON.parse(ev.data);
  if (msg.msg_type === "status_update") {
    console.log(msg.data.tx_id, msg.data.status, msg.data.timestamp, msg.data.message);
  }
};

// Later
// ws.send(JSON.stringify({ action: "unsubscribe", tx_id: "tx_123" }));
```

---

## Server-side behavior and guarantees (practical notes)

- The server uses an internal `broadcast::Sender<TransactionStatusEvent>`.
- Subscription matching is by exact string match on `tx_id`.
- There is no explicit ordering guarantee across multiple tx_id subscriptions on a single connection beyond what the underlying broadcast channel provides.

---

## Unsupported / non-goals

- No batching protocol for subscribe/unsubscribe.
- No authentication or authorization in the WebSocket layer (none is implemented in this code path).
- No server-side validation of tx_id format.
- No ping/pong keepalive semantics defined by this protocol (use WebSocket defaults).

