use crate::utils;
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
}

impl From<HyperResponse> for Response {
  fn from(response: HyperResponse) -> Self {
    Response::new(response)
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
