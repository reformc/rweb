pub mod quic_client;
//pub mod c_so;
#[cfg(feature="p2p")]
mod p2p_client;
#[cfg(feature="p2p")]
mod symmetric;
//pub use c_so::quic_node_run;
//#[cfg(feature="p2p")]
//pub use c_so::p2pclient;
use tokio::io::{AsyncRead, AsyncWrite};
pub const PORT_LEN: u16 = 1000;

pub trait AsyncReadWrite: AsyncRead + AsyncWrite + Unpin {}

impl<T: AsyncRead + AsyncWrite + Unpin> AsyncReadWrite for T {}