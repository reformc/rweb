

use std::{pin::Pin, task::{Context, Poll}};
use crate::{Header, RwebError};
use tokio::io::{AsyncReadExt, ReadBuf, AsyncRead, AsyncWrite};

pub struct PeekableStream<T: AsyncRead + AsyncWrite + Unpin> {
    inner: T,
    peek_buf: Vec<u8>,
}

impl <R: AsyncRead + AsyncWrite + Unpin + Send>super::ResetHeader for PeekableStream<R> {
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
                    let n = self.inner.read(&mut temp).await.map_err(|e| RwebError::new(501,e))?;
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
        self.inner.read_exact(&mut buf).await.map_err(|e| RwebError::new(402,e))?;
        Ok(buf.into())
    }

    fn peek_remove(&mut self) {
        self.peek_buf.clear();
    }
}

impl<T: AsyncRead + AsyncWrite + Unpin> PeekableStream <T> {
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            peek_buf: Vec::new(),
        }
    }
    
    pub async fn peek(&mut self, buf: &mut [u8]) -> tokio::io::Result<usize> {
        if self.peek_buf.is_empty() {
            let mut temp = [0; 1];
            let n = self.inner.read(&mut temp).await?;
            if n > 0 {
                self.peek_buf.extend_from_slice(&temp[..n]);
            }
        }
        
        let len = std::cmp::min(buf.len(), self.peek_buf.len());
        buf[..len].copy_from_slice(&self.peek_buf[..len]);
        Ok(len)
    }

    pub async fn peek_header(&mut self)->Result<Header,RwebError>{
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
                    let n = self.inner.read(&mut temp).await.map_err(|e| RwebError::new(501,e))?;
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

    pub async fn peek_header_set(&mut self,k:String,v:String)->Result<(),RwebError>{
        let mut header:Header = self.peek_buf.clone().try_into()?;
        header.set(k, v);
        self.peek_buf = header.into();
        Ok(())
    }

    pub async fn peek_header_remove(&mut self,k:&str)->Result<(),RwebError>{
        let mut header:Header = self.peek_buf.clone().try_into()?;
        header.remove(k);
        self.peek_buf = header.into();
        Ok(())
    }

    pub async fn peek_header_insert(&mut self,k:String,v:String)->Result<(),RwebError>{
        let mut header:Header = self.peek_buf.clone().try_into()?;
        header.set(k, v);
        self.peek_buf = header.into();
        Ok(())
    }
}

impl <T: AsyncRead + AsyncWrite + Unpin> AsyncRead for PeekableStream<T> {
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
        Pin::new(&mut this.inner).poll_read(cx, buf)
    }
}

impl <T: AsyncRead + AsyncWrite + Unpin> AsyncWrite for PeekableStream<T> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.get_mut().inner).poll_write(cx, buf)
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        let inner = unsafe { self.map_unchecked_mut(|s| &mut s.inner) };
        inner.poll_flush(cx)
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        let inner = unsafe { self.map_unchecked_mut(|s| &mut s.inner) };
        inner.poll_shutdown(cx)
    }
}