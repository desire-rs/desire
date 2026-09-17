//! Server-Sent Events: wire format and response headers.

use desire::prelude::*;
use futures_core::Stream;

async fn events(_ctx: Context) -> Sse<impl Stream<Item = Result<Event, Error>>> {
  let items: Vec<Result<Event, Error>> = vec![
    Ok(Event::new().event("greet").data("hello")),
    Ok(Event::new().comment("keep-alive")),
    Ok(
      Event::new()
        .id("2")
        .json_data(&serde_json::json!({ "n": 1 })),
    ),
  ];
  Sse::new(tokio_stream::iter(items))
}

#[tokio::test]
async fn sse_streams_events_with_correct_headers() {
  let app = App::new().route("/events", get(events));
  let tc = TestClient::new(app);
  let res = tc.get("/events").send().await;
  res.assert_status_ok();
  assert_eq!(res.header("content-type"), Some("text/event-stream"));
  assert_eq!(res.header("cache-control"), Some("no-cache"));
  assert_eq!(
    res.text(),
    "event:greet\ndata:hello\n\n:keep-alive\n\nid:2\ndata:{\"n\":1}\n\n"
  );
}

#[tokio::test]
async fn keep_alive_emits_comments_while_idle() {
  use std::time::Duration;

  async fn slow(
    _ctx: Context,
  ) -> desire::sse::KeepAliveSse<impl Stream<Item = Result<Event, Error>>> {
    // one real event after 1s; the stream then ends — keep-alive
    // comments fill the silent window
    let stream = futures_util::stream::once(async move {
      tokio::time::sleep(Duration::from_secs(1)).await;
      Ok(Event::new().data("late"))
    });
    Sse::new(stream).keep_alive(Duration::from_millis(200))
  }

  let app = App::new().route("/slow", get(slow));
  let tc = TestClient::new(app);
  let res = tc.get("/slow").send().await;
  res.assert_status_ok();
  let text = res.text();
  assert!(text.contains("data:late"), "real event must arrive: {text}");
  let keep_alives = text.matches(":keep-alive").count();
  assert!(
    keep_alives >= 3,
    "expected several keep-alive comments, got {keep_alives} in: {text}"
  );
}
