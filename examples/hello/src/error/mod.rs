use thiserror::Error;
#[derive(Debug, Error)]
pub enum Error {
  #[error("hyper error")]
  DesireError(#[from] desire::Error),
  #[error("json error")]
  JsonError(#[from] serde_json::Error),
}
