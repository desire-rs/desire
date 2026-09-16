//! TLS support (requires the `tls` feature): rustls with ALPN
//! negotiating HTTP/2 and HTTP/1.1 on the same port.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;

use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio::net::TcpStream;
use tokio_rustls::TlsAcceptor;

/// A configured TLS acceptor. Build via [`TlsConfig::builder`].
#[derive(Clone)]
pub struct TlsConfig {
  pub(crate) acceptor: TlsAcceptor,
}

impl TlsConfig {
  /// Start building a TLS configuration from PEM files.
  pub fn builder() -> TlsConfigBuilder {
    TlsConfigBuilder {
      certs: Vec::new(),
      key: None,
    }
  }

  pub(crate) async fn accept(
    &self,
    stream: TcpStream,
  ) -> std::io::Result<tokio_rustls::server::TlsStream<TcpStream>> {
    self.acceptor.accept(stream).await
  }
}

/// Builder for [`TlsConfig`]: load PEM-encoded certificate chain and
/// private key from files.
pub struct TlsConfigBuilder {
  certs: Vec<CertificateDer<'static>>,
  key: Option<PrivateKeyDer<'static>>,
}

impl TlsConfigBuilder {
  /// Load a PEM certificate chain (leaf first, then intermediates).
  pub fn cert_file(mut self, path: impl AsRef<Path>) -> std::io::Result<Self> {
    let mut reader = BufReader::new(File::open(path.as_ref())?);
    let certs: Vec<_> = rustls_pemfile::certs(&mut reader).collect::<Result<_, _>>()?;
    if certs.is_empty() {
      return Err(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "no certificates found",
      ));
    }
    self.certs = certs;
    Ok(self)
  }

  /// Load the PEM private key (PKCS#8, PKCS#1, or SEC1).
  pub fn key_file(mut self, path: impl AsRef<Path>) -> std::io::Result<Self> {
    let mut reader = BufReader::new(File::open(path.as_ref())?);
    let key = rustls_pemfile::private_key(&mut reader)?
      .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "no key found"))?;
    self.key = Some(key);
    Ok(self)
  }

  /// Finalize. ALPN is configured to offer `h2` and `http/1.1`.
  pub fn build(self) -> std::io::Result<TlsConfig> {
    let key = self.key.ok_or_else(|| {
      std::io::Error::new(std::io::ErrorKind::InvalidData, "no private key configured")
    })?;
    let mut server_config = rustls::ServerConfig::builder()
      .with_no_client_auth()
      .with_single_cert(self.certs, key)
      .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    server_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(TlsConfig {
      acceptor: TlsAcceptor::from(Arc::new(server_config)),
    })
  }
}
