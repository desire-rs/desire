//! The smallest useful desire app. Run with `cargo run --example hello`.

use desire::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
  let app = App::new().route("/hello/{name}", get(hello));
  app.run("0.0.0.0:3000").await
}

async fn hello(ctx: Context) -> Result<Resp<String>> {
  let name = ctx.param_raw("name")?;
  Ok(Resp::ok(format!("hello, {name}!")))
}
