use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::io::Write;
use std::thread;
use std::time::Duration;

#[warn(deprecated)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- INICIANDO DIAGNÓSTICO DE AUDIO ---");

    // 1. Obtener el Host de audio del sistema (ALSA, WASAPI, CoreAudio, etc.)
    let host = cpal::default_host();
    println!("Host de audio: {:?}", host.id());

    // 2. Buscar el dispositivo de entrada por defecto
    let input_device = host.default_input_device()
        .expect("¡ERROR FATAL: No se detectó ningún micrófono!");
    
    println!("Micrófono detectado: {}", input_device.name().unwrap_or("Desconocido".to_string()));

    // 3. Obtener la configuración NATIVA soportada por el hardware
    // Esto es crucial: Nos dirá si tu mic corre a 44100Hz o 48000Hz por defecto.
    let default_config = input_device.default_input_config()?;
    println!("Configuración Nativa del Hardware:");
    println!("  - Canales: {}", default_config.channels());
    println!("  - Sample Rate: {}", default_config.sample_rate());
    println!("  - Formato: {:?}", default_config.sample_format());

    let config: cpal::StreamConfig = default_config.into();

    println!("\nIniciando captura de prueba (5 segundos)...");
    println!("(Si ves puntos '...', están llegando datos. Si no, algo bloquea el mic)");

    // 4. Construir el stream de captura
    // Usamos move para capturar variables si fuera necesario, aunque aquí solo imprimimos.
    let input_stream = input_device.build_input_stream(
        &config,
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            // Imprimimos la cantidad de muestras que llegan en este "paquete" del hardware
            // Usamos print! en lugar de println! para no saturar la consola, y flush para que salga inmediato.
            print!("[{}] ", data.len());
            std::io::stdout().flush().unwrap();
        },
        move |err| {
            eprintln!("\nERROR EN STREAM: {}", err);
        },
        None // Timeout
    )?;

    // 5. Iniciar
    input_stream.play()?;

    // Dejamos correr 5 segundos
    thread::sleep(Duration::from_secs(5));
    println!("\n\n--- TEST FINALIZADO ---");
    
    Ok(())
}