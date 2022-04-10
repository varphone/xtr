use log::info;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use xtr::{Client, ClientEvent, ClientHandler, ClientState, PackedValues, Packet, PacketFlags};

struct MyHandler;

impl ClientHandler for MyHandler {
    fn on_packet(&self, packet: Arc<Packet>) {
        info!("RX PKT");
    }
    fn on_state(&self, state: ClientState) {
        info!("STATE {:?}", state);
    }
}

fn main() {
    use env_logger::Builder;
    use log::LevelFilter;

    let mut builder = Builder::from_default_env();

    builder
        // .format(|buf, record| writeln!(buf, "{} - {}", record.level(), record.args()))
        .filter(None, LevelFilter::Trace)
        .init();

    let handler = Arc::new(MyHandler {});
    let mut client = Arc::new(Mutex::new(Client::new("127.0.0.1:9900", handler)));
    client.lock().unwrap().start();
    {
        let client = Arc::clone(&client);
        std::thread::spawn(move || loop {
            let mut pv = PackedValues::new();
            pv.put_i16(0x0001, -1234);
            pv.put_i32(0x0001, -5678);
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
        Err(err) => {}
    }
    info!("Stopped!");
    client.lock().unwrap().stop();
}
