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
}
