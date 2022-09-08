use desire::Middleware;
pub struct Logger;
use desire::async_trait;
use std::time::Instant;

#[async_trait]
impl Middleware for Logger {
  async fn handle(&self, ctx: desire::Context, next: desire::Next<'_>) -> desire::Result {
    let start = Instant::now();
    let method = ctx.request().method().to_string();
    let path = ctx.request().uri().path().to_string();
    let res = next.run(ctx).await?;
    println!(
      "{} {} {} {}ms",
      method,
      path,
      res.response().status().to_string(),
      start.elapsed().as_millis()
    );
    Ok(res)
  }
}
