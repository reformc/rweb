#[tokio::main(flavor = "current_thread")]
async fn main() {    
    //console_subscriber::init();
    simple_logger::init_with_level(log::Level::Info).unwrap();
    rwebs::run().await;
}