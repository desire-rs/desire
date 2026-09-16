//! OpenAPI 3.0 document generation (requires the `openapi` feature).
//!
//! Declare your API surface with a builder; desire serves the JSON
//! document at `/openapi.json` and a Swagger UI at `/docs`.
//!
//! ```ignore
//! use desire::openapi::{OpenApi, PathDoc};
//!
//! let app = App::new()
//!     .route("/users/{id}", get(get_user))
//!     .openapi(
//!         OpenApi::new("My API", "1.0.0")
//!             .path(
//!                 PathDoc::get("/users/{id}")
//!                     .summary("Fetch one user")
//!                     .tag("users")
//!                     .path_param("id", "numeric user id")
//!                     .resp::<User>(200, "the user"),
//!             ),
//!     );
//! ```
//!
//! Payload types need `#[derive(schemars::JsonSchema)]`.

use serde_json::{Value, json};

/// The API document: title, version, and a list of declared paths.
#[derive(Debug, Default)]
pub struct OpenApi {
  title: String,
  version: String,
  description: Option<String>,
  paths: Vec<PathDoc>,
}

impl OpenApi {
  /// A document with a title and an API version.
  pub fn new(title: impl Into<String>, version: impl Into<String>) -> Self {
    OpenApi {
      title: title.into(),
      version: version.into(),
      description: None,
      paths: Vec::new(),
    }
  }

  /// Human-readable API description.
  pub fn description(mut self, description: impl Into<String>) -> Self {
    self.description = Some(description.into());
    self
  }

  /// Declare one operation.
  pub fn path(mut self, path: PathDoc) -> Self {
    self.paths.push(path);
    self
  }

  /// Serialize as an OpenAPI 3.0 JSON document.
  pub fn to_json(&self) -> Value {
    let mut paths = serde_json::Map::new();
    for p in &self.paths {
      let entry = paths.entry(p.path.clone()).or_insert_with(|| json!({}));
      entry[p.method.to_lowercase()] = p.operation_json();
    }
    let mut info = json!({
        "title": self.title,
        "version": self.version,
    });
    if let Some(description) = &self.description {
      info["description"] = json!(description);
    }
    json!({
        "openapi": "3.0.3",
        "info": info,
        "paths": paths,
    })
  }
}

/// One declared operation: `PathDoc::get("/users/{id}")…`.
#[derive(Debug)]
pub struct PathDoc {
  method: &'static str,
  path: String,
  summary: Option<String>,
  description: Option<String>,
  tag: Option<String>,
  params: Vec<Value>,
  request_schema: Option<Value>,
  responses: Vec<(i64, String, Option<Value>)>,
}

impl PathDoc {
  /// Declare a `GET` operation.
  pub fn get(path: impl Into<String>) -> Self {
    PathDoc::new("GET", path)
  }

  /// Declare a `POST` operation.
  pub fn post(path: impl Into<String>) -> Self {
    PathDoc::new("POST", path)
  }

  /// Declare a `PUT` operation.
  pub fn put(path: impl Into<String>) -> Self {
    PathDoc::new("PUT", path)
  }

  /// Declare a `PATCH` operation.
  pub fn patch(path: impl Into<String>) -> Self {
    PathDoc::new("PATCH", path)
  }

  /// Declare a `DELETE` operation.
  pub fn delete(path: impl Into<String>) -> Self {
    PathDoc::new("DELETE", path)
  }

  fn new(method: &'static str, path: impl Into<String>) -> Self {
    PathDoc {
      method,
      path: path.into(),
      summary: None,
      description: None,
      tag: None,
      params: Vec::new(),
      request_schema: None,
      responses: Vec::new(),
    }
  }

  /// One-line summary (shown next to the path in Swagger UI).
  pub fn summary(mut self, summary: impl Into<String>) -> Self {
    self.summary = Some(summary.into());
    self
  }

  /// Longer operation description.
  pub fn description(mut self, description: impl Into<String>) -> Self {
    self.description = Some(description.into());
    self
  }

  /// Group the operation under a tag.
  pub fn tag(mut self, tag: impl Into<String>) -> Self {
    self.tag = Some(tag.into());
    self
  }

  /// Declare a path parameter: `.path_param("id", "numeric user id")`.
  pub fn path_param(mut self, name: &str, description: &str) -> Self {
    self.params.push(json!({
        "name": name,
        "in": "path",
        "required": true,
        "schema": { "type": "string" },
        "description": description,
    }));
    self
  }

  /// Declare query parameters from a struct's JSON schema. Optional
  /// (`Option<T>`) fields become non-required parameters.
  pub fn query<T: schemars::JsonSchema>(mut self) -> Self {
    let schema = serde_json::to_value(schemars::schema_for!(T)).unwrap_or(Value::Null);
    let properties = schema
      .get("properties")
      .cloned()
      .unwrap_or_else(|| json!({}));
    let required = schema.get("required").and_then(Value::as_array).cloned();
    if let Some(props) = properties.as_object() {
      for (name, prop) in props {
        let is_required = required
          .as_ref()
          .is_some_and(|r| r.iter().any(|v| v.as_str() == Some(name)));
        self.params.push(json!({
            "name": name,
            "in": "query",
            "required": is_required,
            "schema": strip_null(prop),
        }));
      }
    }
    self
  }

  /// Declare a JSON request body with the schema of `T`.
  pub fn body<T: schemars::JsonSchema>(mut self) -> Self {
    self.request_schema = Some(type_schema::<T>());
    self
  }

  /// Declare a success response whose envelope data is `T`.
  pub fn resp<T: schemars::JsonSchema>(mut self, status: u16, description: &str) -> Self {
    self.responses.push((
      i64::from(status),
      description.to_owned(),
      Some(envelope(type_schema::<T>())),
    ));
    self
  }

  /// Declare a response without a payload (e.g. 204).
  pub fn empty_resp(mut self, status: u16, description: &str) -> Self {
    self
      .responses
      .push((i64::from(status), description.to_owned(), None));
    self
  }

  fn operation_json(&self) -> Value {
    let mut op = json!({});
    if let Some(summary) = &self.summary {
      op["summary"] = json!(summary);
    }
    if let Some(description) = &self.description {
      op["description"] = json!(description);
    }
    if let Some(tag) = &self.tag {
      op["tags"] = json!([tag]);
    }
    if !self.params.is_empty() {
      op["parameters"] = json!(self.params);
    }
    if let Some(schema) = &self.request_schema {
      op["requestBody"] = json!({
          "required": true,
          "content": { "application/json": { "schema": schema } },
      });
    }
    let mut responses = serde_json::Map::new();
    for (status, description, schema) in &self.responses {
      let mut entry = json!({ "description": description });
      if let Some(schema) = schema {
        entry["content"] = json!({ "application/json": { "schema": schema } });
      }
      responses.insert(status.to_string(), entry);
    }
    op["responses"] = Value::Object(responses);
    op
  }
}

/// The `Resp` envelope shape wrapping a data schema.
fn envelope(data: Value) -> Value {
  json!({
      "type": "object",
      "properties": {
          "code": { "type": "integer", "description": "0 = success" },
          "msg": { "type": "string" },
          "data": data,
      },
      "required": ["code", "msg"],
  })
}

/// The JSON schema of `T`, as a raw value.
fn type_schema<T: schemars::JsonSchema>() -> Value {
  serde_json::to_value(schemars::schema_for!(T)).unwrap_or(Value::Null)
}

/// schemars marks optional fields with a `null` type entry; OpenAPI
/// prefers absent + non-required.
fn strip_null(schema: &Value) -> Value {
  let mut out = schema.clone();
  let types = match out.get("type").and_then(Value::as_array) {
    Some(types) => types.clone(),
    None => return out,
  };
  let kept: Vec<&Value> = types
    .iter()
    .filter(|t| t.as_str() != Some("null"))
    .collect();
  if kept.len() == 1 {
    out["type"] = kept[0].clone();
  }
  out
}

/// Minimal Swagger UI page backed by a CDN build.
pub(crate) const SWAGGER_HTML: &str = r##"<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8" />
  <title>API Docs</title>
  <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css" />
</head>
<body>
  <div id="swagger-ui"></div>
  <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
  <script>
    SwaggerUIBundle({ url: "/openapi.json", dom_id: "#swagger-ui" });
  </script>
</body>
</html>
"##;
