pub mod mac;
pub mod io;
pub use io::header::{get_header,Header};

#[derive(Debug,Clone)]
pub struct RwebError{
    pub code:i32,
    pub msg:String,
}