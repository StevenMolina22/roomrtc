# Taller de Programacion {Los Compiladores Felices}

## Integrantes
- Molina Buitrago, Marlon Stiven (112018)
- Cortez Aguilar, Diego Alejandro (111753)
- Perez D'Angelo, Tomás (111834)
- Politti, Ignacio (112034)

## Como usar

A continuacion se detallan los pasos para compilar y ejecutar el programa.

### Compilacion

### Como correr

## Como testear

```bash
# Terminal 1
cargo run --bin client 0 > offer.sdp

# Terminal 2
cat offer.sdp | cargo run --bin client 1 > answer.sdp

# Terminal 3
cat answer.sdp | cargo run --bin client 0
```
