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
    #[clap(long, default_value = "server.aaa.cn")]
    server_host: String,
    ///本地http_proxy端口，启动后会监听此端口，浏览器可以设置http代理地址为此端口。
    #[clap(long, default_value = "5677")]
    server_port: u16,
    ///本地http_proxy端口，启动后会监听此端口，浏览器可以设置http代理地址为此端口。一定要是http地址，以http://或者https://开头，不然会报错-34
    #[clap(short, long, default_value = "https://192.168.3.240/")]
    proxy_addr: String,
    ///rwebs的设备标签，一定要是mac地址类型，否则会报错-32
    #[clap(short, long, default_value = "aabbccddeeff")]
    mac: String,
}

fn main(){
    #[cfg(target_os = "windows")]
    let lib_path = "target/release/rwebc.dll";
    #[cfg(target_os = "linux")]
    let lib_path = "target/release/librwebc.so";
    #[cfg(target_os = "macos")]
    let lib_path = "target/release/librwebc.dylib";
    let opts = Opts::parse();
    let _ret = unsafe{
        //c语言的字符串要加\0结尾
        let server_host = opts.server_host+"\0";
        let proxy_addr = opts.proxy_addr+"\0";
        let mac = opts.mac+"\0";
        let lib = libloading::Library::new(lib_path).unwrap();
        let func: libloading::Symbol<unsafe extern "C" fn(*const c_char, c_int, *const c_char, *const c_char) -> i32> = lib.get(b"quic_node_run").unwrap();
        func(
            server_host.as_ptr() as *const c_char,
            opts.server_port as c_int,
            proxy_addr.as_ptr() as *const c_char,
            mac.as_ptr() as *const c_char,
        )
        //返回值为0表示成功，其他值表示失败
        //正常情况下不返回,如果出错则返回，请自行处理重连接，idle_timeout为21秒，重连间隔请大于这个间隔，不然可能会返回-20错误，-20错误表示mac重复
    };
    //println!("ret: {}", ret);
}