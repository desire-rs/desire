//! Throughput / allocation comparison harness (desire side).
//! Same counting allocator and equivalent handler work as bench_axum:
//! one path param + a `{"code":0,"msg":"ok","data":…}` JSON response.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

static ALLOCS: AtomicUsize = AtomicUsize::new(0);

struct Counting;

unsafe impl GlobalAlloc for Counting {
  unsafe fn alloc(&self, l: Layout) -> *mut u8 {
    ALLOCS.fetch_add(1, Ordering::Relaxed);
    System.alloc(l)
  }
  unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
    System.dealloc(p, l)
  }
  unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
    ALLOCS.fetch_add(1, Ordering::Relaxed);
    System.realloc(p, l, n)
  }
  unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
    ALLOCS.fetch_add(1, Ordering::Relaxed);
    System.alloc_zeroed(l)
  }
}

#[global_allocator]
static A: Counting = Counting;

use desire::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
  let app = App::new().route("/hello/{name}", get(hello)).route(
    "/stats",
    get(|| async { Resp::ok(ALLOCS.load(Ordering::Relaxed) as i64) }),
  );
  app.run("0.0.0.0:3000").await
}

async fn hello(ctx: Context) -> Result<Resp<String>> {
  let name = ctx.param_raw("name")?;
  Ok(Resp::ok(format!("hello, {name}!")))
}
