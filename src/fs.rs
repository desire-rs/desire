//! Static file serving with path-traversal protection, ETag
//! conditional requests, and MIME detection.

use std::path::{Path, PathBuf};

use bytes::Bytes;
use httpdate::fmt_http_date;
use hyper::StatusCode;
use hyper::header::{ETAG, HeaderValue, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED};
use mime_guess::MimeGuess;

use crate::context::Context;
use crate::response::raw_response;
use crate::{Error, Handler, Response, Result, body};

/// Serve files from a directory. Mount it on a wildcard route:
///
/// ```ignore
/// app.route("/static/{*path}", ServeDir::new("assets").cache_control("public, max-age=86400"));
/// ```
///
/// Traversal attempts (`..`, backslashes, absolute segments) and
/// dotfile segments (`.git`, `.env`, …) are rejected — pass
/// [`ServeDir::allow_dotfiles`] to serve them; symlinks that escape
/// the root are refused via canonicalization. Directory requests
/// serve `index.html` if present.
pub struct ServeDir {
  root: PathBuf,
  cache_control: Option<String>,
  allow_dotfiles: bool,
  fallback_file: Option<PathBuf>,
  files_listing: bool,
}

impl ServeDir {
  /// Serve files under this directory.
  pub fn new(root: impl Into<PathBuf>) -> Self {
    ServeDir {
      root: root.into(),
      cache_control: None,
      allow_dotfiles: false,
      fallback_file: None,
      files_listing: false,
    }
  }

  /// Render an HTML listing when a directory has no `index.html`
  /// (instead of a 404). File names are HTML-escaped.
  pub fn files_listing(mut self) -> Self {
    self.files_listing = true;
    self
  }

  /// Serve this file when the requested path does not exist — the
  /// single-page-app pattern (`fallback_file("index.html")` lets the
  /// client router take over unknown paths). The response is 200.
  pub fn fallback_file(mut self, path: impl Into<PathBuf>) -> Self {
    self.fallback_file = Some(path.into());
    self
  }

  /// Emit a `Cache-Control` header on successful responses.
  pub fn cache_control(mut self, value: impl Into<String>) -> Self {
    self.cache_control = Some(value.into());
    self
  }

  /// Serve dotfile segments (`.hidden`, `.well-known/…`). Off by
  /// default so secrets like `.git/config` or `.env` never leak.
  pub fn allow_dotfiles(mut self) -> Self {
    self.allow_dotfiles = true;
    self
  }
}

impl Handler<()> for ServeDir {
  fn call(&self, ctx: Context) -> crate::types::BoxFuture<'static, Result> {
    let root = self.root.clone();
    let cache_control = self.cache_control.clone();
    let allow_dotfiles = self.allow_dotfiles;
    let fallback_file = self.fallback_file.clone();
    let files_listing = self.files_listing;
    Box::pin(async move {
      // Wildcard routes carry `{*path}`; a route registered on the bare
      // prefix has no param and serves the root (i.e. index.html).
      let rel = ctx.param_raw("path").unwrap_or("");
      let decoded = crate::context::percent_decode(rel);
      let is_dotfile = decoded
        .split('/')
        .any(|seg| seg.starts_with('.') && seg != ".");
      if !decoded.is_empty()
        && (!is_safe_relative_path(&decoded) || (is_dotfile && !allow_dotfiles))
      {
        return Err(Error::not_found("file"));
      }

      let target = if decoded.is_empty() {
        root.clone()
      } else {
        root.join(&decoded)
      };
      let canonical = match tokio::fs::canonicalize(&target).await {
        Ok(canonical) => canonical,
        Err(_) => match &fallback_file {
          // SPA mode: unknown paths hand off to the client router.
          Some(fallback) => {
            let mut served = file_response(ctx.headers(), fallback).await?;
            if let Some(cc) = &cache_control {
              set(&mut served, hyper::header::CACHE_CONTROL, cc);
            }
            return Ok(served);
          }
          None => return Err(Error::not_found("file")),
        },
      };
      let canonical_root = tokio::fs::canonicalize(&root)
        .await
        .map_err(Error::internal)?;
      if !canonical.starts_with(&canonical_root) {
        return Err(Error::not_found("file"));
      }

      if is_dir(&canonical).await {
        let index = tokio::fs::canonicalize(canonical.join("index.html")).await;
        if let Ok(index) = index {
          let mut res = file_response(ctx.headers(), &index).await?;
          if let Some(cc) = &cache_control {
            set(&mut res, hyper::header::CACHE_CONTROL, cc);
          }
          return Ok(res);
        }
        if files_listing {
          let mut res = render_listing(ctx.path(), &canonical, allow_dotfiles).await?;
          if let Some(cc) = &cache_control {
            set(&mut res, hyper::header::CACHE_CONTROL, cc);
          }
          return Ok(res);
        }
        return Err(Error::not_found("file"));
      }

      let mut res = file_response(ctx.headers(), &canonical).await?;
      if let Some(cc) = &cache_control {
        set(&mut res, hyper::header::CACHE_CONTROL, cc);
      }
      Ok(res)
    })
  }
}

/// Serve a single fixed file:
///
/// ```ignore
/// app.route("/favicon.ico", ServeFile::new("assets/favicon.ico"));
/// ```
pub struct ServeFile {
  path: PathBuf,
  cache_control: Option<String>,
}

impl ServeFile {
  /// Serve this exact file.
  pub fn new(path: impl Into<PathBuf>) -> Self {
    ServeFile {
      path: path.into(),
      cache_control: None,
    }
  }

  /// Emit a `Cache-Control` header on successful responses.
  pub fn cache_control(mut self, value: impl Into<String>) -> Self {
    self.cache_control = Some(value.into());
    self
  }
}

impl Handler<()> for ServeFile {
  fn call(&self, ctx: Context) -> crate::types::BoxFuture<'static, Result> {
    let path = self.path.clone();
    let cache_control = self.cache_control.clone();
    Box::pin(async move {
      let canonical = tokio::fs::canonicalize(&path)
        .await
        .map_err(|_| Error::not_found("file"))?;
      let mut res = file_response(ctx.headers(), &canonical).await?;
      if let Some(cc) = &cache_control {
        set(&mut res, hyper::header::CACHE_CONTROL, cc);
      }
      Ok(res)
    })
  }
}

/// Render a minimal HTML directory listing. Names are escaped.
async fn render_listing(request_path: &str, dir: &Path, allow_dotfiles: bool) -> Result<Response> {
  let mut rows = Vec::new();
  let mut reader = tokio::fs::read_dir(dir).await.map_err(Error::internal)?;
  while let Some(entry) = reader.next_entry().await.map_err(Error::internal)? {
    let name = entry.file_name().to_string_lossy().into_owned();
    if !allow_dotfiles && name.starts_with('.') {
      continue;
    }
    let kind = if entry.file_type().await.map(|t| t.is_dir()).unwrap_or(false) {
      "dir"
    } else {
      "file"
    };
    rows.push((name, kind));
  }
  rows.sort();
  let rows: String = rows
    .iter()
    .map(|(name, kind)| format!("<tr><td>{}</td><td>{}</td></tr>", html_escape(name), kind))
    .collect();
  let html = format!(
    "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>Index of {}</title>\
     <style>body{{font:14px ui-monospace,monospace;margin:40px auto;max-width:640px}}\
     td{{padding:2px 12px 2px 0}}table{{border-collapse:collapse}}</style></head>\
     <body><h3>Index of {}</h3><table>{}</table></body></html>",
    html_escape(request_path),
    html_escape(request_path),
    rows
  );
  Ok(Response::html(html))
}

fn html_escape(s: &str) -> String {
  s.replace('&', "&amp;")
    .replace('<', "&lt;")
    .replace('>', "&gt;")
    .replace('"', "&quot;")
    .replace('\'', "&#39;")
}

async fn is_dir(path: &Path) -> bool {
  tokio::fs::metadata(path)
    .await
    .map(|m| m.is_dir())
    .unwrap_or(false)
}

/// Reject anything that could escape the root: `..`, empty, backslash,
/// NUL, or absolute segments.
fn is_safe_relative_path(rel: &str) -> bool {
  !rel.is_empty()
    && !rel.starts_with('/')
    && !rel.contains('\0')
    && !rel.contains('\\')
    && rel
      .split('/')
      .all(|seg| !seg.is_empty() && seg != ".." && seg != ".")
}

async fn file_response(headers: &hyper::HeaderMap, path: &Path) -> Result<Response> {
  let meta = tokio::fs::metadata(path)
    .await
    .map_err(|_| Error::not_found("file"))?;
  if meta.is_dir() {
    return Err(Error::not_found("file"));
  }
  let modified = meta.modified().map_err(Error::internal)?;
  let etag = {
    let nanos = modified
      .duration_since(std::time::UNIX_EPOCH)
      .map(|d| d.as_nanos())
      .unwrap_or(0);
    format!("\"{nanos:x}-{:x}\"", meta.len())
  };
  let last_modified = fmt_http_date(modified);

  // Conditional requests: prefer If-None-Match, fall back to
  // If-Modified-Since.
  if let Some(inm) = header_str(headers, IF_NONE_MATCH) {
    if if_none_match_matches(&inm, &etag) {
      return Ok(not_modified(&etag, &last_modified));
    }
  } else if let Some(ims) = header_str(headers, IF_MODIFIED_SINCE) {
    if let Ok(since) = httpdate::parse_http_date(&ims) {
      if modified <= since {
        return Ok(not_modified(&etag, &last_modified));
      }
    }
  }

  let bytes = tokio::fs::read(path)
    .await
    .map_err(|_| Error::not_found("file"))?;
  let mime = MimeGuess::from_path(path).first_or_octet_stream();

  // Range requests (single range only).
  if let Some(range) = header_str(headers, hyper::header::RANGE) {
    match parse_range(&range, meta.len()) {
      ParsedRange::Valid(start, end) => {
        return partial_response(
          path,
          start,
          end,
          meta.len(),
          mime.as_ref(),
          &etag,
          &last_modified,
        )
        .await;
      }
      ParsedRange::Unsatisfiable => return Ok(range_unsatisfiable(meta.len(), mime.as_ref())),
      ParsedRange::Ignore => {}
    }
  }

  let mut res = raw_response(StatusCode::OK, None, body::full(Bytes::from(bytes)));
  set(&mut res, hyper::header::CONTENT_TYPE, mime.as_ref());
  set(&mut res, ETAG, &etag);
  set(&mut res, LAST_MODIFIED, &last_modified);
  set(&mut res, hyper::header::ACCEPT_RANGES, "bytes");
  Ok(res)
}

/// Outcome of parsing a `Range` header against a resource length.
enum ParsedRange {
  /// A single satisfiable range, inclusive bounds.
  Valid(u64, u64),
  /// Syntactically valid but outside the resource → 416.
  Unsatisfiable,
  /// Malformed or unsupported (multi-range) → ignore, serve 200.
  Ignore,
}

fn parse_range(header: &str, len: u64) -> ParsedRange {
  let Some(spec) = header.strip_prefix("bytes=") else {
    return ParsedRange::Ignore;
  };
  if spec.contains(',') {
    // Multi-range unsupported; serving the full body is RFC-compliant.
    return ParsedRange::Ignore;
  }
  let Some((start_str, end_str)) = spec.trim().split_once('-') else {
    return ParsedRange::Ignore;
  };
  if start_str.is_empty() {
    // Suffix form: last N bytes.
    let Ok(n) = end_str.trim().parse::<u64>() else {
      return ParsedRange::Ignore;
    };
    if n == 0 || len == 0 {
      return ParsedRange::Unsatisfiable;
    }
    let start = len.saturating_sub(n);
    return ParsedRange::Valid(start, len - 1);
  }
  let Ok(start) = start_str.trim().parse::<u64>() else {
    return ParsedRange::Ignore;
  };
  let end = if end_str.is_empty() {
    len.saturating_sub(1)
  } else {
    match end_str.trim().parse::<u64>() {
      Ok(end) => end.min(len.saturating_sub(1)),
      Err(_) => return ParsedRange::Ignore,
    }
  };
  if start >= len {
    return ParsedRange::Unsatisfiable;
  }
  if end < start {
    // RFC: last-byte-pos < first-byte-pos is syntactically invalid.
    return ParsedRange::Ignore;
  }
  ParsedRange::Valid(start, end)
}

/// Serve a byte range with `seek` + bounded read — only the requested
/// slice is read from disk.
async fn partial_response(
  path: &Path,
  start: u64,
  end: u64,
  total: u64,
  mime: &str,
  etag: &str,
  last_modified: &str,
) -> Result<Response> {
  use tokio::io::{AsyncReadExt as _, AsyncSeekExt as _};

  let mut file = tokio::fs::File::open(path)
    .await
    .map_err(|_| Error::not_found("file"))?;
  file
    .seek(std::io::SeekFrom::Start(start))
    .await
    .map_err(Error::internal)?;
  let length = end - start + 1;
  let mut buf = Vec::with_capacity(length as usize);
  file
    .take(length)
    .read_to_end(&mut buf)
    .await
    .map_err(Error::internal)?;

  let mut res = raw_response(
    StatusCode::PARTIAL_CONTENT,
    None,
    body::full(Bytes::from(buf)),
  );
  set(&mut res, hyper::header::CONTENT_TYPE, mime);
  set(
    &mut res,
    hyper::header::CONTENT_RANGE,
    &format!("bytes {start}-{end}/{total}"),
  );
  set(&mut res, ETAG, etag);
  set(&mut res, LAST_MODIFIED, last_modified);
  set(&mut res, hyper::header::ACCEPT_RANGES, "bytes");
  Ok(res)
}

fn range_unsatisfiable(total: u64, mime: &str) -> Response {
  let mut res = raw_response(StatusCode::RANGE_NOT_SATISFIABLE, Some(mime), body::empty());
  set(
    &mut res,
    hyper::header::CONTENT_RANGE,
    &format!("bytes */{total}"),
  );
  res
}

fn not_modified(etag: &str, last_modified: &str) -> Response {
  let mut res = raw_response(StatusCode::NOT_MODIFIED, None, body::empty());
  set(&mut res, ETAG, etag);
  set(&mut res, LAST_MODIFIED, last_modified);
  res
}

fn if_none_match_matches(header: &str, etag: &str) -> bool {
  header.split(',').any(|candidate| {
    let candidate = candidate.trim();
    candidate == "*" || candidate == etag || candidate == format!("W/{etag}")
  })
}

fn header_str(headers: &hyper::HeaderMap, name: hyper::header::HeaderName) -> Option<String> {
  headers.get(name)?.to_str().ok().map(ToOwned::to_owned)
}

fn set(res: &mut Response, name: hyper::header::HeaderName, value: &str) {
  if let Ok(v) = HeaderValue::from_str(value) {
    res.headers_mut().insert(name, v);
  }
}
