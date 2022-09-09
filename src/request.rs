use crate::AnyResult;
use crate::HyperRequest;
use bytes::Buf;
pub struct Request {
  pub inner: HyperRequest,
}

impl Request {
  pub fn new(request: HyperRequest) -> Self {
    Self { inner: request }
  }
  pub fn request(request: HyperRequest) -> Self {
    Request::new(request)
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
}

impl From<HyperRequest> for Request {
  fn from(request: HyperRequest) -> Self {
    Request::new(request)
  }
}
