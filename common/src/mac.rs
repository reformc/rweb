use std::str::FromStr;

#[derive(Debug,Copy,Clone,Eq,PartialEq,Hash)]
pub struct Mac {
    mac:[u8;6]
}

impl AsRef<[u8]> for Mac{
    fn as_ref(&self)->&[u8]{
        &self.mac
    }
}

impl std::fmt::Display for Mac{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f,"{}",self.mac.iter().map(|x|format!("{:02x}",x)).collect::<String>())
    }
}

impl From<[u8;6]> for Mac{
    fn from(mac:[u8;6])->Self{
        Self{mac}
    }
}

impl From<Mac> for [u8;6]{
    fn from(mac:Mac)->[u8;6]{
        mac.mac
    }
}

impl FromStr for Mac{
    type Err = Box<dyn std::error::Error+Send+Sync>;
    fn from_str(s:&str)->Result<Self,Self::Err>{
        let mac = s.replace(":", "").replace("-", "").replace(" ", "").trim().to_uppercase();
        if mac.len() != 12{
            return Err("mac address error".into());
        }else{
            let mut mac_bytes = [0x00;6];
            for i in 0..6{
                match u8::from_str_radix(&mac[i*2..i*2+2], 16){
                    Ok(v)=>mac_bytes[i] = v,
                    Err(e)=>return Err(e.into())
                }
            }
            Ok(mac_bytes.into())
        }
    }
}

impl TryFrom<String> for Mac{
    type Error = Box<dyn std::error::Error+Send+Sync>;
    fn try_from(s:String)->Result<Self,Self::Error>{
        let mac = s.replace(":", "").replace("-", "").replace(" ", "").trim().to_uppercase();
        if mac.len() != 12{
            return Err("mac address error".into());
        }else{
            let mut mac_bytes = [0x00;6];
            for i in 0..6{
                match u8::from_str_radix(&mac[i*2..i*2+2], 16){
                    Ok(v)=>mac_bytes[i] = v,
                    Err(e)=>return Err(e.into())
                }
            }
            Ok(mac_bytes.into())
        }
    }
}

impl TryFrom<&str> for Mac{
    type Error = Box<dyn std::error::Error+Send+Sync>;
    fn try_from(s:&str)->Result<Self,Self::Error>{
        let mac = s.replace(":", "").replace("-", "").replace(" ", "").trim().to_uppercase();
        if mac.len() != 12{
            return Err("mac address error".into());
        }else{
            let mut mac_bytes = [0x00;6];
            for i in 0..6{
                match u8::from_str_radix(&mac[i*2..i*2+2], 16){
                    Ok(v)=>mac_bytes[i] = v,
                    Err(e)=>return Err(e.into())
                }
            }
            Ok(mac_bytes.into())
        }
    }
}

impl Into<String> for Mac{
    fn into(self)->String{
        self.mac.iter().map(|x|format!("{:02x}",x)).collect::<String>()
    }
}