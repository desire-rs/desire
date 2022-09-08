#![deny(warnings)]
#![warn(rust_2018_idioms)]
mod middleware;

use desire::types::Result;
use desire::Context;
use desire::Response;
use desire::Router;
use desire::StatusCode;
#[tokio::main]
async fn main() -> Result<()> {
  let mut app = Router::new();
  app.get("/", echo_hello);
  app.get("/liveness", liveness);
  app.with(middleware::Logger);
  let addr = "127.0.0.1:1337".parse()?;
  let serve = desire::new(addr);
  serve.run(app).await?;
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
