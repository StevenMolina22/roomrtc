use room_rtc::logger::Logger;
use room_rtc::config::Config;
use room_rtc::clock::Clock;
use room_rtc::media::audio::Microphone;
use room_rtc::media::audio::speaker::Speaker;
use room_rtc::media::audio::{AudioEncoder, AudioDecoder};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use std::path::Path;

fn main() {
    println!("--- TEST DE INTEGRACIÓN DE AUDIO ---");
    println!("Objetivo: Validar Micrófono Inteligente -> Codec -> Parlante Inteligente");
    
    // 1. Setup Mock
    // Logger::new devuelve Result, usamos unwrap()
    let logger = Logger::new("test_audio.log").expect("No se pudo crear logger");
    
    // Como Config::default() no existe, intentamos cargar uno real O creamos un pánico controlado.
    // Para este test, necesitamos una config válida. 
    // Opción A: Cargar el archivo real
    let config = match Config::load(Path::new("room_rtc.conf")) {
        Ok(c) => Arc::new(c),
        Err(_) => {
            println!("⚠️ No encontré room_rtc.conf, el test fallará si Microphone usa la config.");
            // Si no tenés el archivo, esto va a paniquear. Asegurate de tener room_rtc.conf en la raíz.
            panic!("Necesitas un archivo room_rtc.conf para correr este test");
        }
    };
    
    let clock = Arc::new(Clock::new());

    // 2. Iniciar Componentes
    println!("[1/4] Iniciando Micrófono...");
    let mut mic = Microphone::new(clock.clone(), config.clone(), logger.clone());
    let rx_mic = mic.start().expect("Falló al iniciar Micrófono");

    println!("[2/4] Iniciando Parlante...");
    let speaker = Speaker::new(logger.clone(), config.media.audio_sample_rate).expect("Falló al iniciar Parlante");

    println!("[3/4] Iniciando Codec Opus...");
    let mut encoder = AudioEncoder::new().expect("Fallo inicializar el encoder");
    let mut decoder = AudioDecoder::new().expect("Fallo iniciar el decoder");

    println!("[4/4] Iniciando Bucle (Loopback)...");
    println!(">>> HABLA AHORA. Deberías escucharte con un leve retraso.");
    println!(">>> Presiona Ctrl+C para salir.");

    // 3. Hilo de procesamiento (Simula lo que haría el MediaPipeline)
    thread::spawn(move || {
        for frame in rx_mic {
            // A. CODIFICAR (Simula envío)
            match encoder.encode(frame) {
                Ok(encoded_bytes) => {
                    // B. DECODIFICAR (Simula recepción)
                    match decoder.decode(&encoded_bytes) {
                        Ok(decoded_samples) => {
                            // C. REPRODUCIR
                            speaker.play(decoded_samples);
                        },
                        Err(e) => println!("Error Decode: {:?}", e),
                    }
                },
                Err(e) => println!("Error Encode: {:?}", e),
            }
        }
    });

    // Mantener el programa vivo
    loop {
        thread::sleep(Duration::from_secs(1));
    }
}