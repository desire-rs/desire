use crate::HyperResponse;
use bytes::Bytes;
use http_body_util::Full;
pub struct Response {
  pub inner: HyperResponse,
}

impl Response {
  fn new(response: HyperResponse) -> Self {
    Self { inner: response }
  }
  pub fn status(&self) -> hyper::StatusCode {
    self.inner.status()
  }
  pub fn with_status(status: u16, val: String) -> Self {
    hyper::http::Response::builder()
      .header(
        hyper::header::CONTENT_TYPE,
        mime::TEXT_PLAIN_UTF_8.to_string(),
      )
      .status(hyper::StatusCode::from_u16(status).unwrap())
      .body(Full::new(Bytes::from(val)))
      .unwrap()
      .into()
  }
  pub fn json<T>(payload: T) -> Response
  where
    T: serde::Serialize + Sized + Send + Sync + 'static,
  {
    let data = serde_json::to_string(&payload).unwrap();
    let response = hyper::http::Response::builder()
      .header(
        hyper::header::CONTENT_TYPE,
        mime::APPLICATION_JSON.to_string(),
      )
      .body(Full::new(Bytes::from(data)))
      .unwrap()
      .into();
    response
  }
}

impl From<HyperResponse> for Response {
  fn from(response: HyperResponse) -> Self {
    Response::new(response)
  }
}

impl From<()> for Response {
  fn from(_: ()) -> Self {
    hyper::http::Response::builder()
      .header(
        hyper::header::CONTENT_TYPE,
        mime::TEXT_PLAIN_UTF_8.to_string(),
      )
      .body(Full::new(Bytes::default()))
      .unwrap()
      .into()
  }
}

impl From<String> for Response {
  fn from(val: String) -> Self {
    hyper::http::Response::builder()
      .header(
        hyper::header::CONTENT_TYPE,
        mime::TEXT_PLAIN_UTF_8.to_string(),
      )
      .body(Full::new(Bytes::from(val)))
      .unwrap()
      .into()
  }
}

impl From<&'static str> for Response {
  fn from(val: &'static str) -> Self {
    hyper::http::Response::builder()
      .header(
        hyper::header::CONTENT_TYPE,
        mime::TEXT_PLAIN_UTF_8.to_string(),
      )
      .body(Full::new(Bytes::from(val)))
      .unwrap()
      .into()
  }
}

impl From<anyhow::Error> for Response {
  fn from(e: anyhow::Error) -> Self {
    Response::with_status(500, e.to_string())
  }
}
