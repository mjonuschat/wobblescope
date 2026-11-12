//! What the daemon's websockets to an `https://` Moonraker trust and refuse.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use rustls::crypto::ring;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tokio_tungstenite::tungstenite::Message;
use wobblescope::moonraker::bridge::Bridge;
use wobblescope::moonraker::rpc::{Rpc, RpcError};
use wobblescope::moonraker::tls::{Tls, TlsError, TlsOptions};

const TIMEOUT: Duration = Duration::from_secs(5);

/// A Moonraker stand-in that speaks TLS and answers every JSON-RPC call with `"pong"`.
struct TlsServer {
    addr: SocketAddr,
    cert_pem: String,
}

impl TlsServer {
    async fn start(names: &[&str]) -> Self {
        let names: Vec<String> = names.iter().map(|n| (*n).to_owned()).collect();
        let certified = rcgen::generate_simple_self_signed(names).unwrap();
        let cert_pem = certified.cert.pem();
        let config =
            rustls::ServerConfig::builder_with_provider(Arc::new(ring::default_provider()))
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_no_client_auth()
                .with_single_cert(
                    vec![CertificateDer::from(certified.cert.der().to_vec())],
                    PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
                        certified.signing_key.serialize_der(),
                    )),
                )
                .unwrap();
        let acceptor = TlsAcceptor::from(Arc::new(config));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((tcp, _)) = listener.accept().await else {
                    return;
                };
                let acceptor = acceptor.clone();
                tokio::spawn(async move {
                    let Ok(tls) = acceptor.accept(tcp).await else {
                        return;
                    };
                    let Ok(mut socket) = tokio_tungstenite::accept_async(tls).await else {
                        return;
                    };
                    while let Some(Ok(Message::Text(text))) = socket.next().await {
                        let call: Value = serde_json::from_str(&text).unwrap();
                        let reply = json!({"jsonrpc": "2.0", "id": call["id"], "result": "pong"});
                        if socket.send(Message::text(reply.to_string())).await.is_err() {
                            return;
                        }
                    }
                });
            }
        });
        Self { addr, cert_pem }
    }

    fn url(&self) -> String {
        format!("wss://{}/websocket", self.addr)
    }

    fn trust_file(&self, dir: &TempDir) -> PathBuf {
        let path = dir.path().join("moonraker.pem");
        std::fs::write(&path, &self.cert_pem).unwrap();
        path
    }
}

fn trusting(path: PathBuf) -> Tls {
    Tls::new(&TlsOptions {
        ca_file: Some(path),
        insecure_skip_verify: false,
    })
    .unwrap()
}

async fn ping(url: &str, tls: &Tls) -> Result<Value, RpcError> {
    let (rpc, _notes) = Rpc::connect(url, TIMEOUT, tls).await?;
    rpc.call("server.info", None, TIMEOUT).await
}

#[tokio::test]
async fn a_server_whose_certificate_is_in_the_ca_file_is_trusted() {
    let server = TlsServer::start(&["127.0.0.1"]).await;
    let dir = TempDir::new().unwrap();
    let tls = trusting(server.trust_file(&dir));
    assert_eq!(ping(&server.url(), &tls).await.unwrap(), json!("pong"));
}

#[tokio::test]
async fn a_self_signed_certificate_is_refused_with_the_default_roots() {
    let server = TlsServer::start(&["127.0.0.1"]).await;
    let err = ping(&server.url(), &Tls::default()).await.unwrap_err();
    assert!(matches!(err, RpcError::Connect(_)), "{err:?}");
    assert!(
        err.to_string().to_lowercase().contains("certificate"),
        "the error does not mention the certificate: {err}"
    );
}

#[tokio::test]
async fn a_certificate_for_another_name_is_refused_even_when_trusted() {
    let server = TlsServer::start(&["printer.example"]).await;
    let dir = TempDir::new().unwrap();
    let tls = trusting(server.trust_file(&dir));
    let err = ping(&server.url(), &tls).await.unwrap_err();
    assert!(matches!(err, RpcError::Connect(_)), "{err:?}");
}

#[tokio::test]
async fn skipping_verification_accepts_any_certificate() {
    let server = TlsServer::start(&["printer.example"]).await;
    let tls = Tls::new(&TlsOptions {
        ca_file: None,
        insecure_skip_verify: true,
    })
    .unwrap();
    assert_eq!(ping(&server.url(), &tls).await.unwrap(), json!("pong"));
}

#[tokio::test]
async fn the_bridge_verifies_the_server_the_same_way() {
    let server = TlsServer::start(&["127.0.0.1"]).await;
    let url = format!("wss://{}/klippysocket?token=T", server.addr);
    let open = |tls: Tls| {
        let url = url.clone();
        async move { Bridge::open(&url, "adxl345/dump_adxl345", None, TIMEOUT, &tls).await }
    };
    assert!(open(Tls::default()).await.is_err());
    let dir = TempDir::new().unwrap();
    open(trusting(server.trust_file(&dir))).await.unwrap();
}

#[test]
fn a_ca_file_and_skipping_verification_together_are_refused() {
    let err = Tls::new(&TlsOptions {
        ca_file: Some(PathBuf::from("/etc/ssl/ca.pem")),
        insecure_skip_verify: true,
    })
    .unwrap_err();
    assert!(matches!(err, TlsError::Conflict), "{err:?}");
}

#[test]
fn a_ca_file_that_cannot_be_used_is_refused_naming_the_file() {
    let dir = TempDir::new().unwrap();
    let options = |path: PathBuf| TlsOptions {
        ca_file: Some(path),
        insecure_skip_verify: false,
    };

    let missing = dir.path().join("missing.pem");
    let err = Tls::new(&options(missing.clone())).unwrap_err();
    assert!(matches!(err, TlsError::Read { .. }), "{err:?}");
    assert!(err.to_string().contains("missing.pem"), "{err}");

    let empty = dir.path().join("empty.pem");
    std::fs::write(&empty, "no certificate here\n").unwrap();
    let err = Tls::new(&options(empty)).unwrap_err();
    assert!(matches!(err, TlsError::NoCertificate(_)), "{err:?}");

    let broken = dir.path().join("broken.pem");
    std::fs::write(
        &broken,
        "-----BEGIN CERTIFICATE-----\nnot base64 !!\n-----END CERTIFICATE-----\n",
    )
    .unwrap();
    let err = Tls::new(&options(broken)).unwrap_err();
    assert!(err.to_string().contains("broken.pem"), "{err}");
}
