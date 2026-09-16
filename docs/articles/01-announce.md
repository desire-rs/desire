# desire 0.2:一个把人体工程学押到底的 Rust Web 框架

> 开源仓库:[github.com/desire-rs/desire](https://github.com/desire-rs/desire) ·
> crate:[crates.io/crates/desire](https://crates.io/crates/desire) ·
> `cargo add desire`

Rust 写 Web 服务,性能从来不是问题,写起来的体验才是。axum 很优秀,但 extractor 元组、Tower 概念、类型报错这些门槛,劝退过不少人。

[desire](https://github.com/desire-rs/desire) 是一个基于 hyper 1.x 的新框架,整个框架只有 **6 个核心概念**,押注一个方向:**写起来的体验和诊断能力**。0.2.0 已发布到 crates.io,本文带你 3 分钟看完它长什么样。

## Hello, 10 行

```rust
use desire::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
    let app = App::new().route("/hello/{name}", get(hello));
    app.run("0.0.0.0:3000").await
}

async fn hello(ctx: Context) -> Result<Resp<String>> {
    let name = ctx.param_raw("name")?;
    Ok(Resp::ok(format!("hello, {name}!")))
}
```

```text
$ curl localhost:3000/hello/desire
{"code":0,"msg":"ok","data":"hello, desire!"}
```

`async fn(ctx: Context)` 直接注册,没有 trait 实现样板,没有宏。App 启动时默认挂好 Ctrl+C 优雅关停。

## 六个概念,就是全部

| 概念 | 一句话 |
|------|--------|
| `App` | 路由 + 状态 + 中间件 + 启动入口 |
| `Context` | 请求的一切:参数、query、body、cookie、状态 |
| `Handler` | 任何 `async fn(Context) -> impl IntoResponse` |
| `Middleware` / `Next` | 洋葱模型,普通 async fn 就能写 |
| `Router` / `MethodRouter` | 可组合的路由树 |
| `Resp<T>` | 统一响应信封 |

对比一下:学 axum 你要理解 extractor 系统、Tower `Service`、`IntoResponse` 的多层泛型;学 desire,上面这张表就是全部。

## 统一响应信封:前后端不再互相猜

所有 API 返回同一个形状:

```json
{"code": 0, "msg": "ok", "data": {"id": 1, "name": "alice"}}
```

```rust
async fn get_user(ctx: Context) -> Result<Resp<User>> {
    let id: i64 = ctx.param("id")?;
    let db = ctx.state::<Db>()?;
    let user = db.find_user(id).await?;   // 自定义错误 From 一下就能 ?
    Ok(Resp::ok(user))                    // .created(v) / .page(v, 1, 20, total)
}
```

业务错误 `Resp::err(40001, "余额不足")` 走 HTTP 200 + 业务码;框架错误(404、401、参数错误)自动渲染成同样形状的信封,HTTP 状态码正确。前端只需要写一次解析逻辑。

## 错误诊断精确到字段

这是 desire 和其他框架拉开差距的地方。参数错了:

```json
{"code":400, "msg":"invalid param `id`, expected `i64`: invalid digit found in string"}
```

JSON body 少了字段:

```json
{"code":400, "msg":"invalid json body: missing field `email` at line 1 column 20"}
```

哪一层(path / query / body)、哪个字段、期望什么类型,一目了然。前后端联调时,错误信息本身就是文档。

## 生产级 Server,一行起步

```rust
app.run("0.0.0.0:3000").await?;
```

开箱包含:HTTP/1.1 + HTTP/2 同端口自适应(hyper-util auto)、优雅关停、默认 2MiB 请求体上限。要 TLS,一行:

```rust
Server::new(app)
    .bind("0.0.0.0:443")?
    .tls(TlsConfig::builder().cert_file("cert.pem")?.key_file("key.pem")?.build()?)
    .graceful_shutdown(async { let _ = tokio::signal::ctrl_c().await; })
    .run()
    .await
```

rustls 实现,ALPN 自动协商 h2。

## 该有的都有

- **中间件**:内置 `logger()` / `cors()` / `timeout()` / `body_limit()` / `gzip()`;自定义中间件和 handler 写法同构
- **文件上传**:`ctx.form_data()` 解析 multipart,文本字段和文件分好类
- **静态文件**:路径穿越攻击免疫(有专门测试),ETag/304,Range 分段下载(206/416)
- **实时**:SSE 一等支持(`Event::new().data(..)`);WebSocket `ctx.websocket()?.on_upgrade(...)`
- **Cookie**:`ctx.cookie()` 读,`ctx.set_cookie()` 写
- **OpenAPI**:builder 声明 + schemars 推导,自动挂 `/openapi.json` 和 Swagger UI `/docs`

## 不起 socket 的测试

```rust
#[tokio::test]
async fn get_user_ok() {
    let tc = TestClient::new(app());
    let res = tc.get("/users/1").send().await;
    res.assert_status_ok();
    let user: User = res.assert_ok_data();
}
```

走的是真实的中间件 + 路由 + handler 管线,毫秒级出结果。WebSocket 这种必须真连接的除外,其余全部不用起端口。

## 诚实的定位

desire 基于和 axum 相同的 hyper 1.x,性能天花板一致,但我们没有跑分截图可秀 —— 这个项目的赌注是**框架的使用体验本身就是产品**:更少的概念、更精确的诊断、约定俗成的响应格式。

如果你要 Tower 生态的中间件全家桶、极致的 extractor 抽象复用,请继续用 axum,它依然是默认的正确选择。如果你在做前后端分离的 API 服务,受够了样板代码和模糊的报错,欢迎试试 desire。

- 仓库:[github.com/desire-rs/desire](https://github.com/desire-rs/desire)(求 Star ✨)
- 文档:[docs.rs/desire](https://docs.rs/desire)
- 安装:`cargo add desire`

0.x 阶段,API 仍可能演进,欢迎提 issue 聊设计。
