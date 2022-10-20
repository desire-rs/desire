use crate::Error;
use crate::Response;
use hyper::body::Body;
pub type AnyResult<T> = anyhow::Result<T, anyhow::Error>;
pub type Result<T = Response> = std::result::Result<T, Error>;

pub type HyperResponse = hyper::Response<Body>;
pub type HyperRequest = hyper::Request<Body>;