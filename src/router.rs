use crate::{
  Context, DynEndpoint, Endpoint, HyperRequest, Middleware, Next, Response, Result,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
pub struct Router {
  pub prefix: Option<String>,
  pub middlewares: Vec<Arc<dyn Middleware>>,
  pub routes: HashMap<hyper::Method, route_recognizer::Router<Box<DynEndpoint>>>,
  pub not_found_handler: Box<DynEndpoint>,
}

async fn default_handler(_ctx: Context) -> Result {
  Ok(Response::with_status(
    hyper::StatusCode::from_u16(404).unwrap(),
    "handler not found".to_string(),
  ))
}

impl Router {
  pub fn new() -> Self {
    Router {
      prefix: None,
      middlewares: Vec::new(),
      routes: HashMap::new(),
      not_found_handler: Box::new(default_handler),
    }
  }
  pub fn at(&mut self, method: hyper::Method, route: &str, dest: impl Endpoint) {
    let mut path = String::from("");
    if let Some(prefix) = &self.prefix {
      path.push_str(prefix.as_str());
    }
    path.push_str(route);
    self
      .routes
      .entry(method)
      .or_insert_with(route_recognizer::Router::new)
      .add(path.as_str(), Box::new(dest));
  }

  pub fn get(&mut self, route: &str, dest: impl Endpoint) {
    self.at(hyper::Method::GET, route, dest);
  }

  pub fn post(&mut self, route: &str, dest: impl Endpoint) {
    self.at(hyper::Method::POST, route, dest);
  }

  pub fn delete(&mut self, route: &str, dest: impl Endpoint) {
    self.at(hyper::Method::DELETE, route, dest);
  }

  pub fn patch(&mut self, route: &str, dest: impl Endpoint) {
    self.at(hyper::Method::PATCH, route, dest);
  }

  pub fn put(&mut self, route: &str, dest: impl Endpoint) {
    self.at(hyper::Method::PUT, route, dest);
  }

  pub fn options(&mut self, route: &str, dest: impl Endpoint) {
    self.at(hyper::Method::OPTIONS, route, dest);
  }

  pub fn head(&mut self, route: &str, dest: impl Endpoint) {
    self.at(hyper::Method::HEAD, route, dest);
  }

  pub fn connect(&mut self, route: &str, dest: impl Endpoint) {
    self.at(hyper::Method::CONNECT, route, dest);
  }

  pub async fn dispatch(
    &self,
    req: HyperRequest,
    remote_addr: Option<SocketAddr>,
  ) -> Result {
    let method = req.method();
    let path = req.uri().path();

    let mut params = route_recognizer::Params::new();
    let endpoint = match self.routes.get(method) {
      Some(route) => match route.recognize(path) {
        Ok(m) => {
          params = m.params().to_owned();
          &***m.handler()
        }
        Err(_e) => &*self.not_found_handler,
      },
      None => &*self.not_found_handler,
    };

    let mut ctx = Context::new(req, remote_addr);
    ctx.params = params;
    let next = Next {
      endpoint: endpoint,
      middlewares: &self.middlewares,
    };
    next.run(ctx).await
  }
}
