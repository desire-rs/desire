//! OpenAPI document generation and Swagger UI (requires the `openapi`
//! feature).
#![cfg(feature = "openapi")]

use desire::openapi::{OpenApi, PathDoc};
use desire::prelude::*;
use schemars::JsonSchema;
use serde::Serialize;

#[derive(JsonSchema)]
#[allow(dead_code)] // exists to be described by the generated schema
struct ListQuery {
  page: i64,
  /// Only referenced through the generated schema.
  #[allow(dead_code)]
  q: Option<String>,
}

#[derive(JsonSchema)]
#[allow(dead_code)]
struct CreateUser {
  name: String,
}

#[derive(JsonSchema, Serialize)]
#[allow(dead_code)]
struct User {
  id: i64,
  name: String,
}

fn app() -> App {
  let doc = OpenApi::new("Test API", "1.0.0")
    .description("a test API")
    .path(
      PathDoc::get("/users/{id}")
        .summary("get one user")
        .tag("users")
        .path_param("id", "numeric user id")
        .query::<ListQuery>()
        .resp::<User>(200, "the user"),
    )
    .path(
      PathDoc::post("/users")
        .summary("create a user")
        .body::<CreateUser>()
        .resp::<User>(201, "created"),
    );
  App::new().openapi(doc)
}

#[tokio::test]
async fn openapi_json_document() {
  let tc = TestClient::new(app());
  let res = tc.get("/openapi.json").send().await;
  res.assert_status_ok();
  assert_eq!(res.header("content-type"), Some("application/json"));

  let doc: serde_json::Value = res.json().unwrap();
  assert_eq!(doc["openapi"], "3.0.3");
  assert_eq!(doc["info"]["title"], "Test API");
  assert_eq!(doc["info"]["version"], "1.0.0");

  let get = &doc["paths"]["/users/{id}"]["get"];
  assert_eq!(get["summary"], "get one user");
  assert_eq!(get["tags"][0], "users");

  // parameters: path param + schema-derived query params
  let params = get["parameters"].as_array().unwrap();
  let id = params.iter().find(|p| p["name"] == "id").unwrap();
  assert_eq!(id["in"], "path");
  assert_eq!(id["required"], true);
  let page = params.iter().find(|p| p["name"] == "page").unwrap();
  assert_eq!(page["in"], "query");
  assert_eq!(page["required"], true);
  let q = params.iter().find(|p| p["name"] == "q").unwrap();
  assert_eq!(q["required"], false);
  assert_eq!(q["schema"]["type"], "string");

  // envelope response schema wraps the User schema
  let schema200 = &get["responses"]["200"]["content"]["application/json"]["schema"];
  assert_eq!(schema200["properties"]["code"]["type"], "integer");
  assert_eq!(
    schema200["properties"]["data"]["properties"]["id"]["type"],
    "integer"
  );

  // request body schema
  let post = &doc["paths"]["/users"]["post"];
  let body = &post["requestBody"]["content"]["application/json"]["schema"];
  assert_eq!(body["properties"]["name"]["type"], "string");
}

#[tokio::test]
async fn swagger_ui_page_served() {
  let tc = TestClient::new(app());
  let res = tc.get("/docs").send().await;
  res.assert_status_ok();
  assert!(
    res
      .header("content-type")
      .unwrap_or("")
      .starts_with("text/html")
  );
  assert!(res.text().contains("swagger-ui"));
  assert!(res.text().contains("/openapi.json"));
}
