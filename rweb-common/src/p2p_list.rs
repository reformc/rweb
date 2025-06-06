use std::collections::HashMap;
use serde::{ser::SerializeStruct, Deserialize, Serialize};

#[derive(Debug,Clone)]
pub struct P2pCell{
    pub mac:super::mac::Mac,
    pub port:u16,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StringOrInt<'a> {
    Str(&'a str),
    Int(u16),
}

impl<'de> Deserialize<'de> for P2pCell{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s:HashMap<&'de str,StringOrInt> = HashMap::deserialize(deserializer)?;
        let mac = match s.get("mac").ok_or(serde::de::Error::custom("missing mac"))?{
            StringOrInt::Str(mac) => mac.to_string(),
            StringOrInt::Int(_) => return Err(serde::de::Error::custom("mac should be string")),
        };
        let port = match s.get("port").ok_or(serde::de::Error::custom("missing port"))?{
            StringOrInt::Str(_) => return Err(serde::de::Error::custom("port should be u16")),
            StringOrInt::Int(port) => *port,
        };
        Ok(Self{
            mac:mac.parse().map_err(|_|serde::de::Error::custom("invalid mac"))?,
            port,
        })
    }
}

impl Serialize for P2pCell{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {        
        let mut state = serializer.serialize_struct("P2pCell", 2)?;
        state.serialize_field("mac", &self.mac.to_string())?;
        state.serialize_field("port", &self.port)?;
        state.end()
    }
}