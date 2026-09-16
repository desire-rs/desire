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
/// app.route("/static/{*path}", ServeDir::new("assets"));
/// ```
///
/// Traversal attempts (`..`, backslashes, absolute segments) are
/// rejected; symlinks that escape the root are refused via
/// canonicalization. Directory requests serve `index.html` if present.
pub struct ServeDir {
  root: PathBuf,
}

impl ServeDir {
  /// Serve files under this directory.
  pub fn new(root: impl Into<PathBuf>) -> Self {
    ServeDir { root: root.into() }
  }
}

impl Handler<()> for ServeDir {
  fn call(&self, ctx: Context) -> crate::types::BoxFuture<'static, Result> {
    let root = self.root.clone();
    Box::pin(async move {
      // Wildcard routes carry `{*path}`; a route registered on the bare
      // prefix has no param and serves the root (i.e. index.html).
      let rel = ctx.param_raw("path").unwrap_or("");
      let decoded = crate::context::percent_decode(rel);
      if !decoded.is_empty() && !is_safe_relative_path(&decoded) {
        return Err(Error::not_found("file"));
      }

      let target = if decoded.is_empty() {
        root.clone()
      } else {
        root.join(&decoded)
      };
      let canonical = tokio::fs::canonicalize(&target)
        .await
        .map_err(|_| Error::not_found("file"))?;
      let canonical_root = tokio::fs::canonicalize(&root)
        .await
        .map_err(Error::internal)?;
      if !canonical.starts_with(&canonical_root) {
        return Err(Error::not_found("file"));
      }

      let target = if is_dir(&canonical).await {
        canonical.join("index.html")
      } else {
        canonical
      };
      file_response(ctx.headers(), &target).await
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
}

impl ServeFile {
  /// Serve this exact file.
  pub fn new(path: impl Into<PathBuf>) -> Self {
    ServeFile { path: path.into() }
  }
}

impl Handler<()> for ServeFile {
  fn call(&self, ctx: Context) -> crate::types::BoxFuture<'static, Result> {
    let path = self.path.clone();
    Box::pin(async move {
      let canonical = tokio::fs::canonicalize(&path)
        .await
        .map_err(|_| Error::not_found("file"))?;
      file_response(ctx.headers(), &canonical).await
    })
  }
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

  let mut res = raw_response(StatusCode::OK, None, body::full(Bytes::from(bytes)));
  set(&mut res, hyper::header::CONTENT_TYPE, mime.as_ref());
  set(&mut res, ETAG, &etag);
  set(&mut res, LAST_MODIFIED, &last_modified);
  Ok(res)
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
