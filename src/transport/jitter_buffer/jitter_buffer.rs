use std::sync::Arc;
use std::thread::sleep;
use std::time::{Duration, Instant};
use crate::transport::rtp::RtpPacket;
use super::rr_metrics::RrMetrics;
use crate::clock::Clock;

const TOLERANCE_MILLIS: u128 = 120;

pub struct JitterBuffer<const N: usize> {
    packets: [Option<RtpPacket>; N],

    read_idx: usize,
    write_idx: usize,

    read_seq: Option<u64>,
    write_seq: Option<u64>,

    i_frame_needed: bool,

    last_frame_completed_timestamp: u128,
    last_deliver_timestamp: u128,

    //metrics: RrMetrics,
    clock: Arc<Clock>,
}

impl<const N: usize> JitterBuffer<N>  {
    pub fn new(clock: Arc<Clock>) -> Self {
        Self {
            packets: std::array::from_fn(|_| None),
            read_idx: 0,
            write_idx: 0,
            read_seq: None,
            write_seq: None,
            i_frame_needed: true,
            last_frame_completed_timestamp: 0,
            last_deliver_timestamp: 0,
            //metrics: RrMetrics::default(),
            clock
        }
    }

    pub(crate) fn add(&mut self, packet: RtpPacket) {
        let seq = packet.sequence_number;
        if (self.i_frame_needed && !packet.is_i_frame)
        {
            println!("Need intra frame. Do not add this packet. SKIP");
            return
        }
        if packet.timestamp < self.last_frame_completed_timestamp
        {
            println!("Packet timestamp to old to show. SKIP");
            return
        }

        if !self.valid_packet_seq_num(seq)
        {
            println!("Packet sequence number {} not in his window to be shown. SKIP", seq);
            return
        }

        let pos = (seq % N as u64) as usize;

        match (self.read_seq, self.write_seq) {
            (Some(read), Some(write)) => {
                if seq < read {
                    println!("Adding late packet to jitter. Setting new read idx");
                    self.read_idx = pos;
                    self.read_seq = Some(seq);
                    self.packets[pos] = Some(packet);
                } else if seq > write {
                    println!("Seq number is higher than write index. Check for overwrite");
                    let old_write_idx = self.write_idx;
                    self.write_idx = pos;
                    self.write_seq = Some(seq);
                    self.packets[pos] = Some(packet);

                    if (self.read_idx <= old_write_idx && pos >= self.read_idx && pos <= old_write_idx)
                        || (self.read_idx > old_write_idx && (pos >= self.read_idx || pos <= old_write_idx)) {
                        println!("OVERWRITE. CLEAR BUFF UNTIL INTRA");
                        self.resync_or_clear();
                    }

                } else if seq > read && seq < write {
                    println!("Adding packet to jitter. Just add data, no re-setting indexes");
                    self.packets[pos] = Some(packet);
                }
            }
            _ => {
                println!("First packet. Empty buffer");
                self.read_seq = Some(seq);
                self.write_seq = Some(seq);
                self.read_idx = pos;
                self.write_idx = pos;
                self.packets[pos] = Some(packet);
                self.i_frame_needed = false;
            }
        }
    }

    pub(crate) fn pop(&mut self) -> Option<Vec<u8>> {
        let ts = match &self.packets[self.read_idx] {
            Some(p) => {
                println!("There is a packet to read in read idx");
                p.timestamp
            },
            None => {
                println!("No packet here. Frame incomplete. SKIP");
                return None
            },
        };

        if !self.valid_playout_time(ts) {
            println!("Frame is no longer able to be shown. Too old. Clear buffer until next iframe or clear entire buff");
            self.resync_or_clear();
            if self.write_seq.is_none() || self.read_seq.is_none() {
                println!("Buffer empty. No seq number. SKIP");
                return None
            }
        };

        let mut idx = self.read_idx;
        let mut frame_data = Vec::new();
        let mut chunks_processed = 0;

        for _ in 0..N {
            println!("INSIDE LOOP. Try to make a frame from packets");
            let packet = match self.packets[idx].clone() {
                Some(p) => p,
                None => return None
            };

            if packet.timestamp != ts {
                println!("The packets have different timestamps. Different frames. SKIP");
                return None;
            }

            frame_data.extend_from_slice(&packet.payload);
            chunks_processed += 1;

            idx = (idx + 1) % N;

            if packet.marker == 1 {
                println!("FOUND END OF FRAME PACKET");
                if chunks_processed == packet.total_chunks as usize {
                    println!("FRAME COMPLETED. Proceed to clean packets");
                    while self.read_idx != idx {
                        self.packets[self.read_idx % N] = None;
                        self.read_idx += 1;
                    }

                    let mut found_next = false;
                    println!("TRY TO FIND NEXT FRAME TO DECODE");
                    for _ in 0..N {
                        if let Some(next_p) = &self.packets[self.read_idx] {
                            println!("FOUND NEXT FRAME");
                            self.read_seq = Some(next_p.sequence_number);
                            found_next = true;
                            break;
                        }
                        self.read_idx = (self.read_idx + 1) % N;
                    }

                    if !found_next {
                        println!("ONLY ONE FRAME IN BUFF. SETTING ALL TO DEFAULT VALUES");
                        self.read_idx = 0;
                        self.write_idx = 0;
                        self.read_seq = None;
                        self.write_seq = None;
                    }

                    if self.last_deliver_timestamp != 0 {
                        let delta_rtp = packet.timestamp - self.last_frame_completed_timestamp;
                        let expected_playout_time_local = self.last_deliver_timestamp + delta_rtp;
                        println!("EXPECTED PLAYOUT TIME {expected_playout_time_local}");
                        let now = self.clock.now();
                        println!("NOW: {now}");
                        let sleep_time = expected_playout_time_local.saturating_sub(now);

                        println!("sleep: {sleep_time}\n");

                        if sleep_time > 0 {
                            sleep(Duration::from_millis(sleep_time as u64));
                        }
                    }


                    self.last_frame_completed_timestamp = packet.timestamp;
                    if packet.is_i_frame {
                        self.i_frame_needed = false
                    }

                    self.last_deliver_timestamp = self.clock.now();
                    return Some(frame_data)

                } else {
                    println!("FRAME INCOMPLETE. SKIP");
                    return None
                }
            }
        }
        None
    }

    fn resync_or_clear(&mut self) {
        let mut idx = self.read_idx;
        let read_timestamp = self.packets[idx].as_ref().unwrap().timestamp;

        for _ in 0..N {
            if let Some(pkt) = &self.packets[idx]
                && pkt.is_i_frame && pkt.timestamp != read_timestamp
            {
                println!("FOUND IFRAME");
                self.read_idx = idx;
                self.read_seq = Some(pkt.sequence_number);
                self.i_frame_needed = false;
                return;
            }
            self.packets[idx] = None;
            idx = (idx + 1) % N;
        }

        println!("WHOLE BUFFER CLEANED");

        self.read_idx = 0;
        self.write_idx = 0;
        self.read_seq = None;
        self.write_seq = None;
        self.i_frame_needed = true;
    }

    fn valid_packet_seq_num(&self, seq_num: u64) -> bool {
        match &self.packets[self.read_idx] {
            Some(read_packet) => {
                let window = (self.read_idx + N - self.write_idx) as u64;
                if window > seq_num  {
                    return true
                }

                let bound = read_packet.sequence_number.wrapping_sub(window);
                seq_num > bound
            },
            None => true,
        }
    }

    fn valid_playout_time(&self, frame_timestamp: u128) -> bool {
        if self.last_deliver_timestamp == 0 {
            println!("primer timestamp");
            return true;
        }

        let delta_rtp = frame_timestamp - self.last_frame_completed_timestamp;
        let expected_playout_time_local = self.last_deliver_timestamp + delta_rtp;
        let expiration_deadline = expected_playout_time_local + TOLERANCE_MILLIS;
        let actual = self.clock.now();
        println!("frame = {frame_timestamp}\nlast completed: {}\nlast delivered: {}\ndelta: {delta_rtp}\nexpected: {expected_playout_time_local}\nexpriration: {expiration_deadline}\nactual: {actual}\n\n", self.last_frame_completed_timestamp, self.last_deliver_timestamp);

        expiration_deadline >= actual
    }

    /*
    fn update_rr_metrics(&mut self, packet: &RtpPacket) {
        let seq_num = packet.sequence_number;
        let metrics = &mut self.metrics;

        // 1. Contar paquete recibido
        metrics.packets_received = metrics.packets_received.wrapping_add(1);

        // 2. Cálculo de Pérdida y Secuencia Máxima (Con corrección de Wrapping)
        const MAX_DROPOUT: u64 = 3000;
        const SEQ_MOD: u64 = 1 << 15; // 32768

        if metrics.max_sequence_number.is_none() {
            // Initialization: first packet received
            metrics.max_sequence_number = Some(seq_num);
        } else {
            let max_seq = metrics.max_sequence_number.unwrap();
            // Calculamos la distancia modular entre el nuevo seq y el máximo visto
            let delta = seq_num.wrapping_sub(max_seq);

            // Si delta < 32768, asumimos que el paquete es "futuro" (nuevo)
            if delta < SEQ_MOD {
                if delta < MAX_DROPOUT {
                    // Normal case: either the next packet arrived or there
                    // was a small loss/gap.
                    let gap = delta.wrapping_sub(1);

                    metrics.cumulative_lost = metrics.cumulative_lost.wrapping_add(gap);
                    metrics.packets_expected = metrics.packets_expected.wrapping_add(u32::from(delta));
                    metrics.max_sequence_number = Some(seq_num); // Update maximum seen
                } else {
                    // Large jump: likely source restart or abrupt wrap; re-sync
                    // without counting a massive loss.
                    metrics.max_sequence_number = Some(seq_num);
                }
            } else {
                // delta >= SEQ_MOD indicates an old or out-of-order packet
                // We do not update max_sequence_number in this case.
            }
        }

        // 3. Calc of Jitter (RFC 3550 A.8)
        let current_arrival_time = self.start_time.elapsed();

        if metrics.last_arrival_time != Duration::ZERO && metrics.last_rtp_timestamp != 0 {
            // Arrival time difference (ms)
            let d_arrival_ms = current_arrival_time.as_millis();
            let d_arrival_prev_ms = metrics.last_arrival_time.as_millis();

            let diff_arrival = d_arrival_ms as i64 - d_arrival_prev_ms as i64;

            // Difference in RTP timestamps (clock units)
            let diff_rtp = packet.timestamp as i64 - metrics.last_rtp_timestamp as i64;

            let delay_diff = diff_arrival - diff_rtp;
            let abs_delay_diff = delay_diff.unsigned_abs() as u32;

            // RFC 3550 jitter update: J = J + (|D| - J)/16
            let current_jitter = metrics.interarrival_jitter as f64;
            let diff = abs_delay_diff as f64;

            let new_jitter = current_jitter + ((diff - current_jitter) / 16.0);

            metrics.interarrival_jitter = new_jitter.max(0.0) as u32;
        }

        metrics.last_arrival_time = current_arrival_time;
        metrics.last_rtp_timestamp = packet.timestamp;
    }

     */
}

/*
#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    // --- Helper para crear paquetes rápidamente ---
    fn make_packet(
        seq: u64,
        ts: i64,
        is_i_frame: bool,
        marker: u8,
        total_chunks: u8,
        payload_byte: u8,
    ) -> RtpPacket {
        RtpPacket {
            version: 2,
            marker,
            total_chunks,
            is_i_frame,
            payload_type: 96,
            sequence_number: seq,
            timestamp: ts,
            ssrc: 12345,
            payload: vec![payload_byte], // Payload simple de 1 byte para verificar fácil
        }
    }

    #[test]
    fn test_initial_state_needs_iframe() {
        let mut jitter = JitterBuffer::<10>::new();

        // 1. Intentamos meter un P-Frame (Delta) al principio
        // Debería ser ignorado porque i_frame_needed es true por defecto
        let p_frame = make_packet(1, 1000, false, 1, 1, 0xAA);
        jitter.add(p_frame);

        assert!(jitter.pop().is_none(), "El buffer no debería devolver nada si no llegó un I-Frame primero");

        // 2. Metemos un I-Frame
        let i_frame = make_packet(2, 2000, true, 1, 1, 0xFF);
        jitter.add(i_frame);

        // 3. Ahora sí debería salir
        let result = jitter.pop();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), vec![0xFF]);
    }

    #[test]
    fn test_frame_assembly_ordered() {
        let mut jitter = JitterBuffer::<10>::new();

        // Setup: Meter I-Frame inicial para desbloquear
        jitter.add(make_packet(1, 1000, true, 1, 1, 0xFF));
        jitter.pop();
        // Test: Meter un frame partido en 3 chunks ordenados
        // Frame Timestamp: 2000
        // Seq: 2, 3, 4
        jitter.add(make_packet(2, 2000, false, 0, 3, 0xA1));
        jitter.add(make_packet(3, 2000, false, 0, 3, 0xA2));
        jitter.add(make_packet(4, 2000, false, 1, 3, 0xA3)); // Marker = 1

        let result = jitter.pop();
        assert!(result.is_some(), "Debería haber ensamblado el frame");

        // Verificamos que pegó los payloads en orden: A1, A2, A3
        assert_eq!(result.unwrap(), vec![0xA1, 0xA2, 0xA3]);
    }

    #[test]
    fn test_frame_assembly_out_of_order() {
        let mut jitter = JitterBuffer::<10>::new();

        // Setup: I-Frame inicial
        jitter.add(make_packet(1, 1000, true, 1, 1, 0xFF));
        jitter.pop();

        // Test: Meter chunks desordenados (3, luego 2)
        // El pop debería esperar hasta tener todo
        jitter.add(make_packet(3, 2000, false, 1, 2, 0xBB)); // Último chunk llega primero

        // Intentamos pop, debería ser None porque falta el chunk 1 (seq 2)
        assert!(jitter.pop().is_none(), "No debería entregar frame incompleto");

        // Llega el chunk faltante
        jitter.add(make_packet(2, 2000, false, 0, 2, 0xAA));

        let result = jitter.pop();
        assert!(result.is_some());
        assert_eq!(result.unwrap(), vec![0xAA, 0xBB], "Debería reordenar internamente");
    }

    #[test]
    fn test_missing_packet_returns_none() {
        let mut jitter = JitterBuffer::<10>::new();
        // Setup
        jitter.add(make_packet(1, 1000, true, 1, 1, 0xFF));
        jitter.pop();

        // Frame de 3 partes: 2, 3, 4. Falta el 3.
        jitter.add(make_packet(2, 2000, false, 0, 3, 0xA1));
        // (El 3 se pierde en la red)
        jitter.add(make_packet(4, 2000, false, 1, 3, 0xA3));

        // Pop debería fallar porque el chunk count no coincide o hay hueco
        assert!(jitter.pop().is_none());
    }

    #[test]
    fn test_tolerance_expiration() {
        let mut jitter = JitterBuffer::<10>::new();

        // 1. Frame inicial (Sale bien)
        jitter.add(make_packet(1, 1000, true, 1, 1, 0xA1));
        jitter.pop(); // Esto setea last_deliver_timestamp a "AHORA"

        // 2. Simulamos que pasa tiempo en la vida real para que el siguiente expire
        // El siguiente frame tiene timestamp 1033 (33ms después).
        // Si dormimos 600ms (mayor a TOLERANCE 500ms), debería expirar.

        // Nota: Como no podemos mockear Local::now() fácilmente sin crates externos,
        // forzamos la lógica metiendo un frame con timestamp muy viejo relativo al anterior.

        // Simulamos un salto grande en el video que no se condice con el tiempo real
        // O simplemente dormimos el thread (lento pero efectivo para tests unitarios simples)
        thread::sleep(Duration::from_millis(600));

        // Metemos el siguiente frame.
        // Delta RTP = 33ms.
        // Expected playout = Last_Deliver + 33ms.
        // Deadline = Expected + 500ms.
        // Como Last_Deliver fue hace 600ms, Deadline ya pasó.
        jitter.add(make_packet(2, 1033, false, 1, 1, 0xB1));

        // Al hacer pop, valid_playout_time debería dar false, llamar a resync_or_clear
        // y como no hay I-frames nuevos, devolver None.
        assert!(jitter.pop().is_none(), "El frame debería haber expirado por tolerancia");

        // Verificamos que pidió un I-Frame nuevo (reseteó el estado)
        // Para verificar esto indirectamente: intentamos meter un P-Frame, debería rebotar.
        jitter.add(make_packet(3, 1066, false, 1, 1, 0xC1));
        assert!(jitter.pop().is_none());

        // Metemos un I-Frame nuevo, debería aceptarlo.
        jitter.add(make_packet(4, 2000, true, 1, 1, 0xD1));
        let res = jitter.pop();
        assert!(res.is_some());
        assert_eq!(res.unwrap(), vec![0xD1]);
    }

    #[test]
    fn test_circular_buffer_resync_on_overwrite() {
        // Buffer chico de tamaño 4
        let mut jitter = JitterBuffer::<4>::new();

        // Llenamos el buffer con frames validos (pero no los popeamos)
        // Indices: 0, 1, 2, 3 ocupados.
        // Seq: 10, 11, 12, 13
        jitter.add(make_packet(10, 1000, true, 1, 1, 0xA));
        jitter.add(make_packet(11, 1100, false, 1, 1, 0xB));
        jitter.add(make_packet(12, 1200, false, 1, 1, 0xC));
        jitter.add(make_packet(13, 1300, false, 1, 1, 0xD));

        // Ahora metemos el packet 14.
        // 14 % 4 = 2. Va a sobrescribir el seq 12 en la posicion 2.
        // Como seq 12 > seq 10 (read), esto es un overwrite del futuro sobre el medio.
        // Debería disparar resync_or_clear.

        // Marcamos este como I-Frame para ver si el resync lo encuentra
        let packet_overwrite = make_packet(14, 1400, true, 1, 1, 0xE);
        jitter.add(packet_overwrite);

        // La logica de resync_or_clear busca desde read_idx hacia adelante un I-Frame.
        // read_idx estaba en 10 (pos 2). Fue pisado.
        // Debería encontrar el 14 (pos 2) que acabamos de meter si es I-Frame.

        let res = jitter.pop();
        assert!(res.is_some());
        // Debería haber saltado al 14 (0xE)
        assert_eq!(res.unwrap(), vec![0xE]);
    }

    #[test]
    fn test_long_running_simulation_30fps_realtime() {
        let mut jitter = JitterBuffer::<512>::new();

        let total_frames = 300; // 10 segundos aprox
        let fps = 30;
        let frame_interval_ms = 33;

        // 1. GENERAR DATOS (Igual que antes)
        let mut network_queue: Vec<RtpPacket> = Vec::new();
        let mut seq_counter = 1;
        let mut ts_counter = 1000;

        for i in 0..total_frames {
            let is_keyframe = i % 30 == 0;
            network_queue.push(make_packet(seq_counter, ts_counter, is_keyframe, 0, 2, 0xA0));
            seq_counter += 1;
            network_queue.push(make_packet(seq_counter, ts_counter, is_keyframe, 1, 2, 0xA1));
            seq_counter += 1;
            ts_counter += frame_interval_ms;
        }

        // Simular pérdida del frame #50
        network_queue.remove(100);
        network_queue.remove(100);

        // 2. SIMULACIÓN EN TIEMPO REAL
        let start_time = Local::now().timestamp_millis();
        let mut frames_decoded = 0;
        let mut network_index = 0;

        // Control de tiempos
        let mut next_network_push = start_time;
        let mut next_decoder_pop = start_time;

        // Corremos hasta terminar o timeout de seguridad (12s)
        while frames_decoded < total_frames - 5 { // -5 margen por perdidas
            let now = Local::now().timestamp_millis();

            if now.elapsed().as_secs() > 12 {
                break; // Timeout del test
            }

            // A. RED: Simular envío (Burst de 2 paquetes cada 20ms)
            // Esto es aprox la velocidad necesaria para 30fps (60 packets/s)
            if now >= next_network_push {
                let burst = 2;
                for _ in 0..burst {
                    if network_index < network_queue.len() {
                        jitter.add(network_queue[network_index].clone());
                        network_index += 1;
                    }
                }
                next_network_push += Duration::from_millis(20);
            }

            // B. DECODER: Intentar leer a 30 FPS (33ms)
            if now >= next_decoder_pop {
                if let Some(_) = jitter.pop() {
                    frames_decoded += 1;
                }
                next_decoder_pop += Duration::from_millis(33);
            }

            // Dormir un poquito para no quemar CPU al 100% en el test
            // y permitir que el OS actualice los relojes.
            std::thread::sleep(Duration::from_millis(1));
        }

        println!("Frames Decodificados: {} / {}", frames_decoded, total_frames);

        // Validaciones
        // Se espera perder algunos por el packet loss forzado y el resync subsecuente,
        // pero deberíamos tener la gran mayoría.
        assert!(frames_decoded > 250, "Rendimiento pobre: se perdieron demasiados frames");
    }

    #[test]
    fn test_simulation_150_frames_debug() {
        let mut jitter = JitterBuffer::<512>::new();

        let total_frames = 150;
        let fps = 30;
        let frame_duration = 33;

        // 1. GENERAMOS EL TRÁFICO
        let mut network_queue: Vec<RtpPacket> = Vec::new();
        let mut seq_counter = 1;
        let mut ts_counter = 1000;

        for i in 0..total_frames {
            let is_keyframe = i % 30 == 0;

            // Chunk 1
            network_queue.push(make_packet(seq_counter, ts_counter, is_keyframe, 0, 2, 0xA0));
            seq_counter += 1;
            // Chunk 2 (Marker)
            network_queue.push(make_packet(seq_counter, ts_counter, is_keyframe, 1, 2, 0xA1));
            seq_counter += 1;

            ts_counter += frame_duration;
        }

        // 2. SIMULAMOS PÉRDIDA DE PAQUETES (PACKET LOSS)
        // Borramos el Frame #20 completo (paquetes indices 40 y 41)
        // Esto dejará un hueco (None) en el buffer.
        println!("🔥 Simulando pérdida del Frame #20 (Seq {} y {})", network_queue[40].sequence_number, network_queue[41].sequence_number);
        network_queue.remove(40);
        network_queue.remove(40); // El indice se desplaza

        // 3. SIMULACIÓN
        let mut frames_decoded = 0;
        let mut network_index = 0;
        let mut clock_ms = 0;

        // Corremos la simulación un poco más del tiempo teórico (150 frames * 33ms = 4950ms)
        // Damos 6000ms para dar tiempo a recuperaciones.
        while clock_ms < 6000 {

            // A. RED: Entrega 2 paquetes cada 20ms (flujo constante)
            if clock_ms % 20 == 0 {
                let burst_size = 2;
                for _ in 0..burst_size {
                    if network_index < network_queue.len() {
                        let pkt = network_queue[network_index].clone();
                        // println!("-> RED: Entra Seq {}", pkt.sequence_number);
                        jitter.add(pkt);
                        network_index += 1;
                    }
                }
            }

            // B. DECODER: Intenta leer a 30 FPS (cada 33ms)
            if clock_ms > 0 && clock_ms % 33 == 0 {
                match jitter.pop() {
                    Some(data) => {
                        frames_decoded += 1;
                        // println!("<- POP: Frame decodificado. Total: {}", frames_decoded);
                    },
                    None => {
                        // Debug para ver si se queda atascado
                        // println!(".. Buffering / Esperando en ms {}", clock_ms);
                    }
                }
            }

            // Avanzamos tiempo (simulado rapido)
            // thread::sleep(Duration::from_micros(10)); // Descomentar si queres verlo lento en consola
            clock_ms += 1;
        }

        println!("================ RESULTADOS ================");
        println!("Total frames enviados: {}", total_frames);
        println!("Frames decodificados: {}", frames_decoded);
        println!("Perdidos esperados: 1 (Frame #20)");

        // VALIDACIONES
        // Esperamos haber decodificado al menos 140 frames
        // (150 total - 1 perdido - quizás algunos por resync/tolerancia)
        assert!(frames_decoded >= 140, "Se perdieron demasiados frames. El buffer se atascó.");

        // No puede ser perfecto porque borramos uno a propósito
        assert!(frames_decoded < total_frames, "Imposible: Decodificó frames que borramos");
    }
}

 */