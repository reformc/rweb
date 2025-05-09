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
    ///so或dell文件地址,这个参数不是传给so或dll函数的
    #[clap(short, long)]
    lib_path: Option<String>,
    ///代理清单,为json数组，例如为[{"mac":"aabbccddeeff","url":"http://192.168.0.1"}],mac必须是合法的mac地址，url必须是合法的url地址，不然会报错-32
    #[clap(short, long)]
    proxy_list: Option<String>,
    ///proxy_list可以写入json文件，文件路径，demo中优先使用proxy_list，如果为空则读取proxy_list_file
    #[clap(short, long)]
    proxy_list_file: Option<String>,
    ///p2p连接列表,为json数组，例如为[{"mac":"aabbccddeeff","port":"8080"}],mac必须是合法的mac地址，port必须是u16,如果p2p连接成功，本机会监听这个端口代理到p2p流
    #[clap(short, long)]
    p2p_list: Option<String>,
    ///p2p_list可以写入json文件，文件路径，demo中优先使用p2p_list，如果为空则读取p2p_list_file
    #[clap(short, long)]
    p2p_list_file: Option<String>
}

fn main(){
    #[cfg(target_os = "windows")]
    let lib_path = "rwebc.dll";//target/release/rwebc.dll
    #[cfg(target_os = "linux")]
    let lib_path = "librwebc.so";//target/release/librwebc.so
    #[cfg(target_os = "macos")]
    let lib_path = "librwebc.dylib";//target/release/librwebc.dylib
    let opts = Opts::parse();
    let lib_path = opts.lib_path.unwrap_or(lib_path.to_string());
    
    //c语言的字符串要加\0结尾
    let server_host = opts.server_host+"\0";
    let proxy_list = opts.proxy_list.unwrap_or(std::fs::read_to_string(opts.proxy_list_file.unwrap()).unwrap())+"\0";
    println!("server_host: {}, server_port: {}, proxy_list_file: {}", server_host, opts.server_port, proxy_list);
    let lib = unsafe{libloading::Library::new(&lib_path).unwrap()};
    //只有编译rwebc时使用p2p这个featrure才能加载p2pclient
    let _ret = if let Some(Ok(p2p_list)) = opts.p2p_list.map(|l|Some(Ok(l+"\0"))).unwrap_or(opts.p2p_list_file.map(|p|std::fs::read_to_string(p))){
        unsafe{
            let func: libloading::Symbol<unsafe extern "C" fn(*const c_char, c_int, *const c_char, *const c_char) -> i32> = lib.get(b"p2pclient").unwrap();
            func(
                server_host.as_ptr() as *const c_char,
                opts.server_port as c_int,
                proxy_list.as_ptr() as *const c_char,
                p2p_list.as_ptr() as *const c_char
            )
        }
    }else{
        unsafe{
            let func: libloading::Symbol<unsafe extern "C" fn(*const c_char, c_int, *const c_char) -> i32> = lib.get(b"quic_node_run").unwrap();
            func(
                server_host.as_ptr() as *const c_char,
                opts.server_port as c_int,
                proxy_list.as_ptr() as *const c_char
            )
        }
    };
    //返回值为0表示成功，其他值表示失败
    //正常情况下不返回,如果出错则返回，请自行处理重连接，idle_timeout为21秒，重连间隔请大于这个间隔，不然可能会返回-20错误，-20错误表示mac重复
    println!("ret: {}", _ret);
}