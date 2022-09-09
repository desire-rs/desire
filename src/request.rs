use crate::error::{invalid_param, missing_param};
use crate::AnyResult;
use crate::HyperRequest;
use crate::Result;
use bytes::Buf;
use route_recognizer::Params;
use std::net::SocketAddr;
use std::sync::Arc;

pub struct Request {
  pub inner: HyperRequest,
  pub params: Params,
  pub remote_addr: Option<Arc<SocketAddr>>,
}

impl Request {
  pub fn new(request: HyperRequest, remote_addr: Option<Arc<SocketAddr>>) -> Self {
    Self {
      inner: request,
      params: Params::new(),
      remote_addr: remote_addr,
    }
  }
  pub fn request(request: HyperRequest) -> Self {
    Request::new(request, None)
  }
  pub fn method(&self) -> &hyper::Method {
    self.inner.method()
  }
  pub fn uri(&self) -> &hyper::Uri {
    self.inner.uri()
  }
  pub fn path(&self) -> &str {
    self.inner.uri().path()
  }
  pub fn params(&self) -> &Params {
    &self.params
  }
  pub async fn body<T>(self) -> AnyResult<T>
  where
    T: serde::de::DeserializeOwned + Send + Sync + 'static,
  {
    let body = hyper::body::aggregate(self.inner).await?;
    let payload: T = serde_json::from_reader(body.reader())?;
    Ok(payload)
  }

  pub fn get_query<T>(&self) -> AnyResult<Option<T>>
  where
    T: serde::de::DeserializeOwned,
  {
    if let Some(query) = self.uri().query() {
      let result = serde_urlencoded::from_str::<T>(query)?;
      Ok(Some(result))
    } else {
      Ok(None)
    }
  }
  pub fn get_param<T>(&self, param: &str) -> Result<T>
  where
    T: std::str::FromStr,
    <T as std::str::FromStr>::Err: std::error::Error,
  {
    match self.params.find(param) {
      Some(param) => param
        .parse()
        .map_err(|e| invalid_param(param, std::any::type_name::<T>(), e)),
      None => Err(missing_param(param)),
    }
  }
}

impl From<HyperRequest> for Request {
  fn from(request: HyperRequest) -> Self {
    Request::new(request, None)
  }
}
