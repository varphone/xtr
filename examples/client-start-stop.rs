use log::info;
use std::error::Error;
use std::sync::{Arc, Mutex};
use xtr::{Client, ClientHandler, ClientState, Packet};

struct MyHandler;

impl ClientHandler for MyHandler {
    fn on_packet(&self, _packet: Arc<Packet>) {
        info!("RX PKT");
    }
    fn on_state(&self, state: ClientState) {
        info!("STATE {:?}", state);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    use env_logger::Builder;
    use log::LevelFilter;

    let mut builder = Builder::from_default_env();

    builder.filter(None, LevelFilter::Trace).init();

    for _a in 0..1000 {
        let handler = Arc::new(MyHandler {});
        let client = Arc::new(Mutex::new(Client::new("192.168.2.127:6600", handler)));
        let _r = client.lock().unwrap().start().await;
        let _r = client.lock().unwrap().stop().await;
    }
    Ok(())
}
