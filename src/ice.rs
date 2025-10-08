#[derive(Clone)]
pub enum CandidateType {
    Host,
    ServerReflexive,
}

impl CandidateType {
    /// Retorna la prioridad del tipo de candidato según RFC 8445
    pub fn priority(&self) -> u32 {
        match self {
            CandidateType::Host => 126,
            CandidateType::ServerReflexive => 100,
        }
    }
}

impl std::fmt::Display for CandidateType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CandidateType::Host => write!(f, "host"),
            CandidateType::ServerReflexive => write!(f, "srflx"),
        }
    }
}

// Representa un candidato ICE
#[derive(Clone)]
pub struct Candidate {
    pub candidate_type: CandidateType,
    pub priority: u32,
    pub address: String,
    pub port: u16,
    pub component_id: u8,
    pub foundation: String,
}

impl Candidate {
    /// Crea un nuevo candidato de tipo Host con la dirección IP especificada
    pub fn new_host(address: String, component_id: u8) -> Self {
        let candidate_type = CandidateType::Host;
        let priority = Self::calculate_priority(&candidate_type, 65535); // Preferencia local alta
        let foundation = "1".to_string(); // En un caso real, esto sería más complejo

        Candidate {
            candidate_type,
            priority,
            address,
            port: 0, // El puerto se asignará al recolectar candidatos
            component_id,
            foundation,
        }
    }

    fn calculate_priority(candidate_type: &CandidateType, local_preference: u16) -> u32 {
        let type_preference = match candidate_type {
            CandidateType::Host => 126,
            CandidateType::ServerReflexive => 100,
        };
        // (type_preference << 24) | (local_preference << 8) | (256 - component_id)
        (type_preference << 24) | ((local_preference as u32) << 8) | (256 - 1)
    }
}

impl std::fmt::Display for Candidate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}@{} ({})", 
               self.address, 
               self.port, 
               self.candidate_type, 
               self.priority)
    }
}

#[derive(Clone)]
pub enum ConnectivityState {
    Waiting,
    InProgress,
    Succeeded,
    Failed,
}

impl std::fmt::Display for ConnectivityState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectivityState::Waiting => write!(f, "Waiting"),
            ConnectivityState::InProgress => write!(f, "In Progress"),
            ConnectivityState::Succeeded => write!(f, "Succeeded"),
            ConnectivityState::Failed => write!(f, "Failed"),
        }
    }
}

#[derive(Clone)]
pub struct CandidatePair {
    pub local: Candidate,
    pub remote: Candidate,
    pub priority: u64,
    pub state: ConnectivityState,
}

impl CandidatePair {
    pub fn new(local: Candidate, remote: Candidate) -> Self {
        let priority = Self::calculate_pair_priority(&local, &remote);
        CandidatePair {
            local,
            remote,
            priority,
            state: ConnectivityState::Waiting,
        }
    }

    fn calculate_pair_priority(local: &Candidate, remote: &Candidate) -> u64 {
            let g = std::cmp::min(local.priority, remote.priority) as u64;
            let l = std::cmp::max(local.priority, remote.priority) as u64;
            (1u64 << 32) * g + 2 * l + if local.priority > remote.priority { 1 } else { 0 }
        }
}

impl std::fmt::Display for CandidatePair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} <-> {} [{}] (priority: {})", 
               self.local, 
               self.remote, 
               self.state, 
               self.priority)
    }
}

pub struct IceAgent {
    local_candidates: Vec<Candidate>,     // Mis candidatos (direcciones IP/puerto)
    remote_candidates: Vec<Candidate>,    // Candidatos del otro peer
    candidate_pairs: Vec<CandidatePair>,  // Todas las combinaciones posibles
    selected_pair: Option<CandidatePair>, // El par ganador para comunicarse
}

impl IceAgent {
    /// Crea un nuevo agente ICE vacío
    pub fn new() -> Self {
        IceAgent {
            local_candidates: Vec::new(),
            remote_candidates: Vec::new(),
            candidate_pairs: Vec::new(),
            selected_pair: None,
        }
    }

    /// Recolecta candidatos locales (direcciones IP disponibles)
    /// En esta implementación básica solo obtiene la IP local
    pub fn gather_candidates(&mut self, port: u16) -> Result<(), String> {
        // Encuentra mi IP local y crea un candidato
        let local_ip = self.get_local_ip()?;  // Obtiene IP (ej: 192.168.1.2)
        let mut candidate = Candidate::new_host(local_ip, 1);  // Crea candidato host
        candidate.port = port; // Asigna el puerto especificado
        self.local_candidates.push(candidate);

        // En un ICE real, aquí se haría interacción con STUN/TURN
        Ok(())
    }

    fn get_local_ip(&self) -> Result<String, String> {
        use std::net::UdpSocket;
        let socket = UdpSocket::bind("0.0.0.0:0")
            .map_err(|e| format!("Error creating socket: {}", e))?;
        socket.connect("8.8.8.8:80")
            .map_err(|e| format!("Error connecting socket: {}", e))?;
        let local_addr = socket.local_addr()
            .map_err(|e| format!("Error getting local address: {}", e))?;
        Ok(local_addr.ip().to_string())
    }

    /// Añade candidatos remotos recibidos del otro peer y crea pares de candidatos
    pub fn add_remote_candidates(&mut self, candidates: Vec<Candidate>) -> Result<(), String> {
        for candidate in candidates {
            self.remote_candidates.push(candidate);
        }
        // Cada vez que se añaden candidatos remotos, se crean nuevos pares
        self.create_candidate_pair()
    }

    fn create_candidate_pair(&mut self) -> Result<(), String> {
        if self.local_candidates.is_empty() || self.remote_candidates.is_empty() {
            return Err("No candidates available to create a pair".into());
        }

        for local in &self.local_candidates {
            for remote in &self.remote_candidates {
                let pair = CandidatePair::new(local.clone(), remote.clone());
                self.candidate_pairs.push(pair);
            }
        }
        Ok(())
    }

    pub fn start_connectivity_checks(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.candidate_pairs.is_empty() {
            return Err("No candidate pairs to verify".into());
        }
        
        // Para simplificar, seleccionamos el primer par (mayor prioridad)
        // En una implementación real, aquí se harían verificaciones STUN
        let mut selected_pair = self.candidate_pairs[0].clone();
        selected_pair.state = ConnectivityState::Succeeded;
        
        self.selected_pair = Some(selected_pair.clone());
        
        println!("Selected pair: {}", selected_pair);
        
        Ok(())
    }

    pub fn get_local_candidate(&self) -> Option<&Candidate> {
        self.local_candidates.first()
    }

    pub fn get_selected_pair(&self) -> Option<&CandidatePair> {
        self.selected_pair.as_ref()
    }

    pub fn generate_candidate_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for candidate in &self.local_candidates {
            let line = format!(
                "a=candidate:{} {} {} {} {} {} typ {}\r\n",
                candidate.foundation,
                candidate.component_id,
                "UDP",
                candidate.priority,
                candidate.address,
                candidate.port,
                match candidate.candidate_type {
                    CandidateType::Host => "host",
                    CandidateType::ServerReflexive => "srflx",
                }
            );
            lines.push(line);
        }
        lines
    }

    /// Parsea una línea de candidato SDP y retorna un Candidate
    /// Formato: "a=candidate:foundation component transport priority ip port typ type"
    pub fn parse_candidate_line(line: &str) -> Result<Candidate, String> {
        // Remover prefijos y sufijos
        let line = line.trim().trim_end_matches("\r\n");
        
        if !line.starts_with("a=candidate:") {
            return Err("Line is not a valid candidate".to_string());
        }
        
        // Remover "a=candidate:" y dividir por espacios
        let parts: Vec<&str> = line[12..].split_whitespace().collect();
        
        if parts.len() < 8 {
            return Err("Invalid candidate format".to_string());
        }
        
        let foundation = parts[0].to_string();
        let component_id = parts[1].parse::<u8>()
            .map_err(|_| "Invalid component ID")?;
        let _transport = parts[2]; // "UDP" o "TCP" - no usado por ahora
        let priority = parts[3].parse::<u32>()
            .map_err(|_| "Invalid priority")?;
        let address = parts[4].to_string();
        let port = parts[5].parse::<u16>()
            .map_err(|_| "Invalid port")?;
        
        // parts[6] debería ser "typ"
        if parts[6] != "typ" {
            return Err("Invalid candidate format: missing 'typ'".to_string());
        }
        
        let candidate_type = match parts[7] {
            "host" => CandidateType::Host,
            "srflx" => CandidateType::ServerReflexive,
            _ => return Err(format!("Unsupported candidate type: {}", parts[7])),
        };
        
        Ok(Candidate {
            candidate_type,
            priority,
            address,
            port,
            component_id,
            foundation,
        })
    }
}