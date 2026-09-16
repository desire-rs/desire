//! Server-Sent Events. Run with `cargo run --example sse`, then
//! `curl -N localhost:3000/ticks`.

use std::{
  convert::Infallible,
  sync::atomic::{AtomicUsize, Ordering},
  time::Duration,
};

use desire::prelude::*;
use futures_core::Stream;
use tokio_stream::StreamExt as _;
use tokio_stream::wrappers::IntervalStream;

#[tokio::main]
async fn main() -> Result<()> {
  let app = App::new().route("/ticks", get(ticks));
  app.run("0.0.0.0:3000").await
}

async fn ticks(_ctx: Context) -> Sse<impl Stream<Item = Result<Event, Infallible>> + Send> {
  let n = AtomicUsize::new(0);
  let stream = IntervalStream::new(tokio::time::interval(Duration::from_secs(1))).map(move |_| {
    let count = n.fetch_add(1, Ordering::Relaxed) + 1;
    Ok(
      Event::new()
        .event("tick")
        .id(format!("{count}"))
        .json_data(&serde_json::json!({ "count": count })),
    )
  });
  Sse::new(stream)
}
