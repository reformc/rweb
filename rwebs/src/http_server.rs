use rweb_common::{io::peek_stream::PeekableStream, mac::Mac};
use tokio::{io::{AsyncRead, AsyncWrite, AsyncWriteExt}, net::{TcpListener, TcpStream}};
use crate::quic_server::QuicServer;
use rustls::{pki_types::pem::PemObject, ServerConfig};
use tokio_rustls::TlsAcceptor;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
enum Scheme {
    TCP,
    TLS,
}

pub async fn run_https(port:u16,quic_server:QuicServer,priv_key:&str,cert_der:&str) -> Result<(), Box<dyn std::error::Error+Send+Sync>> {
    let priv_key = rustls::pki_types::PrivateKeyDer::from_pem_slice(priv_key.as_bytes())?;
    let cert_chain = extract_full_pem_certificates(cert_der).into_iter().filter_map(|s|rustls::pki_types::CertificateDer::from_pem_slice(s.as_bytes()).ok()).collect::<Vec<_>>();
    let config = ServerConfig::builder()
    .with_no_client_auth()
    .with_single_cert(cert_chain, priv_key)?;
    let tls_config = Arc::new(config);
    let listener = TcpListener::bind(format!("0.0.0.0:{}",port)).await?;
    log::info!("https_server listen on {}",listener.local_addr()?);
    loop{
        match listener.accept().await {
            Ok((stream, addr)) => {
                log::debug!("accept from {}", addr);
                let quic_server = quic_server.clone();
                let server_config = tls_config.clone();
                tokio::spawn(async move {
                    handle_stream(stream,quic_server,server_config).await.unwrap_or_else(|e| {
                        log::warn!("handle client error:{}", e);
                    });
                });
            }
            Err(e) => {
                log::warn!("accept error:{}", e);
            }
        }
    }
}

pub async fn handle_stream(stream: TcpStream, quic_server: QuicServer,tls_config:Arc<ServerConfig>) -> Result<(), Box<dyn std::error::Error+Send+Sync>> {
    let mut first_byte = [0x00;1];
    stream.peek(&mut first_byte).await?;
    match first_byte[0] {//https连接
        0x16 => {
            let acceptor = TlsAcceptor::from(tls_config);
            match acceptor.accept(stream).await {
                Ok(tls_stream) => {
                    let server_name = tls_stream.get_ref().1.server_name().map(|s|s.to_string());
                    log::info!("server name:{:?}",server_name);
                    if let Err(e) = handle_client(PeekableStream::new(tls_stream), quic_server, server_name, Scheme::TLS).await {
                        log::debug!("tls handle client error:{}", e);
                    }
                }
                Err(e) => {
                    log::warn!("TLS handshake error: {}", e);
                }
            }
        }
        _ => {
            if let Err(e) = handle_client(stream, quic_server, None, Scheme::TCP).await {
                log::debug!("tcp handle client error:{}", e);
            }
        }
    }
    Ok(())
}

async fn handle_client<T: AsyncRead + AsyncWrite + Unpin>(stream: T, quic_server: QuicServer,http_proxy_host: Option<String>,schme:Scheme) -> Result<(), Box<dyn std::error::Error+Send+Sync>> {
    let mut stream = PeekableStream::new(stream);
    let header = stream.peek_header().await?;
    log::info!("header: {:?}", header);
    if header.method.as_str() == "OPTIONS" && header.version.as_str() == "RTSP/1.0" {//代理rtsp协议，仅支持tcp和端口复用的rtsp，也就是支持NAT的rtsp
        let url = url::Url::parse(&header.uri).map_err(|e| format!("url parse error:{}", e))?;
        let host = url.host_str().ok_or("host error")?.to_string();
        let mac:Mac = host.split('.').next().ok_or("host error")?.try_into()?;
        if let Err(e) = quic_server.translate(mac,stream).await{
            log::warn!("translate error:{}", e);
            return Err("rweb http_proxy not support rtsp, you can use tcp".into());
        }
        return Ok(());
    }
    if header.method == "CONNECT" && schme == Scheme::TCP {//http_proxy仅支持https地址
        stream.write_all("HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\n".as_bytes()).await?;
        return Err("rweb http_proxy not support http, you can use https".into());
    }
    let host_header = http_proxy_host.unwrap_or(header.get("Host").ok_or("not found Host header")?.to_string());
    let mac:Mac = host_header.split('.').next().ok_or("host error")?.try_into()?;
    log::info!("method: {}, version: {}, mac: {}", header.method, header.version, mac);
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