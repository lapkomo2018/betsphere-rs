use axum::response::Html;
use axum::routing::get;
use utoipa_axum::router::OpenApiRouter;

use crate::state::AppState;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().route("/tests/chat", get(chat_ws_test_page))
}

async fn chat_ws_test_page() -> Html<&'static str> {
    Html(CHAT_WS_TEST_PAGE)
}

const CHAT_WS_TEST_PAGE: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Betsphere WS Chat Test</title>
  <style>
    body { font-family: system-ui, sans-serif; margin: 24px; line-height: 1.35; }
    h1 { margin-top: 0; }
    .row { display: flex; gap: 8px; flex-wrap: wrap; margin-bottom: 10px; }
    input, button { font: inherit; padding: 8px; }
    input { min-width: 320px; }
    #log { border: 1px solid #ccc; border-radius: 8px; padding: 12px; min-height: 240px; max-height: 420px; overflow: auto; background: #fafafa; white-space: pre-wrap; }
    .small { color: #555; font-size: 0.92rem; }
  </style>
</head>
<body>
  <h1>WS chat test page</h1>

  <div class="row">
    <input id="token" placeholder="JWT access token" />
    <button id="connect">Connect</button>
    <button id="disconnect" disabled>Disconnect</button>
  </div>

  <div class="row">
    <input id="channel" value="global_chat" placeholder="channel (e.g. global_chat)" />
    <button id="subscribe" disabled>Subscribe</button>
  </div>

  <div class="row">
    <input id="message" placeholder="message text" />
    <button id="send" disabled>Send message</button>
  </div>

  <p class="small">Open this page on the same host as API. It connects to <code>/ws?token=...</code>.</p>
  <div id="log"></div>

  <script>
    let ws = null;

    const $ = (id) => document.getElementById(id);
    const tokenInput = $("token");
    const channelInput = $("channel");
    const messageInput = $("message");
    const connectBtn = $("connect");
    const disconnectBtn = $("disconnect");
    const subscribeBtn = $("subscribe");
    const sendBtn = $("send");
    const logBox = $("log");

    function log(text) {
      const atBottom = logBox.scrollTop + logBox.clientHeight >= logBox.scrollHeight - 8;
      logBox.textContent += text + "\n";
      if (atBottom) {
        logBox.scrollTop = logBox.scrollHeight;
      }
    }

    function setConnected(connected) {
      connectBtn.disabled = connected;
      disconnectBtn.disabled = !connected;
      subscribeBtn.disabled = !connected;
      sendBtn.disabled = !connected;
    }

    function wsUrl(token) {
      const protocol = location.protocol === "https:" ? "wss:" : "ws:";
      return `${protocol}//${location.host}/ws?token=${encodeURIComponent(token)}`;
    }

    connectBtn.addEventListener("click", () => {
      const token = tokenInput.value.trim();
      if (!token) {
        log("[client] token is required");
        return;
      }
      if (ws && ws.readyState === WebSocket.OPEN) {
        log("[client] already connected");
        return;
      }

      const url = wsUrl(token);
      log(`[client] connecting: ${url}`);
      ws = new WebSocket(url);

      ws.addEventListener("open", () => {
        log("[ws] open");
        setConnected(true);
      });

      ws.addEventListener("message", (event) => {
        log(`[ws <-] ${event.data}`);
      });

      ws.addEventListener("error", () => {
        log("[ws] error");
      });

      ws.addEventListener("close", (event) => {
        log(`[ws] close code=${event.code} reason=${event.reason || ""}`);
        ws = null;
        setConnected(false);
      });
    });

    disconnectBtn.addEventListener("click", () => {
      if (!ws) {
        return;
      }
      ws.close();
    });

    subscribeBtn.addEventListener("click", () => {
      if (!ws || ws.readyState !== WebSocket.OPEN) {
        log("[client] websocket is not open");
        return;
      }

      const channel = channelInput.value.trim();
      if (!channel) {
        log("[client] channel is required");
        return;
      }

      const frame = { type: "subscribe", channel };
      ws.send(JSON.stringify(frame));
      log(`[ws ->] ${JSON.stringify(frame)}`);
    });

    sendBtn.addEventListener("click", () => {
      if (!ws || ws.readyState !== WebSocket.OPEN) {
        log("[client] websocket is not open");
        return;
      }

      const channel = channelInput.value.trim();
      const body = messageInput.value.trim();
      if (!channel || !body) {
        log("[client] channel and message are required");
        return;
      }

      const frame = { type: "chat_message", channel, body };
      ws.send(JSON.stringify(frame));
      log(`[ws ->] ${JSON.stringify(frame)}`);
      messageInput.value = "";
      messageInput.focus();
    });

    setConnected(false);
  </script>
</body>
</html>
"#;
