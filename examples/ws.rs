//! WebSocket echo server. Requires the `ws` feature:
//!
//! ```text
//! cargo run --example ws --features ws
//! ```

use desire::prelude::*;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

#[tokio::main]
async fn main() -> Result<()> {
  let app = App::new().route("/ws", get(echo));
  app.run("0.0.0.0:3000").await
}

async fn echo(ctx: Context) -> Result {
  let ws = ctx.websocket()?;
  Ok(ws.on_upgrade(|mut socket| async move {
    while let Some(Ok(msg)) = socket.next().await {
      if (msg.is_text() || msg.is_binary()) && socket.send(msg).await.is_err() {
        break;
      }
    }
  }))
}

/// A handler that rejects non-websocket requests with a 400 envelope.
#[allow(dead_code)]
fn message_note() -> Message {
  Message::text("WebSocket is a Stream + Sink of Message")
}
