use std::{error::Error, net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs}, sync::Arc};
use quinn::{ClientConfig, Endpoint, SendStream, RecvStream, TransportConfig};
use rustls::pki_types::pem::PemObject;
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::TcpStream, select};
use common::mac::Mac;

const KEEPALIVE_INTERVAL_MILLIS:u64=3000;
const IDLE_TIMEOUT_MILLIS:u32=10_000;
const CER_BIN:&[u8] = include_bytes!("E:/reform.cer");

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
        .max_concurrent_bidi_streams(100_u8.into())
        .max_concurrent_uni_streams(100_u8.into());
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
                //println!("accept bi stream");
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
    //println!("quic client end");
    Ok(())
}

async fn handle_uni(mut send_stream:SendStream,mac:Mac){
    loop{
        if let Err(e) = send_stream.write_all(mac.as_ref()).await{
            println!("write error:{}",e);
            break;
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}

async fn handle_bi((mut send_stream, mut recv_stream):(SendStream,RecvStream),proxy_addr:Arc<String>){
    let tcp_stream = TcpStream::connect(proxy_addr.as_ref()).await.unwrap();
    let (mut tcp_read, mut tcp_write) = tokio::io::split(tcp_stream);
    let a = tokio::spawn(async move{
        let mut buf = [0u8; 256*256];
        loop{
            match recv_stream.read(&mut buf).await{
                Ok(Some(0))=>break,
                Ok(Some(len))=>{
                    if let Err(e) = tcp_write.write_all(&buf[..len]).await{
                        println!("tcp send error:{}",e);
                        break;
                    }
                },
                Ok(None)=>{
                    println!("recv stream closed");
                    break;
                },
                Err(e)=>{
                    println!("recv error:{}",e);
                    break;
                }
            }
        }
    });
    let b = tokio::spawn(async move{
        let mut buf = [0u8; 256*256];
        loop{
            match tcp_read.read(&mut buf).await{
                Ok(0)=>break,
                Ok(len)=>{
                    if let Err(e) = send_stream.write_all(&buf[..len]).await{
                        println!("quic send error:{}",e);
                        break;
                    }
                },
                Err(e)=>{
                    println!("tcp read error:{}",e);
                    break;
                }
            }
        }
    });
    select! {
        _ = a => {},
        _ = b => {},
    }
}