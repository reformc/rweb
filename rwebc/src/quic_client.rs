use std::{error::Error, net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs}, sync::Arc};
use quinn::{ClientConfig, Endpoint, SendStream, RecvStream, TransportConfig};
use rustls::pki_types::pem::PemObject;
use tokio::{io::{self, AsyncRead, AsyncWrite, AsyncWriteExt}, net::TcpStream, select};
use common::{mac::Mac,header::Header};

const KEEPALIVE_INTERVAL_MILLIS:u64=3000;
const IDLE_TIMEOUT_MILLIS:u32=10_000;
const CER_BIN:&[u8] = include_bytes!("../../reform.cer");

fn make_client_endpoint(addr:SocketAddr) -> Result<Endpoint, Box<dyn Error+Send+Sync>> {
    let mut endpoint = Endpoint::client(addr)?;
    endpoint.set_default_client_config(configure_host_client(CER_BIN));
    Ok(endpoint)
}

fn configure_host_client(cert_der:&[u8]) -> ClientConfig {
    let mut certs = rustls::RootCertStore::empty();
    certs.add(rustls::pki_types::CertificateDer::from_pem_slice(cert_der).unwrap()).unwrap();
    let mut client_config = ClientConfig::with_root_certificates(Arc::new(certs)).unwrap();
    let mut transport_config = TransportConfig::default();
    transport_config.keep_alive_interval(Some(std::time::Duration::from_millis(KEEPALIVE_INTERVAL_MILLIS)))
        .max_idle_timeout(Some(quinn::IdleTimeout::from(quinn::VarInt::from_u32(IDLE_TIMEOUT_MILLIS))))
        .max_concurrent_bidi_streams(10000_u16.into())
        .max_concurrent_uni_streams(1000_u16.into());
    client_config.transport_config(std::sync::Arc::new(transport_config));
    client_config
}

pub async fn run(server_host:&str,server_port:u16,proxy_addr:Arc<String>,mac:Mac)->Result<(),Box<dyn Error+Send+Sync>>{
    let server_addr = (server_host, server_port).to_socket_addrs()?.next().ok_or("can't resolve")?;
    let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    let client = make_client_endpoint(bind_addr)?;
    let conn = client.connect(server_addr, "reform")?;
    let connection = conn.await?;
    let uni_stream = connection.open_uni().await?;
    let a = tokio::spawn(async move{
        handle_uni(uni_stream,mac).await;
    });
    let b = tokio::spawn(async move{
        loop{
            if let Ok(bi_stream) = connection.accept_bi().await{
                let proxy_addr = proxy_addr.clone();
                tokio::spawn(async move {
                    handle_bi(bi_stream,proxy_addr).await;
                });
            };
        }
    });
    select! {
        _ = a => {},
        _ = b => {},
    }
    Ok(())
}

async fn handle_uni(mut send_stream:SendStream,mac:Mac){
    loop{
        if let Err(e) = send_stream.write_all(mac.as_ref()).await{
            log::error!("connect fail,write error:{}",e);
            break;
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}

async fn handle_bi((send_stream, mut recv_stream):(SendStream,RecvStream),proxy_addr:Arc<String>){
    let header = get_header(&mut recv_stream).await;
    log::debug!("header:{:?}",header);
    match header.method.as_str(){
        "CONNECT"=>{
            proxy_translate((send_stream,recv_stream),&header.uri).await.unwrap_or_default();
        },
        _=>{
            tcp_translate((send_stream,recv_stream),proxy_addr,header.into()).await.unwrap_or_default();
        }
    }
}

async fn tcp_translate((send_stream, recv_stream):(SendStream,RecvStream),proxy_addr:Arc<String>,buf:Vec<u8>)->Result<(),Box<dyn Error+Send+Sync>>{
    let tcp_stream = TcpStream::connect(proxy_addr.as_ref()).await?;
    let (tcp_read, mut tcp_write) = tokio::io::split(tcp_stream);
    if !buf.is_empty(){
        tcp_write.write_all(&buf).await?;
    }
    let a = tokio::spawn(async_copy(recv_stream, tcp_write));
    let b = tokio::spawn(async_copy(tcp_read, send_stream));
    select! {
        _ = a => Ok(()),
        _ = b => Ok(()),
    }
}

async fn proxy_translate((mut send_stream, mut recv_stream):(SendStream,RecvStream),proxy_addr:&str)->Result<(),Box<dyn Error+Send+Sync>>{
    let tcp_stream = TcpStream::connect(proxy_addr).await?;
    let (mut tcp_read, mut tcp_write) = tokio::io::split(tcp_stream);
    send_stream.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await?;
    let a = tokio::spawn(async move{async_copy(&mut recv_stream, &mut tcp_write).await.unwrap_or_default()});
    let b = tokio::spawn(async move{async_copy(&mut tcp_read, &mut send_stream).await.unwrap_or_default()});
    select! {
        _ = a => Ok(()),
        _ = b => Ok(()),
    }
}

pub async fn get_header(stream:&mut RecvStream)->Header{
    let mut buf = Vec::new();
    let mut header = [0u8; 1];
    loop{
        match stream.read_exact(&mut header).await{
            Ok(_)=>{
                buf.push(header[0]);
                if buf.ends_with(b"\r\n\r\n"){
                    break
                }else{
                    if buf.len() > 256*256{
                        log::warn!("header too long");
                        break
                    }
                }
            }
            Err(e)=>{
                log::warn!("recv error:{}",e);
                break;
            }
        }
    }
    buf.into()
}

async fn async_copy<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    mut reader: R,
    mut writer: W,
) -> io::Result<u64> {
    io::copy(&mut reader, &mut writer).await
}