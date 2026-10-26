# Guía de Compilación e Instalación - OxideTube (Mitub)

Este documento detalla los requisitos del sistema y el proceso de construcción de OxideTube.

---

## 1. Requisitos Previos

Se requiere el toolchain oficial de Rust (versión 1.80 o superior):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup default stable
```

---

## 2. Dependencias del Sistema

### Arch Linux / Manjaro
```bash
sudo pacman -S --needed base-devel git mpv wayland libxkbcommon mesa vulkan-intel vulkan-radeon wl-clipboard
```

### Debian / Ubuntu (22.04, 24.04+)
```bash
sudo apt update
sudo apt install -y build-essential pkg-config libmpv-dev libwayland-dev libxkbcommon-dev mesa-va-drivers wl-clipboard
```

### Fedora (39, 40+)
```bash
sudo dnf install -y gcc gcc-c++ mpv-libs-devel wayland-devel libxkbcommon-devel mesa-va-drivers wl-clipboard
```

---

## 3. Compilación

### Modo Debug (desarrollo rápido)
```bash
cargo build
```

### Modo Release (máxima optimización)
El perfil release utiliza LTO delgado (`lto = "thin"`), optimización nivel 3 (`opt-level = 3`), despojo de símbolos de depuración (`strip = true`) y un único codegen unit para máxima velocidad y mínima huella binaria:

```bash
cargo build --release
```

El binario ejecutable compilado se genera en:
`target/release/mitub`

---

## 4. Instalación

### En el directorio del usuario (`~/.local/bin`)
```bash
install -m 755 target/release/mitub ~/.local/bin/mitub
```

Asegúrate de que `~/.local/bin` esté incluido en tu variable de entorno `PATH`:
```bash
export PATH="$HOME/.local/bin:$PATH"
```

### Instalación en todo el sistema (`/usr/local/bin`)
```bash
sudo install -m 755 target/release/mitub /usr/local/bin/mitub
```

---

## 5. Pruebas y Validación

```bash
# Comprobación de sintaxis y tipos
cargo check --release

# Ejecución de tests unitarios
cargo test --release
```
