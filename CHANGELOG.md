# Changelog

All notable changes to desire are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/); versions follow
[SemVer](https://semver.org) (0.x: breaking changes ship as minor bumps).

## [1.0.0-rc.1] - 2026-09-17

The public API is now **frozen**: from here, breaking changes ship only in
2.0. Four weeks without a blocking incident promotes this to 1.0.0.

### Added

- `Server::on_bound(callback)`: learn the actually bound address — required
  for port-0 binds in tests and service meshes. (Final API-audit fix.)
- Root re-exports completed: `FormData`, `UploadedFile`, `Event`, `Sse` are
  now reachable at the crate root like every other core type.

### Policy

- SemVer and MSRV policy documented in the README Stability section and
  `docs/1.0-plan.md` (all audit items checked off).

## [1.0.0-rc.2] - 2026-09-17

Performance and architecture review round (no public API changes except
where noted).

### Changed

- **Dispatch: middleware chains are baked at build time.** The combined
  global + route chain used to be rebuilt with a fresh `Vec` + boxed slice
  on every request; it is now computed once per route at startup. Measured
  with a counting allocator on a 5-middleware + path-param route: **38.4 →
  35.4 heap allocations per request** (−3 exactly: the path copy, the chain
  `Vec`, and its `Arc<[..]>` conversion), plus ~5 fewer atomic refcount
  operations. Throughput on small workloads is unchanged (JSON + hyper
  dominate); the saving is O(1) per request instead of O(middleware count).
- The request path is borrowed straight from the request parts instead of
  copied for route matching.
- 405 responses now also flow through route-level middleware (previously
  global only) — consistent with the CORS-preflight design — and their
  `Allow` header is precomputed per route instead of sorted/joined per hit.
- `gzip()` skips bodies smaller than 1 KiB (buffered bodies report their
  size exactly; streams are always compressed) and bodyless statuses
  (204/304/1xx) — compressing tiny bodies cost CPU and grew the payload.
  Tunable via the new `gzip_with(min_size)` (`gzip_with(0)` compresses
  everything). Behavior change is documented as a performance fix.

## [0.6.0] - 2026-09-17

### Added

- The docs.rs front page is now a full guide + cookbook: routing, extraction,
  responses, state, middleware, errors, static files, SSE/WebSockets,
  testing, deployment — every example is compile-checked as a doctest.
- Adversarial path battery test: 26 hostile encodings (double-encoded
  `%252e`, overlong `%c0%ae`, `..;`, NUL, Windows forms, …) verified to
  never leak outside the served root.
- Doc examples for `Error`, `Json<T>`, `Html<T>`.
- `docs.rs` builds with all features (`package.metadata.docs.rs`), so the
  `tls`/`ws`/`gzip`/`openapi` modules are documented.

## [0.5.0] - 2026-09-17

### Added

- `Response::add_cookie(...)`: set cookies in middleware after `next.run`.
- `Sse::new(..).keep_alive(interval)`: `:keep-alive` comment frames while
  the event stream is idle, so proxies and browsers keep the connection.
- `ServeDir::files_listing()`: HTML directory listing when a directory has
  no `index.html` — HTML-escaped names, dotfiles hidden unless
  `allow_dotfiles()` is set, `index.html` still takes precedence.
- CI: `cargo audit` job; Stability section in the README; `docs/1.0-plan.md`
  audit progress.

## [0.4.0] - 2026-09-17

### Added

- `examples/demo.rs`: an interactive browser showcase — live SSE ticks,
  WebSocket echo, multipart upload, and the response envelope in one page
  (verified end-to-end in a real browser).
- `ServeDir::fallback_file("index.html")`: single-page-app mode — unknown
  paths serve the fallback file with 200 so the client router takes over.
- `docs/1.0-plan.md`: the roadmap and API-stability checklist for 1.0.

## [0.3.0] - 2026-09-17

### Added

- `Context::body_stream()`: the request body as a chunk stream, for proxying
  and large uploads that should not be buffered. Consumes the raw body;
  calling it after a buffered read yields the cached bytes instead.
- `ServeDir::cache_control(...)` / `ServeFile::cache_control(...)`: emit a
  `Cache-Control` header on successful responses.
- `ServeDir` rejects dotfile segments (`.git/config`, `.env`, …) by default;
  opt back in with `ServeDir::allow_dotfiles()`.

## [0.2.0] - 2026-09-17

### Added

- `Context::form_data()`: buffered `multipart/form-data` parsing
  (`FormData` / `UploadedFile`).
- `Context::set_cookie(...)`: queue `Set-Cookie` on the response from a
  handler (drained by the framework after the handler returns).
- `gzip()` middleware (feature `gzip`): content negotiation via
  `Accept-Encoding`, streaming compression.
- OpenAPI generation (feature `openapi`): `PathDoc` builder with
  schemars-derived schemas, `Resp` envelope wrapping, `App::openapi`
  mounting `/openapi.json` + Swagger UI `/docs`.
- `Error`: `From<Error> for std::io::Error`, `From<Infallible>`.

## [0.1.0] - 2026-09-16

### Added

- Initial release: Context-style handlers, `Resp<T>` unified envelope,
  matchit routing with nest/merge, Context-style middleware with builtins
  (logger/cors/timeout/body_limit), streaming responses, hyper-util server
  (HTTP/1.1 + HTTP/2, graceful shutdown, concurrency limit, body cap),
  rustls TLS (feature `tls`), traversal-safe static files, in-memory
  `TestClient`.
- Server-Sent Events (`sse`), single-range static file requests (206/416),
  WebSocket support (feature `ws`).
