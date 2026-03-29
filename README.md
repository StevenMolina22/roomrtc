# RoomRTC

Proyecto final de Taller de Programacion (FIUBA) desarrollado por el grupo **RoomRTC**.

RoomRTC es una aplicacion de videollamadas escrita en Rust. Usa un servidor central para autenticacion y senalizacion, y establece una conexion peer-to-peer entre clientes para transportar audio, video y archivos.

## Integrantes

- Molina Buitrago, Marlon Stiven (112018)
- Cortez Aguilar, Diego Alejandro (111753)
- Perez D'Angelo, Tomas (111834)
- Politti, Ignacio (112034)

## Que hace el proyecto

- registro e inicio de sesion de usuarios
- listado de usuarios disponibles
- llamadas entre peers con intercambio de SDP e ICE
- transporte de audio y video en tiempo real
- cifrado de la comunicacion con DTLS/SRTP
- envio de archivos durante la llamada por data channels
- interfaz grafica de escritorio con `eframe/egui`

## Arquitectura

- **Servidor central:** maneja usuarios, login y senalizacion
- **Cliente:** interfaz grafica, captura de audio/video y control de la llamada
- **Conexion P2P:** una vez negociada la llamada, los peers intercambian medios directamente sobre UDP

## Requisitos

- Rust estable
- OpenCV 4 con headers de desarrollo
- OpenSSL con headers de desarrollo
- `clang`, `llvm`, `pkg-config`

En Ubuntu/Debian hay un script base para instalar dependencias del entorno:

```bash
bash ./scripts/dependencies.sh
```

Segun tu sistema puede que tambien necesites instalar `libssl-dev`.

## Compilacion

```bash
cargo build
```

Para una build optimizada:

```bash
cargo build --release
```

## Ejecucion

El proyecto expone dos binarios principales: `server` y `client`.

### 1. Levantar el servidor

```bash
cargo run --bin server room_rtc.conf
```

El servidor usa la configuracion del archivo INI y escribe logs en `room_rtc.server.log`.

### 2. Levantar un cliente

```bash
cargo run --bin client room_rtc.conf 127.0.0.1:8080
```

Donde el segundo argumento es la direccion del servidor de senalizacion (`client_server_addr`).

El cliente escribe logs en `room_rtc.log`.

### Orden recomendado

1. Iniciar el servidor.
2. Abrir uno o mas clientes.
3. Registrarse o iniciar sesion.
4. Seleccionar un usuario disponible y comenzar la llamada.

## Configuracion

El archivo `room_rtc.conf` incluye las secciones principales del sistema:

- `[network]`: sockets y tamano maximo de paquetes UDP
- `[media]`: camara, video H.264, audio Opus y parametros RTP
- `[rtcp]` y `[rtp]`: reportes, timeouts y tamanos de paquete
- `[sdp]` e `[ice]`: negociacion de sesion y candidatos
- `[server]`: direcciones del servidor, archivos TLS y archivo de usuarios
- `[dcep]`: timeouts para data channels

El archivo de ejemplo ya apunta a:

- servidor de senalizacion en `0.0.0.0:8080`
- canal servidor-cliente en `0.0.0.0:8081`
- certificados TLS en `tls_server/`
- base simple de usuarios en `src/server/data.txt`

## Testing y calidad

```bash
cargo test
```

```bash
cargo clippy --all-targets --all-features
```

## Documentacion

```bash
cargo doc --open
```

## Archivos utiles

- `room_rtc.conf`: configuracion de ejemplo
- `src/bin/server/main.rs`: punto de entrada del servidor
- `src/bin/client/main.rs`: punto de entrada del cliente
- `docs/Informe.md`: informe tecnico del proyecto
