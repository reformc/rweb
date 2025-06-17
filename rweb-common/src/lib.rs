pub mod mac;
pub mod io;
pub mod proxy_list;
#[cfg(feature="p2p")]
pub mod p2p_list;
use std::error::Error;
pub use io::header::{get_header,Header};
pub mod key;

#[derive(Debug,Clone)]
pub struct RwebError{
    pub code:i32,
    pub msg:String,
}

impl Error for RwebError{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

impl std::fmt::Display for RwebError{
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{
        write!(f,"code:{},msg:{}",self.code,self.msg)
    }
}

unsafe impl Send for RwebError{}
unsafe impl Sync for RwebError{}

impl RwebError{
    pub fn new<T:std::fmt::Display>(code:i32,msg:T)->Self{
        Self{code, msg:msg.to_string()}
    }
}