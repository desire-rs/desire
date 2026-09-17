//! WebSocket end-to-end test: a real hyper server on a real socket,
//! driven by a real tungstenite client. Requires the `ws` feature.
#![cfg(feature = "ws")]

use desire::prelude::*;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

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

async fn spawn_server() -> u16 {
  let (tx, rx) = tokio::sync::oneshot::channel();
  let app = App::new().route("/ws", get(echo));
  tokio::spawn(async move {
    let _ = Server::new(app)
      .bind("127.0.0.1:0")
      .unwrap()
      .on_bound(move |addr| {
        tx.send(addr).ok();
      })
      .run()
      .await;
  });
  rx.await.expect("server reported no bound address").port()
}

#[tokio::test]
async fn websocket_echo_roundtrip() {
  let port = spawn_server().await;
  let url = format!("ws://127.0.0.1:{port}/ws");

  // Retry until the server accepts connections.
  let (mut ws, _) = loop {
    match tokio_tungstenite::connect_async(&url).await {
      Ok(pair) => break pair,
      Err(_) => tokio::time::sleep(std::time::Duration::from_millis(50)).await,
    }
  };

  ws.send(Message::text("ping")).await.unwrap();
  ws.send(Message::binary(vec![1, 2, 3])).await.unwrap();

  let first = tokio::time::timeout(std::time::Duration::from_secs(3), ws.next())
    .await
    .expect("timed out")
    .expect("stream ended")
    .expect("ws error");
  assert_eq!(first, Message::text("ping"));

  let second = tokio::time::timeout(std::time::Duration::from_secs(3), ws.next())
    .await
    .expect("timed out")
    .expect("stream ended")
    .expect("ws error");
  assert_eq!(second, Message::binary(vec![1, 2, 3]));

  let _ = ws.close(None).await;
}

#[tokio::test]
async fn non_websocket_request_to_ws_route_is_400() {
  let port = spawn_server().await;
  let tc = TestClient::new(App::new().route("/ws", get(echo)));
  let res = tc.get(&format!("http://127.0.0.1:{port}/ws")).send().await;
  res.assert_status(StatusCode::BAD_REQUEST);
}
