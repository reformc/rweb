use std::ffi::CStr;
use std::os::raw::{c_int,c_char};
use common::mac::Mac;
use url::Url;
use super::quic_client::run;

#[repr(C)]
pub enum QuicError {
    Success = 0,
    ConnectionFailed = -1,
    StreamError = -2,
    InvalidArgs = -3,
    RuntimeError = -4,
}

#[unsafe(no_mangle)]
pub extern "C" fn quic_client_run(
    server_host: *const c_char,
    server_port: c_int,
    proxy_addr: *const c_char,
    mac_addr: *const c_char,
) -> c_int {
    // 转换C字符串到Rust字符串
    if let Ok(server_host) = unsafe { CStr::from_ptr(server_host).to_str() } {
        if server_host.is_empty() {
            return -3;
        }
        if let Ok(proxy_addr) = unsafe { CStr::from_ptr(proxy_addr).to_str() } {
            if proxy_addr.is_empty() {
                return -3;
            }
            match proxy_addr.parse::<Url>(){
                Ok(url) => {
                    if let Ok(mac_addr) = unsafe { CStr::from_ptr(mac_addr).to_str() } {
                        if let Ok(mac) = mac_addr.parse::<Mac>() {
                            let rt = match tokio::runtime::Runtime::new() {
                                Ok(rt) => rt,
                                Err(_) => return -4,
                            };
                            let url = std::sync::Arc::new(url);
                            loop{
                                match rt.block_on(run(server_host,server_port as u16,url.clone(),mac)) {
                                    Ok(_) => {},
                                    Err(e) => {eprintln!("{:?}",e)},
                                }
                                std::thread::sleep(std::time::Duration::from_secs(30));//max_idle_timeout为21秒,这里如果是因为mac地址重复而无法连接的话，立即重连会被踢掉。
                            }
                        } else {
                            -3
                        }
                    } else {
                        -3
                    }
                },
                Err(_) => return -3,
            }
        } else {
            -3
        }
    } else {
        -3
    }
}