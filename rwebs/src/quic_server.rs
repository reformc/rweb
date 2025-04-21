use std::{
    collections::HashMap, error::Error, net::{IpAddr, Ipv4Addr, SocketAddr}, sync::Arc, time::Duration
};
use rustls::pki_types::pem::PemObject;
use common::mac::Mac;
use quinn::{Connection, Endpoint, Incoming, ServerConfig, VarInt};
use tokio::{io::{AsyncRead, AsyncWrite, AsyncWriteExt}, sync::RwLock, time::timeout};

const CER_BIN:&[u8] = include_bytes!("../../reform.cer");
const KEY_BIN:&[u8] = include_bytes!("../../reform.key");
const KEEPALIVE_INTERVAL_MILLIS:u64=10_000;
const IDLE_TIMEOUT_MILLIS:u32=21_000;

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
                        if let Err(_e) = handle_incomming(conn,peers).await{
                            //println!("handle incomming error:{}",_e);
                        }
                    });
                },
                None=>{
                    //println!("endpoint accept none");
                }
            }
        }
    }

    pub async fn translate<T:AsyncRead+AsyncWrite+Unpin>(&self,mac:Mac,mut tcp_stream:T)->Result<(),Box<dyn Error+Send+Sync>>{
        let peers = self.peers.read().await;
        if let Some(conn) = peers.get(&mac){
            if let Ok(stream) = conn.open_bi().await{
                drop(peers);
                let mut quic_stream = common::io::stream_copy::Stream::new(stream);
                tokio::io::copy_bidirectional(&mut tcp_stream, &mut quic_stream).await?;
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

async fn handle_incomming(incoming:Incoming,peers:Arc<RwLock<HashMap<Mac,Connection>>>)->Result<(),Box<dyn Error+Send+Sync>>{
    let conn = incoming.await?;
    let mut uni = conn.accept_uni().await?;
    let mut buf = [0x00;6];
    timeout(Duration::from_secs(5), uni.read_exact(&mut buf)).await??;
    uni.stop(VarInt::from_u32(0)).unwrap_or_default();
    drop(uni);
    let mut peers_s = peers.write().await;
    if let Some(_) = peers_s.get(&buf.into()){
        log::warn!("node_mac already online:{}",Mac::from(buf));
        conn.close(VarInt::from_u32(401), "node_mac already online".as_bytes().into());
        return Ok(());
    }
    let mac = Mac::from(buf);
    peers_s.insert(mac, conn.clone());
    drop(peers_s);
    //println!("{} online",mac);
    log::info!("{} online",mac);
    let close = conn.closed().await;
    //println!("{} closed,{}",mac,close);
    let mut peers_s = peers.write().await;
    log::info!("{} closed,{}",mac,close);
    peers_s.remove(&mac);
    drop(peers_s);
    Ok(())
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
        .max_idle_timeout(Some(quinn::IdleTimeout::from(VarInt::from_u32(IDLE_TIMEOUT_MILLIS))))
        .max_concurrent_bidi_streams(10000_u16.into())
        .max_concurrent_uni_streams(10000_u16.into());
    Ok(server_config)
}