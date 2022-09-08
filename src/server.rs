use crate::HyperRequest;
use crate::HyperResponse;
use crate::Result;
use crate::Router;
use hyper::server::conn::Http;
use hyper::service::Service;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Context;
use std::task::Poll;
use tokio::net::TcpListener;
use tracing::{error, info};

pub struct Svc {
  pub router: Arc<Router>,
}

impl Service<HyperRequest> for Svc {
  type Response = HyperResponse;
  type Error = crate::error::Error;
  type Future = Pin<Box<dyn Future<Output = Result<Self::Response>> + Send>>;

  fn poll_ready(&mut self, _: &mut Context) -> Poll<Result<()>> {
    Poll::Ready(Ok(()))
  }

  fn call(&mut self, req: HyperRequest) -> Self::Future {
    let router = self.router.clone();
    let res = dispatch(req, router);
    Box::pin(async { res.await })
  }
}

pub async fn dispatch(req: HyperRequest, router: Arc<Router>) -> Result<HyperResponse> {
  let res = router.dispatch(req, None).await?;
  Ok(res.inner)
}

pub struct Server {
  addr: SocketAddr,
}
impl Server {
  pub fn bind(addr: SocketAddr) -> Self {
    Server { addr }
  }

  pub async fn run(&self, router: Router) -> Result<()> {
    let addr: SocketAddr = self.addr.into();
    let listener = TcpListener::bind(addr).await?;
    info!("Listening on http://{}", addr);
    let router = Arc::new(router);
    loop {
      let router = router.clone();
      let (stream, _) = listener.accept().await?;
      tokio::task::spawn(async move {
        if let Err(err) = Http::new().serve_connection(stream, Svc { router }).await {
          error!("Failed to serve connection: {:?}", err);
        }
      });
    }
  }
}
