use std::os::raw::{c_int,c_char};
use clap::Parser;

#[derive(Parser)]
#[clap(
    author = "reform <reformgg@gmail.com>",
    version = "0.1.0",
    about = "rwebc调用示例",
    long_about = "rwebc调用示例"
)]
struct Opts {
    ///服务器域名
    #[clap(long, default_value = "aabbccddeeff.aaa.cn")]
    server_host: String,
    ///本地http_proxy端口，启动后会监听此端口，浏览器可以设置http代理地址为此端口。
    #[clap(long, default_value = "5677")]
    server_port: u16,
    ///本地http_proxy端口，启动后会监听此端口，浏览器可以设置http代理地址为此端口。
    #[clap(short, long, default_value = "https://www.baidu.com")]
    proxy_addr: String,
    ///rwebs的设备地址
    #[clap(short, long, default_value = "aabbccddeeff")]
    mac: String,
}

fn main(){
    simple_logger::init_with_level(log::Level::Info).unwrap();
    #[cfg(target_os = "windows")]
    let lib_path = "target/release/rwebc.dll";
    #[cfg(target_os = "linux")]
    let lib_path = "target/release/librwebc.so";
    #[cfg(target_os = "macos")]
    let lib_path = "target/release/librwebc.dylib";
    let opts = Opts::parse();
    let ret = unsafe{
        //c语言的字符串要加\0结尾
        let server_host = opts.server_host+"\0";
        let proxy_addr = opts.proxy_addr+"\0";
        let mac = opts.mac+"\0";
        let lib = libloading::Library::new(lib_path).unwrap();
        let func: libloading::Symbol<unsafe extern "C" fn(*const c_char, c_int, *const c_char, *const c_char) -> i32> = lib.get(b"quic_client_run").unwrap();
        func(
            server_host.as_ptr() as *const c_char,
            opts.server_port as c_int,
            proxy_addr.as_ptr() as *const c_char,
            mac.as_ptr() as *const c_char,
        )
    };
    println!("ret: {}", ret);
}