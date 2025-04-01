use clap::Parser;
use rcgen::CertifiedKey;

#[derive(Parser, Debug)]
#[clap(
    author="reform <reformgg@gmail.com>", 
    version="0.1.0",
    about="quic证书",
)]
struct Args {
    /// 域名。
    #[clap(long,default_value = "reform")]
    host: String,
    /// 文件名。
    #[clap(long,short,default_value = "E:/")]
    path: String
}

fn main() {
    let args = Args::parse();
    let path = 
    match &args.path as &str{
        ""=>{dirs::home_dir().unwrap()},
        _=>{std::path::PathBuf::from(&args.path)}
    };
    configure_server(&args.host,path);
}

fn configure_server(host:&str,mut path:std::path::PathBuf) {
    let CertifiedKey { cert, key_pair } = rcgen::generate_simple_self_signed(vec![host.into()]).unwrap();
    let cert_der = cert.pem();
    let priv_key = key_pair.serialize_pem();
    path.push("reform.cer");
    println!("{}",path.as_path().display());
    std::fs::write(&path, cert_der).unwrap();
    path.set_file_name("reform.key");
    std::fs::write(&path, priv_key).unwrap();
}