//! A JSON API with shared state, the `Resp` envelope, path params and
//! body extraction. Run with `cargo run --example json_api`.

use desire::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
struct MemoryDb {
  users: std::sync::Arc<tokio::sync::RwLock<Vec<User>>>,
}

impl MemoryDb {
  fn new() -> Self {
    MemoryDb {
      users: std::sync::Arc::new(tokio::sync::RwLock::new(vec![User {
        id: 1,
        name: "alice".to_owned(),
      }])),
    }
  }
}

#[derive(Serialize, Deserialize, Clone)]
struct User {
  id: i64,
  name: String,
}

#[derive(Deserialize)]
struct CreateUser {
  name: String,
}

#[tokio::main]
async fn main() -> Result<()> {
  let app = App::new()
    .state(MemoryDb::new())
    .route("/users", get(list_users).post(create_user))
    .route("/users/{id}", get(get_user).delete(delete_user));

  app.run("0.0.0.0:3000").await
}

async fn list_users(ctx: Context) -> Result<Resp<Vec<User>>> {
  let db = ctx.state::<MemoryDb>()?;
  let users = db.users.read().await.clone();
  Ok(Resp::page(users, 1, 20, 1))
}

async fn get_user(ctx: Context) -> Result<Resp<User>> {
  let id: i64 = ctx.param("id")?;
  let db = ctx.state::<MemoryDb>()?;
  let users = db.users.read().await;
  let user = users.iter().find(|u| u.id == id).cloned();
  match user {
    Some(user) => Ok(Resp::ok(user)),
    None => Err(Error::not_found(format!("user {id}"))),
  }
}

async fn create_user(ctx: Context) -> Result<Resp<User>> {
  let input: CreateUser = ctx.json().await?;
  let db = ctx.state::<MemoryDb>()?;
  let mut users = db.users.write().await;
  let user = User {
    id: users.len() as i64 + 1,
    name: input.name,
  };
  users.push(user.clone());
  Ok(Resp::created(user))
}

async fn delete_user(ctx: Context) -> Result<Resp<()>> {
  let id: i64 = ctx.param("id")?;
  let db = ctx.state::<MemoryDb>()?;
  db.users.write().await.retain(|u| u.id != id);
  Ok(Resp::ok(()))
}
