# Mitub

Mitub es un cliente de escritorio nativo, ligero y de alto rendimiento para YouTube, desarrollado en Rust con una interfaz gráfica basada en iced y un motor de renderizado de video acelerado por hardware basado en libmpv.

Disenado para ofrecer un consumo minimo de recursos (CPU y memoria RAM), arranque instantaneo, soporte completo de Wayland, reproduccion multiventana Picture-in-Picture (PiP), gestion de pestanas inteligentes con precarga y sincronizacion de sesiones con navegadores locales.

---

## Caracteristicas Principales

- Motor Multimedia Nativo libmpv: Reproduccion fluida de video y audio desacoplados (DASH) mediante libmpv con decodificacion acelerada por hardware (Intel VA-API / QuickSync, NVIDIA NVDEC, AMD VAAPI).
- Interfaz Grafica Nativa en Rust (iced 0.13): Diseno moderno, minimalista y responsivo sin el sobrecosto de un runtime web o Electron.
- Sistema de Pestanas Inteligentes: Navegacion multi-pestana inspirada en navegadores web. Permite abrir videos con click medio o boton secundario, con desplazamiento bidireccional y restauracion de hasta 5 pestanas cerradas recientemente (Ctrl+Shift+T).
- Modo Picture-in-Picture (PiP): Miniventana flotante siempre visible (KWin rules integradas), con boton de cierre y reacoplamiento integrado directamente sobre el video.
- Sincronizacion de Cuenta Multiplataforma: Inicio y cierre de sesion con importacion automatica de cookies desde Firefox y navegadores basados en Chromium (Google Chrome, Chromium, Brave, Microsoft Edge, Vivaldi) en Linux, Windows y macOS.
- Control de Calidad y Codec Flexible: Selector de resolucion con indicador predeterminado directamente desde el reproductor OSD, soporte para AV1, VP9 y H.264 con cascada de fallback automatico.
- Gestion Inteligente de Cache: Almacenamiento local configurable con limite ajustable (1 GB, 2 GB, 5 GB, 10 GB, 20 GB o Ilimitado) y desalojo automatico LRU para evitar saturacion de disco.
- Optimizacion de Memoria RAM: Politica activa de liberacion de memoria en background para instancias mpv inactivas y reciclaje de texturas de video.
- Integracion de Sistema: Inhibicion automatica del protector de pantalla via D-Bus org.freedesktop.ScreenSaver durante la reproduccion.

---

## Requisitos del Sistema

### Linux
- Sistema operativo Linux con servidor grafico Wayland (KDE Plasma 6+, GNOME 40+, Sway o Hyprland) o X11.
- Bibliotecas del sistema:
  - libmpv (version 2 / libmpv-dev o mpv-devel)
  - wayland-client (libwayland-client)
  - libxkbcommon
  - mesa / controladores graficos con soporte VA-API o Vulkan
- Herramientas CLI auxiliares opcionales:
  - wl-clipboard (para comando wl-copy al copiar enlaces)
  - xdg-utils (para abrir enlaces externos)

---

## Inicio Rapido

### Compilar y Ejecutar en Desarrollo
```bash
cargo run --release
```

Para mas detalles sobre instalacion de dependencias y empaquetado, consulta BUILD.md.
Para detalles sobre la arquitectura interna del reproductor y concurrencia, consulta ARCHITECTURE.md.

---

## Atajos de Teclado

- Espacio: Reproducir / Pausar
- Izquierda / Derecha: Retroceder / Avanzar 5 segundos
- J / L: Retroceder / Avanzar 10 segundos
- Arriba / Abajo: Subir / Bajar volumen
- M: Silenciar / Activar sonido
- F: Pantalla completa
- T: Modo cine
- I o P: Activar / Desactivar Picture-in-Picture (PiP)
- C: Activar / Desactivar subtitulos
- Ctrl+T: Abrir nueva pestana
- Ctrl+W: Cerrar pestana activa
- Ctrl+Shift+T: Reabrir la ultima pestana cerrada (hasta 5 pestanas)
- Ctrl+Tab / Ctrl+Shift+Tab: Navegar entre pestanas abiertas
- Escape: Cerrar modales, sugerencias, menus flotantes o salir de pantalla completa

---

## Licencia

Este proyecto esta licenciado bajo los terminos de la licencia MIT.
