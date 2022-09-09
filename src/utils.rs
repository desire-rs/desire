use bytes::Bytes;
use http_body_util::Full;
use hyper::http::Response;

use crate::HyperResponse;
pub fn mk_resp(val: String) -> HyperResponse {
  Response::builder()
    .header(
      hyper::header::CONTENT_TYPE,
      mime::TEXT_PLAIN_UTF_8.to_string(),
    )
    .body(Full::new(Bytes::from(val)))
    .unwrap()
}
