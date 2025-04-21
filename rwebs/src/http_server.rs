use common::{io::peek_stream::PeekableStream, mac::Mac};
use tokio::{io::{AsyncRead, AsyncWrite}, net::TcpListener};
use crate::quic_server::QuicServer;
use rustls::{pki_types::pem::PemObject, ServerConfig};
use tokio_rustls::TlsAcceptor;

pub async fn run(port:u16,quic_server:QuicServer) -> Result<(), Box<dyn std::error::Error+Send+Sync>> {
    let listener = TcpListener::bind(format!("0.0.0.0:{}",port)).await?;
    log::info!("http_server listen on {}",listener.local_addr()?);
    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                log::debug!("accept from {}", addr);
                let quic_server = quic_server.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_client(stream,quic_server,None).await{
                        log::warn!("handle client error:{}",e);
                    }
                });
            }
            Err(e) => {
                log::warn!("accept error:{}", e);
            }
        }
    }
}

pub async fn run_https(port:u16,quic_server:QuicServer,priv_key:&str,cert_der:&str) -> Result<(), Box<dyn std::error::Error+Send+Sync>> {
    let priv_key = rustls::pki_types::PrivateKeyDer::from_pem_slice(priv_key.as_bytes())?;
    let cert_chain = extract_full_pem_certificates(cert_der).into_iter().filter_map(|s|rustls::pki_types::CertificateDer::from_pem_slice(s.as_bytes()).ok()).collect::<Vec<_>>();
    let config = ServerConfig::builder()
    .with_no_client_auth()
    .with_single_cert(cert_chain, priv_key)?;
    let acceptor = TlsAcceptor::from(std::sync::Arc::new(config));
    let listener = TcpListener::bind(format!("0.0.0.0:{}",port)).await?;
    log::info!("https_server listen on {}",listener.local_addr()?);
    loop{
        match listener.accept().await {
            Ok((stream, addr)) => {
                log::debug!("accept from {}", addr);
                let quic_server = quic_server.clone();
                let acceptor = acceptor.clone();
                tokio::spawn(async move {
                    match acceptor.accept(stream).await {
                        Ok(tls_stream) => {
                            let server_name = tls_stream.get_ref().1.server_name().map(|s|s.to_string());
                            log::info!("server name:{:?}",server_name);
                            if let Err(e) = handle_client(tls_stream, quic_server, server_name).await {
                                log::warn!("handle client error:{}", e);
                            }
                        }
                        Err(e) => {
                            log::warn!("TLS handshake error: {}", e);
                        }
                    }
                });
            }
            Err(e) => {
                log::warn!("accept error:{}", e);
            }
        }
    }
}

pub async fn handle_client<T: AsyncRead + AsyncWrite + Unpin>(stream: T, quic_server: QuicServer,http_proxy_host: Option<String>) -> Result<(), Box<dyn std::error::Error+Send+Sync>> {
    let mut stream = PeekableStream::new(stream);
    let header = stream.peek_header().await?;
    let host_header = http_proxy_host.unwrap_or(header.get("Host").ok_or("not found Host header")?.to_string());
    let mac:Mac = host_header.split('.').next().ok_or("host error")?.try_into()?;
    quic_server.translate(mac,stream).await?;
    Ok(())
}

pub fn extract_full_pem_certificates(pem_content: &str) -> Vec<String> {
    let mut certificates = Vec::new();
    let mut current_cert = String::new();
    let mut in_certificate = false;    
    for line in pem_content.lines() {
        if line.starts_with("-----BEGIN CERTIFICATE-----") {
            in_certificate = true;
            current_cert.clear();
            current_cert.push_str(line);
            current_cert.push('\n');
            continue;
        }        
        if line.starts_with("-----END CERTIFICATE-----") {
            if in_certificate {
                current_cert.push_str(line);
                certificates.push(current_cert.clone());
            }
            in_certificate = false;
            continue;
        }        
        if in_certificate {
            current_cert.push_str(line);
            current_cert.push('\n');
        }
    }    
    certificates
}