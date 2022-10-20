use crate::HyperRequest;
use crate::HyperResponse;
use crate::Result;
use crate::Router;
use hyper::service::Service;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tracing::info;

pub struct Svc {
  pub router: Arc<Router>,
}

impl Service<HyperRequest> for Svc {
  type Response = HyperResponse;
  type Error = crate::error::Error;
  type Future = Pin<Box<dyn Future<Output = Result<Self::Response>> + Send>>;
  fn poll_ready(&mut self, _: &mut Context) -> Poll<std::result::Result<(), Self::Error>> {
    Poll::Ready(Ok(()))
  }
  fn call(&mut self, req: HyperRequest) -> Self::Future {
    let router = self.router.clone();
    let res = dispatch(req, router);
    Box::pin(async { res.await })
  }
}

struct MakeSvc {
  pub router: Arc<Router>,
}

impl<T> Service<T> for MakeSvc {
  type Response = Svc;
  type Error = hyper::Error;
  type Future =
    Pin<Box<dyn Future<Output = std::result::Result<Self::Response, Self::Error>> + Send>>;

  fn poll_ready(&mut self, _: &mut Context) -> Poll<std::result::Result<(), Self::Error>> {
    Poll::Ready(Ok(()))
  }

  fn call(&mut self, _req: T) -> Self::Future {
    let router = self.router.clone();
    let fut = async move { Ok(Svc { router }) };
    Box::pin(fut)
  }
}

pub async fn dispatch(req: HyperRequest, router: Arc<Router>) -> Result<HyperResponse> {
  let response = router.dispatch(req.into()).await?;
  Ok(response.inner)
}

pub struct Server {
  addr: SocketAddr,
}

impl Server {
  pub fn bind(addr: &str) -> Self {
    Server {
      addr: addr.parse().unwrap(),
    }
  }

  pub async fn run(&self, router: Router) -> Result<()> {
    let addr: SocketAddr = self.addr.into();
    info!("Listening on http://{}", addr);
    let router = Arc::new(router);
    // let remote_addr = Arc::new(remote_addr);
    let server = hyper::Server::bind(&addr).serve(MakeSvc { router });
    server.await?;
    Ok(())
  }
}
