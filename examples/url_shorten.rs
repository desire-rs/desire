//! Dogfood app: a URL shortener. POST a long URL, get a short code,
//! GET the short code to redirect, track visits.
//!
//! ```text
//! cargo run --example url_shorten
//! # open http://localhost:3000
//! ```

use desire::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone, Default)]
struct Store {
  // code -> (target url, visits)
  links: Arc<RwLock<HashMap<String, Link>>>,
  seq: Arc<AtomicCounter>,
}

#[derive(Clone, Default)]
struct AtomicCounter(Arc<std::sync::atomic::AtomicU64>);

impl AtomicCounter {
  fn next(&self) -> u64 {
    self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
  }
}

#[derive(Clone)]
struct Link {
  url: String,
  visits: u64,
}

#[derive(Deserialize)]
struct ShortenInput {
  url: String,
}

#[derive(Serialize)]
struct ShortenOutput {
  code: String,
  short_url: String,
}

const FORM: &str = r##"<!DOCTYPE html>
<html lang="zh"><head><meta charset="utf-8"><title>desire 短链</title>
<style>
 body { font: 15px/1.6 -apple-system, "PingFang SC", sans-serif; background: #0d1117;
        color: #e6edf3; max-width: 560px; margin: 60px auto; padding: 0 20px; }
 input { width: 100%; padding: 10px 14px; border-radius: 8px; border: 1px solid #30363d;
         background: #161b22; color: #e6edf3; font-size: 15px; }
 button { margin-top: 12px; padding: 9px 20px; border-radius: 8px; border: 1px solid #30363d;
          background: #21262d; color: #e6edf3; cursor: pointer; }
 ul { padding-left: 18px; color: #8b949e; }
 code { color: #79c0ff; }
 a { color: #79c0ff; }
</style></head><body>
<h1>&lt;<code>desire</code>/&gt; 短链服务</h1>
<p>输入一个长链接,缩短它。短链 302 跳转,访问次数会被统计。</p>
<input id="url" placeholder="https://example.com/some/very/long/path" />
<button onclick="go()">缩短</button>
<ul id="list"></ul>
<script>
const esc = (s) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));
async function refresh() {
  const res = await fetch("/api/list");
  const body = await res.json();
  document.getElementById("list").innerHTML = (body.data || [])
    .map((l) => `<li><code>/${l.code}</code> → ${esc(l.url)} · ${l.visits} 次</li>`).join("");
}
async function go() {
  const url = document.getElementById("url").value;
  const res = await fetch("/api/shorten", {
    method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ url }),
  });
  const body = await res.json();
  document.getElementById("url").value = body.msg === "ok" ? location.origin + "/" + body.data.code : "错误: " + body.msg;
  refresh();
}
refresh();
</script></body></html>
"##;

fn encode(mut n: u64) -> String {
  const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
  let mut code = Vec::new();
  loop {
    code.push(CHARS[(n % 36) as usize]);
    n /= 36;
    if n == 0 {
      break;
    }
  }
  code.reverse();
  String::from_utf8(code).expect("base36 is ascii")
}

#[tokio::main]
async fn main() -> Result<()> {
  let app = App::new()
    .state(Store::default())
    .route("/", get(|| async { Html(FORM) }))
    .route(
      "/api/shorten",
      post(|ctx: Context| async move {
        let input: ShortenInput = ctx.json().await?;
        if !input.url.starts_with("http://") && !input.url.starts_with("https://") {
          return Err(Error::msg("url must start with http:// or https://"));
        }

        let store = ctx.state::<Store>()?;
        let code = encode(store.seq.next() + 36); // skip the 1-char space
        let host = ctx.header("host").unwrap_or("localhost:3000").to_owned();
        store.links.write().await.insert(
          code.clone(),
          Link { url: input.url, visits: 0 },
        );
        Ok(Resp::ok(ShortenOutput {
          code,
          short_url: format!("http://{host}/"),
        }))
      }),
    )
    .route(
      "/api/list",
      get(|ctx: Context| async move {
        let store = ctx.state::<Store>()?;
        let links = store.links.read().await;
        let mut items: Vec<serde_json::Value> = links
          .iter()
          .map(|(code, link)| {
            serde_json::json!({ "code": code, "url": link.url, "visits": link.visits })
          })
          .collect();
        items.sort_by_key(|item| item["code"].as_str().map(ToOwned::to_owned));
        Ok::<_, Error>(Resp::ok(items))
      }),
    )
    .route(
      "/{code}/stats",
      get(|ctx: Context| async move {
        let code = ctx.param_raw("code")?;
        let store = ctx.state::<Store>()?;
        let link = store.links.read().await.get(code).cloned();
        match link {
          Some(link) => Ok(Resp::ok(serde_json::json!({
              "code": code,
              "url": link.url,
              "visits": link.visits,
          }))),
          None => Err(Error::not_found(format!("unknown code `{code}`"))),
        }
      }),
    )
    .route(
      "/{code}",
      get(|ctx: Context| async move {
        let code = ctx.param_raw("code")?.to_owned();
        let store = ctx.state::<Store>()?;
        let mut links = store.links.write().await;
        let Some(link) = links.get_mut(&code) else {
          return Err(Error::not_found(format!("unknown code `{code}`")));
        };
        link.visits += 1;
        Ok(Response::redirect(StatusCode::FOUND, &link.url))
      }),
    );

  app.run("0.0.0.0:3000").await
}

