pub mod header;
pub mod peek_stream;
pub mod stream_copy;

pub trait ResetHeader {
    fn reset_header(&mut self, header: super::Header);
    fn peek_header(&mut self)->impl std::future::Future<Output = Result<super::Header, crate::RwebError>> + Send;
    fn peek_remove(&mut self);
    fn read_mac(&mut self)->impl std::future::Future<Output = Result<crate::mac::Mac, crate::RwebError>> + Send;
}