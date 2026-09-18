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

## Reference: same-workload comparison with axum

Per the 1.0 plan this is published as reference data, not a selling point.
Two example binaries do byte-identical work — one path param + a
`{"code":0,"msg":"ok","data":…}` JSON response — under the same counting
allocator (`examples/bench.rs`, `examples/bench_axum.rs`, axum 0.8):

| Framework | allocs/request | throughput (ab, 20 conn) |
|-----------|---------------:|--------------------------|
| desire 1.0.0-rc.3 | **30.4** (stable across runs) | 5.4k–11.8k (noisy) |
| axum 0.8 | **46.4** (stable across runs) | 7.0k–13.0k (noisy) |

Read honestly: throughput on this localhost debug-build setup cannot
separate the two (both sit on hyper; run-to-run variance is ±40%), and we
do not claim a throughput difference. The reproducible signal is
allocation pressure: desire's dispatch + envelope do ~35% fewer heap
allocations for equivalent work, which shows up as steadier latency under
allocator contention rather than as headline RPS.

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

## rc.4 polish: transport + cookie fast path

- Accepted sockets now set `TCP_NODELAY` (hyper-util's auto builder does
  not expose it): prevents Nagle/delayed-ACK stalls on real networks.
  On loopback ab the effect is within noise (non-keepalive ~24k RPS,
  keep-alive ~75k RPS both with and without) — it is a
  production-correctness default, not a local benchmark win.
- The per-request cookie queue gained a non-empty flag: cookie-less
  requests (the majority) skip the mutex entirely.
- `serde_json::to_vec` already pre-allocates 128 bytes — verified and
  left alone (an example of checking before "optimizing").

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
