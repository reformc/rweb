use std::{collections::HashMap, net::SocketAddr};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
#[cfg(feature="p2p")]
use std::str::FromStr;

use crate::{mac::Mac, RwebError};

pub const METHOD_P2P:&str = "P2P";
pub const METHOD_P2PTEST:&str = "P2PTEST";

pub enum UniCommand{
    MacList = 0x00,
    Addr = 0x01
}

impl From<UniCommand> for u8{
    fn from(cmd:UniCommand)->u8{
        match cmd{
            UniCommand::MacList => 0x00,
            UniCommand::Addr => 0x01
        }
    }
}

impl TryFrom<u8> for UniCommand{
    type Error = RwebError;
    fn try_from(cmd:u8)->Result<Self,Self::Error>{
        match cmd{
            0x00 => Ok(UniCommand::MacList),
            0x01 => Ok(UniCommand::Addr),
            _ => Err(RwebError::new(2404,"unknown command"))
        }
    }
}

#[allow(unused)]
#[derive(Debug,Clone)]
pub struct Header{
    pub method:String,
    pub uri:String,
    pub version:String,
    pub header:HashMap<String,String>
}

#[allow(unused)]
impl Header{
    pub fn get<'a>(&'a self,key:&str)->Option<&'a String>{
        self.header.get(key)
    }
    pub fn set(&mut self,key:String,value:String){
        self.header.insert(key,value);
    }

    pub fn remove(&mut self,key:&str)->Option<String>{
        self.header.remove(key)
    }

    pub fn insert(&mut self,key:String,value:String){
        self.header.insert(key,value);
    }

    #[cfg(feature="p2p")]
    pub fn new_p2p(mac:Mac,addr:SocketAddr,self_addr:Option<SocketAddr>)->Self{
        let mut header = HashMap::new();
        header.insert("mac".to_string(), mac.to_string());
        header.insert("addr".into(), addr.to_string());
        if let Some(self_addr) = self_addr{
            header.insert("self_addr".into(), self_addr.to_string());
        }
        Self{
            method:METHOD_P2P.to_string(),
            uri:"/p2p".to_string(),
            version:"HTTP/1.1".to_string(),
            header,
        }
    }

    #[cfg(feature="p2p")]
    pub fn new_p2ptest(mac:Mac,addr:SocketAddr,self_addr:Option<SocketAddr>)->Self{
        let mut header = HashMap::new();
        header.insert("mac".to_string(), mac.to_string());
        header.insert("addr".into(), addr.to_string());
        if let Some(self_addr) = self_addr{
            header.insert("self_addr".into(), self_addr.to_string());
        }
        Self{
            method:METHOD_P2P.to_string(),
            uri:"/p2ptest".to_string(),
            version:"HTTP/1.1".to_string(),
            header,
        }
    }

    #[cfg(feature="p2p")]
    pub fn parse_p2p(&self)->Result<(Mac,SocketAddr,Option<SocketAddr>),RwebError>{
        let mac = Mac::from_str(self.get("mac").ok_or(RwebError::new(2401,"header error"))?).map_err(|e|RwebError::new(2402,e))?;
        let addr = self.get("addr").ok_or(RwebError::new(2403,"header error"))?.parse::<SocketAddr>().map_err(|e|RwebError::new(61,e))?;
        let self_addr = self.get("self_addr").map(|s|s.parse::<SocketAddr>().map_err(|e|RwebError::new(62,e))).transpose()?;
        Ok((mac,addr,self_addr))
    }
}

impl TryFrom<Vec<u8>> for Header{
    type Error = RwebError;
    fn try_from(buf:Vec<u8>)->Result<Self,Self::Error>{
        let mut header = HashMap::new();
        let mut lines = buf.split(|&b| b == b'\r' || b == b'\n');
        let mut method = "";
        let mut uri = "";
        let mut version = "";
        if let Some(first_line) = lines.next() {
            if let Ok(line) = std::str::from_utf8(first_line) {
                let parts: Vec<&str> = line.split(' ').collect();
                if parts.len() > 2 {
                    method = parts[0];
                    uri = parts[1];
                    version = parts[2];
                    header.insert(parts[0].into(), parts[1].trim().to_string());
                }
            }
        }
        while let Some(line) = lines.next() {
            if let Ok(line) = std::str::from_utf8(line) {
                let parts: Vec<&str> = line.splitn(2,':').collect();//只分割第一个冒号
                if parts.len() > 1 {
                    if parts[0].trim()==method {
                        continue;
                    }
                    header.insert(parts[0].into(), parts[1].trim().to_string());
                }
            }
        }
        if method.is_empty() || uri.is_empty() || version.is_empty() {
            return Err(RwebError::new(2400,"header error"));
        }
        Ok(Self{
            method:method.to_string(),
            uri:uri.to_string(),
            version:version.to_string(),
            header:header
        })
    }
}

impl Into<Vec<u8>> for Header{
    fn into(self)->Vec<u8>{
        let mut buf = Vec::new();
        buf.extend_from_slice(self.method.as_bytes());
        buf.push(b' ');
        buf.extend_from_slice(self.uri.as_bytes());
        buf.push(b' ');
        buf.extend_from_slice(self.version.as_bytes());
        buf.push(b'\r');
        buf.push(b'\n');
        for (key,value) in self.header.iter(){
            buf.extend_from_slice(key.as_bytes());
            buf.push(b':');
            buf.push(b' ');
            buf.extend_from_slice(value.as_bytes());
            buf.push(b'\r');
            buf.push(b'\n');
        }
        buf.push(b'\r');
        buf.push(b'\n');
        buf
    }
}

pub async fn get_header<R: AsyncRead + Unpin>(stream:&mut R)->Result<Header,RwebError>{
    let mut buf = Vec::new();
    let mut header = [0u8; 1];
    loop{ 
        if let Ok(_) = stream.read_exact(&mut header).await{
            buf.push(header[0]);
            if buf.ends_with(b"\r\n\r\n"){
                break
            }else{
                if buf.len() > 256*256{
                    return Err(RwebError::new(500,"header too long"))
                }
            }
        }
    }
    buf.try_into()
}

pub async fn write_header<R: AsyncWrite + Unpin>(header:Header,stream:&mut R)->Result<(),RwebError>{
    let v:Vec<u8> = header.into();
    stream.write_all(&v).await.map_err(|e|RwebError::new(500,e))?;
    Ok(())
}

pub async fn write_addr<S:AsyncWrite+Unpin>(s:&mut S,addr:SocketAddr)->Result<(),RwebError>{
    match addr{
        SocketAddr::V4(addr) => {
            let ip = addr.ip().octets();
            s.write_u8(0x04).await.map_err(|e|RwebError::new(500,e))?;
            s.write(&ip).await.map_err(|e|RwebError::new(500,e))?;
            s.write_all(&addr.port().to_be_bytes()).await.map_err(|e|RwebError::new(500,e))?;
        },
        SocketAddr::V6(addr) => {
            let ip = addr.ip().octets();            
            s.write_u8(0x06).await.map_err(|e|RwebError::new(500,e))?;
            s.write(&ip).await.map_err(|e|RwebError::new(500,e))?;
            s.write_all(&addr.port().to_be_bytes()).await.map_err(|e|RwebError::new(500,e))?;
        }
    }
    Ok(())
}

pub async fn read_addr<S:AsyncRead+Unpin>(s:&mut S)->Result<SocketAddr,RwebError>{
    let version = s.read_u8().await.map_err(|e|RwebError::new(500,e))?;
    //s.read_exact(&mut buf).await.map_err(|e|RwebError::new(500,e))?;
    match version{
        0x04 => {
            let mut ip = [0u8; 4];
            s.read_exact(&mut ip).await.map_err(|e|RwebError::new(501,e))?;
            let mut port = [0u8; 2];
            s.read_exact(&mut port).await.map_err(|e|RwebError::new(502,e))?;
            Ok(SocketAddr::from((ip,u16::from_be_bytes(port))))
        },
        0x06 => {
            let mut ip = [0u8; 16];
            s.read_exact(&mut ip).await.map_err(|e|RwebError::new(503,e))?;
            let mut port = [0u8; 2];
            s.read_exact(&mut port).await.map_err(|e|RwebError::new(504,e))?;
            Ok(SocketAddr::from((ip,u16::from_be_bytes(port))))
        },
        _ => Err(RwebError::new(505,"addr error"))
    }
}

pub async fn write_mac_list<S:AsyncWrite+Unpin>(s:&mut S,macs:&[Mac])->Result<(),RwebError>{
    s.write_u16(macs.len() as u16).await.map_err(|e|RwebError::new(500,e))?;
    for mac in macs{
        s.write_all(mac.as_ref()).await.map_err(|e|RwebError::new(500,e))?;
    }
    Ok(())
}

pub async fn read_mac_list<S:AsyncRead+Unpin>(s:&mut S)->Result<Vec<Mac>,RwebError>{
    let len = s.read_u16().await.map_err(|e|RwebError::new(500,e))?;
    let mut macs = vec![];
    for _ in 0..len{
        let mut buf = [0x00;6];
        s.read_exact(&mut buf).await.map_err(|e|RwebError::new(500,e))?;
        macs.push(buf.into());
    }
    Ok(macs)
}