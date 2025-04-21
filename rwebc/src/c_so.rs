use std::ffi::CStr;
use std::os::raw::{c_int,c_char};
use common::mac::Mac;
use url::Url;
use super::quic_client::run;

#[unsafe(no_mangle)]
pub extern "C" fn quic_node_run(
    server_host: *const c_char,
    server_port: c_int,
    proxy_addr: *const c_char,
    mac_addr: *const c_char,
) -> c_int {
    // 转换C字符串到Rust字符串
    if let Ok(server_host) = unsafe { CStr::from_ptr(server_host).to_str() } {
        if server_host.is_empty() {
            return -30;
        }
        if let Ok(proxy_addr) = unsafe { CStr::from_ptr(proxy_addr).to_str() } {
            if proxy_addr.is_empty() {
                return -31;
            }
            match proxy_addr.parse::<Url>(){
                Ok(url) => {
                    if let Ok(mac_addr) = unsafe { CStr::from_ptr(mac_addr).to_str() } {
                        if let Ok(mac) = mac_addr.parse::<Mac>() {
                            //多线程会消耗更多的资源,如果并发不高的话可以使用单线程
                            // let rt = match tokio::runtime::Runtime::new() {
                            //     Ok(rt) => rt,
                            //     Err(_) => return -37,
                            // };
                            //单线程消耗资源更少,如果并发较高的话可以使用多线程
                            let rt = match tokio::runtime::Builder::new_current_thread()
                            .enable_io()
                            .enable_time()
                            .build() {
                                Ok(rt) => rt,
                                Err(_) => return -37,
                            };
                            let url = std::sync::Arc::new(url);
                            loop{
                                match rt.block_on(run(server_host,server_port as u16,url.clone(),mac)) {
                                    Ok(_) => {},
                                    Err(_e) => {
                                        #[cfg(feature="log")]
                                        eprintln!("{:?}",_e)
                                    },
                                }
                                std::thread::sleep(std::time::Duration::from_secs(30));//max_idle_timeout为21秒,这里如果是因为mac地址重复而无法连接的话，立即重连会被踢掉。
                            }
                        } else {
                            -32
                        }
                    } else {
                        -33
                    }
                },
                Err(_) => -34,
            }
        } else {
            -35
        }
    } else {
        -36
    }
}