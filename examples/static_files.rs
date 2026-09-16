//! Static file serving. Run with `cargo run --example static_files`,
//! then open http://localhost:3000/static/hello.txt.

use desire::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
  let app = App::new()
    .route("/static/{*path}", ServeDir::new("assets"))
    .route("/hello.txt", ServeFile::new("assets/hello.txt"));

  app.run("0.0.0.0:3000").await
}
