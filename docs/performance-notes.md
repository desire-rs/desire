# Performance notes

Methodology and measurements for desire's hot-path work. All numbers are
from debug builds on Apple Silicon (macOS 26, `ab -n 10000 -c 20`, hello
route with one path param and a JSON envelope response) — absolute
throughput is meaningless across machines; the allocation counts are the
stable metric.

## Allocations per request (counting global allocator)

| Version | allocs/request | change |
|---------|---------------:|--------|
| 1.0.0-rc.1 | 38.4 | baseline |
| 1.0.0-rc.2 | 35.4 | −3.0: middleware chain baked at build time (was: per-request `Vec` + `Arc<[..]>` conversion), path borrowed instead of copied, `Allow` header precomputed |
| 1.0.0-rc.3 | 30.6 | −4.8: path params `HashMap` → small assoc list (no hashbrown table + SipHash per request), `MethodRouter` method lookup `HashMap` → assoc list (no hashing/probing per request) |

Net: **−20% heap allocations per request**, with zero public API changes.

## Throughput (ab, debug build, same machine)

Unchanged within noise (~15k RPS ±5% across configurations): on small
JSON responses the cost is dominated by serde_json serialization, hyper
HTTP/1 parsing, and tokio scheduling — not by desire's dispatch layer.
The allocation reductions compound with middleware count (chain rebuild
was O(middleware count) per request, now O(1)) and shrink allocator
pressure on hot paths.

## Sampling profile findings (macOS `sample`, 50 concurrent)

- Kernel waits (`psynch_cvwait`, `kevent`, `swtch_pri`) dominate at 50
  concurrent connections — the server is mostly waiting, not burning CPU
  in framework code.
- Largest framework-adjacent CPU items: `hyper::proto::h1` request
  parsing, `memcpy` in buffered writes, `serde_json` string escaping,
  (pre-fix) hashbrown `reserve_rehash` from the per-request params map.
- desire's own `dispatch` registers only a handful of samples after the
  rc.2/rc.3 work.

## Decisions

- `Next` owns the remaining chain (an `Arc`), so middleware futures stay
  `'static` — borrowing it would infect every middleware's future with a
  lifetime and break the plain-`async fn` contract.
- The per-request cookie jar is one small allocation, shared between the
  dispatcher and the context; lazily creating it from a `&Context` is not
  possible without another allocation, so it stays.
- Static file caching in user space was considered and rejected: the OS
  page cache already serves repeated reads at ~1μs, and user-space caches
  add invalidation risk for marginal gain.
