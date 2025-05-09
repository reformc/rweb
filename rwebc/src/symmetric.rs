use std::net::SocketAddr;
use tokio::time::{timeout, Duration};
use common::RwebError;
use quinn::{Connection, Endpoint};

pub async fn symmetric_connect(endpoint:Endpoint,addr:SocketAddr,port_len:u16,cell_timeout:Duration)->Result<Connection,RwebError>{
    let mut joins = vec![];
    for i in 0..port_len{
        let port = 
        match addr.port(){
            0..1025=>{
                if 65535 - addr.port() < i{addr.port() + i}else{addr.port() + i}//一般不允许小雨1024
            },
            1025..16384=>{
                if 65535 - addr.port() < i{addr.port() - 1025 + i}else{addr.port() + i}//可能是自定义的
            },
            13684..32768=>{
                if 32767 - addr.port() < i{addr.port() - 16384 + i}else{addr.port() + i}//家用设备
            },
            32768..49152=>{
                if 60999 - addr.port() < i{addr.port() - 32768 + i}else{addr.port() + i}//可能是自定义的
            },
            49152..60100=>{
                //if 60999 - addr.port() < i{addr.port() - 32767 + i}else{addr.port() + i}//linux默认                
                if 65535 - addr.port() < i{addr.port() - 49152 + i}else{addr.port() + i}//RFC 6056，windows默认
            },
            60100..=65535=>{
                if 65535 - addr.port() < i{addr.port() - 49152 + i}else{addr.port() + i}//RFC 6056，windows默认
            }
        };
        joins.push(Box::pin(p2p_cell(endpoint.clone(), SocketAddr::new(addr.ip(), port),cell_timeout)));
    }
    let a = futures::future::select_all(joins).await;
    a.0
}

async fn p2p_cell(endpoint:Endpoint,addr:SocketAddr,tmo:Duration)->Result<Connection,RwebError>{
    let p2p_conn = endpoint.connect(addr, "reform").map_err(|e|RwebError::new(5032,e))?;
    match timeout(tmo,p2p_conn).await{
        Ok(Ok(p2p_conn)) => {
            Ok(p2p_conn)
        },
        Ok(Err(_e)) => {
            tokio::time::sleep(tmo).await;
            Err(RwebError::new(5033,_e))
        },
        Err(_e) => {
            tokio::time::sleep(tmo).await;
            Err(RwebError::new(5034,"timeout".to_string()))
        }   
    }
}