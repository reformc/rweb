use std::os::raw::{c_int,c_char};

fn main(){
    #[cfg(target_os = "windows")]
    let lib_path = "target/release/rwebc.dll";
    #[cfg(target_os = "linux")]
    let lib_path = "target/release/librwebc.so";
    #[cfg(target_os = "macos")]
    let lib_path = "target/release/librwebc.dylib";
    let ret = unsafe{
        let lib = libloading::Library::new(lib_path).unwrap();
        let func: libloading::Symbol<unsafe extern "C" fn(*const c_char, c_int, *const c_char, *const c_char) -> i32> = lib.get(b"quic_client_run").unwrap();
        func(
            "127.0.0.1\0".as_ptr() as *const c_char,
            5677,
            "192.168.31.121:80\0".as_ptr() as *const c_char,
            "aabbccddeeff\0".as_ptr() as *const c_char,
        )
    };
    println!("ret: {}", ret);
}