//! Concurrency stress: hammer shared state, the cookie queue, body
//! caching, and middleware through many parallel requests, then assert
//! consistency. These run against the full dispatch pipeline.

use desire::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Clone, Default)]
struct Counter {
  hits: std::sync::Arc<AtomicUsize>,
  notes: std::sync::Arc<tokio::sync::RwLock<Vec<Note>>>,
}

#[derive(Serialize, Deserialize, Clone)]
struct Note {
  body: String,
}

fn stress_app() -> App {
  App::new()
    .state(Counter::default())
    .with(desire::middleware::logger())
    .with(desire::middleware::gzip())
    .route(
      "/notes",
      get(|ctx: Context| async move {
        let counter = ctx.state::<Counter>()?;
        counter.hits.fetch_add(1, Ordering::Relaxed);
        let (notes, total) = {
          let notes = counter.notes.read().await;
          (notes.clone(), notes.len() as i64)
        };
        Ok::<_, Error>(Resp::page(notes, 1, 100, total))
      })
      .post(|ctx: Context| async move {
        let note: Note = ctx.json().await?;
        let counter = ctx.state::<Counter>()?;
        counter.notes.write().await.push(note.clone());
        counter.hits.fetch_add(1, Ordering::Relaxed);
        Ok::<_, Error>(Resp::created(note))
      }),
    )
    .route(
      "/hit",
      get(|ctx: Context| async move {
        let counter = ctx.state::<Counter>()?;
        let hits = counter.hits.fetch_add(1, Ordering::Relaxed) + 1;
        Ok::<_, Error>(Resp::ok(hits))
      }),
    )
    .route(
      "/cookie",
      post(|mut ctx: Context| async move {
        ctx.set_cookie(desire::cookie::Cookie::build(("k", "v")).build());
        Ok::<_, Error>(Resp::ok(()))
      }),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn parallel_note_writes_are_all_recorded() {
  let tc = TestClient::new(stress_app());
  let tasks = 48;
  let per_task = 8;

  let mut handles = Vec::new();
  for t in 0..tasks {
    let tc = tc.clone();
    handles.push(tokio::spawn(async move {
      for i in 0..per_task {
        let res = tc
          .post("/notes")
          .json(&Note {
            body: format!("note-{t}-{i}"),
          })
          .send()
          .await;
        res.assert_status(StatusCode::CREATED);
      }
    }));
  }
  for h in handles {
    h.await.unwrap();
  }

  let res = tc.get("/notes").send().await;
  res.assert_status_ok();
  let body: Resp<Vec<Note>> = res.json().unwrap();
  let total = tasks * per_task;
  assert_eq!(
    body.page.as_ref().unwrap().total,
    total as i64,
    "every write must be recorded exactly once"
  );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mixed_traffic_stays_consistent() {
  let tc = TestClient::new(stress_app());
  let mut handles = Vec::new();

  for i in 0..64 {
    let tc = tc.clone();
    handles.push(tokio::spawn(async move {
      match i % 4 {
        0 => {
          let res = tc.get("/hit").send().await;
          res.assert_status_ok();
          let hits = res.assert_ok_data::<i64>();
          assert!(hits > 0);
        }
        1 => {
          let res = tc
            .post("/notes")
            .json(&Note {
              body: format!("n{i}"),
            })
            .send()
            .await;
          res.assert_status(StatusCode::CREATED);
        }
        2 => {
          let res = tc
            .get("/notes")
            .header("accept-encoding", "gzip")
            .send()
            .await;
          res.assert_status_ok();
        }
        _ => {
          let res = tc.post("/cookie").send().await;
          res.assert_status_ok();
          assert_eq!(res.header("set-cookie"), Some("k=v"));
        }
      }
    }));
  }
  for h in handles {
    h.await.unwrap();
  }
}
