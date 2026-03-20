# AGENTS.md

> Guidelines for agentic coding in the desire-rs repository.

## Project Overview

**desire** is a minimal Rust web application framework built on hyper.
- Repository: https://github.com/desire-rs/desire
- Edition: 2024
- License: Apache-2.0

## Build / Lint / Test Commands

```bash
# Build the project
cargo build

# Run all tests
cargo test

# Run a single test (by name)
cargo test hello -- --nocapture

# Lint with clippy
cargo clippy --all-targets --all-features

# Format code
cargo fmt

# Check formatting
cargo fmt -- --check
```

## Code Style Guidelines

### Formatting
- **Tab width**: 2 spaces (per `rustfmt.toml`)
- **Edition**: 2024
- Run `cargo fmt` before committing

### Naming Conventions
| Element | Convention | Example |
|---------|------------|---------|
| Modules | snake_case | `mod router;` |
| Structs | PascalCase | `struct Router` |
| Enums | PascalCase | `enum Error` |
| Functions | snake_case | `fn new()`, `pub fn get()` |
| Variables | snake_case | `let remote_addr` |
| Constants | SCREAMING_SNAKE | `ENV_NAME` |
| Type aliases | PascalCase | `ApiResult<T>`, `DynEndpoint` |
| Traits | PascalCase | `Endpoint`, `Middleware` |
| Public fields | snake_case | `pub prefix`, `pub routes` |
| Private fields | snake_case | `inner`, `params` |

### Imports
```rust
// Group 1: extern crate imports (async-trait, serde, etc.)
// Group 2: std imports
// Group 3: crate-relative imports (crate::)
// Group 4: other crate imports (hyper, tokio, etc.)

use std::sync::Arc;
use bytes::Buf;
use crate::{DynEndpoint, Endpoint, Request, Result};
use hyper::http::Extensions;
```

### Error Handling
- Use `thiserror` for defining errors with `#[derive(Debug, Error)]`
- Provide user-friendly `#[error(...)]` messages
- Helper functions for common errors:
  ```rust
  pub fn missing_param(name: &str) -> Error
  pub fn error_msg(msg: &str) -> Error
  pub fn invalid_param(name, expected, err) -> Error
  ```
- Framework exposes `desire::Error` and `desire::Result<T = Response>`

### Type Aliases Pattern
```rust
// Standard framework result
pub type Result<T = Response> = std::result::Result<T, Error>;

// Anyhow wrapper for flexible errors
pub type AnyResult<T> = anyhow::Result<T>;

// Hyper types
pub type HyperResponse = hyper::Response<Full<Bytes>>;
pub type HyperRequest = hyper::Request<Incoming>;
```

### Traits
- Use `#[async_trait::async_trait]` for async traits
- Bounds: `Send + Sync + 'static` for endpoint/middleware types
- `IntoResponse` trait for converting types to `Result`

### Serde / JSON
- Use `#[serde(rename = "camelCase")]` for JSON field naming
- Use `#[serde(skip_serializing_if = "Option::is_none")]` for optional fields
- Derive `Serialize`, `Deserialize` together

### Module Organization
```
src/
├── lib.rs          # Public API exports
├── error.rs        # Error types and helpers
├── router.rs       # Route registration
├── kernel.rs       # Endpoint, Middleware traits
├── request.rs      # Request handling
├── response.rs     # Response building
├── into_response.rs # IntoResponse trait impls
├── server.rs       # Server setup
├── types.rs        # Type aliases
└── utils.rs        # Utilities (empty placeholder)
```

### Logging
- Use `tracing` crate for structured logging
- Use `#[instrument]` or manual `info!`, `error!`, `debug!`

## Common Patterns

### Adding a New Route
```rust
// In Router setup
app.get("/path/:id", controller::handler);
app.post("/path", controller::create);

// In controller
pub async fn handler(req: Request) -> Result<String> {
    let id = req.param::<String>("id")?;
    Ok(id.into_response())
}
```

### Creating Middleware
```rust
pub struct MyMiddleware;

#[async_trait::async_trait]
impl desire::Middleware for MyMiddleware {
    async fn handle(&self, req: Request, next: Next<'_>) -> Result {
        // Pre-processing
        let res = next.run(req).await?;
        // Post-processing
        Ok(res)
    }
}
```

### Accessing Request Data
```rust
// Path parameters
let id = req.param::<String>("id")?;

// Query strings
let query = req.query::<QueryType>()?;

// Body parsing
let data = req.body::<CreateUserPayload>().await?;
```

### Response Types
The framework provides multiple ways to return responses:
```rust
// Using IntoResponse (preferred)
Ok(Resp::data(value))           // JSON response
"hello"                          // Plain text
String                           // Plain text
Response::json(payload)          // Explicit JSON
Response::with_status(404, msg)  // Status with message

// Tuple syntax (StatusCode, body)
(200, "OK")                      // Status + string
(404, "Not found")
```

## Before Committing
1. Run `cargo fmt`
2. Run `cargo clippy --all-targets`
3. Run `cargo test`
4. Check for any compiler warnings
