//! The HTTP server: hyper-util's auto builder (HTTP/1.1 + HTTP/2 on the
//! same port), optional TLS, graceful shutdown, and a concurrency cap.

use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;

use hyper::service::Service;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto;
use hyper_util::server::graceful::GracefulShutdown;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tracing::info;

use crate::app::{BuiltApp, dispatch};
use crate::types::{BoxFuture, HyperRequest, HyperResponse};
use crate::{App, Result};

/// The `hyper` service bridging into the dispatch pipeline. One per
/// connection.
#[derive(Clone)]
pub(crate) struct Svc {
  pub app: Arc<BuiltApp>,
  pub remote_addr: Option<SocketAddr>,
}

impl Service<HyperRequest> for Svc {
  type Response = HyperResponse;
  type Error = std::convert::Infallible;
  type Future = BoxFuture<'static, std::result::Result<HyperResponse, std::convert::Infallible>>;

  fn call(&self, req: HyperRequest) -> Self::Future {
    let app = Arc::clone(&self.app);
    let remote_addr = self.remote_addr;
    Box::pin(async move { Ok(dispatch(&app, req, remote_addr).await) })
  }
}

/// The server builder. Construct via [`Server::new`] and [`Server::bind`].
pub struct Server {
  app: App,
  addr: SocketAddr,
  shutdown: Option<BoxFuture<'static, ()>>,
  concurrency: Option<usize>,
  on_bound: Option<Box<dyn FnOnce(SocketAddr) + Send>>,
  #[cfg(feature = "tls")]
  tls: Option<crate::tls::TlsConfig>,
}

impl Server {
  /// Wrap an [`App`] for serving.
  pub fn new(app: App) -> Self {
    Server {
      app,
      addr: "127.0.0.1:3000".parse().expect("default addr"),
      shutdown: None,
      concurrency: None,
      on_bound: None,
      #[cfg(feature = "tls")]
      tls: None,
    }
  }

  /// Parse and set the bind address. Returns an error for malformed
  /// addresses instead of panicking.
  pub fn bind(mut self, addr: &str) -> Result<Self> {
    self.addr = addr
      .parse()
      .map_err(|e| crate::Error::msg(format!("invalid bind address `{addr}`: {e}")))?;
    Ok(self)
  }

  /// Enable TLS (requires the `tls` feature). With TLS on, ALPN
  /// negotiates HTTP/2 and HTTP/1.1.
  #[cfg(feature = "tls")]
  pub fn tls(mut self, config: crate::tls::TlsConfig) -> Self {
    self.tls = Some(config);
    self
  }

  /// Resolve into graceful shutdown once the future completes (e.g.
  /// Ctrl+C). In-flight connections drain before the process exits.
  pub fn graceful_shutdown<F>(mut self, signal: F) -> Self
  where
    F: Future<Output = ()> + Send + 'static,
  {
    self.shutdown = Some(Box::pin(signal));
    self
  }

  /// Cap concurrent connections. Excess connections wait on accept.
  pub fn concurrency_limit(mut self, limit: usize) -> Self {
    self.concurrency = Some(limit);
    self
  }

  /// Invoke a callback with the actually bound address once the
  /// listener is up — essential when binding port `0` for tests or
  /// service meshes.
  ///
  /// ```no_run
  /// use desire::prelude::*;
  /// # async fn demo(app: App) -> desire::Result<()> {
  /// Server::new(app)
  ///     .bind("127.0.0.1:0")?
  ///     .on_bound(|addr| println!("listening on {addr}"))
  ///     .run()
  ///     .await
  /// # }
  /// # fn main() {}
  /// ```
  pub fn on_bound(mut self, f: impl FnOnce(SocketAddr) + Send + 'static) -> Self {
    self.on_bound = Some(Box::new(f));
    self
  }

  /// Serve until the shutdown signal fires (or forever).
  pub async fn run(self) -> Result<()> {
    let Server {
      app,
      addr,
      shutdown,
      concurrency,
      mut on_bound,
      #[cfg(feature = "tls")]
      tls,
    } = self;

    let app = Arc::new(app.build());
    let listener = TcpListener::bind(addr)
      .await
      .map_err(|e| crate::Error::internal(format!("failed to bind {addr}: {e}")))?;
    let bound = listener.local_addr().map_err(crate::Error::internal)?;
    if let Some(f) = on_bound.take() {
      f(bound);
    }
    info!(%bound, "listening");

    let semaphore = concurrency.map(|n| Arc::new(Semaphore::new(n)));
    let graceful = GracefulShutdown::new();
    // No signal configured: never resolves.
    let mut shutdown: BoxFuture<'static, ()> =
      shutdown.unwrap_or_else(|| Box::pin(std::future::pending()));

    loop {
      tokio::select! {
        accepted = listener.accept() => {
          let (stream, remote_addr) = match accepted {
            Ok(pair) => pair,
            Err(e) => {
              tracing::warn!(error = %e, "accept failed");
              continue;
            }
          };

          // Server closed between the check and the acquire: None.
          let permit = match &semaphore {
            Some(sem) => Arc::clone(sem).acquire_owned().await.ok(),
            None => None,
          };

          let app = Arc::clone(&app);
          let watcher = graceful.watcher();
          #[cfg(feature = "tls")]
          let tls = tls.clone();
          tokio::spawn(async move {
            #[cfg(feature = "tls")]
            if let Some(config) = tls {
              match config.accept(stream).await {
                Ok(tls_stream) => {
                  serve_connection(tls_stream, Some(remote_addr), app, watcher, permit).await;
                }
                Err(e) => {
                  tracing::debug!(error = %e, "tls handshake failed");
                }
              }
              return;
            }
            serve_connection(stream, Some(remote_addr), app, watcher, permit).await;
          });
        }
        _ = &mut shutdown => break,
      }
    }

    drop(listener);
    graceful.shutdown().await;
    info!(%addr, "shutdown complete");
    Ok(())
  }
}

async fn serve_connection<S>(
  io: S,
  remote_addr: Option<SocketAddr>,
  app: Arc<BuiltApp>,
  graceful: hyper_util::server::graceful::Watcher,
  _permit: Option<OwnedSemaphorePermit>,
) where
  S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
  let svc = Svc { app, remote_addr };
  let builder = auto::Builder::new(TokioExecutor::new());
  let conn = builder.serve_connection_with_upgrades(TokioIo::new(io), svc);
  if let Err(e) = graceful.watch(conn).await {
    tracing::debug!(error = %e, "connection error");
  }
  drop(_permit);
}
