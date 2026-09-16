//! WebSocket support (requires the `ws` feature).
//!
//! ```ignore
//! async fn echo(ctx: Context) -> Result {
//!   let ws = ctx.websocket()?;
//!   Ok(ws.on_upgrade(|socket| async move {
//!     // socket: Stream + Sink of Message
//!   }))
//! }
//! ```

use hyper::StatusCode;
use hyper::header::{CONNECTION, HeaderValue, SEC_WEBSOCKET_ACCEPT, UPGRADE};
use tokio_tungstenite::tungstenite::handshake::derive_accept_key;
use tokio_tungstenite::tungstenite::protocol::Role;

use crate::response::raw_response;
use crate::{Response, body};

/// A completed server-side WebSocket session: a `Stream + Sink` of
/// [`Message`] over the upgraded connection.
pub type WebSocket =
  tokio_tungstenite::WebSocketStream<hyper_util::rt::TokioIo<hyper::upgrade::Upgraded>>;

/// A single WebSocket protocol message (Text/Binary/Ping/Pong/Close).
pub use tokio_tungstenite::tungstenite::Message;

/// The handle produced by [`Context::websocket`](crate::Context::websocket).
pub struct WebSocketUpgrade {
  pub(crate) key: String,
  pub(crate) on_upgrade: hyper::upgrade::OnUpgrade,
}

impl WebSocketUpgrade {
  /// Complete the handshake and run `callback` with the established
  /// socket. The 101 response is returned immediately; the callback
  /// runs on its own task once the client's final handshake bytes land.
  pub fn on_upgrade<F, Fut>(self, callback: F) -> Response
  where
    F: FnOnce(WebSocket) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
  {
    let accept = derive_accept_key(self.key.as_bytes());
    let mut res = raw_response(StatusCode::SWITCHING_PROTOCOLS, None, body::empty());
    {
      let headers = res.headers_mut();
      headers.insert(UPGRADE, HeaderValue::from_static("websocket"));
      headers.insert(CONNECTION, HeaderValue::from_static("Upgrade"));
      headers.insert(
        SEC_WEBSOCKET_ACCEPT,
        HeaderValue::from_str(&accept).expect("derived accept key is a valid header"),
      );
    }

    tokio::spawn(async move {
      match self.on_upgrade.await {
        Ok(upgraded) => {
          // hyper's `Upgraded` speaks hyper's rt traits; TokioIo bridges
          // them to tokio's AsyncRead/AsyncWrite for tungstenite.
          let io = hyper_util::rt::TokioIo::new(upgraded);
          let socket = WebSocket::from_raw_socket(io, Role::Server, None).await;
          callback(socket).await;
        }
        Err(e) => tracing::debug!(error = %e, "websocket upgrade did not complete"),
      }
    });
    res
  }
}
