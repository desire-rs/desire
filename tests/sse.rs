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
