use std::{
    collections::HashMap, error::Error, net::{IpAddr, Ipv4Addr, SocketAddr}, sync::Arc, time::Duration
};
use rustls::pki_types::pem::PemObject;
use common::mac::Mac;
use quinn::{Connection, Endpoint, Incoming, RecvStream, SendStream, ServerConfig};
use tokio::{io::{AsyncRead, AsyncWrite, AsyncWriteExt}, sync::RwLock, time::timeout};

const CER_BIN:&[u8] = include_bytes!("../../reform.cer");
const KEY_BIN:&[u8] = include_bytes!("../../reform.key");
const KEEPALIVE_INTERVAL_MILLIS:u64=3000;
const IDLE_TIMEOUT_MILLIS:u32=10_000;

#[derive(Debug,Clone,Default)]
pub struct QuicServer{
    peers:Arc<RwLock<HashMap<Mac,Connection>>>
}

impl QuicServer{
    
    pub async fn start(&self,port:u16)->Result<(),Box<dyn Error>>{
        let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);
        let endpoint = make_server_udp_endpoint(bind_addr,CER_BIN,KEY_BIN)?;
        log::info!("quic server listen on {}",bind_addr);
        loop{
            match endpoint.accept().await{
                Some(conn)=>{
                    let peers = self.peers.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_incomming(conn,peers).await{
                            log::warn!("handle incomming error:{}",e);
                        }
                    });
                },
                None=>{
                    log::warn!("endpoint accept none");
                }
            }
        }
    }

    pub async fn translate<T:AsyncRead+AsyncWrite+Unpin>(&self,mac:Mac,mut tcp_stream:T,header_bytes:Vec<u8>)->Result<(),Box<dyn Error+Send+Sync>>{
        let peers = self.peers.read().await;
        if let Some(conn) = peers.get(&mac){
            if let Ok(stream) = conn.open_bi().await{
                drop(peers);
                translate(tcp_stream,stream,header_bytes).await?;
            }else{
                drop(peers);
                let body = "设备未连接";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                tcp_stream.write_all(response.as_bytes()).await?;
            }
        }else{
            drop(peers);
            let body = "设备未连接";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            tcp_stream.write_all(response.as_bytes()).await?;
        }
        Ok(())
    }
}

async fn translate<T:AsyncRead+AsyncWrite+Unpin>(tcp_stream:T,(mut bi_send,mut bi_recv):(SendStream,RecvStream),header_bytes:Vec<u8>)->Result<(),Box<dyn Error+Send+Sync>>{
    bi_send.write_all(&header_bytes).await?;
    let (mut tcp_recv,mut tcp_send) = tokio::io::split(tcp_stream);
    tokio::select! {
        a = tokio::io::copy(&mut tcp_recv, &mut bi_send) => Ok(a.map(|_|())?),
        b = tokio::io::copy(&mut bi_recv, &mut tcp_send) => Ok(b.map(|_|())?),
    }
}

async fn handle_incomming(incoming:Incoming,peers:Arc<RwLock<HashMap<Mac,Connection>>>)->Result<(),Box<dyn Error+Send+Sync>>{
    let conn = incoming.await?;
    let mut uni = conn.accept_uni().await?;
    let mut buf = [0x00;6];
    timeout(Duration::from_secs(5), uni.read_exact(&mut buf)).await??;
    let mut peers_s = peers.write().await;
    if let Some(_) = peers_s.get(&buf.into()){
        log::warn!("node_mac already online:{}",Mac::from(buf));
        return Ok(());
    }
    peers_s.insert(buf.into(), conn.clone());
    drop(peers_s);
    log::info!("node_mac online:{}",Mac::from(buf));
    uni_stream(&mut uni).await?;
    let mut peers_s = peers.write().await;
    peers_s.remove(&buf.into());
    drop(peers_s);
    log::info!("node_mac offline:{}",Mac::from(buf));
    Ok(())
}

async fn uni_stream(stream:&mut quinn::RecvStream)->Result<(),Box<dyn Error+Send+Sync>>{
    let mut buf = [0x00; 6];
    loop{
        timeout(Duration::from_millis(IDLE_TIMEOUT_MILLIS as u64),stream.read_exact(&mut buf)).await.unwrap().unwrap();
    }
}

pub fn make_server_udp_endpoint(addr:SocketAddr, cert_der:&[u8], priv_key:&[u8]) -> Result<Endpoint, Box<dyn Error>> {
    Ok(Endpoint::server( configure_host_server(cert_der,priv_key)?, addr)?)
}

fn configure_host_server(cert_der:&[u8],priv_key:&[u8]) -> Result<ServerConfig, Box<dyn Error>> {
    let priv_key = rustls::pki_types::PrivateKeyDer::from_pem_slice(priv_key)?;//  ::from_pem_file(priv_key)?;
    let cert_chain = vec![rustls::pki_types::CertificateDer::from_pem_slice(cert_der)?];//from_pem_file(cert_der)?];
    let mut server_config = ServerConfig::with_single_cert(cert_chain, priv_key)?;
    Arc::get_mut(&mut server_config.transport).ok_or("none mutable")?
        .keep_alive_interval(Some(std::time::Duration::from_millis(KEEPALIVE_INTERVAL_MILLIS)))
        .max_idle_timeout(Some(quinn::IdleTimeout::from(quinn::VarInt::from_u32(IDLE_TIMEOUT_MILLIS))))
        .max_concurrent_bidi_streams(10000_u16.into())
        .max_concurrent_uni_streams(10000_u16.into());
    Ok(server_config)
}