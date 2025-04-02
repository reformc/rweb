pub mod http_server;
pub mod quic_server;
use clap::Parser;

#[derive(Parser)]
#[clap(
    author = "reform <reformgg@gmail.com>",
    version = "0.1.0",
    about = "http穿透代理代理服务",
    long_about = "http穿透代理服务"
)]
struct Opts {
    ///web端口
    #[clap(short, long, default_value = "5677")]
    port: u16,
}

#[tokio::main]
pub async fn run(){
    simple_logger::init_with_level(log::Level::Info).unwrap();
    let opts = Opts::parse();
    let quic_s = quic_server::QuicServer::default();
    let peers = quic_s.clone();
    let a = tokio::spawn(async move{
        quic_s.start(opts.port).await.unwrap();
    });
    let b = tokio::spawn(async move{
        http_server::run(opts.port,peers).await;
    });
    tokio::select! {
        _ = a => {},
        _ = b => {},
    }
}