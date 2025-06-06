use std::collections::HashMap;
use serde::{ser::SerializeStruct, Deserialize, Serialize};

#[derive(Debug,Clone)]
pub struct ProxyList{
    pub mac:super::mac::Mac,
    pub url:url::Url,
}

impl<'de> Deserialize<'de> for ProxyList{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s:HashMap<&'de str,&'de str> = HashMap::deserialize(deserializer)?;
        let mac = s.get("mac").ok_or(serde::de::Error::custom("missing mac"))?.to_string();
        let addr = s.get("url").ok_or(serde::de::Error::custom("missing addr"))?.to_string();
        Ok(Self{
            mac:mac.parse().map_err(|_|serde::de::Error::custom("invalid mac"))?,
            url:url::Url::parse(&addr).map_err(|_|serde::de::Error::custom("invalid url"))?,
        })
    }
}

impl Serialize for ProxyList{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {        
        let mut state = serializer.serialize_struct("ProxyList", 2)?;
        state.serialize_field("mac", &self.mac.to_string())?;
        state.serialize_field("addr", &self.url.to_string())?;
        state.end()
    }
}