//! `Resp<T>` — the unified response envelope.
//!
//! Every API response shares one JSON shape: `{"code": 0, "msg": "ok", "data": ...}`.
//! Business success is `code == 0`; framework errors render the same shape with
//! a non-zero code (see [`Error`](crate::Error)).

use hyper::StatusCode;
use serde::Serialize;

/// The unified response envelope.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct Resp<T = ()> {
  /// Business code: `0` means success, anything else is an error code
  /// defined by the application or the framework.
  /// Business code: `0` means success.
  pub code: i64,
  /// The message, `"ok"` on success.
  pub msg: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  /// The payload, absent on errors.
  pub data: Option<T>,
  #[serde(skip_serializing_if = "Option::is_none")]
  /// Pagination info, present only for [`Resp::page`].
  pub page: Option<Page>,
  #[serde(skip)]
  pub(crate) status: Option<StatusCode>,
}

/// Pagination info, present only when built via [`Resp::page`].
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct Page {
  /// 1-based page index.
  pub index: i64,
  /// Page size.
  pub size: i64,
  /// Total number of items across all pages.
  pub total: i64,
}

impl<T> Resp<T> {
  /// A success envelope wrapping `data` (code 0, HTTP 200).
  /// A success envelope wrapping `data` (code 0, HTTP 200).
  pub fn ok(data: T) -> Self {
    Resp {
      code: 0,
      msg: "ok".to_owned(),
      data: Some(data),
      page: None,
      status: None,
    }
  }

  /// Success with HTTP 201 Created.
  /// Success with HTTP 201 Created.
  pub fn created(data: T) -> Self {
    let mut resp = Resp::ok(data);
    resp.status = Some(StatusCode::CREATED);
    resp
  }

  /// A paginated success envelope: `{"code":0,"data":[...],"page":{...}}`.
  pub fn page(data: Vec<T>, index: i64, size: i64, total: i64) -> Resp<Vec<T>> {
    Resp {
      code: 0,
      msg: "ok".to_owned(),
      data: Some(data),
      page: Some(Page { index, size, total }),
      status: None,
    }
  }

  /// Override the HTTP status (business errors default to HTTP 200).
  /// Override the HTTP status (business errors default to HTTP 200).
  pub fn with_status(mut self, status: StatusCode) -> Self {
    self.status = Some(status);
    self
  }

  /// The HTTP status this envelope renders with.
  /// The HTTP status this envelope renders with.
  pub fn status(&self) -> StatusCode {
    self.status.unwrap_or(StatusCode::OK)
  }
}

impl Resp<()> {
  /// A business error envelope: custom code and message, HTTP 200.
  /// A business error envelope: custom code and message, HTTP 200.
  pub fn err(code: i64, msg: impl Into<String>) -> Self {
    Resp {
      code,
      msg: msg.into(),
      data: None,
      page: None,
      status: None,
    }
  }

  /// A business error envelope with code 500.
  /// A business error envelope with code 500.
  pub fn err_msg(msg: impl Into<String>) -> Self {
    Resp::err(500, msg)
  }
}
