use std::{error::Error, net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs}, sync::Arc};
use quinn::{ClientConfig, Connection, Endpoint, RecvStream, SendStream, TransportConfig};
use rustls::pki_types::{pem::PemObject, CertificateDer, UnixTime};
use tokio::{io::AsyncWriteExt, net::TcpStream, select};
use common::{mac::Mac,get_header,Header};
use url::Url;
use tokio_rustls::TlsConnector;
use rustls::{client::danger::{ServerCertVerified,ServerCertVerifier},pki_types::ServerName};

const KEEPALIVE_INTERVAL_MILLIS:u64=3_000;
const IDLE_TIMEOUT_MILLIS:u32=10_000;
const CER_BIN:&[u8] = include_bytes!("../../reform.cer");

static CONNECTOR:once_cell::sync::Lazy<TlsConnector> = once_cell::sync::Lazy::new(|| {
    let provider = rustls::crypto::ring::default_provider();
    rustls::crypto::CryptoProvider::install_default(provider).expect("failed to install crypto provider");
    let mut config = rustls::ClientConfig::builder()
        .with_root_certificates(rustls::RootCertStore::empty())
        .with_no_client_auth(); 
    config.dangerous().set_certificate_verifier(Arc::new(NoVerify));
    TlsConnector::from(Arc::new(config))
});

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

pub async fn run(server_host:&str,server_port:u16,proxy_addr:Arc<Url>,mac:Mac)->Result<(),Box<dyn Error+Send+Sync>>{
    let server_addr = (server_host, server_port).to_socket_addrs()?.next().ok_or("can't resolve")?;
    let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);
    let client = make_client_endpoint(bind_addr)?;
    let conn = client.connect(server_addr, "reform")?;
    let connection = conn.await?;
    let uni_stream = connection.open_uni().await?;
    select! {
        r = handle_uni(uni_stream,mac) => r,
        r = listen_bi(connection, proxy_addr) => r,
    }
}

async fn listen_bi(connection:Connection,proxy_addr:Arc<Url>)->Result<(),Box<dyn Error+Send+Sync>>{
    loop{
        match connection.accept_bi().await{
            Ok(bi_stream) => {
                let proxy_addr = proxy_addr.clone();
                tokio::spawn(async move {
                    handle_bi(bi_stream,proxy_addr).await;
                });
            },
            Err(e) => {
                return Err(e.into())
            }
        }
    }
}

async fn handle_uni(mut send_stream:SendStream,mac:Mac)->Result<(),Box<dyn Error+Send+Sync>>{
    loop{
        send_stream.write_all(mac.as_ref()).await?;
        tokio::time::sleep(tokio::time::Duration::from_millis(KEEPALIVE_INTERVAL_MILLIS)).await;
    }
}

async fn handle_bi((send_stream, mut recv_stream):(SendStream,RecvStream),proxy_addr:Arc<Url>){
    if let Ok(header) = get_header(&mut recv_stream).await{    
        match header.method.as_str(){
            "CONNECT"=>{
                if let Err(_e) = proxy_translate((send_stream,recv_stream),&header.uri).await{
                    //println!("proxy translate error:{}",_e);
                };
            },
            _=>{
                if let Err(_e) = http_translate((send_stream,recv_stream),proxy_addr,header).await{
                    //println!("http translate error:{}",_e);
                };
            }
        }
    }
}

async fn http_translate((mut send_stream, mut recv_stream):(SendStream,RecvStream),proxy_addr:Arc<Url>,header:Header)->Result<(),Box<dyn Error+Send+Sync>>{
    let host = proxy_addr.host_str().ok_or("proxy_addr have no host")?.to_string();
    //header.set("Host".to_string(), host.clone());
    match proxy_addr.scheme(){
        "http" => {
            let addr = if host.contains(":"){host.clone()}else{host+":80"};
            let tcp_stream = TcpStream::connect(addr).await?;
            let (mut tcp_read, mut tcp_write) = tokio::io::split(tcp_stream);
            tcp_write.write_all(&Into::<Vec<u8>>::into(header)).await?;
            select! {
                _ = tokio::io::copy(&mut recv_stream, &mut tcp_write) => Ok(()),
                _ = tokio::io::copy(&mut tcp_read, &mut send_stream) => Ok(()),
            }
        },
        "https" => {
            let addr = if host.contains(":"){host.clone()}else{host.clone()+":443"};
            let tcp_stream = TcpStream::connect(addr).await?;
            let tls_stream = CONNECTOR.connect(ServerName::try_from(host.split(":").next().ok_or("proxy_addr have not host")?.to_string())?, tcp_stream).await?;
            let (mut tls_read, mut tls_write) = tokio::io::split(tls_stream);
            tls_write.write_all(&Into::<Vec<u8>>::into(header)).await?;
            select! {
                _ = tokio::io::copy(&mut recv_stream, &mut tls_write) => Ok(()),
                _ = tokio::io::copy(&mut tls_read, &mut send_stream) => Ok(()),
            }
        },
        _ => return Err("not support scheme".into())
    }
}

async fn proxy_translate((mut send_stream, mut recv_stream):(SendStream,RecvStream),proxy_addr:&str)->Result<(),Box<dyn Error+Send+Sync>>{
    let tcp_stream = TcpStream::connect(proxy_addr).await?;
    let (mut tcp_read, mut tcp_write) = tokio::io::split(tcp_stream);
    send_stream.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await?;
    select! {
        _ = tokio::io::copy(&mut recv_stream, &mut tcp_write) => Ok(()),
        _ = tokio::io::copy(&mut tcp_read, &mut send_stream) => Ok(()),
    }
}

#[derive(Debug)]
struct NoVerify;

impl ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer,
        _intermediates: &[CertificateDer],
        _server_name: &ServerName,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        // 永远返回“验证通过”
        Ok(ServerCertVerified::assertion())
    }
    
    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }
    
    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }
    
    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![0x0201,0x0203,0x0401,0x0403,0x0501,0x0503,0x0601,0x0603,0x0804,0x0805,0x0806,0x0807,0x0808].into_iter().map(|s| s.into()).collect()
    }
}