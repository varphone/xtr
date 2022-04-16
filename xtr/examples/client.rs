use log::info;
use std::sync::Arc;
use xtr::{Client, ClientHandler, ClientState, Packet};

struct MyHandler;

impl ClientHandler for MyHandler {
    fn on_packet(&self, packet: Arc<Packet>) {
        info!("RX PKT");
    }
    fn on_state(&self, state: ClientState) {}
}

fn main() {
    use env_logger::Builder;
    use log::LevelFilter;

    let mut builder = Builder::from_default_env();

    builder
        // .format(|buf, record| writeln!(buf, "{} - {}", record.level(), record.args()))
        .filter(None, LevelFilter::Info)
        .init();

    let handler = Arc::new(MyHandler {});
    let mut client = Client::new("192.168.2.252:9900", handler);
    client.start();
    let mut s = String::new();
    match std::io::stdin().read_line(&mut s) {
        Ok(_) => {}
        Err(err) => {}
    }
    info!("Stopped!");
    client.stop();
}
