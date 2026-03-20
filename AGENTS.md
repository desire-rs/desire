# AGENTS.md

> Guidelines for agentic coding in the desire-rs repository.

## Project Overview

**desire** is a minimal Rust web application framework built on hyper.
- Repository: https://github.com/desire-rs/desire
- Edition: 2024 (NOT 2021)
- License: Apache-2.0

## Build / Lint / Test Commands

```bash
# Build the project
cargo build

# Run all tests
cargo test

# Run a single test (by name)
cargo test hello -- --nocapture  # runs tests matching "hello"

# Run tests in a specific file
cargo test --test integration

# Lint with clippy
cargo clippy --all-targets --all-features

# Format code
cargo fmt

# Check formatting
cargo fmt -- --check

# Build examples
cargo build --examples
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
pub type AnyResult<T> = anyhow::Result<T, anyhow::Error>;

// Hyper types
pub type HyperResponse = hyper::Response<Full<Bytes>>;
pub type HyperRequest = hyper::Request<Incoming>;

// Application-specific
pub type ApiResult<T> = std::result::Result<Resp<T>, Error>;
```

### Traits
- Use `#[async_trait::async_trait]` for async traits
- Bounds: `Send + Sync + 'static` for endpoint/middleware types
- `IntoResponse` trait for converting types to `Result`

### Async/Await
- All async handlers use `async fn`
- Return `Result` or `ApiResult<T>` or types implementing `IntoResponse`
- Example controller pattern:
  ```rust
  pub async fn get_user(req: Request) -> ApiResult<String> {
      let id = req.param::<String>("id")?;
      Ok(Resp::data(id))
  }
  ```

### Serde / JSON
- Use `#[serde(rename = "camelCase")]` for JSON field naming
- Use `#[serde(skip_serializing_if = "Option::is_none")]` for optional fields
- Derive `Serialize`, `Deserialize` together

### Response Types
The framework provides multiple ways to return responses:
```rust
// 1. Using IntoResponse (preferred)
Ok(Resp::data(value))           // JSON response
"hello"                          // Plain text
String                           // Plain text
Response::json(payload)          // Explicit JSON
Response::with_status(404, msg)  // Status with message

// 2. Tuple syntax (StatusCode, body)
(200, "OK")                      // Status + string
(404, "Not found")
```

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
└── fs.rs           # Static file serving

examples/hello/
├── main.rs         # Entry point
├── controller/     # Request handlers
├── service/        # Business logic
├── model/          # Data structures
├── middleware/     # Custom middleware
├── types/          # Response types (Resp<T>, PageData<T>)
├── error/          # App-specific errors
└── config/         # Configuration (use once_cell::Lazy)
```

### Logging
- Use `tracing` crate for structured logging
- Use `#[instrument]` or manual `info!`, `error!`, `debug!`
- Example: `info!("method: {}", method)`

### Dependencies (Key)
- `hyper` (1.x) - HTTP server
- `tokio` - Async runtime
- `serde` / `serde_json` - Serialization
- `thiserror` - Error derivation
- `anyhow` - Flexible error handling
- `async-trait` - Async trait methods
- `tracing` - Logging
- `dotenv` - Environment variables

## Common Patterns

### Adding a New Route
```rust
// In Router setup
app.get("/path/:id", controller::handler);
app.post("/path", controller::create);

// In controller
pub async fn handler(req: Request) -> ApiResult<Resp<String>> {
    let id = req.param::<String>("id")?;
    Ok(Resp::data(id))
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

## Testing
- Use `cargo test` for unit tests
- Tests can be in the same file or `tests/` directory
- Use `#[cfg(test)]` module for inline tests

## Before Committing
1. Run `cargo fmt`
2. Run `cargo clippy --all-targets`
3. Run `cargo test`
4. Check for any compiler warnings
