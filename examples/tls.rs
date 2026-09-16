//! HTTPS with rustls. Requires the `tls` feature:
//!
//! ```text
//! cargo run --example tls --features tls
//! curl -k https://localhost:3000/
//! ```
//!
//! For local testing, generate a self-signed certificate:
//!
//! ```text
//! openssl req -x509 -newkey rsa:2048 -nodes -keyout key.pem \
//!   -out cert.pem -days 365 -subj "/CN=localhost"
//! ```

use desire::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
  let tls = TlsConfig::builder()
    .cert_file("cert.pem")?
    .key_file("key.pem")?
    .build()?;

  let app = App::new().route("/", get(|| async { Resp::ok("over https") }));

  Server::new(app)
    .bind("0.0.0.0:3443")?
    .tls(tls)
    .graceful_shutdown(async {
      let _ = tokio::signal::ctrl_c().await;
    })
    .run()
    .await
}
