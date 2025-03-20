use log::info;
use std::error::Error;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use xtr::{Client, ClientEvent, ClientHandler, ClientState, PackedValues, Packet, PacketFlags};

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

    builder
        // .format(|buf, record| writeln!(buf, "{} - {}", record.level(), record.args()))
        .filter(None, LevelFilter::Trace)
        .init();

    let handler = Arc::new(MyHandler {});
    let client = Arc::new(Mutex::new(Client::new("127.0.0.1:9900", handler)));
    let _r = client.lock().unwrap().start().await;
    {
        let client = Arc::clone(&client);
        std::thread::spawn(move || loop {
            let mut pv = PackedValues::new();
            pv.put_u32(0x0001, 5000);
            pv.put_u32(0x0002, 50);
            pv.put_u32(0x0003, 1);
            pv.put_u32(0x0004, 1);
            let pkt = Packet::with_packed_values(pv, PacketFlags::empty(), 1);
            client
                .lock()
                .unwrap()
                .send(ClientEvent::Packet(Arc::new(pkt)));
            std::thread::sleep(Duration::from_millis(10));
        });
    }
    let mut s = String::new();
    match std::io::stdin().read_line(&mut s) {
        Ok(_) => {}
        Err(_err) => {}
    }
    info!("Stopped!");
    let _r = client.lock().unwrap().stop().await;
    Ok(())
}
