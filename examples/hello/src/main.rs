mod config;
mod controller;
mod middleware;
mod service;
mod model;
mod types;
mod error;
use config::ENV_NAME;
use std::env;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;
#[tokio::main]
async fn main() -> desire::Result<()> {
  let subscriber = FmtSubscriber::builder()
    .with_max_level(Level::INFO)
    .finish();
  tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
  let arguments: Vec<String> = env::args().collect();
  let env_name = arguments.get(1).expect("env name must be provided");
  let env_file = format!("env/{}.env", env_name);
  dotenv::from_filename(env_file).ok();

  info!("ENV_NAME: {}", ENV_NAME.to_string());
  let mut app = desire::Router::new();
  app.with(middleware::Logger);
  app.get("/", controller::hello);
  app.get("/hello", controller::hello);
  app.get("/user", controller::get_users);
  app.get("/query", controller::get_query);
  app.get("/user/:id", controller::get_user_by_id);
  app.post("/user", controller::create_users);

  let addr = "127.0.0.1:1337".parse()?;
  let serve = desire::new(addr);
  serve.run(app).await?;
  info!("hello");
  Ok(())
}
