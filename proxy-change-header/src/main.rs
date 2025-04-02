use tokio::{io::{self, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt}, net::{TcpListener, TcpStream}};
use common::header::Header;
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
    let mut header = get_header(&mut client_stream).await;
    header.set("Host".to_string(), device_addr.clone());
    let server_stream = TcpStream::connect(device_addr).await?;
    let (server_tcp_read, mut server_tcp_write) = tokio::io::split(server_stream);
    let (client_tcp_read, client_tcp_write) = tokio::io::split(client_stream);
    server_tcp_write.write_all(&Into::<Vec<u8>>::into(header)).await?;
    let a = tokio::spawn(async_copy(server_tcp_read, client_tcp_write));
    let b = tokio::spawn(async_copy(client_tcp_read, server_tcp_write));
    tokio::select! {
        _ = a => Ok(()),
        _ = b => Ok(()),
    }
}

pub async fn get_header(stream:&mut TcpStream)->Header{
    let mut buf = Vec::new();
    let mut header = [0u8; 1];
    loop{        
        match stream.read_exact(&mut header).await{
            Ok(_)=>{
                buf.push(header[0]);
                if buf.ends_with(b"\r\n\r\n"){
                    break
                }else{
                    if buf.len() > 256*256{
                        log::warn!("header too long");
                        break
                    }
                }
            }
            Err(e)=>{
                log::warn!("recv error:{}",e);
                break;
            }
        }
    }
    buf.into()
}

async fn async_copy<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
    mut reader: R,
    mut writer: W,
) -> io::Result<u64> {
    io::copy(&mut reader, &mut writer).await
}
