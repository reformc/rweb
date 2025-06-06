use std::{
    error::Error,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc
};
use rustls::pki_types::pem::PemObject;
use quinn::{ClientConfig, Endpoint, Incoming, ServerConfig, TransportConfig, VarInt};
use rweb_common::io::header::write_addr;

const CER_BIN:&[u8] = include_bytes!("../../reform.cer");
const KEY_BIN:&[u8] = include_bytes!("../../reform.key");
const KEEPALIVE_INTERVAL_MILLIS:u64=10_000;
const IDLE_TIMEOUT_MILLIS:u32=21_000;

fn make_server_udp_endpoint(addr:SocketAddr, cert_der:&[u8], priv_key:&[u8]) -> Result<Endpoint, Box<dyn Error>> {
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

fn configure_host_client(cert_der:&[u8]) -> ClientConfig {
    let mut certs = rustls::RootCertStore::empty();
    certs.add(rustls::pki_types::CertificateDer::from_pem_slice(cert_der).unwrap()).unwrap();
    let mut client_config = ClientConfig::with_root_certificates(Arc::new(certs)).unwrap();
    let mut transport_config = TransportConfig::default();
    transport_config
        //.keep_alive_interval(Some(std::time::Duration::from_millis(KEEPALIVE_INTERVAL_MILLIS)))
        .max_idle_timeout(Some(quinn::IdleTimeout::from(quinn::VarInt::from_u32(10))))
        .max_concurrent_bidi_streams(10000_u16.into())
        .max_concurrent_uni_streams(1000_u16.into());
    client_config.transport_config(std::sync::Arc::new(transport_config));
    client_config
}

pub async fn run(port:u16){
    let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);
    let mut endpoint = make_server_udp_endpoint(bind_addr,CER_BIN,KEY_BIN).unwrap();
    endpoint.set_default_client_config(configure_host_client(CER_BIN));
    li(endpoint).await;
}
    
async fn li(endpoint:Endpoint){
    loop{
        let inc = endpoint.accept().await;
        match inc{
            Some(conn)=>{
                tokio::spawn(handle_incomming(conn));
            },
            None=>{
            }
        }
    }
}

async fn handle_incomming(inc:Incoming)->Result<(),Box<dyn Error+Send+Sync>>{
    println!("accept from {}", inc.remote_address());
    let conn = inc.await?;
    let mut uni = conn.open_uni().await?;
    let addr = conn.remote_address();
    write_addr(&mut uni,addr).await?;
    uni.stopped().await.unwrap_or_default();//等待对方接收完数据关闭流，如果不等待的话，这里会直接关闭流，导致对方无法接收数据。
    Ok(())
}


#[cfg(test)]
mod tests{
    use std::{error::Error, net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs}, sync::Arc};
    use rweb_common::io::header::read_addr;
    use quinn::{ClientConfig, Endpoint, ServerConfig, TransportConfig, VarInt};
    use rustls::pki_types::pem::PemObject;

    use crate::quic_server::{CER_BIN, KEY_BIN, KEEPALIVE_INTERVAL_MILLIS, IDLE_TIMEOUT_MILLIS};

    #[tokio::test]
    async fn tt(){
        let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);
        let mut endpoint = make_server_udp_endpoint(bind_addr,CER_BIN,KEY_BIN).unwrap();
        endpoint.set_default_client_config(configure_host_client(CER_BIN));
        let addr = "127.0.0.1:5678".to_socket_addrs().unwrap().next().unwrap();
        //let li = li(endpoint.clone());
        //tokio::spawn(li);
        println!("addr:{}",addr.to_string());
        let conn = endpoint.connect(addr, "reform").unwrap().await.unwrap();
        let mut uni = conn.accept_uni().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let addr = read_addr(&mut uni).await.unwrap();
        println!("addr:{}",addr.to_string());
        // let (mut _send_stream,mut recv_stream) = conn.open_bi().await.unwrap();
        // _send_stream.write_all(b"hello").await.unwrap();
        // let addr = read_addr(&mut recv_stream).await.unwrap();
        // println!("addr:{}",addr.to_string());
    }

    fn configure_host_client(cert_der:&[u8]) -> ClientConfig {
        let mut certs = rustls::RootCertStore::empty();
        certs.add(rustls::pki_types::CertificateDer::from_pem_slice(cert_der).unwrap()).unwrap();
        let mut client_config = ClientConfig::with_root_certificates(Arc::new(certs)).unwrap();
        let mut transport_config = TransportConfig::default();
        transport_config
            //.keep_alive_interval(Some(std::time::Duration::from_millis(KEEPALIVE_INTERVAL_MILLIS)))
            .max_idle_timeout(Some(quinn::IdleTimeout::from(quinn::VarInt::from_u32(10))))
            .max_concurrent_bidi_streams(10000_u16.into())
            .max_concurrent_uni_streams(1000_u16.into());
        client_config.transport_config(std::sync::Arc::new(transport_config));
        client_config
    }

    fn make_server_udp_endpoint(addr:SocketAddr, cert_der:&[u8], priv_key:&[u8]) -> Result<Endpoint, Box<dyn Error>> {
        Ok(Endpoint::server( configure_host_server(cert_der,priv_key)?, addr)?)
    }
    
    fn configure_host_server(cert_der:&[u8],priv_key:&[u8]) -> Result<ServerConfig, Box<dyn Error>> {
        let priv_key = rustls::pki_types::PrivateKeyDer::from_pem_slice(priv_key)?;
        let cert_chain = vec![rustls::pki_types::CertificateDer::from_pem_slice(cert_der)?];
        let mut server_config = ServerConfig::with_single_cert(cert_chain, priv_key)?;
        Arc::get_mut(&mut server_config.transport).ok_or("none mutable")?
            .keep_alive_interval(Some(std::time::Duration::from_millis(KEEPALIVE_INTERVAL_MILLIS)))
            .max_idle_timeout(Some(quinn::IdleTimeout::from(VarInt::from_u32(IDLE_TIMEOUT_MILLIS))))
            .max_concurrent_bidi_streams(10000_u16.into())
            .max_concurrent_uni_streams(10000_u16.into());
        Ok(server_config)
    }
}