use tokio::{io::AsyncWriteExt, net::{TcpListener, TcpStream}};
use common::get_header;
use std::error::Error;
use clap::Parser;

#[derive(Parser)]
#[clap(
    author = "reform <reformgg@gmail.com>",
    version = "0.1.0",
    about = "http穿透代理代理服务http_proxy本地代理",
    long_about = "如果需要使用设备的http_proxy功能,必须运行本程序,"
)]
struct Opts {
    ///本地http_proxy端口，启动后会监听此端口，浏览器可以设置http代理地址为此端口。
    #[clap(short, long, default_value = "5678")]
    port: u16,
    ///rwebs的设备地址
    #[clap(short, long, default_value = "aabbccddeeff.aaa.cn:5677")]
    device_addr: String,
}

#[tokio::main]
async fn main() {
    simple_logger::init_with_level(log::Level::Info).unwrap();
    let opts = Opts::parse();
    let listener = TcpListener::bind(format!("0.0.0.0:{}",opts.port)).await.unwrap();
    log::info!("listen on {}",listener.local_addr().unwrap());
    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                log::debug!("accept from {}", addr);
                let device_addr = opts.device_addr.clone();
                tokio::spawn(async move {
                    translate(stream,device_addr).await.unwrap_or_default();
                });
            }
            Err(e) => {
                log::warn!("accept error:{}", e);
            }
        }
    }
}

async fn translate(mut client_stream:TcpStream,device_addr:String)->Result<(),Box<dyn Error+Send+Sync>>{
    let mut header = get_header(&mut client_stream).await?;
    header.set("Host".to_string(), device_addr.clone());
    let server_stream = TcpStream::connect(device_addr).await?;
    let (mut server_tcp_read, mut server_tcp_write) = tokio::io::split(server_stream);
    let (mut client_tcp_read, mut client_tcp_write) = tokio::io::split(client_stream);
    server_tcp_write.write_all(&Into::<Vec<u8>>::into(header)).await?;
    tokio::select! {
        _ = tokio::io::copy(&mut server_tcp_read, &mut client_tcp_write) => Ok(()),
        _ = tokio::io::copy(&mut client_tcp_read, &mut server_tcp_write) => Ok(()),
    }
}