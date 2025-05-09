pub mod http_server;
pub mod quic_server;
//pub mod quic_p2p_server;
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
    ///key文件路径
    #[clap(short, long, default_value = "reform.key")]
    key: String,
    ///证书文件路径
    #[clap(short, long, default_value = "reform.cer")]
    cert: String,
}

pub async fn run(){
    let opts = Opts::parse();
    //let quic_s = quic_server::QuicServer::default();
    let quic_s = quic_server::QuicServer::default();
    let peers = quic_s.clone();
    rustls::crypto::aws_lc_rs::default_provider()
    .install_default()
    .expect("failed to install default crypto provider");
    let key = std::fs::read_to_string(opts.key).unwrap();
    let cert = std::fs::read_to_string(opts.cert).unwrap();
    tokio::select! {
        _ = quic_s.start(opts.port) => {},
        //_ = http_server::run(opts.port,peers.clone()) => {},//如果用http代理，必须使用proxy_change_header，如果用https则不用。
        _ = http_server::run_https(opts.port,peers.clone(),&key,&cert) => {},
    }
}