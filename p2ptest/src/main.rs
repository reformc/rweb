#[tokio::main(flavor = "current_thread")]
async fn main() {
    p2ptest::quic_server::run(5678).await;
    println!("Hello, world!");
}
