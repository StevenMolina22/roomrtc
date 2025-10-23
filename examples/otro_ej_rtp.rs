use std::net::SocketAddr;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use roomrtc::rtp::rtp_communicator::{RtpReceiver, RtpSender};
use roomrtc::rtp::rtp_package::RtpPackage;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Elegimos un puerto para el receptor
    let recv_port = 5004;

    // Inicializamos el receptor
    let mut receiver = RtpReceiver::new(recv_port)?;

    // Construimos la dirección de destino para el sender
    let dest: SocketAddr = format!("127.0.0.1:{}", recv_port).parse()?;
    let mut sender = RtpSender::new(dest, 12345)?;

    // Enviamos 10 paquetes en un hilo separado
    thread::spawn(move || {
        for i in 0..10 {
            let payload = format!("Paquete {}", i).into_bytes();
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u32;
            sender.send(&payload, 96, timestamp, true).unwrap();
            thread::sleep(Duration::from_millis(200)); // 200ms entre paquetes
        }
    });

    // Receptor intenta leer paquetes durante 3 segundos
    let start = SystemTime::now();
    while SystemTime::now().duration_since(start)?.as_secs() < 3 {
        if let Some(pkg) = receiver.try_receive()? {
            let text = String::from_utf8_lossy(&pkg.payload);
            println!(
                "Recibido seq={} ts={} payload={}",
                pkg.sequence_number, pkg.timestamp, text
            );
        }
        thread::sleep(Duration::from_millis(50));
    }

    Ok(())
}
