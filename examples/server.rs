use log::info;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;
use xtr::{
    PackedValues, Packet, PacketFlags, Server, ServerEvent, SessionHandler, SessionId, SessionState,
};

struct MyHandler;

impl SessionHandler for MyHandler {
    fn on_packet(&self, _ssid: &SessionId, _packet: Arc<Packet>) {
        info!("RX PKT");
    }
    fn on_state(&self, _ssid: &SessionId, _state: SessionState) {}
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
    let server = Arc::new(Server::new("127.0.0.1:9900", handler).await);
    let _r = server.start().await;
    {
        let server = Arc::clone(&server);
        std::thread::spawn(move || loop {
            let mut pv = PackedValues::new();
            pv.put_i16(0x0001, -1234);
            pv.put_i32(0x0001, -5678);
            let pkt = Packet::with_packed_values(pv, PacketFlags::empty(), 1);
            server.blocking_send(ServerEvent::Packet {
                packet: Arc::new(pkt),
                ssid: None,
            });
            std::thread::sleep(Duration::from_millis(10));
        });
    }
    let mut s = String::new();
    match std::io::stdin().read_line(&mut s) {
        Ok(_) => {}
        Err(_err) => {}
    }
    info!("Stopped!");
    let _r = server.stop().await;
    info!("Stop Okay");
    Ok(())
}
