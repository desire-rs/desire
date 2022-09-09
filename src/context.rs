use crate::error::{invalid_param, missing_param};
use crate::Result;
use route_recognizer::Params;
use std::net::SocketAddr;
use std::sync::Arc;
pub struct Context {
  pub params: Params,
  pub remote_addr: Arc<SocketAddr>,
}

impl Context {
  pub fn new(remote_addr: Arc<SocketAddr>) -> Self {
    Context {
      params: Params::new(),
      remote_addr,
    }
  }
  pub fn set_params(&mut self, params: Params) {
    self.params = params;
  }
  pub fn get_params(&self) -> &Params {
    &self.params
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
