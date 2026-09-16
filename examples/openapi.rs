//! OpenAPI docs + Swagger UI. Requires the `openapi` feature:
//!
//! ```text
//! cargo run --example openapi --features openapi
//! # then open http://localhost:3000/docs
//! ```

use desire::openapi::{OpenApi, PathDoc};
use desire::prelude::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Default)]
struct Db;

#[derive(JsonSchema, Serialize)]
#[allow(dead_code)]
struct User {
  id: i64,
  name: String,
}

#[derive(JsonSchema, Deserialize)]
struct CreateUser {
  name: String,
}

#[derive(JsonSchema)]
#[allow(dead_code)]
struct ListQuery {
  page: i64,
  q: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
  let docs = OpenApi::new("User API", "1.0.0")
    .description("Demonstrates desire's OpenAPI generation")
    .path(
      PathDoc::get("/users")
        .summary("List users")
        .tag("users")
        .query::<ListQuery>()
        .resp::<Vec<User>>(200, "paginated users"),
    )
    .path(
      PathDoc::get("/users/{id}")
        .summary("Fetch one user")
        .tag("users")
        .path_param("id", "numeric user id")
        .resp::<User>(200, "the user")
        .empty_resp(404, "unknown id"),
    )
    .path(
      PathDoc::post("/users")
        .summary("Create a user")
        .tag("users")
        .body::<CreateUser>()
        .resp::<User>(201, "created"),
    );

  let app = App::new()
    .state(Db)
    .route("/users", get(list_users).post(create_user))
    .openapi(docs);

  app.run("0.0.0.0:3000").await
}

async fn list_users(_ctx: Context) -> Resp<Vec<User>> {
  Resp::page(
    vec![User {
      id: 1,
      name: "alice".to_owned(),
    }],
    1,
    20,
    1,
  )
}

async fn create_user(ctx: Context) -> Result<Resp<User>> {
  let input: CreateUser = ctx.json().await?;
  Ok(Resp::created(User {
    id: 1,
    name: input.name,
  }))
}
