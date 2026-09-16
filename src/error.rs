//! The framework error type and its rendering into the unified
//! `Resp` envelope.

use std::fmt;

use hyper::StatusCode;

use crate::{IntoResponse, Resp, Response};

/// The framework error. Every error carries enough context to render a
/// precise message; internal details never leak to the client.
#[derive(Debug, thiserror::Error)]
pub enum Error {
  /// The request itself is invalid (HTTP 400).
  #[error("bad request: {0}")]
  BadRequest(String),
  /// Authentication missing or invalid (HTTP 401).
  #[error("unauthorized")]
  Unauthorized,
  /// Authenticated but not permitted (HTTP 403).
  #[error("forbidden")]
  Forbidden,
  /// No such resource (HTTP 404).
  #[error("not found: {0}")]
  NotFound(String),
  /// Path matched but the method did not (HTTP 405).
  #[error("method not allowed")]
  MethodNotAllowed,
  /// Request body exceeded the configured limit (HTTP 413).
  #[error("payload too large")]
  PayloadTooLarge,
  /// The downstream chain exceeded its deadline (HTTP 504).
  #[error("request timeout")]
  Timeout,
  /// A path parameter failed to parse.
  #[error("invalid param `{name}`, expected `{expected}`: {detail}")]
  Param {
    /// The parameter name from the route pattern.
    name: String,
    /// The Rust type the parameter was parsed into.
    expected: &'static str,
    /// The parse failure, rendered.
    detail: String,
  },
  /// The JSON body failed to deserialize (HTTP 400, with serde detail).
  #[error("invalid json body: {0}")]
  Json(#[from] serde_json::Error),
  /// The query string failed to deserialize (HTTP 400, with serde detail).
  #[error("invalid query string: {0}")]
  Query(#[from] serde_urlencoded::de::Error),
  /// The raw body was unreadable or of the wrong shape (HTTP 400).
  #[error("invalid body: {0}")]
  Body(String),
  /// An application-defined business error, rendered with its own code
  /// and message at HTTP 200.
  #[error("{msg}")]
  Business {
    /// The application-defined business code.
    code: i64,
    /// The client-facing message.
    msg: String,
  },
  /// A server-side failure. The source is logged; the client sees a
  /// sanitized "internal server error" (HTTP 500).
  #[error("internal server error")]
  Internal {
    /// Details for logging only; never sent to the client.
    logged: String,
  },
  /// An I/O failure, mapped to a sanitized 500.
  #[error("io error: {0}")]
  Io(#[from] std::io::Error),
}

impl Error {
  /// A 400 error with a custom message.
  pub fn bad_request(msg: impl Into<String>) -> Self {
    Error::BadRequest(msg.into())
  }

  /// A 401 error.
  pub fn unauthorized() -> Self {
    Error::Unauthorized
  }

  /// A 403 error.
  pub fn forbidden() -> Self {
    Error::Forbidden
  }

  /// A 404 error with a custom message.
  pub fn not_found(msg: impl Into<String>) -> Self {
    Error::NotFound(msg.into())
  }

  /// A shorthand for a 400 error with a custom message.
  /// A shorthand for a 400 error with a custom message.
  pub fn msg(msg: impl Into<String>) -> Self {
    Error::BadRequest(msg.into())
  }

  /// A business error with a custom code. Rendered with HTTP 200; the
  /// client decides success by the envelope `code`.
  /// A business error with a custom code. Rendered with HTTP 200; the
  /// client decides success by the envelope `code`.
  pub fn business(code: i64, msg: impl Into<String>) -> Self {
    Error::Business {
      code,
      msg: msg.into(),
    }
  }

  /// An internal error: logs the source and returns a sanitized 500.
  /// An internal error: logs the source and returns a sanitized 500.
  pub fn internal(err: impl fmt::Display) -> Self {
    let logged = err.to_string();
    tracing::error!(error = %logged, "internal error");
    Error::Internal { logged }
  }

  /// The HTTP status this error maps to.
  /// The HTTP status this error maps to.
  pub fn status(&self) -> StatusCode {
    match self {
      Error::BadRequest(_)
      | Error::Param { .. }
      | Error::Json(_)
      | Error::Query(_)
      | Error::Body(_) => StatusCode::BAD_REQUEST,
      Error::Unauthorized => StatusCode::UNAUTHORIZED,
      Error::Forbidden => StatusCode::FORBIDDEN,
      Error::NotFound(_) => StatusCode::NOT_FOUND,
      Error::MethodNotAllowed => StatusCode::METHOD_NOT_ALLOWED,
      Error::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
      Error::Timeout => StatusCode::GATEWAY_TIMEOUT,
      Error::Business { .. } => StatusCode::OK,
      Error::Internal { .. } | Error::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
  }

  /// The business code for the envelope. Business errors keep their
  /// code; everything else uses its HTTP status number.
  /// The business code for the envelope. Business errors keep their
  /// code; everything else uses its HTTP status number.
  pub fn business_code(&self) -> i64 {
    match self {
      Error::Business { code, .. } => *code,
      other => i64::from(other.status().as_u16()),
    }
  }

  /// The message rendered into the envelope. Internal details are
  /// stripped; they were already logged in [`Error::internal`].
  /// The message rendered into the envelope. Internal details are
  /// stripped; they were already logged in [`Error::internal`].
  pub fn user_message(&self) -> String {
    match self {
      Error::Internal { .. } | Error::Io(_) => "internal server error".to_owned(),
      Error::Json(e) => format!("invalid json body: {e}"),
      Error::Query(e) => format!("invalid query string: {e}"),
      Error::Param { name, expected, .. } => {
        format!("invalid param `{name}`, expected `{expected}`")
      }
      other => other.to_string(),
    }
  }

  /// Render the error as a unified `Resp` envelope response body.
  /// Render the error as a unified `Resp` envelope response body.
  pub fn to_resp(&self) -> Resp<()> {
    Resp::err(self.business_code(), self.user_message()).with_status(self.status())
  }
}

impl From<Error> for Response {
  fn from(err: Error) -> Self {
    err.to_resp().into_response()
  }
}

/// Convenience: convert any displayable failure into an internal error.
pub trait IntoInternal<T> {
  /// Map the error into an [`Error::Internal`], logging it.
  fn internal(self) -> std::result::Result<T, Error>;
}

impl<T, E: fmt::Display> IntoInternal<T> for std::result::Result<T, E> {
  /// Map the error into an [`Error::Internal`], logging it.
  fn internal(self) -> std::result::Result<T, Error> {
    self.map_err(Error::internal)
  }
}

/// Context for a failing route param parse.
pub(crate) fn param_error(name: &str, expected: &'static str, err: impl fmt::Display) -> Error {
  Error::Param {
    name: name.to_owned(),
    expected,
    detail: err.to_string(),
  }
}

/// Context when a required route param is absent.
pub(crate) fn missing_param(name: &str) -> Error {
  Error::Param {
    name: name.to_owned(),
    expected: "present",
    detail: "missing route parameter".to_owned(),
  }
}
