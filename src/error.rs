use crate::Response;
use crate::StatusCode;
use thiserror::Error;
#[derive(Debug, Error)]
pub enum Error {
  #[error("hyper error")]
  HyperError(#[from] hyper::Error),
  #[error("json error")]
  JsonError(#[from] serde_json::Error),
  #[error("IO error")]
  IOError(#[from] std::io::Error),
  #[error("any error")]
  AnyError(#[from] anyhow::Error),
  #[error("addr parse error")]
  AddrParseError(#[from] std::net::AddrParseError),
  #[error("missing url param {name:?}")]
  MissingParam { name: String },
  #[error("invalid param {name:?} as {expected:?}, {err:?}")]
  InvalidParam {
    name: String,
    expected: &'static str,
    err: String,
  },
  #[error("error msg {msg:?}")]
  Message { msg: String },
}

pub fn miss_param(name: &str) -> Error {
  Error::MissingParam {
    name: name.to_string(),
  }
}

pub fn invalid_param(
  name: impl ToString,
  expected: &'static str,
  err: impl std::error::Error,
) -> Error {
  Error::InvalidParam {
    name: name.to_string(),
    expected,
    err: err.to_string(),
  }
}

impl From<Error> for Response {
  fn from(err: Error) -> Self {
    Response::with_status(500, err.to_string())
  }
}
