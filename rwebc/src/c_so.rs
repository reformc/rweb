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
) -> QuicError {
    // 转换C字符串到Rust字符串
    if let Ok(server_host) = unsafe { CStr::from_ptr(server_host).to_str() } {
        if server_host.is_empty() {
            return QuicError::InvalidArgs;
        }
        if let Ok(proxy_addr) = unsafe { CStr::from_ptr(proxy_addr).to_str() } {
            if proxy_addr.is_empty() {
                return QuicError::InvalidArgs;
            }
            match proxy_addr.parse::<Url>(){
                Ok(url) => {
                    if let Ok(mac_addr) = unsafe { CStr::from_ptr(mac_addr).to_str() } {
                        if let Ok(mac) = mac_addr.parse::<Mac>() {
                            let rt = match tokio::runtime::Runtime::new() {
                                Ok(rt) => rt,
                                Err(_) => return QuicError::RuntimeError,
                            };
                            match rt.block_on(run(
                                server_host,server_port as u16,std::sync::Arc::new(url),mac
                            )) {
                                Ok(_) => QuicError::Success,
                                Err(_) => QuicError::ConnectionFailed,
                            }
                        } else {
                            QuicError::InvalidArgs
                        }
                    } else {
                        QuicError::InvalidArgs
                    }
                },
                Err(_) => return QuicError::InvalidArgs,
            }
        } else {
            QuicError::InvalidArgs
        }
    } else {
        QuicError::InvalidArgs
    }
}