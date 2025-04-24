use std::{error::Error, net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs}, sync::Arc};
use quinn::{ClientConfig, Connection, Endpoint, TransportConfig};
use rustls::pki_types::{pem::PemObject, CertificateDer, UnixTime};
use tokio::{io::{AsyncRead, AsyncWrite, AsyncWriteExt}, net::TcpStream};
use url::Url;
use tokio_rustls::TlsConnector;
use rustls::{client::danger::{ServerCertVerified,ServerCertVerifier},pki_types::ServerName};
use common::{io::{stream_copy::Stream, ResetHeader}, proxy_list::ProxyList};
use common::{RwebError,mac::Mac};

const IDLE_TIMEOUT_MILLIS:u32=21_000;
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

pub trait DiyStrem: Send + Sync + Unpin+ Clone + 'static {
    fn new_diy_stream(&self, mac: Mac, proxy_addr:Option<SocketAddr>)->impl Future<Output = Result<impl AsyncReadWrite + Send, RwebError>> + Send;
    fn mac_list(&self)->Vec<&Mac>;
}

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
    transport_config
        //.keep_alive_interval(Some(std::time::Duration::from_millis(KEEPALIVE_INTERVAL_MILLIS)))
        .max_idle_timeout(Some(quinn::IdleTimeout::from(quinn::VarInt::from_u32(IDLE_TIMEOUT_MILLIS))))
        .max_concurrent_bidi_streams(10000_u16.into())
        .max_concurrent_uni_streams(1000_u16.into());
    client_config.transport_config(std::sync::Arc::new(transport_config));
    client_config
}

pub async fn run_diy_stream(server_host:&str,server_port:u16,diy_stream:impl DiyStrem)->Result<(),RwebError>{
    let server_addr = (server_host, server_port).to_socket_addrs().map_err(|e|RwebError{code:-10,msg:e.to_string()})?.next().ok_or("can't resolve").map_err(|e|RwebError{code:-11,msg:e.to_string()})?;
    let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);
    let client = make_client_endpoint(bind_addr).map_err(|e|RwebError{code:-12,msg:e.to_string()})?;
    let conn = client.connect(server_addr, "reform").map_err(|e|RwebError{code:-13,msg:e.to_string()})?;
    let connection = conn.await.map_err(|e|RwebError{code:-14,msg:e.to_string()})?;
    let mut uni_stream = connection.open_uni().await.map_err(|e|RwebError{code:-15,msg:e.to_string()})?;
    let mac_list = diy_stream.mac_list();
    uni_stream.write_u16(mac_list.len() as u16).await.map_err(|e|RwebError{code:-17,msg:e.to_string()})?;
    for v in mac_list.iter(){
        uni_stream.write_all(v.as_ref()).await.map_err(|e|RwebError{code:-18,msg:e.to_string()})?;
    }
    uni_stream.finish().unwrap_or_default();
    drop(uni_stream);
    listen_bi(connection, diy_stream).await
}

pub async fn run(server_host:&str,server_port:u16,proxy_list:Vec<ProxyList>)->Result<(),RwebError>{
    let server_addr = (server_host, server_port).to_socket_addrs().map_err(|e|RwebError{code:-10,msg:e.to_string()})?.next().ok_or("can't resolve").map_err(|e|RwebError{code:-11,msg:e.to_string()})?;
    let diy_stream = ProxyStringList::new(Arc::new(proxy_list),server_addr);
    run_diy_stream(server_host,server_port,diy_stream).await
}

async fn listen_bi(connection:Connection,diy_stream:impl DiyStrem)->Result<(),RwebError>{
    loop{
        match connection.accept_bi().await{
            Ok(bi_stream) => {
                let diy_stream = diy_stream.clone();
                tokio::spawn(async move {
                    #[cfg(feature="log")]
                    println!("accept bi stream from {}",server_addr);
                    handle_bi(bi_stream,diy_stream).await.unwrap_or_else(|_e| {
                        #[cfg(feature="log")]
                        println!("handle_bi error:{}", _e);
                    });
                });
            },
            Err(e) => {
                return Err(RwebError{code:-20,msg:e.to_string()});
            }
        }
    }
}

async fn handle_bi<S: AsyncWrite + Unpin + Send, R: AsyncRead + Unpin + Send>(bi_stream:(S,R),diy_stream:impl DiyStrem)->Result<(),Box<dyn Error+Send+Sync>>{
    let mut quic_stream = Stream::new(bi_stream);
    if let Ok(mac) = quic_stream.read_mac().await{
        if let Ok(mut header) = quic_stream.peek_header().await{
            match header.method.as_str(){
                "CONNECT"=>{
                    quic_stream.peek_remove();//这个header不要了
                    let url_addr = header.uri.to_socket_addrs()?.next().ok_or("can't resolve")?;
                    let mut stream = diy_stream.new_diy_stream(mac,Some(url_addr)).await?;
                    quic_stream.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await?;
                    quic_stream.flush().await?;
                    tokio::io::copy_bidirectional(&mut quic_stream, &mut stream).await?;
                },
                _=>{
                    if let Some(_keep_alive) = header.get("Proxy-Connection"){//旧版http_proxy代理协议和新版区别很大.
                        let uri = Url::try_from(header.uri.as_str())?;
                        let proxy_addr = format!("{}:{}",uri.host_str().ok_or("no host")?,uri.port().unwrap_or(if uri.scheme()=="http"{80}else{443}));
                        header.remove("Proxy-Connection");//去掉Proxy-Connection头
                        let method = header.method.clone();
                        header.remove(&method);//去掉方法头
                        header.insert("Connection".to_string(),"close".to_string());//close会减慢旧版http代理速度，但是会减少很多处理逻辑
                        let host_str = match uri.port(){
                            Some(port) => format!("{}:{}",uri.host_str().ok_or("no host")?,port),
                            None => uri.host_str().ok_or("no host")?.to_string()
                        };
                        header.uri = header.uri.replace(&format!("{}://{}",uri.scheme(),host_str),"");
                        if header.uri.is_empty(){
                            header.uri = "/".to_string();
                        }
                        quic_stream.reset_header(header);
                        let proxy_addr = proxy_addr.to_socket_addrs()?.next().ok_or("can't resolve")?;
                        let mut stream = diy_stream.new_diy_stream(mac,Some(proxy_addr)).await?;
                        tokio::io::copy_bidirectional(&mut quic_stream, &mut stream).await?;
                    }else{
                        let mut stream = diy_stream.new_diy_stream(mac,None).await?;
                        tokio::io::copy_bidirectional(&mut quic_stream, &mut stream).await?;
                    }
                }
            }
        }
    }else{
        quic_stream.write_all(b"HTTP/1.1 400 Bad Request\r\n\
               Content-Type: text/plain; charset=utf-8\r\n\
               Content-Length: 23\r\n\r\n\
               Error: Bad Request").await?;
        return Ok(());
    }
    Ok(())
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


pub trait AsyncReadWrite: AsyncRead + AsyncWrite + Unpin + Send {}

impl<T: AsyncRead + AsyncWrite + Unpin + Send> AsyncReadWrite for T {}
#[derive(Debug,Clone)]
pub struct ProxyStringList{
    pub proxy_list:Arc<Vec<ProxyList>>,
    server_addr:SocketAddr,
}

impl ProxyStringList{
    pub fn new(proxy_list:Arc<Vec<ProxyList>>,server_addr:SocketAddr)->Self{
        Self{proxy_list,server_addr}
    }
}

impl DiyStrem for ProxyStringList{
    #[allow(refining_impl_trait)]
    async fn new_diy_stream(&self,mac: Mac,proxy_addr:Option<SocketAddr>)->Result<Box<dyn AsyncReadWrite>, RwebError> {
        #[cfg(feature="log")]
        println!("proxy addr:tcp:{},server_addr:{}",proxy_addr,server_addr);
        match proxy_addr{
            Some(proxy_addr) => {
                if proxy_addr == self.server_addr{
                    #[cfg(feature="log")]
                    eprintln!("proxy addr:tcp:{},server_addr:{},loop detected",proxy_addr,server_addr);
                    return Err(RwebError::new(5023,"loop detected"));
                }
                let tcp_stream = TcpStream::connect(proxy_addr).await.map_err(|e|RwebError::new(5025,e.to_string()))?;
                Ok(Box::new(tcp_stream))
            },
            None => {
                let forward_url = &self.proxy_list.iter().find(|x|x.mac==mac).ok_or(RwebError::new(5024, "not found proxy addr"))?.url;
                let host = forward_url.host_str().ok_or(RwebError::new(5026, "proxy_addr have no host"))?.to_string();
                let forward_addr = if host.contains(":"){
                    host.clone()
                }else{
                    host.clone() + match forward_url.scheme(){"http"=>":80","rtsp"=>":554","https"=>":443",_=>"error"}
                };
                let forward_addr = forward_addr.to_socket_addrs().map_err(|e|RwebError::new(5027, e))?.next().ok_or(RwebError::new(5028, "can't resolve"))?;
                if forward_addr == self.server_addr{
                    return Err(RwebError{code:5026,msg:"loop detected".to_string()}.into());
                }
                let tcp_stream = TcpStream::connect(forward_addr).await.map_err(|e|RwebError::new(5029,e.to_string()))?;

                match forward_url.scheme(){
                    "http"|"rtsp" => {
                        Ok(Box::new(tcp_stream))
                    },
                    "https" => {
                        let tls_stream = CONNECTOR.connect(ServerName::try_from(host.split(":").next().ok_or(RwebError::new(5029,"proxy_addr have not host"))?.to_string())
                        .map_err(|e|RwebError::new(5030,e))?, tcp_stream).await.map_err(|e|RwebError::new(5031,e.to_string()))?;
                        //header.set("Host".to_string(), host.clone());//将host设置为代理地址的头，注释掉的话，会变成带mac的服务器地址
                        //quic_stream.reset_header(header);
                        Ok(Box::new(tls_stream))
                    },
                    _ => Err(RwebError{code:5026,msg:"loop detected".to_string()}.into())
                }
                
            }
        }
    }

    fn mac_list(&self)->Vec<&Mac>{
        self.proxy_list.iter().map(|x|&x.mac).collect()
    }
}