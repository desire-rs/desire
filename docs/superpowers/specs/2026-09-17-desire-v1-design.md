# Desire v1.0 架构设计

日期:2026-09-17
状态:已批准(待实现)
范围:desire crate 的架构级重写,版本 0.0.1 → 0.1.0,公共 API 完全 breaking

---

## 1. 概述与定位

desire 是一个以**极致人体工程学**为核心定位的 Rust Web 框架,基于 hyper 1.x + tokio。
现有代码为 4 年前所写(约 770 行),本次在保留 crate 名称的前提下架构级重写。

**差异化主张**:比 axum 更简洁 —— 更少的概念(6 个核心类型 vs axum 的 extractor tuple +
Tower Service)、更友好的错误诊断(精确到层/字段/类型)、统一响应信封、一行式生产级 Server。

**产品要求**:认真做产品 —— 全量 rustdoc、examples、集成测试、CI、可发布。

### 核心设计决策(已确认)

| 维度 | 决策 |
|------|------|
| Handler 风格 | Context 对象:`async fn(ctx: Context) -> impl IntoResponse` |
| 错误/响应模型 | `Resp<T>` 统一信封 `{code, msg, data}` |
| 中间件 | Context 风格闭包/struct,洋葱模型 |
| Server | 生产级:HTTP/1.1 + HTTP/2 + rustls TLS + 优雅关停 + 流式 body |
| 泛型策略 | 无 proc-macro、无 async-trait,纯 trait + blanket impl |

---

## 2. 目标与非目标

### v1 目标

1. Context 风格 handler/middleware,普通 `async fn` 直接注册,零宏零包装
2. `Resp<T>` 统一响应信封,框架错误自动渲染为信封 JSON
3. matchit 路由:`{param}` / `{*wildcard}`、MethodRouter 链式注册、nest/merge、405+Allow、fallback
4. 生产级 Server:hyper-util auto(H1+H2 自适应)、rustls TLS(feature 门控)、优雅关停、并发上限、请求体大小限制
5. 流式响应(`Body::from_stream`)
6. 安全的静态文件服务(修复旧版路径穿越漏洞)
7. 内存测试客户端(`desire::test`,不走 socket)
8. 内置中间件:logger、cors、timeout、body_limit
9. 全量 rustdoc + 5+ examples + 集成测试 + CI

### v1 非目标(明确不做)

- WebSocket(留 v1.1,需 HTTP upgrade 处理)
- SSE 帮助类型(留 v1.1;`Body::from_stream` 已可手写 SSE)
- OpenAPI 生成
- Range 请求
- Tower Service 兼容层
- 多 crate workspace(单 crate,除非需要 proc-macro —— 本次设计不需要)

---

## 3. 总体架构与数据流

```
TCP 连接 (hyper-util auto::Builder: HTTP/1.1 + HTTP/2 自适应; 配置 TLS 时 ALPN 协商)
    │  每连接一个 tokio task,受并发信号量约束
    ▼
构建 Context { 请求只读视图, body: Mutex<Option<Body>>, extensions, Arc<StateMap>, remote_addr }
    │
    ▼
App 全局中间件链 (logger → cors → 用户全局中间件 → ...)
    │
    ▼
Router 匹配 (matchit radix tree)
    ├─ 路径未命中 → fallback handler (默认 404 信封)
    └─ 路径命中但方法未注册 → 405 + Allow 头
    │
    ▼
路由级中间件链 (nest 子路由自带,逐层包裹)
    │
    ▼
Handler::call (Arc<dyn Handler> 动态派发) → IntoResponse::into_response
    │
    ▼
Response { status, headers, Body = BoxBody<Bytes, Error> } → hyper 写回
```

**核心概念 6 个**:`App`、`Context`、`Handler`、`Middleware`/`Next`、`Router`/`MethodRouter`、`Resp<T>`。

---

## 4. 核心抽象与 trait 签名(已实验验证)

### 4.1 关键技术约束与结论

handler 若写 `async fn(ctx: &Context)`(借用式),其返回 future 借用参数生命周期,
无法通过普通 blanket impl 收敛到 `dyn Handler`(HRTB 限制:"one type is more general than
the other")。要支持借用式只有两条路:salvo 式 proc-macro 属性,或 async-trait crate。

**决策:采用 owned Context + `self: Arc<Self>` 派发**。已在独立实验中验证以下形态
可编译、可动态派发(2026-09-17,/tmp/hrtb-test):

```rust
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub trait Handler: Send + Sync + 'static {
    fn call(self: Arc<Self>, ctx: Context) -> BoxFuture<'static, Result>;
}

// blanket impl:任何 async fn(Context) -> impl IntoResponse 直接成为 Handler
impl<F, Fut, R> Handler for F
where
    F: Fn(Context) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = R> + Send + 'static,
    R: IntoResponse,
{
    fn call(self: Arc<Self>, ctx: Context) -> BoxFuture<'static, Result> {
        Box::pin(async move { (self)(ctx).await.into_response() })
    }
}
```

要点:
- future 持有 owned `Arc<Self>`,**无需 `F: Clone` 约束**(axum 式 clone-free)
- 另有 arity-0 blanket impl(`Fn() -> Fut`),支持 `get(|| async { Resp::ok(()) })`;
  两个 impl 按 arity 天然不冲突
- 全链 future 均为 `'static`,内部实现显著简化

**用户侧签名(本设计的定案)**:

```rust
async fn get_user(ctx: Context) -> Resp<User>          // handler: owned,不加 mut
async fn auth(mut ctx: Context, next: Next) -> Result  // 中间件:需要改写时声明 mut
```

### 4.2 Middleware 与 Next

```rust
pub trait Middleware: Send + Sync + 'static {
    fn call(self: Arc<Self>, ctx: Context, next: Next) -> BoxFuture<'static, Result>;
}

#[derive(Clone)]
pub struct Next { /* 剩余中间件切片 + 终端 Handler,全部 Arc,故 'static */ }

impl Next {
    pub async fn run(self, ctx: Context) -> Result; // Result<Response>
}
```

- blanket impl 同 Handler(`Fn(Context, Next) -> Fut`)
- 中间件可:前置改写 ctx(`mut ctx`)、短路返回 `Err`/自构响应、后置修改响应
  (拿到 `Result<Response>` 后改 headers/status)。**限制**:next.run 消费 ctx,
  后置阶段无法再读 ctx —— 可接受(actix/poem 同样如此),v1.1 如有需求再评估
- 洋葱模型,链条 `Arc<[Arc<dyn Middleware>]>` 逐层消费

### 4.3 IntoResponse

```rust
pub trait IntoResponse {
    fn into_response(self) -> Response;  // 不可能失败:错误类型经 Error→Response 渲染
}
```

v1 实现清单:`Response`(identity)、`Resp<T>`、`Error`、`()`、`StatusCode`、
`&'static str` / `String` / `Cow<'static, str>`(text/plain; charset=utf-8)、
`Bytes` / `Vec<u8>` / `&'static [u8]`(application/octet-stream)、`serde_json::Value`、
`Json<T>`(newtype,非信封 JSON 出口)、`Html<String>`(newtype)、
`(StatusCode, R: IntoResponse)`、`(u16, R: IntoResponse)`、
`Result<T, E> where T: IntoResponse, E: Into<Error>`。

---

## 5. Context 详设

```rust
pub struct Context {
    // 请求只读视图
    method: Method,
    uri: Uri,                      // path() / query_string() 由此派生
    headers: HeaderMap,
    params: HashMap<String, String>,   // 路由参数(matchit 已 percent-decode)
    body: Mutex<Option<Body>>,         // 内部可变性:handler 不需要 mut 即可读 body
    extensions: Extensions,            // per-request 类型化 map(http::Extensions)
    state: Arc<StateMap>,              // App 级共享状态
    remote_addr: Option<SocketAddr>,
}

pub type StateMap = HashMap<TypeId, Box<dyn Any + Send + Sync>>;
```

### 提取 API(handler 日常代码)

| 方法 | 语义 | 失败行为 |
|------|------|----------|
| `ctx.param::<T>("id") -> Result<T>` | 单个路径参数,`FromStr` 解析 | `Error::Param{层:param, name, expected}` |
| `ctx.params::<T>() -> Result<T>` | 整个结构体从路径参数反序列化(serde) | 同上,逐字段报错 |
| `ctx.query::<T>() -> Result<T>` | query string → 结构体(serde_urlencoded) | `Error::Query` + serde 细节 |
| `ctx.json::<T>() -> Result<T>` | body 读全 + serde_json;校验 Content-Type 含 json | `Error::Json` + serde 路径(如 `body.email: missing field`) |
| `ctx.form::<T>() -> Result<T>` | `application/x-www-form-urlencoded` | `Error::Body` |
| `ctx.body_bytes() -> Result<Bytes>` / `body_text() -> Result<String>` | 原始 body(幂等缓存) | `Error::Body` |
| `ctx.header(name) -> Option<&str>` / `ctx.headers()` | 请求头 | — |
| `ctx.cookie(name) -> Option<Cookie>` | cookie 读取(cookie crate 解析) | — |
| `ctx.state::<T>() -> Result<&T>` | App 状态借用(零克隆) | `Error::Internal`(状态未注册,启动期可查) |
| `ctx.insert::<T>(v)` / `ctx.get::<T>() -> Option<&T>` | per-request 数据(中间件→handler 传值,如 CurrentUser) | — |
| `ctx.method()` / `ctx.path()` / `ctx.query_string()` / `ctx.remote_addr()` | 视图 | — |

- **body 幂等**:`body_bytes` 读全后缓存回 `Mutex<Option<…>>`,同请求内可重复读取
  (json/form/text 共享缓存);Mutex 仅在 take/put 短临界区持有,不跨 await
- **精确诊断**:所有提取错误携带「层(param/query/body)+ 字段名 + 期望类型 + serde 原始信息」,
  经 `IntoResponse for Error` 渲染为信封 JSON(见 §6)
- `state` 与 `insert/get` 是两个不同概念:前者 App 启动时注册、全请求共享只读;
  后者每请求独立、可变,用于中间件向 handler 传递请求级数据

---

## 6. Resp<T> 与错误模型

### 6.1 统一信封

```rust
#[derive(Serialize)]
pub struct Resp<T = ()> {
    code: i64,          // 业务码:0 = 成功
    msg: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    page: Option<Page>,  // { index, size, total },仅分页时出现
}

impl<T> Resp<T> {
    pub fn ok(data: T) -> Self;                  // code 0, msg "ok", HTTP 200
    pub fn created(data: T) -> Self;             // 同上,HTTP 201
    pub fn page(data: Vec<T>, index: i64, size: i64, total: i64) -> Self;
    pub fn with_status(self, status: StatusCode) -> Self;  // 覆盖 HTTP 状态
    pub fn err(code: i64, msg: impl Into<String>) -> Resp<()>;   // 关联函数
    pub fn err_msg(msg: impl Into<String>) -> Resp<()>;          // code = 500
}
```

业务错误默认 HTTP 200(前后端以 code 判定),需要时 `with_status` 覆盖。

### 6.2 框架 Error

```rust
#[derive(Debug, Error)]
pub enum Error {
    #[error("bad request: {0}")]        BadRequest(String),
    #[error("unauthorized")]            Unauthorized,
    #[error("forbidden")]               Forbidden,
    #[error("not found: {0}")]          NotFound(String),
    #[error("method not allowed")]      MethodNotAllowed,
    #[error("payload too large")]       PayloadTooLarge,
    #[error("request timeout")]         Timeout,
    #[error("invalid param `{name}`, expected {expected}")]  Param { name: String, expected: &'static str, #[source] source: Box<dyn Error + Send + Sync> },
    #[error("invalid json body: {0}")]  Json(#[from] serde_json::Error),
    #[error("invalid query string: {0}")] Query(#[from] serde_urlencoded::de::Error),
    #[error("invalid body: {0}")]       Body(String),
    #[error("{msg}")]                   Business { code: i64, msg: String },
    #[error("internal server error")]   Internal { logged: String },  // 详情只进日志
}
```

- `Result<T = Response, E = Error>` 为框架 Result;**移除**旧 `AnyResult`,anyhow 依赖删除
- `IntoResponse for Error` 渲染规则:
  - `Business{code,msg}` → 信封 `{code, msg}`,HTTP 200
  - `BadRequest/Param/Json/Query/Body` → code 400,HTTP 400;`Unauthorized` → 401;
    `Forbidden` → 403;`NotFound` → 404;`MethodNotAllowed` → 405 + Allow;
    `PayloadTooLarge` → 413;`Timeout` → 504;`Internal` → code 500 + msg
    "internal server error"(**不回显内部细节**,`tracing::error!` 记录 logged)
- 便捷构造:`Error::internal(impl Display)`(记录来源)、`Error::msg(impl Display)`(400)、
  `unauthorized()` / `forbidden()` / `not_found(impl Display)` / `bad_request(impl Display)`
- 用户自定义错误:实现 `From<MyError> for desire::Error`(推荐 `Business` 或
  `BadRequest` 变体),handler 内 `?` 全程贯通;或直接返回 `Resp::err(...)`

---

## 7. 路由

- 引擎:**matchit 0.8**(axum 同款 radix tree,维护活跃);语法 `/users/{id}`、
  `/files/{*path}`;移除 route-recognizer 依赖
- `MethodRouter`:每路径一个,内部 `FnvMap<Method, Arc<dyn Handler>>` + 已注册方法集

```rust
app.route("/users", get(list).post(create));      // 链式
app.route("/health", get(health_handler));        // 单 handler 自动包成 MethodRouter
```

- `Router`(树节点)与 `App`(顶层)API:
  - `App::route(path, method_router_or_handler)`(接受两者,`impl Into<MethodRouter>` 风格)
  - `App::nest(prefix, sub_router)`:前缀剥离后交给子路由,子路由中间件随之生效
  - `App::merge(other_router)`:逐条插入(**修复旧版整表覆盖 bug**);路径冲突在注册期 panic(fail-fast,与 axum/actix 一致)
  - `App::with(middleware)` 全局;`Router::with(middleware)` 路由组级
  - `App::fallback(handler)` 自定义 404(默认:404 信封 JSON)
- 405:路径命中但方法未注册 → 405 + `Allow: GET, POST` 头 + 信封 body
- HEAD:无显式注册时自动复用 GET handler,响应剥离 body
- 路径参数 percent-decode 后存入 `ctx.params`

---

## 8. 中间件与内置项

自定义示例(与 handler 心智同构):

```rust
async fn auth(mut ctx: Context, next: Next) -> Result {
    let token = ctx.header("authorization").ok_or_else(Error::unauthorized)?;
    ctx.insert(CurrentUser(verify(token)?));   // per-request 数据
    next.run(ctx).await                        // 洋葱后半程:可改响应
}
app.with(auth);
```

内置(feature 全默认):

| 中间件 | 行为 |
|--------|------|
| `logger()` | tracing `info!`:method、path、status、耗时 ms |
| `cors(CorsConfig)` | allow_origin/methods/headers/credentials/max_age;预检 OPTIONS 短路 |
| `timeout(Duration)` | 超时返回 504 信封 |
| `body_limit(n)` | 请求体上限(字节);App 默认强制 2 MiB(`App::max_body_size` 可调),超限 413 |

---

## 9. Body 与流式响应

```rust
pub type Body = http_body_util::combinators::BoxBody<Bytes, Error>;
```

- 构造:`Body::empty()` / `from(Bytes)` / `from(&'static str|String)` / `from_json(&T)` /
  `from_stream(S)` —— `S: Stream<Item = Result<impl Into<Bytes>, E>>, E: Into<Error>`
- 响应体一律 `Body`(修复旧版 `Full<Bytes>` 固定不能流式的问题);
  请求侧 hyper `Incoming` 首读即收集,受 body 限制约束

---

## 10. Server(生产级)

```rust
// 便捷模式(默认挂 Ctrl+C 优雅关停)
app.run("0.0.0.0:3000").await?;

// 完整构建器
desire::Server::new(app)
    .bind(addr)?                                    // 返回 Result,不再 unwrap panic
    .tls(TlsConfig::builder()
        .cert_file("cert.pem").key_file("key.pem").build()?)   // feature = "tls"
    .graceful_shutdown(async { signal::ctrl_c().await.ok(); })
    .concurrency_limit(1024)
    .run().await?;
```

- hyper-util `auto::Builder`:同一端口 HTTP/1.1 与 HTTP/2 prior-knowledge 自适应;
  启用 TLS 时 rustls ALPN 广播 `h2, http/1.1`
- 每连接一个 tokio task;`concurrency_limit` 用信号量在 accept 侧限流
- 优雅关停:监听 shutdown future,触发后停止 accept、进行中的连接 drain 至完成
- 请求体上限在派发层强制(§8),TLS 依赖:rustls 0.23 + tokio-rustls + rustls-pemfile,
  全部置于 `tls` feature 之后

---

## 11. 静态文件(修复安全漏洞)

```rust
app.route("/static/{*path}", ServeDir::new("assets"));
app.route("/favicon.ico", ServeFile::new("assets/favicon.ico"));
```

- **路径穿越防护**(旧版 `ServeDir` 直接 join 用户输入,可读任意文件 —— 本版修复):
  percent-decode → 按 `/` 切段 → 拒绝 `..`、空段、绝对路径段 → join 后
  `canonicalize` 并校验 `starts_with(root)`(防符号链接逃逸)
- `mime_guess` 推断 Content-Type(修复旧版 substring 匹配 + png 分支重复 bug)
- ETag(mtime+len 强弱校验)/ `Last-Modified`;`If-None-Match` / `If-Modified-Since` → 304
- 目录请求 → 尝试 `index.html`,否则 404 信封
- 明确不做(v1):Range 请求、目录列表、sendfile 零拷贝

---

## 12. 测试支持

```rust
use desire::test::TestClient;

#[tokio::test]
async fn get_user_ok() {
    let tc = TestClient::new(test_app());
    let res = tc.get("/users/1").send().await;
    res.assert_status_ok();                                  // 断言助手
    let body: Resp<User> = res.json().await;
}

let res = tc.post("/users").json(&create_body)
             .header("x-token", "t").send().await;
```

- `TestClient` 直接驱动 App 派发链(中间件+路由+handler),不走 socket/端口,
  毫秒级;请求构造器支持 `header / json / form / body`
- `TestResponse`:`status()`、`headers()`、`bytes()`、`text()`、`json::<T>()`、
  `resp::<T>()`(信封解包:校验 code==0 后返回 data)
- 位于 `desire::test` 模块,始终编译(体积小,仅依赖核心依赖),无需 feature 门控

---

## 13. 模块布局与依赖

```
src/
├── lib.rs            # 模块声明 + pub use + prelude
├── app.rs            # App:路由+状态+全局中间件+派发入口
├── context.rs        # Context
├── handler.rs        # Handler trait + blanket impls
├── middleware.rs     # Middleware/Next + 内置 logger/cors/timeout/body_limit
├── router.rs         # Router/MethodRouter
├── response.rs       # Response 具体类型 + 构造器
├── resp.rs           # Resp<T> 信封
├── body.rs           # Body 别名与构造
├── into_response.rs  # IntoResponse + 全部 impl
├── error.rs          # Error/Result
├── server.rs         # Server、连接循环、优雅关停
├── tls.rs            # TlsConfig(feature = "tls")
├── fs.rs             # ServeDir/ServeFile
├── state.rs          # StateMap
├── test.rs           # TestClient(feature = "test")
└── types.rs          # BoxFuture 等别名
examples/
├── hello.rs  json_api.rs  middleware.rs  static_files.rs  tls.rs  graceful.rs
```

依赖(hyper 1.x 系):`hyper(full)`、`hyper-util(full)`、`http-body`、`http-body-util`、
`bytes`、`tokio(full)`、`matchit 0.8`、`serde+derive`、`serde_json`、`serde_urlencoded`、
`thiserror 2`、`tracing`、`mime_guess`、`cookie 0.18`;
features:`tls`(rustls 0.23 / tokio-rustls / rustls-pemfile)。
**移除**:`async-trait`、`route-recognizer`、`anyhow`、`dotenv`、`chrono`。

工程:edition 2024,MSRV 1.85,`rustfmt.toml` 修正为 `edition = "2024"`(现文件里是过期
的 2021),`tab_spaces = 2`;版本 `0.1.0`,`publish = true`。

---

## 14. 与旧 API 的差异(breaking 清单)

| 旧(0.0.1) | 新(0.1.0) |
|---|---|
| `desire::new(addr)` | `App::new()....run(addr)` |
| `Router::at(method, path, handler)` | `App::route(path, get(h))` |
| `Endpoint` trait / `DynEndpoint` | `Handler`(`self: Arc<Self>` 派发) |
| `Request` 拥有 hyper request | `Context`(owned;hyper 细节不再暴露) |
| `Response::with_status(u16, String)` | `IntoResponse` + `(StatusCode, R)` / `Resp` |
| `IntoResponse::into_response() -> Result` | `-> Response`(infallible) |
| `AnyResult` / anyhow 双轨 | 单一 `Result<T = Response, E = Error>` |
| `route_recognizer` `:param` 语法 | matchit `{param}` / `{*wildcard}` |
| `Server::bind` panic | 返回 `Result` |
| `ServeDir` 直接 join(可穿越) | 规范化 + canonicalize 校验 |

---

## 15. 成功标准(Definition of Done)

1. 全部 examples 编译运行:hello(10 行内起服务)、json_api(state+Resp+CRUD)、
   middleware(自定义+内置)、static_files、tls、graceful
2. `cargo test` 通过:模块单测 + `tests/` 集成测试(TestClient 覆盖:路由/405/404/
   参数提取错误信封/json 提取/中间件洋葱序/405 Allow/静态文件穿越拒绝)
3. `cargo clippy --all-targets --all-features` 零警告;`cargo fmt -- --check` 通过
4. 全部公共项 rustdoc;`cargo doc` 无警告;README 快速上手(≤10 行起一个
   state+middleware+JSON 的 API)
5. GitHub Actions:fmt + clippy + test + doc(MSRV 1.85 与 stable 双矩阵)
6. 安全:路径穿越用例被拒绝(集成测试断言);body 超限 413;错误响应不泄漏内部细节

---

## 16. 实现修正记录(2026-09-17 实现完成后回写)

实现过程中相对本规格的修正与补充(均为实现细节层面,核心架构不变):

1. **Next 为 owned 类型**(无 `Next<'a>` 生命周期参数):中间件链全部 Arc 持有,
   future 均为 `'static`,与 owned Context 决策一致
2. **Handler 要求 `Clone`**:erasure 采用「`fn call(&self)` + blanket impl 内部
   clone + 存储 `Arc<dyn Fn(Context) -> BoxFuture>`」方案,免 proc-macro
3. **`Error` 增加 `Io(#[from] std::io::Error)` 变体**(映射为脱敏 500);
   `Error::Param` 的 source 以 `String` 存储(规避 rustc HRTB 限制)
4. **body 限流使用自研 `Bounded` 包装**(固定错误类型),不用 `http_body_util::Limited`
   —— 后者的 `B::Error: Into<Box<dyn Error>>` 约束在 handler future 内触发
   rustc "implementation of From is not general enough"
5. **Body 类型为 `UnsyncBoxBody<Bytes, Error>`**(Send 即可,Sync 非必需)
6. **405 响应改为流经全局中间件链**(终端 handler 渲染 405 + Allow),
   保证 CORS 预检在未注册 OPTIONS 的路由上也能工作
7. **`Resp::ok(())` 渲染 `data: null`**(Rust 单元类型序列化的自然结果),
   需要完全省略 data 字段时直接构造结构体
8. 依赖新增 `futures-core`(Stream trait 定义,零依赖)、`httpdate`(HTTP 日期解析)
9. prelude 额外导出 `Bytes`、`TestClient`;闭包 handler 返回 `Result` 时需标注
   错误类型 `Ok::<_, Error>(...)`(Rust 类型推断固有行为,文档已注明)

## 17. 风险与已知取舍

- **owned Context 的取舍**:中间件后置阶段拿不到 ctx(只有响应);handler 需要读 body
  之外的内部可变场景极罕见。换来零宏、零 async-trait、纯 async fn 注册 —— 值得
- matchit 路径冲突在注册期 panic:fail-fast 是 web 框架惯例,文档明示
- body 全缓冲(json/form/bytes):对常规 API 足够;超大上传场景应流式
  (`body_bytes` 之外 v1 不提供流式读取 API,留 v1.1)
- cookie 手工传递:Set-Cookie 便捷 builder 暂缺,经 `Response` headers 设置(cookie crate
  的 `Cookie` 格式化可用),完整的 CookieJar 管理留 v1.1

---

## 18. v1.1 追加实现(发布 0.1.0 之后)

1. **SSE**(`src/sse.rs`):`Event` builder(event/data/id/retry/comment/json_data,
   多行 data 重复 data 字段)+ `Sse<S>` 响应(text/event-stream + no-cache)
2. **Range 请求**(`src/fs.rs`):单区间 `bytes=a-b|a-|-n` → 206 + Content-Range,
   seek + take 有界读取;不可满足 → 416(`bytes */total`);多区间/畸形 → 忽略
   返回 200(RFC 合规);200 响应携带 `Accept-Ranges: bytes`
3. **WebSocket**(feature `ws`,tokio-tungstenite):`ctx.websocket()` 校验握手头
   → `WebSocketUpgrade::on_upgrade(cb)` 构建 101(handshake derive accept key)并
   spawn 回调;hyper `OnUpgrade` 从请求 extensions 摘取后经 Context 传递,
   `Upgraded` 经 `TokioIo` 桥接 tungstenite;真实 socket 回显测试覆盖
