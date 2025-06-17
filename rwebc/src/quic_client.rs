use std::{error::Error, net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs}, sync::Arc};
use quinn::{ClientConfig, Connection, Endpoint, EndpointConfig, Incoming, ServerConfig, TransportConfig, VarInt};
use rustls::pki_types::{pem::PemObject, CertificateDer, UnixTime};
use tokio::{io::{AsyncRead, AsyncWrite, AsyncWriteExt}, net::TcpStream, select};
#[cfg(feature="p2p")]
use tokio::time::{sleep, Duration};
use url::Url;
use tokio_rustls::TlsConnector;
use rustls::{client::danger::{ServerCertVerified,ServerCertVerifier},pki_types::ServerName};
use rweb_common::{io::{stream_copy::Stream, ResetHeader}, proxy_list::ProxyList, RwebError, mac::Mac};
#[cfg(feature="p2p")]
use rweb_common::{io::header::METHOD_P2P,p2p_list::P2pCell,Header};
#[cfg(feature="p2p")]
use super::{p2p_client::{DiyTcpListener, P2PListener, P2pListen,p2p_connect}, symmetric};
#[cfg(all(feature="p2ptest",feature="p2p"))]
use super::p2p_client::p2ptest;
use super::AsyncReadWrite;

use rweb_common::key::{CER_BIN, KEY_BIN};

const IDLE_TIMEOUT_MILLIS:u32=21_000;
const KEEPALIVE_INTERVAL_MILLIS:u64=10_000;
//const CER_BIN:&[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../reform.cer"));
//const KEY_BIN:&[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../reform.key"));

static CONNECTOR:once_cell::sync::Lazy<TlsConnector> = once_cell::sync::Lazy::new(|| {
    let provider = rustls::crypto::ring::default_provider();
    rustls::crypto::CryptoProvider::install_default(provider).expect("failed to install crypto provider");
    let mut config = rustls::ClientConfig::builder()
        .with_root_certificates(rustls::RootCertStore::empty())
        .with_no_client_auth(); 
    config.dangerous().set_certificate_verifier(Arc::new(NoVerify));
    TlsConnector::from(Arc::new(config))
});

pub trait DiyStream: Send + Sync + Unpin+ Clone + 'static {
    fn new_diy_stream(&self, mac: Mac, proxy_addr:Option<SocketAddr>)->impl Future<Output = Result<impl AsyncReadWrite + Send, RwebError>> + Send;
    fn mac_list(&self)->Vec<&Mac>;
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

#[cfg(feature="p2p")]
pub async fn run_diy_stream<K: PartialEq + Clone + 'static,L: P2pListen + 'static>(server_host:&str,server_port:u16,diy_stream:impl DiyStream,p_client:Option<impl P2PListener<K,L>>)->Result<(),RwebError>{
    let server_addr = (server_host, server_port).to_socket_addrs().map_err(|e|RwebError{code:-10,msg:e.to_string()})?.next().ok_or("can't resolve").map_err(|e|RwebError{code:-11,msg:e.to_string()})?;
    let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);
    let socket = std::net::UdpSocket::bind(bind_addr).map_err(|e|RwebError{code:-10,msg:e.to_string()})?;
    let server_config = configure_host_server(CER_BIN, KEY_BIN).map_err(|e|RwebError{code:-11,msg:e.to_string()})?;
    let mut endpoint = Endpoint::new(EndpointConfig::default(), Some(server_config), socket, Arc::new(quinn::TokioRuntime)).map_err(|e|RwebError{code:-12,msg:e.to_string()})?;
    endpoint.set_default_client_config(configure_host_client(CER_BIN));//客户端服务端共用一个socket
    let conn = endpoint.connect(server_addr, "reform").map_err(|e|RwebError{code:-13,msg:e.to_string()})?;
    let connection = conn.await.map_err(|e|RwebError{code:-14,msg:e.to_string()})?;
    let mut uni_stream = connection.open_uni().await.map_err(|e|RwebError{code:-15,msg:e.to_string()})?;
    let mac_list = diy_stream.mac_list();
    uni_stream.write_u16(mac_list.len() as u16).await.map_err(|e|RwebError{code:-17,msg:e.to_string()})?;
    for v in mac_list.iter(){
        uni_stream.write_all(v.as_ref()).await.map_err(|e|RwebError{code:-18,msg:e.to_string()})?;
    }
    #[cfg(all(feature="p2ptest",feature="log"))]
    if let Ok(addr) = p2ptest(endpoint.clone()).await{
        println!("测试服务器检测此节点地址为:{}",addr);
    }
    uni_stream.finish().unwrap_or_default();
    drop(uni_stream);
    let res = if let Some(p_li) = p_client{
        select! {
            a = listen_incoming(endpoint.clone(), diy_stream.clone())=>a,//监听p2p对端数据
            b = listen_bi(endpoint.clone(), connection.clone(), diy_stream)=>b,//监听服务器命令
            c = p2p_connect(endpoint, connection, p_li) =>c
        }
    }else{
        select! {
            a = listen_incoming(endpoint.clone(), diy_stream.clone())=>a,//监听p2p对端数据
            b = listen_bi(endpoint.clone(), connection, diy_stream)=>b,//监听服务器命令
        }
    };
    #[cfg(feature="log")]
    println!("{:?}",res);
    res
}

#[cfg(not(feature="p2p"))]
pub async fn run_diy_stream(server_host:&str,server_port:u16,diy_stream:impl DiyStream)->Result<(),RwebError>{
    let server_addr = (server_host, server_port).to_socket_addrs().map_err(|e|RwebError{code:-10,msg:e.to_string()})?.next().ok_or("can't resolve").map_err(|e|RwebError{code:-11,msg:e.to_string()})?;
    let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);
    let socket = std::net::UdpSocket::bind(bind_addr).map_err(|e|RwebError{code:-10,msg:e.to_string()})?;
    let server_config = configure_host_server(CER_BIN.as_bytes(), KEY_BIN.as_bytes()).map_err(|e|RwebError{code:-11,msg:e.to_string()})?;
    let mut endpoint = Endpoint::new(EndpointConfig::default(), Some(server_config), socket, Arc::new(quinn::TokioRuntime)).map_err(|e|RwebError{code:-12,msg:e.to_string()})?;
    endpoint.set_default_client_config(configure_host_client(CER_BIN.as_bytes()));//客户端服务端共用一个socket
    let conn = endpoint.connect(server_addr, "reform").map_err(|e|RwebError{code:-13,msg:e.to_string()})?;
    let connection = conn.await.map_err(|e|RwebError{code:-14,msg:e.to_string()})?;
    let mut uni_stream = connection.open_uni().await.map_err(|e|RwebError{code:-15,msg:e.to_string()})?;
    let mac_list = diy_stream.mac_list();
    uni_stream.write_u16(mac_list.len() as u16).await.map_err(|e|RwebError{code:-17,msg:e.to_string()})?;
    for v in mac_list.iter(){
        uni_stream.write_all(v.as_ref()).await.map_err(|e|RwebError{code:-18,msg:e.to_string()})?;
    }
    uni_stream.finish().unwrap_or_default();
    drop(uni_stream);
    let res = 
    select! {
        a = listen_incoming(endpoint.clone(), diy_stream.clone())=>a,//监听p2p对端数据
        b = listen_bi(endpoint.clone(), connection, diy_stream)=>b,//监听服务器命令
    };
    #[cfg(feature="log")]
    println!("{:?}",res);
    res
}

#[cfg(feature="p2p")]
pub async fn node_run(server_host:&str,server_port:u16,proxy_list:Vec<ProxyList>,p2p_list:Option<Vec<P2pCell>>)->Result<(),RwebError>{
    let server_addr = (server_host, server_port).to_socket_addrs().map_err(|e|RwebError{code:-10,msg:e.to_string()})?.next().ok_or("can't resolve").map_err(|e|RwebError{code:-11,msg:e.to_string()})?;
    let diy_stream = ProxyStringList::new(Arc::new(proxy_list),server_addr);
    run_diy_stream(server_host,server_port,diy_stream, p2p_list.map(|list|DiyTcpListener{list})).await
}

#[cfg(not(feature="p2p"))]
pub async fn node_run(server_host:&str,server_port:u16,proxy_list:Vec<ProxyList>)->Result<(),RwebError>{
    let server_addr = (server_host, server_port).to_socket_addrs().map_err(|e|RwebError{code:-10,msg:e.to_string()})?.next().ok_or("can't resolve").map_err(|e|RwebError{code:-11,msg:e.to_string()})?;
    let diy_stream = ProxyStringList::new(Arc::new(proxy_list),server_addr);
    run_diy_stream(server_host,server_port,diy_stream).await
}

async fn listen_incoming(endpoint:Endpoint, diy_stream:impl DiyStream)->Result<(),RwebError>{
    #[cfg(feature="log")]
    println!("listen incomng");
    loop{
        match endpoint.accept().await{
            Some(incoming) => {
                let diy_stream = diy_stream.clone();
                let endpoint = endpoint.clone();
                tokio::spawn(async move {
                    #[cfg(feature="log")]
                    println!("accept incoming stream from {}",incoming.remote_address());
                    handle_incoming_bi(endpoint, incoming, diy_stream).await.unwrap_or_else(|_e| {
                        #[cfg(feature="log")]
                        println!("handle_incomming error:{}", _e);
                    });
                });
            },
            None => {
                
            }
        }
    }
}

async fn handle_incoming_bi(endpoint:Endpoint, incoming:Incoming, diy_stream:impl DiyStream)->Result<(),RwebError>{
    let incoming = incoming.await.map_err(|e|RwebError{code:-23,msg:e.to_string()})?;
    let bi_stream = incoming.accept_bi().await.map_err(|e|RwebError{code:-21,msg:e.to_string()})?;
    handle_bi(endpoint, incoming.remote_address(), bi_stream, diy_stream).await.map_err(|e|RwebError{code:-22,msg:e.to_string()})?;
    Ok(())
}

//endpoint为quic对象
//connection为与服务器的连接
//diy_stream为将要代理的流集合。
async fn listen_bi(endpoint:Endpoint, connection:Connection,diy_stream:impl DiyStream)->Result<(),RwebError>{
    #[cfg(feature="log")]
    println!("listen bi");
    loop{
        match connection.accept_bi().await{
            Ok(bi_stream) => {
                #[cfg(feature="log")]
                println!("145 accept bi stream from {}",connection.remote_address());
                let diy_stream = diy_stream.clone();
                let endpoint = endpoint.clone();
                let remote_addr = connection.remote_address();
                tokio::spawn(async move {
                    handle_bi(endpoint, remote_addr, bi_stream, diy_stream).await.unwrap_or_else(|_e| {
                        #[cfg(feature="log")]
                        println!("handle_bi error:{}", _e);
                    });
                });
            },
            Err(e) => {
                #[cfg(feature="log")]
                println!("159 accept bi stream error:{}", e);
                return Err(RwebError{code:-20,msg:e.to_string()});
            }
        }
    }
}

async fn handle_bi<S: AsyncWrite + Unpin + Send, R: AsyncRead + Unpin + Send>(_endpoint:Endpoint,bi_remote_addr:SocketAddr, bi_stream:(S,R),diy_stream:impl DiyStream)->Result<(),Box<dyn Error+Send+Sync>>{
    let mut quic_stream = Stream::new(bi_stream,bi_remote_addr);
    if let Ok(mac) = quic_stream.read_mac().await{
        #[cfg(feature="log")]
        println!("handle_bi mac:{}",mac);
        if !diy_stream.mac_list().contains(&&mac){
            #[cfg(feature="log")]
            println!("handle_bi mac not in list:{}",mac);
            quic_stream.write_all(b"HTTP/1.1 400 Bad Request\r\n\
               Content-Type: text/plain; charset=utf-8\r\n\
               Content-Length: 23\r\n\r\n\
               Error: Bad Request").await?;
            return Ok(());
        }
        match quic_stream.peek_header().await{
            Ok(mut header) => {
                #[cfg(feature="log")]
                println!("header: {:?}", header);
                match header.method.as_str(){
                    "CONNECT"=>{
                        quic_stream.peek_remove();
                        let url_addr = header.uri.to_socket_addrs()?.next().ok_or("can't resolve")?;
                        let mut stream = diy_stream.new_diy_stream(mac,Some(url_addr)).await?;
                        quic_stream.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await?;
                        quic_stream.flush().await?;
                        tokio::io::copy_bidirectional(&mut quic_stream, &mut stream).await?;
                    },
                    #[cfg(feature="p2p")]
                    METHOD_P2P=>{
                        quic_stream.peek_remove();
                        let (_mac,addr,_self_addr) = header.parse_p2p()?;
                        #[cfg(feature="p2ptest")]
                        let test_addr = p2ptest(_endpoint.clone()).await.ok();
                        #[cfg(not(feature="p2ptest"))]
                        let test_addr = None;
                        #[cfg(all(feature="p2ptest",feature="log"))]
                        if let Some(test_addr) = test_addr{
                            if let Some(self_addr) = _self_addr{
                                if self_addr != test_addr{
                                    println!("当前节点正处于受限锥形NAT网络,无法直接打洞,{} != {}",self_addr,test_addr);
                                }else{
                                    println!("当前节点正处于全锥形NAT网络,可以直接打洞,{} != {}",self_addr,test_addr);
                                }
                            }
                        }
                        #[cfg(feature="log")]
                        println!("p2p command {},{}",_mac,addr);
                        let mut port_len = 1;//如果一个端口都不给，会报错
                        match header.get("Nat-Type").map(|s|s.as_str()){
                            Some("FullCone")=>{
                                #[cfg(feature="log")]
                                println!("对端处于全锥形NAT网络,可以直接打洞,{}",addr);
                            },
                            Some("Symmetric")=>{
                                port_len = crate::PORT_LEN;
                                #[cfg(feature="log")]
                                println!("对端处于对称NAT网络,无法直接打洞，尝试后延{}个端口,{}",port_len,addr);
                            },
                            _=>{}
                        }
                        let header = Header::new_p2p(_mac,addr,test_addr);
                        let v:Vec<u8> = header.into();
                        quic_stream.write_all(&v).await?;
                        for _i in 0..10{//这里连接只是向对方发送一下udp包使对方的NAT进行标记以便打洞，并不是真的要连接对方
                            match symmetric::symmetric_connect(_endpoint.clone(),addr,port_len,Duration::from_secs(1)).await{
                                Ok(conn)=>{
                                    #[cfg(feature="log")]
                                    println!("p2p connect success {}",addr);
                                    conn.close(VarInt::from_u32(0), b"Ok");
                                    break
                                },
                                Err(_e)=>{
                                    #[cfg(feature="log")]
                                    if _i==9{
                                        println!("p2p connect {} error:{:?}",addr,_e);
                                    }
                                }
                            }
                            sleep(Duration::from_millis(100)).await;
                        }
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
            },
            Err(e) => {
                #[cfg(feature="log")]
                println!("peek header error:{}", e);
                return Err(RwebError{code:-20,msg:e.to_string()}.into());
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

impl DiyStream for ProxyStringList{
    #[allow(refining_impl_trait)]
    async fn new_diy_stream(&self,mac: Mac,proxy_addr:Option<SocketAddr>)->Result<Box<dyn AsyncReadWrite + Send>, RwebError> {
        #[cfg(feature="log")]
        println!("proxy addr:tcp:{:?},server_addr:{}",proxy_addr,self.server_addr);
        match proxy_addr{
            Some(proxy_addr) => {
                if proxy_addr == self.server_addr{
                    #[cfg(feature="log")]
                    eprintln!("proxy addr:tcp:{},server_addr:{},loop detected",proxy_addr,self.server_addr);
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
                #[cfg(feature="log")]
                println!("proxy addr:tcp:{:?},server_addr:{}",forward_addr,self.server_addr);
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