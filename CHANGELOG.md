# Changelog

All notable changes to desire are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/); versions follow
[SemVer](https://semver.org) (0.x: breaking changes ship as minor bumps).

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
