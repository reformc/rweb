use std::{pin::Pin, task::{Context, Poll}};
use tokio::io::{ReadBuf, AsyncRead, AsyncWrite, AsyncReadExt};
use crate::{Header, RwebError};

pub struct Stream<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>{
    send: R,
    recv: W,
    peek_buf: Vec<u8>,
}

impl<R: AsyncRead + Unpin, W: AsyncWrite + Unpin> Stream<R,W>{
    pub fn new((recv,send):(W,R))->Self{
        Self{send,recv,peek_buf:Vec::new()}
    }
}

impl <R:AsyncRead + Unpin + Send, W:AsyncWrite + Unpin + Send> super::ResetHeader for Stream<R,W> {
    fn reset_header(&mut self, header: Header) {
        self.peek_buf = header.into();
    }

    async fn peek_header(&mut self)->Result<crate::Header, crate::RwebError> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&self.peek_buf);
        loop{
            if buf.ends_with(b"\r\n\r\n"){
                break
            }else{
                if buf.len() > 256*256{
                    panic!("header too long")
                }else{
                    let mut temp = [0; 1];
                    let n = self.send.read(&mut temp).await.map_err(|e| RwebError::new(501,e))?;
                    if n > 0 {
                        buf.extend_from_slice(&temp[..n]);
                        self.peek_buf.extend_from_slice(&temp[..n]);
                    }else{
                        break
                    }
                }
            }
        }
        buf.try_into()        
    }

    async fn read_mac(&mut self)->Result<crate::mac::Mac, crate::RwebError> {
        let mut buf = [0x00;6];
        self.send.read_exact(&mut buf).await.map_err(|e| RwebError::new(402,e))?;
        Ok(buf.into())
    }

    fn peek_remove(&mut self) {
        self.peek_buf.clear();
    }
}

impl <R:AsyncRead + Unpin, W:AsyncWrite + Unpin> AsyncRead for Stream<R,W>{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        let this = self.get_mut();
        if !this.peek_buf.is_empty() {
            let len = std::cmp::min(buf.remaining(), this.peek_buf.len());
            buf.put_slice(&this.peek_buf[..len]);
            this.peek_buf.drain(..len);
            return Poll::Ready(Ok(()));
        }
        Pin::new(&mut this.send).poll_read(cx, buf)
    }
}

impl<R:AsyncRead + Unpin, W:AsyncWrite + Unpin>   AsyncWrite for Stream<R,W> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.get_mut().recv).poll_write(cx, buf)
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        let inner = unsafe { self.map_unchecked_mut(|s| &mut s.recv) };
        inner.poll_flush(cx)
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        let inner = unsafe { self.map_unchecked_mut(|s| &mut s.recv) };
        inner.poll_shutdown(cx)
    }
}