# Guia de Compilacion e Instalacion de Mitub

Este documento describe los requisitos previos, las herramientas de desarrollo y los pasos necesarios para compilar, probar e instalar Mitub en sistemas operativos compatibles.

---

## Requisitos de Compilacion

### 1. Toolchain de Rust
Se requiere Rust 1.80 o superior con Cargo:
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup default stable
```

### 2. Dependencias del Sistema

#### En distribuciones basadas en Arch Linux / Manjaro:
```bash
sudo pacman -S --needed base-devel git mpv wayland libxkbcommon mesa vulkan-intel vulkan-radeon wl-clipboard
```

#### En distribuciones basadas en Debian / Ubuntu:
```bash
sudo apt update
sudo apt install -y build-essential pkg-config libmpv-dev libwayland-dev libxkbcommon-dev mesa-va-drivers wl-clipboard
```

#### En distribuciones basadas en Fedora:
```bash
sudo dnf install -y gcc gcc-c++ mpv-libs-devel wayland-devel libxkbcommon-devel mesa-va-drivers wl-clipboard
```

---

## Proceso de Compilacion

### Compilacion en Modo Debug
Para desarrollo y depuracion rapida:
```bash
cargo build
```

### Compilacion en Modo Release Optimizada
El perfil release en Cargo.toml incluye Link-Time Optimization (LTO thin), optimizacion de nivel 3 (opt-level 3), strip de simbolos y eliminacion de desempaquetado de panicos para maxima eficiencia y menor huella binaria:
```bash
cargo build --release
```

El binario ejecutable compilado se ubicara en:
`target/release/mitub`

---

## Verificacion y Pruebas

Para ejecutar el analizador de sintaxis y la suite de pruebas unitarias:
```bash
cargo check
cargo test
```

---

## Instalacion en el Sistema

### Instalacion Local para el Usuario Actual
Para instalar el binario en la ruta estandar de binarios del usuario (`~/.local/bin`):
```bash
install -m 755 target/release/mitub ~/.local/bin/mitub
```

Asegurate de que `~/.local/bin` este presente en tu variable de entorno PATH (`export PATH="$HOME/.local/bin:$PATH"`).

### Instalacion Global para Todo el Sistema
```bash
sudo install -m 755 target/release/mitub /usr/local/bin/mitub
```

---

## Variables de Entorno Utiles

- RUST_LOG: Permite ajustar el nivel de diagnostico (ej. `RUST_LOG=mitub=debug`).
- WAYLAND_DISPLAY: Utilizado por libwayland-client e iced para detectar el compositor activo (ej. `wayland-0`).
