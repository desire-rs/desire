use desire::Response;
pub struct Logger;
#[desire::async_trait]
impl desire::Middleware for Logger {
  async fn handle(&self, req: desire::Request, next: desire::Next<'_>) -> Response {
    let method = req.method().to_string();
    let uri = req.uri().to_string();
    let res = next.run(req).await;
    let status = res.status().to_string();
    let log = format!("{} {} {}", method, uri, status);
    println!("{}", log);
    res
  }
}
