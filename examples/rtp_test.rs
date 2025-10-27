use roomrtc::rtp::rtp_peer::{RtpReceiver, RtpSender};
use std::net::SocketAddr;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dest: SocketAddr = "127.0.0.1:5004".parse().unwrap();
    let mut sender = RtpSender::new(dest, 1234)?;

    let mut receiver = RtpReceiver::new(5004)?;

    // Enviar un paquete RTP “falso”
    sender.send(b"Hola RTP", 96, 0, true)?;

    // Recibir paquetes (demo: intentar una vez y salir si no hay datos)
    if let Some(pkg) = receiver.try_receive()? {
        // payload is private; print the whole package instead or use a public accessor if provided by the API
        println!("Recibido: {:?}", pkg);
    } else {
        println!("No hay paquetes disponibles (non-blocking)");
    }

    Ok(())
}
