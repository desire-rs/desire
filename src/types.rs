use crate::Error;
use crate::Response;
use bytes::Bytes;
use http_body_util::Full;
use hyper::Recv;
use serde::{Deserialize, Serialize};

pub type AnyResult<T> = anyhow::Result<T, anyhow::Error>;
pub type Result<T = Response> = std::result::Result<T, Error>;

pub type HyperResponse = hyper::Response<Full<Bytes>>;
pub type HyperRequest = hyper::Request<Recv>;

#[derive(Debug, Serialize, Deserialize)]
pub struct Resp<T = String> {
  success: bool,
  #[serde(skip_serializing_if = "Option::is_none")]
  data: Option<T>,
  #[serde(skip_serializing_if = "Option::is_none")]
  msg: Option<T>,
}
impl<T> Resp<T>
where
  T: Serialize + Send,
{
  pub fn data(data: T) -> Self {
    Resp {
      success: true,
      data: Some(data),
      msg: None,
    }
  }
  pub fn error(msg: T) -> Self {
    Resp {
      success: false,
      data: None,
      msg: Some(msg),
    }
  }
}