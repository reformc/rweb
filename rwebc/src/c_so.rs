use std::ffi::CStr;
use std::os::raw::{c_int,c_char};
use common::proxy_list;
use super::quic_client::run;

#[unsafe(no_mangle)]
pub extern "C" fn quic_node_run(
    server_host: *const c_char,
    server_port: c_int,
    proxy_list: *const c_char,
) -> c_int {
    // 转换C字符串到Rust字符串
    if let Ok(server_host) = unsafe { CStr::from_ptr(server_host).to_str() } {
        if server_host.is_empty() {
            return -30;
        }
        if let Ok(proxy_addr) = unsafe { CStr::from_ptr(proxy_list).to_str() } {
            if proxy_addr.is_empty() {
                return -31;
            }
            if let Ok(proxy_list) = serde_json::from_str::<Vec<proxy_list::ProxyList>>(proxy_addr) {
                let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_io()
                .enable_time()
                .build() {
                    Ok(rt) => rt,
                    Err(_) => return -37,
                };
                loop{
                    match rt.block_on(run(server_host,server_port as u16,proxy_list.clone())) {
                        Ok(_) => {},
                        Err(_e) => {
                            #[cfg(feature="log")]
                            eprintln!("{:?}",_e)
                        },
                    }
                    std::thread::sleep(std::time::Duration::from_secs(30));//max_idle_timeout为21秒,这里如果是因为mac地址重复而无法连接的话，立即重连会被踢掉。
                }
            } else {
                return -32
            }
        } else {
            -35
        }
    } else {
        -36
    }
}