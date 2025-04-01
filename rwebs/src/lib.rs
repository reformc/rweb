pub mod http_server;
//pub mod tcp_server;
pub mod quic_server;

use clap::Parser;

#[derive(Parser)]
#[clap(
    author = "reform <reformgg@gmail.com>",
    version = "0.1.0",
    about = "http代理服务",
    long_about = "http代理服务"
)]
struct Opts {
    ///web端口
    #[clap(short, long, default_value = "80")]
    port: u16,
    ///mysql为用户名:密码@ip:端口/数据库名(用户名密码需要urlencode)
    #[clap(short, long, default_value = "rweb.hzbit.cn")] //root:2021%40Ymd@192.168.10.109:3306/ymd_iot //ymd_iot.db //brake:754386%40Brake@mysql.recgg.cn:22008/ymd_iot
    host: String,
}

#[tokio::main]
pub async fn run(){
    let quic_s = quic_server::QuicServer::default();
    let peers = quic_s.clone();
    let a = tokio::spawn(async move{
        quic_s.start(5677).await.unwrap();
    });
    let b = tokio::spawn(async move{
        http_server::run(5677,peers).await;
    });
    tokio::select! {
        _ = a => {},
        _ = b => {},
    }
}