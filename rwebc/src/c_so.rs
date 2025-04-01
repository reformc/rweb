use std::ffi::CStr;
use std::os::raw::{c_int,c_char};
use common::mac::Mac;
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
    let server_host = unsafe { CStr::from_ptr(server_host)}.to_str().unwrap();
    let proxy_addr = unsafe { CStr::from_ptr(proxy_addr)}.to_str().unwrap();
    let proxy_addr = std::sync::Arc::new(proxy_addr.to_string());
    let server_port = server_port as u16;
    let mac = unsafe { CStr::from_ptr(mac_addr)}.to_str().unwrap();
    let mac:Mac = mac.try_into().unwrap();

    // 创建运行时
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(_) => return QuicError::RuntimeError,
    };

    // 执行异步代码
    match rt.block_on(run(
        server_host,server_port,proxy_addr,mac
    )) {
        Ok(_) => QuicError::Success,
        Err(_) => QuicError::ConnectionFailed,
    }
}