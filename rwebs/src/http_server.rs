use common::{mac::Mac,get_header};
use tokio::{io::{AsyncRead, AsyncWrite}, net::TcpListener};
use crate::quic_server::QuicServer;
use rustls::{pki_types::pem::PemObject, ServerConfig};
use tokio_rustls::TlsAcceptor;

// use std::{pin::Pin, task::{Context, Poll}};
// use tokio::io::{AsyncBufReadExt,ReadBuf};

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
                    //let mut stream = PeekableReader::new(stream);
                    //let mut buf = [0; 1];
                    // match stream.peek(&mut buf).await {
                    //     Ok(0) => {
                    //         println!("client send not 0x16, close connection");
                    //         log::warn!("client closed connection");
                    //     }
                    //     Ok(_) => {
                    //         if buf[0] != 0x16 {
                    //             println!("client send not 0x16, close connection");
                    //             log::warn!("client send 0x00, close connection");
                    //         }else{
                    //             println!("client send 0x16, start tls handshake");
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
                            //}
                    //     }
                    //     Err(e) => {
                    //         log::warn!("peek error: {}", e);
                    //     }
                    // }
                });
            }
            Err(e) => {
                log::warn!("accept error:{}", e);
            }
        }
    }
}

pub async fn handle_client<T:AsyncRead+AsyncWrite+Unpin>(mut stream: T,quic_server:QuicServer,http_proxy_host:Option<String>) -> Result<(), Box<dyn std::error::Error+Send+Sync>> {
    let header = get_header(&mut stream).await?;
    log::debug!("header:{:?}",header);
    let host_header = http_proxy_host.unwrap_or(header.get("Host").ok_or("not found Host header")?.to_string());
    let mac:Mac = host_header.split('.').next().ok_or("host error")?.try_into()?;
    quic_server.translate(mac,stream,header.into()).await?;
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

// struct PeekableReader<T:AsyncRead + AsyncWrite + Unpin> {
//     inner: T,
//     peek_buf: Vec<u8>,
// }

// impl<T: AsyncRead+AsyncWrite+Unpin> PeekableReader<T> {
//     pub fn new(inner: T) -> Self {
//         Self {
//             inner,
//             peek_buf: Vec::new(),
//         }
//     }
    
//     pub async fn peek(&mut self, buf: &mut [u8]) -> tokio::io::Result<usize> {
//         if self.peek_buf.is_empty() {
//             let mut temp = [0; 1];
//             let n = self.inner.read(&mut temp).await?;
//             if n > 0 {
//                 self.peek_buf.extend_from_slice(&temp[..n]);
//             }
//         }
        
//         let len = std::cmp::min(buf.len(), self.peek_buf.len());
//         buf[..len].copy_from_slice(&self.peek_buf[..len]);
//         Ok(len)
//     }
// }

// impl <T: AsyncRead + AsyncWrite + Unpin>AsyncRead for PeekableReader<T> {
//     fn poll_read(
//         self: Pin<&mut Self>,
//         cx: &mut Context<'_>,
//         buf: &mut ReadBuf<'_>,
//     ) -> std::task::Poll<std::io::Result<()>> {
//         //Pin::new(&mut self.get_mut().inner).poll_read(cx, buf);
//         let this = self.get_mut();
//         if !this.peek_buf.is_empty() {
//             let len = std::cmp::min(buf.remaining(), this.peek_buf.len());
//             buf.put_slice(&this.peek_buf[..len]);
//             this.peek_buf.drain(..len);
//             return Poll::Ready(Ok(()));
//         }
//         Pin::new(&mut this.inner).poll_read(cx, buf)
//     }
// }

// impl <T: AsyncRead + AsyncWrite + Unpin>AsyncWrite for PeekableReader<T> {
//     fn poll_write(
//         self: Pin<&mut Self>,
//         cx: &mut Context<'_>,
//         buf: &[u8],
//     ) -> std::task::Poll<std::io::Result<usize>> {
//         Pin::new(&mut self.get_mut().inner).poll_write(cx, buf)
//     }

//     fn poll_flush(
//         self: Pin<&mut Self>,
//         cx: &mut Context<'_>,
//     ) -> std::task::Poll<Result<(), std::io::Error>> {
//         let inner = unsafe { self.map_unchecked_mut(|s| &mut s.inner) };
//         inner.poll_flush(cx)
//     }

//     fn poll_shutdown(
//         self: Pin<&mut Self>,
//         cx: &mut Context<'_>,
//     ) -> std::task::Poll<Result<(), std::io::Error>> {
//         let inner = unsafe { self.map_unchecked_mut(|s| &mut s.inner) };
//         inner.poll_shutdown(cx)
//     }
// }