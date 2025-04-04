use common::{mac::Mac,get_header};
use tokio::net::{TcpListener,TcpStream};
use crate::quic_server::QuicServer;

pub async fn run(port:u16,quic_server:QuicServer){
    let listener = TcpListener::bind(format!("0.0.0.0:{}",port)).await.unwrap();
    log::info!("http_server listen on {}",listener.local_addr().unwrap());
    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                log::debug!("accept from {}", addr);
                let quic_server = quic_server.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_client(stream,quic_server).await{
                        log::warn!("handle client error:{}",e);
                    }
                });
            }
            Err(e) => {
                log::warn!("accept error:{}", e);
            }
        }
    }
}

pub async fn handle_client(mut stream: TcpStream,quic_server:QuicServer) -> Result<(), Box<dyn std::error::Error+Send+Sync>> {
    let header = get_header(&mut stream).await?;
    log::debug!("header:{:?}",header);
    let host_header = header.get("Host").ok_or("not found Host header")?;
    let mac:Mac = host_header.split('.').next().ok_or("host error")?.try_into()?;
    quic_server.translate(mac,stream,header.into()).await?;
    Ok(())
}