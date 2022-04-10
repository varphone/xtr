use log::info;
use std::sync::Arc;
use xtr::{Client, ClientHandler, ClientState, Packet};

struct MyHandler;

impl ClientHandler for MyHandler {
    fn on_packet(&self, packet: Arc<Packet>) {}
    fn on_state(&self, state: ClientState) {}
}

fn main() {
    env_logger::init();

    let handler = Arc::new(MyHandler {});
    let mut client = Client::new("127.0.0.1:9900", handler);
    client.start();
    let mut s = String::new();
    match std::io::stdin().read_line(&mut s) {
        Ok(_) => {}
        Err(err) => {}
    }
    info!("Stopped!");
    client.stop();
}
