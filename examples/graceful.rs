//! Graceful shutdown: in-flight requests drain before exit.
//! Run with `cargo run --example graceful`, then press Ctrl+C.

use desire::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
  let app = App::new().route("/", get(|| async { Resp::ok("bye soon") }));

  Server::new(app)
    .bind("0.0.0.0:3000")?
    .graceful_shutdown(shutdown_signal())
    .run()
    .await
}

async fn shutdown_signal() {
  let _ = tokio::signal::ctrl_c().await;
  println!("shutting down gracefully…");
}
