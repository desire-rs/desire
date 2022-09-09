use crate::Context;
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
}

impl From<Error> for Context {
  fn from(err: Error) -> Self {
    Context::with_status(StatusCode::from_u16(500).unwrap(), err.to_string())
  }
}
