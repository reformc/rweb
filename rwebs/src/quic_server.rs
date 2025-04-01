use std::{
    collections::HashMap, error::Error, net::{IpAddr, Ipv4Addr, SocketAddr}, sync::Arc
};
use rustls::pki_types::pem::PemObject;
use common::mac::Mac;
use quinn::{Connection, Endpoint, Incoming, RecvStream, SendStream, ServerConfig};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpStream, sync::RwLock};

const CER_BIN:&[u8] = include_bytes!("E:/reform.cer");
const KEY_BIN:&[u8] = include_bytes!("E:/reform.key");
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
        while let Some(conn) = endpoint.accept().await{
            let peers = self.peers.clone();
            tokio::spawn(async move {
                handle_incomming(conn,peers).await;
            });
        }
        Ok(())
    }

    pub async fn translate(&self,mac:Mac,mut tcp_stream:TcpStream,header_bytes:Vec<u8>)->Result<(),Box<dyn Error+Send+Sync>>{
        let peers = self.peers.read().await;
        if let Some(conn) = peers.get(&mac){
            if let Ok(stream) = conn.open_bi().await{
                drop(peers);
                translate(tcp_stream,stream,header_bytes).await;
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

async fn translate(tcp_stream:TcpStream,(mut bi_send,mut bi_recv):(SendStream,RecvStream),header_bytes:Vec<u8>){
    let (mut tcp_recv,mut tcp_send) = tcp_stream.into_split();
    let a = tokio::spawn(async move{
        let mut buf = [0u8; 256*256];
        if let Ok(_) = bi_send.write_all(&header_bytes).await{//header头写入
            loop{
                if let Ok(len) = tcp_recv.read(&mut buf).await{
                    if let Err(e) = bi_send.write_all(&buf[..len]).await{
                        println!("tcp send error:{}",e);
                        break;
                    }
                }else{
                    break
                }
            }
        }
    });
    let b = tokio::spawn(async move{
        let mut buf = [0u8; 256*256];
        loop{
            if let Ok(Some(len)) = bi_recv.read(&mut buf).await{
                if let Err(e) = tcp_send.write_all(&buf[..len]).await{
                    println!("quic send error:{}",e);
                    break;
                }
            }else{
                break
            }
        }
    });
    tokio::select! {
        _ = a => {},
        _ = b => {},
    }
}

async fn handle_incomming(incoming:Incoming,peers:Arc<RwLock<HashMap<Mac,Connection>>>){
    if let Ok(conn) = incoming.await{
        if let Ok(mut uni) = conn.accept_uni().await{
            let mut buf = [0x00;6];
            if let Ok(_) = uni.read_exact(&mut buf).await{
                let mut peers_s = peers.write().await;
                peers_s.insert(buf.into(), conn.clone());
                drop(peers_s);
                println!("node_mac online:{}",Mac::from(buf));
                uni_stream(&mut uni).await;
                let mut peers_s = peers.write().await;
                peers_s.remove(&buf.into());
                drop(peers_s);
                println!("node_mac offline:{}",Mac::from(buf));
            }
        }
    }
}

async fn uni_stream(stream:&mut quinn::RecvStream){
    let mut buf = [0x00; 6];
    loop{
        if let Ok(_) = stream.read_exact(&mut buf).await{
            //println!("node_mac:{}",Mac::from(buf));
        }else{
            break
        }
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
        .max_concurrent_bidi_streams(100_u8.into())
        .max_concurrent_uni_streams(100_u8.into());
    Ok(server_config)
}