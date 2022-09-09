#![warn(rust_2018_idioms)]

use desire::types::Result;
use desire::Context;
use desire::HyperRequest;
use desire::Router;
#[tokio::main]
async fn main() -> Result<()> {
  let mut app = Router::new();
  app.get("/", echo_hello);
  app.get("/liveness", liveness);
  let addr = "127.0.0.1:1337".parse()?;
  let serve = desire::new(addr);
  serve.run(app).await?;
  println!("hello");
  Ok(())
}

pub async fn echo_hello(req: HyperRequest, ctx: Context) -> Result {
  Ok("Hello World!".into())
}
pub async fn liveness(req: HyperRequest, _ctx: Context) -> Result {
  Ok("this is liveness!".into())
}
