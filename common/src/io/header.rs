use std::collections::HashMap;
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::RwebError;

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
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() > 1 {
                    if parts[0].trim()==method {
                        continue;
                    }
                    header.insert(parts[0].into(), parts[1].trim().to_string());
                }
            }
        }
        if method.is_empty() || uri.is_empty() || version.is_empty() {
            return Err(RwebError::new(400,"header error"));
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