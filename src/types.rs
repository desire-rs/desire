use crate::Context;
use crate::Error;
use bytes::Bytes;
use http_body_util::Full;
use hyper::Recv;
pub type AnyResult<T> = anyhow::Result<T, anyhow::Error>;
pub type Result<T = Context> = std::result::Result<T, Error>;

pub type HyperResponse = hyper::Response<Full<Bytes>>;
pub type HyperRequest = hyper::Request<Recv>;
