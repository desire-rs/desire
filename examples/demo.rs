//! An interactive, single-page demo of desire's realtime and form
//! features: SSE ticks, a WebSocket echo, multipart upload, and the
//! unified response envelope. Requires the `ws` feature:
//!
//! ```text
//! cargo run --example demo --features ws
//! # open http://localhost:3000
//! ```

use std::{
  convert::Infallible,
  sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
  },
};

use desire::prelude::*;
use futures_core::Stream;
use futures_util::{SinkExt, StreamExt};
use tokio_stream::wrappers::IntervalStream;

const PAGE: &str = r##"<!DOCTYPE html>
<html lang="zh">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>desire — live demo</title>
<style>
  :root { color-scheme: dark; }
  * { box-sizing: border-box; }
  body {
    margin: 0; min-height: 100vh; background: #0d1117; color: #e6edf3;
    font: 15px/1.6 -apple-system, "PingFang SC", "Segoe UI", sans-serif;
    display: flex; justify-content: center; padding: 48px 20px;
  }
  main { width: 100%; max-width: 880px; }
  h1 { font-size: 26px; margin: 0 0 4px; letter-spacing: .5px; }
  h1 code { color: #ff6b5e; }
  .sub { color: #8b949e; margin: 0 0 32px; font-size: 14px; }
  .sub code { color: #79c0ff; }
  .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(260px, 1fr)); gap: 16px; }
  .card {
    background: #161b22; border: 1px solid #21262d; border-radius: 10px;
    padding: 18px 20px; display: flex; flex-direction: column; gap: 12px;
  }
  .card h2 { font-size: 13px; margin: 0; color: #8b949e; text-transform: uppercase; letter-spacing: 1.2px; }
  .card h2 .dot { display: inline-block; width: 8px; height: 8px; border-radius: 50%; background: #30363d; margin-right: 8px; vertical-align: 1px; }
  .card h2 .dot.on { background: #3fb950; box-shadow: 0 0 6px #3fb95088; }
  pre {
    margin: 0; background: #0d1117; border-radius: 8px; padding: 12px;
    font: 12.5px/1.55 ui-monospace, "SF Mono", Menlo, monospace;
    color: #a5d6ff; height: 190px; overflow-y: auto; white-space: pre-wrap; word-break: break-all;
  }
  .row { display: flex; gap: 8px; }
  input[type=text], input[type=file] {
    flex: 1; background: #0d1117; border: 1px solid #30363d; border-radius: 8px;
    color: #e6edf3; padding: 8px 12px; font-size: 14px; min-width: 0;
  }
  input[type=text]:focus { outline: none; border-color: #79c0ff; }
  button {
    background: #21262d; color: #e6edf3; border: 1px solid #30363d; border-radius: 8px;
    padding: 8px 16px; font-size: 14px; cursor: pointer; white-space: nowrap;
  }
  button:hover { background: #30363d; border-color: #8b949e; }
  footer { margin-top: 28px; color: #8b949e; font-size: 13px; }
  footer a { color: #79c0ff; text-decoration: none; }
</style>
</head>
<body>
<main>
  <h1>&lt;<code>desire</code>/&gt; live demo</h1>
  <p class="sub">一个页面跑通 SSE · WebSocket · multipart 上传 · 统一响应信封 —— 由 <code>examples/demo.rs</code> 单文件驱动</p>
  <div class="grid">
    <section class="card">
      <h2><span class="dot" id="sse-dot"></span>Server-Sent Events <code>/ticks</code></h2>
      <pre id="sse-log"></pre>
      <div class="row"><button id="sse-clear">清空</button></div>
    </section>
    <section class="card">
      <h2><span class="dot" id="ws-dot"></span>WebSocket Echo <code>/ws</code></h2>
      <pre id="ws-log"></pre>
      <div class="row">
        <input type="text" id="ws-input" placeholder="发一条消息…" value="hello, desire!" />
        <button id="ws-send">发送</button>
      </div>
    </section>
    <section class="card">
      <h2>Multipart 上传 <code>/upload</code></h2>
      <pre id="up-log">选择文件,看看信封里回了什么。</pre>
      <div class="row">
        <input type="file" id="up-file" />
        <button id="up-send">上传</button>
      </div>
    </section>
    <section class="card">
      <h2>统一信封 API <code>/api/time</code></h2>
      <pre id="api-log">点击按钮,观察 {"code":0,"msg":"ok",…}。</pre>
      <div class="row"><button id="api-call">GET /api/time</button></div>
    </section>
  </div>
  <footer>powered by <a href="https://github.com/desire-rs/desire">desire</a> — the ergonomic Rust web framework</footer>
</main>
<script>
const $ = (id) => document.getElementById(id);
const log = (id, text) => {
  const el = $(id);
  el.textContent = text + "\n" + el.textContent;
};

// ---- SSE ----
const sse = new EventSource("/ticks");
sse.onopen = () => $("sse-dot").classList.add("on");
sse.addEventListener("tick", (e) => log("sse-log", e.data));
sse.onerror = () => $("sse-dot").classList.remove("on");
$("sse-clear").onclick = () => ($("sse-log").textContent = "");

// ---- WebSocket ----
let ws;
const wsConnect = () => {
  ws = new WebSocket(`ws://${location.host}/ws`);
  ws.onopen = () => $("ws-dot").classList.add("on");
  ws.onclose = () => { $("ws-dot").classList.remove("on"); setTimeout(wsConnect, 1500); };
  ws.onmessage = (e) => log("ws-log", "← " + e.data);
};
wsConnect();
$("ws-send").onclick = () => {
  const v = $("ws-input").value;
  if (ws.readyState === 1 && v) { log("ws-log", "→ " + v); ws.send(v); }
};
$("ws-input").addEventListener("keydown", (e) => e.key === "Enter" && $("ws-send").click());

// ---- upload ----
$("up-send").onclick = async () => {
  const f = $("up-file").files[0];
  if (!f) return log("up-log", "先选一个文件。");
  const fd = new FormData();
  fd.append("title", f.name);
  fd.append("attachment", f);
  const res = await fetch("/upload", { method: "POST", body: fd });
  const body = await res.json();
  log("up-log", JSON.stringify(body, null, 1));
};

// ---- envelope api ----
$("api-call").onclick = async () => {
  const res = await fetch("/api/time");
  const body = await res.json();
  log("api-log", JSON.stringify(body, null, 1));
};
</script>
</body>
</html>
"##;

#[tokio::main]
async fn main() -> Result<()> {
  let app = App::new()
    .route("/", get(|| async { Html(PAGE) }))
    .route("/ticks", get(ticks))
    .route("/ws", get(echo))
    .route("/upload", post(upload))
    .route(
      "/api/time",
      get(|| async { Resp::ok(serde_json::json!({ "now": chrono_now() })) }),
    );

  app.run("0.0.0.0:3000").await
}

async fn ticks(_ctx: Context) -> Sse<impl Stream<Item = Result<Event, Infallible>> + Send> {
  let n = Arc::new(AtomicU64::new(0));
  let stream =
    IntervalStream::new(tokio::time::interval(std::time::Duration::from_secs(1))).map(move |_| {
      let count = n.fetch_add(1, Ordering::Relaxed) + 1;
      Ok(
        Event::new()
          .event("tick")
          .id(format!("{count}"))
          .json_data(&serde_json::json!({ "count": count, "at": chrono_now() })),
      )
    });
  Sse::new(stream)
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

async fn upload(ctx: Context) -> Result<Resp<serde_json::Value>> {
  let form = ctx.form_data().await?;
  let title = form.field("title").unwrap_or_default().to_owned();
  let files: Vec<serde_json::Value> = form
    .files()
    .iter()
    .map(|f| {
      serde_json::json!({
        "filename": f.filename,
        "content_type": f.content_type,
        "bytes": f.bytes.len(),
      })
    })
    .collect();
  Ok(Resp::ok(
    serde_json::json!({ "title": title, "files": files }),
  ))
}

/// Local time as `HH:MM:SS` without pulling in chrono.
fn chrono_now() -> String {
  let secs = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map(|d| d.as_secs())
    .unwrap_or(0);
  let today = secs - secs % 86_400;
  let (h, m, s) = ((secs / 3600) % 24, (secs / 60) % 60, secs % 60);
  format!("{h:02}:{m:02}:{s:02} (unix day {})", today / 86_400)
}
