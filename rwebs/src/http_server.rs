use common::mac::Mac;
use tokio::{io::AsyncReadExt, net::TcpListener};
use crate::quic_server::QuicServer;

pub async fn run(port:u16,quic_server:QuicServer){
    let listener = TcpListener::bind(format!("0.0.0.0:{}",port)).await.unwrap();
    println!("listen on {}",listener.local_addr().unwrap());
    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                println!("accept from {}", addr);
                let quic_server = quic_server.clone();
                tokio::spawn(async move {
                    let _ = handle_client(stream,quic_server).await;
                });
            }
            Err(e) => {
                println!("accept error:{}", e);
            }
        }
    }
}

pub async fn handle_client(mut stream: tokio::net::TcpStream,quic_server:QuicServer) -> Result<(), Box<dyn std::error::Error+Send+Sync>> {
    let mut buf = [0u8; 1024];
    let len = stream.read(&mut buf).await?;
    let request_str = String::from_utf8_lossy(&buf[..len]);
    for line in request_str.lines() {
        if line.to_lowercase().starts_with("host: ") {
            let host_header = &line[6..].trim();
            let mac = host_header.split('.').next().unwrap_or_default();
            let mac:Mac = mac.try_into()?;
            println!("收到Host请求头: {}", host_header);
            quic_server.translate(mac,stream,buf[..len].to_vec()).await?;
            break;
        }
    }
    Ok(())
}