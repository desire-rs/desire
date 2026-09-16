# 用 desire 十分钟写一个生产级 REST API

> desire 是一个基于 hyper 的极简 Rust Web 框架。本文从空目录开始,写一个带数据库状态、文件上传、OpenAPI 文档和测试的用户 API。完整代码都在框架仓库的 examples 里。

## 0. 安装

```bash
cargo new user-api && cd user-api
cargo add desire
cargo add tokio --features full
cargo add serde --features derive
```

## 1. 最小骨架

```rust
// src/main.rs
use desire::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
    let app = App::new().route("/health", get(|| async { Resp::ok("up") }));
    app.run("0.0.0.0:3000").await
}
```

`cargo run`,然后:

```bash
$ curl localhost:3000/health
{"code":0,"msg":"ok","data":"up"}
```

统一信封 `{"code","msg","data"}` 是框架约定:`code == 0` 即成功,业务错误也是这个形状。

## 2. 状态 + CRUD

真实应用要共享数据库连接。`App::state` 注册,`ctx.state` 取用(零克隆借用):

```rust
use desire::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Default)]
struct MemoryDb {
    users: std::sync::Arc<tokio::sync::RwLock<Vec<User>>>,
}

#[derive(Serialize, Deserialize, Clone, schemars::JsonSchema)]
struct User {
    id: i64,
    name: String,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct CreateUser {
    name: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let app = App::new()
        .state(MemoryDb::default())
        .route(
            "/users",
            get(list_users).post(create_user),
        )
        .route("/users/{id}", get(get_user).delete(delete_user));
    app.run("0.0.0.0:3000").await
}

async fn list_users(ctx: Context) -> Resp<Vec<User>> {
    let db = ctx.state::<MemoryDb>().unwrap();
    let users = db.users.read().await.clone();
    Resp::page(users, 1, 20, 1)   // {"code":0,"data":[...],"page":{"index":1,...}}
}

async fn get_user(ctx: Context) -> Result<Resp<User>> {
    let id: i64 = ctx.param("id")?;               // 类型不对?400,报错带字段名
    let db = ctx.state::<MemoryDb>()?;
    let users = db.users.read().await;
    users
        .iter()
        .find(|u| u.id == id)
        .cloned()
        .map(Resp::ok)
        .ok_or_else(|| Error::not_found(format!("user {id}")))
}

async fn create_user(ctx: Context) -> Result<Resp<User>> {
    let input: CreateUser = ctx.json().await?;    // 缺字段?报错指名道姓
    let db = ctx.state::<MemoryDb>()?;
    let mut users = db.users.write().await;
    let user = User { id: users.len() as i64 + 1, name: input.name };
    users.push(user.clone());
    Ok(Resp::created(user))
}

async fn delete_user(ctx: Context) -> Result<Resp<()>> {
    let id: i64 = ctx.param("id")?;
    ctx.state::<MemoryDb>()?.users.write().await.retain(|u| u.id != id);
    Ok(Resp::ok(()))
}
```

注意几个点:

- `ctx.param::<i64>("id")` 解析失败自动返回 400 信封,消息形如
  `invalid param "id", expected "i64": invalid digit found in string`;
- `ctx.json()` 的 serde 错误精确到字段:`missing field "name" at line 1 column 20`;
- `MemoryDb` 里的 `Arc` 是为了让多个请求共享同一把 `RwLock`(内部可变性);
  `ctx.state::<T>()` 返回借用,零克隆。

## 3. 中间件:和 handler 同构

写一个认证中间件,把登录用户塞进请求上下文:

```rust
struct CurrentUser(String);

async fn auth(mut ctx: Context, next: Next) -> Result {
    let token = ctx
        .header("authorization")
        .ok_or_else(Error::unauthorized)?;
    ctx.insert(CurrentUser(token[7..].to_owned()));  // 后续 handler 可读
    next.run(ctx).await                              // 洋葱模型,之后还能改响应
}

async fn me(ctx: Context) -> Resp<String> {
    let user = ctx.get::<CurrentUser>().unwrap();
    Resp::ok(format!("hello, {}", user.0))
}

// 注册:全局
let app = App::new().with(auth);
// 或者只给一组路由
let api = Router::new().with(auth).route("/me", get(me));
let app = App::new().nest("/api", api);
```

内置的 `logger()`、`cors(CorsConfig)`、`timeout(Duration)`、`body_limit(n)`、`gzip()` 直接 `with` 上去就能用。

## 4. 文件上传

```rust
async fn upload(ctx: Context) -> Result<Resp<u64>> {
    let form = ctx.form_data().await?;
    let title = form.field("title").unwrap_or_default();
    if let Some(file) = form.file("attachment") {
        println!("收到文件 {} ({} 字节)", file.filename.as_deref().unwrap_or("?"), file.bytes.len());
    }
    Ok(Resp::ok(title.len() as u64))
}
```

`FormData` 把文本字段和文件分开存放,受框架 2MiB 请求体上限保护(可用 `App::max_body_size` 或 `body_limit()` 中间件调整)。

## 5. OpenAPI + Swagger UI

给结构体加 `#[derive(schemars::JsonSchema)]`,然后声明式生成文档:

```rust
use desire::openapi::{OpenApi, PathDoc};

let docs = OpenApi::new("User API", "1.0.0")
    .path(
        PathDoc::get("/users/{id}")
            .summary("查一个用户")
            .tag("users")
            .path_param("id", "用户 ID")
            .query::<ListQuery>()
            .resp::<User>(200, "用户详情"),
    );

let app = App::new().openapi(docs);   // /openapi.json + /docs(Swagger UI)
```

信封 schema 自动包裹:响应里 `data` 字段就是你的 `User` 结构体。

## 6. 测试:不起端口

```rust
#[tokio::test]
async fn create_then_get() {
    let tc = desire::test::TestClient::new(app());
    let res = tc.post("/users").json(&CreateUser { name: "carol".into() }).send().await;
    res.assert_status(StatusCode::CREATED);

    let res = tc.get("/users/1").send().await;
    res.assert_status_ok();
    let user: User = res.assert_ok_data();
    assert_eq!(user.name, "alice");
}
```

`TestClient` 走真实的中间件 + 路由 + handler 管线,毫秒级。

## 7. 上生产

优雅关停和 TLS 都是现成的:

```rust
Server::new(app)
    .bind("0.0.0.0:443")?
    .tls(TlsConfig::builder().cert_file("cert.pem")?.key_file("key.pem")?.build()?)
    .concurrency_limit(1024)
    .graceful_shutdown(async { let _ = tokio::signal::ctrl_c().await; })
    .run()
    .await
```

Dockerfile 用多阶段构建,产物只有十几 MB:

```dockerfile
FROM rust:1.85 AS build
WORKDIR /src
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=build /src/target/release/user-api /usr/local/bin/
EXPOSE 3000
CMD ["user-api"]
```

## 小结

| 你要做的事 | desire 的答案 |
|-----------|---------------|
| 定义路由 | `app.route("/users/{id}", get(h))` |
| 读参数/JSON | `ctx.param("id")` / `ctx.json()` |
| 返回数据 | `Resp::ok(v)`,统一信封 |
| 共享状态 | `App::state(db)` + `ctx.state::<Db>()` |
| 中间件 | 普通异步函数 + `next.run(ctx)` |
| 接口文档 | `PathDoc` builder → Swagger UI |
| 测试 | `TestClient`,毫秒级 |

仓库里有 9 个可运行的例子:[github.com/desire-rs/desire/tree/main/examples](https://github.com/desire-rs/desire/tree/main/examples)。有问题欢迎提 issue。
