//! Custom and built-in middleware. Run with `cargo run --example middleware`.

use std::time::Duration;

use desire::prelude::*;

struct CurrentUser(String);

#[tokio::main]
async fn main() -> Result<()> {
  let app = App::new()
    .with(logger())
    .with(cors(CorsConfig::default()))
    .with(auth)
    .route("/me", get(me));

  app.run("0.0.0.0:3000").await
}

/// Sets a per-request value handlers can read. Errors short-circuit
/// with a unified error envelope.
async fn auth(mut ctx: Context, next: Next) -> Result {
  let token = ctx
    .header("authorization")
    .ok_or_else(Error::unauthorized)?;
  if !token.starts_with("Bearer ") {
    return Err(Error::unauthorized());
  }
  ctx.insert(CurrentUser(token[7..].to_owned()));
  next.run(ctx).await
}

async fn me(ctx: Context) -> Resp<String> {
  let user = ctx.get::<CurrentUser>().expect("auth ran");
  Resp::ok(format!("hello, {}!", user.0))
}

/// A timeout middleware is built in; here for reference:
#[allow(dead_code)]
fn five_second_timeout() -> impl Middleware {
  timeout(Duration::from_secs(5))
}
