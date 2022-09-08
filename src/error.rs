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
