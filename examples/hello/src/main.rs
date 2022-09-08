#![deny(warnings)]
#![warn(rust_2018_idioms)]
use desire::server::Server;
use desire::types::Result;
use desire::Context;
use desire::Response;
use desire::StatusCode;
use desire::Router;
#[tokio::main]
async fn main() -> Result<()> {
  let mut router = Router::new();
  router.get("/", echo_hello);
  router.get("/liveness", liveness);
  let addr = "127.0.0.1:1337".parse()?;
  let serve = Server::new(addr);
  serve.run(router).await?;
  println!("hello");
  Ok(())
}

pub async fn echo_hello(_ctx: Context) -> Result<Response> {
  let msg = "Hello World!";
  Ok(Response::with_status(
    StatusCode::from_u16(200).unwrap(),
    msg.to_string(),
  ))
}
pub async fn liveness(_ctx: Context) -> Result<Response> {
  let msg = "this is liveness!";
  Ok(Response::with_status(
    StatusCode::from_u16(200).unwrap(),
    msg.to_string(),
  ))
}
