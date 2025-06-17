use std::{
    collections::HashMap, error::Error, net::{IpAddr, Ipv4Addr, SocketAddr}, sync::Arc, time::Duration
};
use rustls::pki_types::pem::PemObject;
use rweb_common::{mac::Mac, RwebError};
use quinn::{Connection, Endpoint, Incoming, ServerConfig, VarInt};
use tokio::{io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt}, sync::RwLock, time::timeout};
use rweb_common::key::{CER_BIN, KEY_BIN};
#[cfg(feature="p2p")]
use tokio::select;
#[cfg(feature="p2p")]
use quinn::{RecvStream, SendStream};
#[cfg(feature="p2p")]
use rweb_common::{get_header,Header};

const KEEPALIVE_INTERVAL_MILLIS:u64=10_000;
const IDLE_TIMEOUT_MILLIS:u32=21_000;

#[derive(Debug,Clone,Default)]
pub struct QuicServer{
    peers:Arc<RwLock<HashMap<Mac,Connection>>>
}

impl QuicServer{
    
    pub async fn start(&self,port:u16)->Result<(),Box<dyn Error>>{
        let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);
        let endpoint = make_server_udp_endpoint(bind_addr,CER_BIN.as_bytes(),KEY_BIN.as_bytes())?;
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
                let mut quic_stream = rweb_common::io::stream_copy::Stream::new(stream,conn.remote_address());
                drop(peers);
                quic_stream.write(mac.as_ref()).await?;//先告诉节点自己要连接的mac地址
                tokio::io::copy_bidirectional(&mut tcp_stream, &mut quic_stream).await?;
                Ok(())
            }else{
                drop(peers);
                let body = "设备未连接";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                tcp_stream.write_all(response.as_bytes()).await?;
                Err(RwebError::new(502,"设备连接无法使用".to_string()).into())
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
            Err(RwebError::new(402,"设备未连接".to_string()).into())
        }
    }    
}

//接收服务端发来的p2p连接请求
#[cfg(feature="p2p")]
async fn handle_bi(connection:Connection,peers:Arc<RwLock<HashMap<Mac,Connection>>>,self_mac:Mac)->Result<(),Box<dyn Error+Send+Sync>>{
    log::info!("handle_bi mac:{}",self_mac);
    loop{
        let bi_stream = connection.accept_bi().await?;
        let connection = connection.clone();
        let peers = peers.clone();
        tokio::spawn(async move{
            let res = handle_bi_cell(connection, bi_stream, peers, self_mac).await;
            log::info!("handle_bi_cell error:{:?}",res);
        });
    }
}

//解析p2p请求,获取对端地址，尝试与对端打洞
#[cfg(feature="p2p")]
async fn handle_bi_cell(connection:Connection,(mut bi_send, mut bi_recv):(SendStream,RecvStream),peers:Arc<RwLock<HashMap<Mac,Connection>>>,self_mac:Mac)->Result<(),Box<dyn Error+Send+Sync>>{
    let header = get_header(&mut bi_recv).await?;
    log::info!("header: {:?}", header);
    let (mac,_,req_self_addr) = header.parse_p2p()?;
    log::info!("handle_p2p_request client mac:{},node mac:{}",self_mac,mac);
    let peers_ = peers.read().await;
    if let Some(peer) = peers_.get(&mac){
        let (mut sendstream,mut recvstream) = peer.open_bi().await?;//打开对端bi流
        let mut bi2_header = Header::new_p2p(self_mac, req_self_addr.unwrap_or(connection.remote_address()),Some(peer.remote_address())); //使用节点自测地址利于预测端口
        if let Some(req_self_addr) = req_self_addr{
            if req_self_addr != connection.remote_address(){
                log::warn!("请求方处于受限锥形NAT网络,第三方测地址{} != 服务器测得地址{}",req_self_addr,connection.remote_address());
                bi2_header.header.insert("Nat-Type".to_string(),"Symmetric".to_string());
            }else{
                log::warn!("请求方处于全锥形NAT网络,第三方测地址{} == 服务器测得地址{}",req_self_addr,connection.remote_address());
                bi2_header.header.insert("Nat-Type".to_string(),"FullCone".to_string());
            }
        }
        let mut bi2_header_vec:Vec<u8> = bi2_header.into();//构造向对端bi流发送p2p请求包
        bi2_header_vec.splice(0..0,mac.as_ref().iter().cloned());//在头部插入mac地址，所有主动向节点发送的bi流的第一个数据包都需要首先发送mac地址以便node得知使用哪条流来对接。
        if let Ok(_) = sendstream.write_all(&bi2_header_vec).await{//向对端发送p2p请求包
            let header = get_header(&mut recvstream).await?;//等待对端回复p2p请求
            let (_,_,resp_self_addr) = header.parse_p2p()?;
            //log::info!("tell {} connect {} success",mac,connection.remote_address());
            let mut header = Header::new_p2p(mac, resp_self_addr.unwrap_or(peer.remote_address()),Some(connection.remote_address()));//使用节点自测地址利于预测端口
            if let Some(resp_self_addr) = resp_self_addr{
                if resp_self_addr != peer.remote_address(){
                    log::warn!("被请求方处于受限锥形NAT网络,第三方测地址{} != 服务器测得地址{}",resp_self_addr,peer.remote_address());
                    header.header.insert("Nat-Type".to_string(),"Symmetric".to_string());
                }else{
                    header.header.insert("Nat-Type".to_string(),"FullCone".to_string());
                }
            }
            let bi_header_vec:Vec<u8> = header.into();
            //bi_header_vec.splice(0..0,mac.as_ref().iter().cloned());//在头部插入mac地址，所有主动向节点发送的bi流的第一个数据包都需要首先发送mac地址以便node得知使用哪条流来对接。
            bi_send.write_all(&bi_header_vec).await?;
            log::info!("tell {} connect {} success ",self_mac,peer.remote_address());
        }

    }else{
        bi_send.write_all(b"HTTP/1.1 404 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: 0\r\n\r\n").await?;
    }
    Ok(())
}

async fn handle_incomming(incoming:Incoming,peers:Arc<RwLock<HashMap<Mac,Connection>>>)->Result<(),Box<dyn Error+Send+Sync>>{
    let conn = incoming.await?;
    let mut uni = conn.accept_uni().await?;
    let mac_list_len = timeout(Duration::from_secs(5), uni.read_u16()).await??;
    let mut mac_list:Vec<Mac> =  Vec::with_capacity(mac_list_len as usize);
    for _ in 0..mac_list_len{
        let mut buf = [0x00;6];
        timeout(Duration::from_secs(5), uni.read_exact(&mut buf)).await??;
        mac_list.push(buf.into());
    }
    let mut peers_s = peers.write().await;
    for mac in mac_list.iter(){
        if let Some(_) = peers_s.get(mac){
            log::warn!("node_mac already online:{}",mac);
            conn.close(VarInt::from_u32(401), "node_mac already online".as_bytes().into());
            return Err(RwebError::new(10402, "node_mac already online").into());
        }
    }
    for mac in mac_list.iter(){
        peers_s.insert(mac.clone(), conn.clone());
    }
    drop(peers_s);
    log::info!("node_mac online:{}",mac_list.iter().map(|m|m.to_string()).collect::<Vec<String>>().join(","));
    #[cfg(feature="p2p")]
    select! {
        _ = handle_bi(connection.clone(), peers_bi.clone(), mac_list[0])=>{},//p2p连接？
        _ = conn.closed()=>{}
    }    
    #[cfg(not(feature="p2p"))]
    conn.closed().await;
    log::info!("node_mac offline:{}",mac_list.iter().map(|m|m.to_string()).collect::<Vec<String>>().join(","));
    let mut peers_s = peers.write().await;
    for mac in mac_list.iter(){
        peers_s.remove(mac);
    }
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