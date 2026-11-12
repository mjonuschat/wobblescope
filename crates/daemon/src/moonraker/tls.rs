//! How the daemon verifies the certificate of an `https://` Moonraker.
//!
//! One [`Tls`] serves every websocket the daemon opens to Moonraker, the control connection and
//! the bridge to Klipper's own socket.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, ring};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, RootCertStore, SignatureScheme};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Error as WsError;
use tokio_tungstenite::{Connector, MaybeTlsStream, WebSocketStream};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TlsOptions {
    /// A PEM file of the certificates to trust instead of the built-in public roots, such as the
    /// private CA that signed Moonraker's certificate, or Moonraker's own self-signed one.
    pub ca_file: Option<PathBuf>,
    /// Accept any certificate without checking it. This removes the protection TLS gives against
    /// an attacker on the network path.
    pub insecure_skip_verify: bool,
}

impl TlsOptions {
    #[must_use]
    pub fn is_set(&self) -> bool {
        self.ca_file.is_some() || self.insecure_skip_verify
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TlsError {
    #[error("set either a CA file or skip verification, not both")]
    Conflict,
    #[error("cannot read {}: {source}", path.display())]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{} is not a valid PEM certificate file: {source}", path.display())]
    Pem {
        path: PathBuf,
        source: rustls::pki_types::pem::Error,
    },
    #[error("{} contains no PEM certificate", .0.display())]
    NoCertificate(PathBuf),
    #[error("{} holds a certificate that cannot be trusted: {source}", path.display())]
    BadCertificate {
        path: PathBuf,
        source: rustls::Error,
    },
}

/// The TLS settings for the daemon's websockets to Moonraker. Clones share them.
#[derive(Clone)]
pub struct Tls {
    config: Arc<ClientConfig>,
}

impl std::fmt::Debug for Tls {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Tls").finish_non_exhaustive()
    }
}

impl Default for Tls {
    fn default() -> Self {
        let roots = RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        Self {
            config: client_config(roots),
        }
    }
}

impl Tls {
    pub fn new(options: &TlsOptions) -> Result<Self, TlsError> {
        match (&options.ca_file, options.insecure_skip_verify) {
            (Some(_), true) => Err(TlsError::Conflict),
            (None, false) => Ok(Self::default()),
            (None, true) => Ok(Self {
                config: insecure_client_config(),
            }),
            (Some(path), false) => Self::trusting(path),
        }
    }

    fn trusting(path: &Path) -> Result<Self, TlsError> {
        let bytes = std::fs::read(path).map_err(|source| TlsError::Read {
            path: path.to_owned(),
            source,
        })?;
        let mut roots = RootCertStore::empty();
        let mut found = false;
        for certificate in CertificateDer::pem_slice_iter(&bytes) {
            let certificate = certificate.map_err(|source| TlsError::Pem {
                path: path.to_owned(),
                source,
            })?;
            roots
                .add(certificate)
                .map_err(|source| TlsError::BadCertificate {
                    path: path.to_owned(),
                    source,
                })?;
            found = true;
        }
        if !found {
            return Err(TlsError::NoCertificate(path.to_owned()));
        }
        Ok(Self {
            config: client_config(roots),
        })
    }
}

pub(super) async fn connect(
    url: &str,
    tls: &Tls,
) -> Result<WebSocketStream<MaybeTlsStream<TcpStream>>, WsError> {
    let connector = Connector::Rustls(Arc::clone(&tls.config));
    let (socket, _) =
        tokio_tungstenite::connect_async_tls_with_config(url, None, false, Some(connector)).await?;
    Ok(socket)
}

fn provider() -> Arc<CryptoProvider> {
    Arc::new(ring::default_provider())
}

fn client_config(roots: RootCertStore) -> Arc<ClientConfig> {
    Arc::new(
        ClientConfig::builder_with_provider(provider())
            .with_safe_default_protocol_versions()
            .expect("the ring provider supports the default protocol versions")
            .with_root_certificates(roots)
            .with_no_client_auth(),
    )
}

fn insecure_client_config() -> Arc<ClientConfig> {
    let provider = provider();
    Arc::new(
        ClientConfig::builder_with_provider(Arc::clone(&provider))
            .with_safe_default_protocol_versions()
            .expect("the ring provider supports the default protocol versions")
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(AcceptAnyCertificate(provider)))
            .with_no_client_auth(),
    )
}

/// Accepts every server certificate. The handshake signatures are still checked, so the
/// connection is encrypted, but the peer is not authenticated.
#[derive(Debug)]
struct AcceptAnyCertificate(Arc<CryptoProvider>);

impl ServerCertVerifier for AcceptAnyCertificate {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}
