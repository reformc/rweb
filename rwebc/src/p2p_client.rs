use crate::{symmetric, AsyncReadWrite};
use rweb_common::{io::{header::{get_header, write_header, Header},stream_copy}, mac::Mac, p2p_list::P2pCell, RwebError};
use quinn::{Endpoint, Connection};
use std::net::SocketAddr;
use tokio::{net::TcpListener, io::AsyncWriteExt, time::Duration};

pub trait Accept{
    fn accept(&self)->impl Future<Output = Result<Box<dyn AsyncReadWrite+Send>, RwebError>> + Send;
}

pub trait P2pListen: Send{    
    type A: Accept + Send;
    fn listen(&self)->impl Future<Output = Result<Self::A, RwebError>> + Send;
    fn mac(&self)->Mac;
}

impl Accept for TcpListener{
    async fn accept(&self)->Result<Box<dyn AsyncReadWrite+Send>, RwebError> {
        let (stream, _addr) = self.accept().await.map_err(|e|RwebError::new(5030,e))?;
        Ok(Box::new(stream))
    }
}

impl P2pListen for P2pCell{
    type A = TcpListener;
    async fn listen(&self)->Result<TcpListener, RwebError> {
        let listener = TcpListener::bind(("0.0.0.0",self.port)).await.map_err(|e|RwebError::new(5031,e))?;
        Ok(listener)
    }
    fn mac(&self)->Mac {
        self.mac.clone()
    }
}

pub trait P2PListener<K: PartialEq + Clone,L: P2pListen + Send>: Send + Sync + Unpin + Clone + 'static{
    fn new_listener(&self,key:K)->Result<L, RwebError>;
    fn list(&self)->Vec<K>;
}

#[derive(Debug, Clone)]
pub struct DiyTcpListener{
    pub list:Vec<P2pCell>,
}

impl P2PListener<u16,P2pCell> for DiyTcpListener{
    fn new_listener(&self,key:u16)->Result<P2pCell, RwebError> {
        if let Some(cell) = self.list.iter().find(|cell| cell.port == key){
            Ok(cell.clone())
        }else{
            Err(RwebError::new(54,"port not found".to_string()))
        }
    }

    fn list(&self)->Vec<u16> {
        self.list.iter().map(|cell| cell.port).collect()
    }
}

pub async fn p2p_connect<K: PartialEq + Clone,L: P2pListen + 'static, D: P2PListener<K,L>>(endpoint:Endpoint,connection:Connection,listeners:D)->Result<(),RwebError>{    
    #[cfg(feature="log")]
    println!("p2p client connect");
    let mut p2p_threads = vec![];
    for p in listeners.list(){
        if let Ok(l) = listeners.new_listener(p){
            let endpoint = endpoint.clone();
            let connection = connection.clone();
            p2p_threads.push(Box::pin(p2p_cell(l, endpoint, connection)));
        }
    }
    let (result, _index, _remaining) = futures::future::select_all(p2p_threads).await;
    match result{
        Ok(_) => {
            #[cfg(feature="log")]
            println!("p2p client connect success");
        },
        Err(e) => {
            #[cfg(feature="log")]
            println!("p2p client connect timeout");
            return Err(RwebError::new(55,e))
        }
    }
    result
}

async fn p2p_cell<L:P2pListen>(l:L,endpoint:Endpoint,connection:Connection)->Result<(),RwebError>{
    let mac = l.mac();
    #[cfg(feature="log")]
    println!("p2p client listen mac:{}",mac);
    let (mut send_stream,mut recv_stream) = connection.open_bi().await.map_err(|e|RwebError{code:-19,msg:e.to_string()})?;
    #[cfg(feature="p2ptest")]
    let test_addr = p2ptest(endpoint.clone()).await.ok();
    #[cfg(not(feature="p2ptest"))]
    let test_addr = None;
    write_header(Header::new_p2p(mac,SocketAddr::from(([0,0,0,0],0)),test_addr), &mut send_stream).await.map_err(|e|RwebError{code:-20,msg:e.to_string()})?;
    #[cfg(feature="log")]
    println!("p2p client send header mac:{}",mac);
    let header =  get_header(& mut recv_stream).await.map_err(|e|RwebError{code:-21,msg:e.to_string()})?;
    let (_mac,addr,_self_addr) = header.parse_p2p().map_err(|e|RwebError{code:-22,msg:e.to_string()})?;
    let mut port_len = 1;//如果一个端口都不给，会报错
    #[cfg(all(feature="p2ptest",feature="log"))]
    if let Some(test_addr) = test_addr{
        if let Some(self_addr) = _self_addr{
            if self_addr != test_addr{
                println!("当前节点处于受限锥形NAT网络(对称NAT),无法直接打洞,{} != {}",self_addr,test_addr);
            }else{
                println!("当前节点处于全锥形NAT网络,可以直接打洞,{} == {}",self_addr,test_addr);
            }
        }
    }
    match header.get("Nat-Type").map(|s|s.as_str()){
        Some("FullCone")=>{
            #[cfg(feature="log")]
            println!("对端处于全锥形NAT网络,可以直接打洞,{}",addr);
        },
        Some("Symmetric")=>{
            port_len = crate::PORT_LEN;
            #[cfg(feature="log")]
            println!("对端处于对称NAT网络,无法直接打洞，尝试后延{}个端口,{}",port_len,addr);
        },
        _=>{}
    }
    #[cfg(feature="log")]
    println!("p2p connect mac:{},addr:{}",_mac,addr);
    for _ in 0..5{
        match symmetric::symmetric_connect(endpoint.clone(),addr,port_len,Duration::from_secs(3)).await{
            Ok(conn_result) => {
                #[cfg(feature="log")]
                println!("p2p connect success,addr:{}",addr);
                let listener = l.listen().await.map_err(|e|RwebError{code:-19,msg:e.to_string()})?;
                loop{
                    if let Ok(accept_stream) = listener.accept().await{
                        let p2p_conn = conn_result.clone();
                        tokio::spawn(async move{
                            p2p_stream_cell(accept_stream, mac, p2p_conn).await.unwrap_or_default();
                        });
                    }else{
                        break
                    }
                }
                return Ok(())
            },
            Err(_e) => {
                #[cfg(feature="log")]
                println!("p2p connect {} error:{:?}",addr,_e);
            }
        }
    }
    Err(RwebError::new(56,"p2p connect timeout".to_string()))
}

async fn p2p_stream_cell(mut accept_stream:impl AsyncReadWrite,mac:Mac,connection:Connection)->Result<(),RwebError>{
    let p2p_stream = connection.open_bi().await.map_err(|e|RwebError::new(52,e))?;//这里出错的话直接退出。
    let mut p2p_stream = stream_copy::Stream::new(p2p_stream,connection.remote_address());
    p2p_stream.write_all(mac.as_ref()).await.unwrap_or_default();//在头部插入mac地址，所有主动向节点发送的bi流的第一个数据包都需要首先发送mac地址以便node得知使用哪条流来对接。
    tokio::io::copy_bidirectional(&mut accept_stream, &mut p2p_stream).await.unwrap_or_default();
    Ok(())
}

#[cfg(feature="p2ptest")]
pub async fn p2ptest(endpoint:Endpoint)->Result<SocketAddr,RwebError>{
    use std::net::ToSocketAddrs;
    use common::io::header::read_addr;
    use quinn::VarInt;
    let test_server_addr = include_str!("../../p2ptest_addr.txt").to_socket_addrs().map_err(|e|RwebError::new(6031,e))?.next().ok_or(RwebError::new(5032,"addr error".to_string()))?;
    let conn = endpoint.connect(test_server_addr, "reform").map_err(|e|RwebError::new(5032,e))?.await.map_err(|e|RwebError::new(5033,e))?;
    let mut uni = conn.accept_uni().await.unwrap();
    let addr = read_addr(&mut uni).await.unwrap();
    conn.close(VarInt::from_u32(200), "Ok".as_bytes());
    Ok(addr)
}