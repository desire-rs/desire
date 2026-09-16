# 为什么 2026 年还要再写一个 Rust Web 框架

每当有人宣布新的 Rust Web 框架,评论区一定会出现那条经典回复:"axum 不够你用吗?"

这个问题值得正面回答。写 desire 的这几个月里,我反复问自己同样的问题。答案不是"axum 不好"——axum 的设计质量有目共睹。答案是:**框架的使用体验本身就是产品,而在这个维度上,仍然存在一个 axum 因历史包袱无法覆盖的空白**。

下面是 desire 的三个核心押注,以及它们背后的取舍。判断这些取舍是否值得,比判断"要不要再写一个框架"更有意义。

## 押注一:Context 对象,而不是 extractor 元组

axum 里,一个 handler 长这样:

```rust
async fn get_user(
    State(db): State<Db>,
    Path(id): Path<i64>,
    Json(body): Json<CreateUser>,
) -> impl IntoResponse { ... }
```

参数的顺序、数量(元组有上限)、每个 extractor 的语义,都是要学的知识。错误发生时,你能拿到一个 ` rejection`——信息通常够编译器看,不够人类看。

desire 里,handler 长这样:

```rust
async fn get_user(ctx: Context) -> Result<Resp<User>> {
    let id: i64 = ctx.param("id")?;
    let body: CreateUser = ctx.json().await?;
    ...
}
```

想读什么就显式调用什么,顺序自由,IDE 补全直接列出全部能力。代价是:你不能用自己的类型参与"参数自动装配"的抽象——所有提取都是显式的方法调用。

换来的是**诊断能力的质变**。desire 的每个提取错误都带三层信息:哪一层(path/query/body)、哪个字段、期望什么类型:

```text
invalid param `id`, expected `i64`: invalid digit found in string
invalid json body: missing field `email` at line 1 column 20
```

在 extractor 元组模型里,这几乎不可能:元组的第 N 个元素失败了,错误信息很难知道"业务上"这个字段叫什么。

## 押注二:统一响应信封

所有响应——成功、业务错误、框架错误——都是同一个 JSON 形状:

```json
{"code": 0, "msg": "ok", "data": {...}}
```

`Resp::ok(user)`、`Resp::err(40001, "余额不足")`、参数解析失败自动产生的 400,渲染完全一致。前端写一次拦截器,终身受用。

这是一个**有立场的设计**:它默认你在做前后端分离的 API 服务,默认你的团队需要统一约定。axum 刻意不做任何假设,把序列化格式完全留给你;desire 假设到底,把约定内建。如果你不需要这个约定,它确实碍事——诚实地讲,这是双向门。

## 押注三:零宏、零 async-trait

desire 的 handler 是纯 Rust:`async fn(ctx: Context) -> impl IntoResponse` 直接实现 `Handler` trait,没有 `#[handler]` 宏,没有 `#[async_trait]`。注册、类型推断、报错,全部是标准 Rust 行为。

这个"没有魔法"的承诺,实现上代价不小,这里有个真实的战争故事:

最初的设计是 `async fn(ctx: &Context)`——借用更优雅。但写下 blanket impl 的时候撞上了 Rust 的 HRTB 墙:异步 fn 的返回 future 借用了参数的生命周期,`for<'a> Fn(&'a Context) -> impl Future + 'a` 这个约束在 stable Rust 上**无法表达**,trait 对象派发直接不可能。salvo 为此写了 proc-macro,poem 干脆传 owned Request。

我的选择是传 owned `Context`(字段全部廉价克隆/共享),配合 `self: Arc<Self>` 派发,让一切回归普通 trait + blanket impl。写实现计划之前,我用一个 60 行的独立 crate 验证了这个形态可以编译——**重要的架构决策,先用编译器投票**。

后来又撞上第二个墙:为了支持请求体大小限制,我用了 `http_body_util::Limited`,它的泛型约束 `B::Error: Into<Box<dyn Error>>` 在 handler future 里触发了 rustc "implementation of From is not general enough" 的已知限制。解法是自己写了 30 行固定错误类型的 body 包装器。这类坑不值得每个用户再踩一遍,所以它们被写进了仓库的 AGENTS.md,给未来的人类和 AI 协作者。

## 我们刻意不做的

一个框架的边界和它的功能同样重要:

- **不兼容 Tower**。Tower `Service` 是优秀的抽象,但它的 `poll_ready`/泛型服务心智模型正是"学习曲线"的主要来源。desire 的中间件是 `async fn(ctx, next)`,简单,但不和 Tower 生态互通。
- **不做 extractor 宏**。所有提取都是显式调用,没有 attribute 魔法。
- **不追 TechEmpower 榜单**。底层是 hyper,天花板和 axum 相同;但 desire 的优化优先级是"人类编译代码的速度",不是"机器处理请求的速度"。

## 什么时候请继续用 axum

为了可信度,这段必须写实:

- 你需要 Tower 生态的某个特定中间件(tower-http 的某些能力、自定义 Service);
- 你在做对抽象复用要求极高的中间件库,需要 extractor 系统的扩展性;
- 你的团队已经熟练掌握 axum,迁移没有增量收益。

desire 的目标用户是:做前后端分离 API 服务、重视统一错误约定、希望新人一周内产出、被 extractor rejection 报错折磨过的人。

## 现状

- 0.2.0 已发布到 [crates.io](https://crates.io/crates/desire),基于 hyper 1.x + edition 2024,MSRV 1.85;
- 功能覆盖:路由、中间件、SSE、WebSocket、multipart、cookie、gzip、静态文件(防穿越 + Range)、OpenAPI + Swagger UI、内存测试客户端;
- 58 个测试,clippy/fmt/doc 零警告,CI 覆盖 MSRV。

0.x 阶段,API 还会演进,但六个核心概念(App、Context、Handler、Middleware/Next、Router/MethodRouter、Resp)已经稳定。欢迎来 [GitHub](https://github.com/desire-rs/desire) 看看,或者直接 `cargo add desire` 感受一下——喜欢的话,一个 Star 就是最好的反馈。
