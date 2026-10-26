# OxideTube (Mitub)

<p align="center">
  <strong>Cliente nativo de YouTube en 100% Rust, ultraligero y de alto rendimiento.</strong><br>
  Sin Electron. Sin Chromium embebido. Sin sobrecarga de runtime web.
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-v0.99.0-red.svg" alt="Versión">
  <img src="https://img.shields.io/badge/language-Rust-orange.svg" alt="Lenguaje Rust">
  <img src="https://img.shields.io/badge/gui-iced%200.13-blue.svg" alt="Iced GUI">
  <img src="https://img.shields.io/badge/video-libmpv-darkgreen.svg" alt="libmpv">
  <img src="https://img.shields.io/badge/license-MIT-green.svg" alt="Licencia MIT">
</p>

---

## Descripción

**OxideTube** es un reproductor de escritorio nativo para YouTube desarrollado desde cero en Rust. Combina una interfaz gráfica reactiva acelerada por GPU sobre **iced 0.13** con el motor multimedia **libmpv**, logrando un consumo mínimo de recursos (CPU y RAM), inicio instantáneo y reproducción fluida en resoluciones hasta 4K60 HDR.

Diseñado con enfoque de ingeniería en eficiencia de memoria, soporte nativo de Wayland/X11, reproducción Picture-in-Picture (PiP), navegación por pestañas inteligentes con indicador visual de reproducción en tiempo real y sincronización segura de cookies locales.

---

## Características Principales

- **Motor Multimedia libmpv de Alto Rendimiento**: Decodificación acelerada por hardware (VA-API, NVDEC) con triple-buffering y perfiles adaptativos de demuxer según resolución y red.
- **Interfaz 100% Rust (iced 0.13)**: Sin navegadores web integrados ni JavaScript en la UI. Huella de memoria mínima (~180–350 MB en reproducción activa).
- **Pestañas Inteligentes**: Pestañas de video con apertura en segundo plano (clic central), cierre rápido, restauración de historial (Ctrl+Shift+T) y desplazamiento horizontal sin saltos de layout (*zero layout shift*).
- **Indicador Perimetral de Reproducción**: Borde iluminado (*glow*) dinámico y suave alrededor de la pestaña que está emitiendo video, visible sin necesidad de mover el cursor.
- **Picture-in-Picture (PiP) Multiventana**: Miniventana flotante siempre visible desacoplada de la interfaz principal.
- **Importación de Cookies de Navegador**: Soporte multiplataforma para perfiles de Firefox y Chromium (Chrome, Brave, Edge, Vivaldi), habilitando historial, suscripciones y contenido autenticado.
- **Selector de Calidad y Códec**: Control directo sobre formatos DASH (AV1, VP9, H.264) con fallback automático.
- **Gestión Inteligente de Caché**: Límite ajustable de almacenamiento local con política LRU y limpieza automática.
- **Integración con el Sistema**: Inhibición automática de protector de pantalla vía D-Bus (`org.freedesktop.ScreenSaver`).

---

## Atajos de Teclado

| Tecla | Acción |
| :--- | :--- |
| `Espacio` | Reproducir / Pausar |
| `←` / `→` | Retroceder / Avanzar 5 segundos |
| `J` / `L` | Retroceder / Avanzar 10 segundos |
| `↑` / `↓` | Subir / Bajar volumen |
| `M` | Silenciar / Restaurar volumen |
| `F` | Pantalla completa |
| `T` | Modo cine |
| `I` o `P` | Activar / Desactivar Picture-in-Picture (PiP) |
| `C` | Activar / Desactivar subtítulos |
| `Ctrl + T` | Abrir nueva pestaña |
| `Ctrl + W` | Cerrar pestaña activa |
| `Ctrl + Shift + T` | Reabrir última pestaña cerrada |
| `Ctrl + Tab` | Siguiente pestaña |
| `Ctrl + Shift + Tab` | Pestaña anterior |
| `Escape` | Salir de pantalla completa o cerrar modales |

---

## Apertura de Enlaces de YouTube a Nivel de Sistema (Linux, macOS, Windows)

OxideTube incluye soporte nativo de **instancia única con IPC local**. Si la aplicación ya está abierta y se ejecuta `mitub <URL>` (desde la terminal, el explorador o un enlace externo), el video se abrirá instantáneamente en una nueva pestaña sin reiniciar la aplicación ni duplicar el consumo de memoria RAM.

### 🐧 Linux

1. **Vía Desktop y Gestor MIME**:
   El archivo `.desktop` instalado registra `mitub %u` con el tipo `x-scheme-handler/vnd.youtube`.
2. **Despachador a nivel de sistema (para que todos los links `https://youtu.be/...` abran en OxideTube)**:
   Puedes utilizar un despachador universal de URLs como [`handlr`](https://github.com/chmln/handlr) o un script wrapper configurado como navegador predeterminado:
   ```bash
   # Si la URL contiene 'youtu.be' o 'youtube.com/watch', se despacha a mitub:
   mitub "$URL"
   # En caso contrario, se reenvía a tu navegador web habitual (ej. firefox "$URL")
   ```

### 🍎 macOS

1. Registra el protocolo personalizado o enlace a través de un wrapper en Automator / AppleScript Application o usando herramientas como **Velja** o **Finicky**:
   ```javascript
   // Configuración en Finicky (~/.finicky.js):
   module.exports = {
     defaultBrowser: "Safari",
     handlers: [
       {
         match: ["*youtube.com/watch*", "*youtu.be/*"],
         browser: {
           name: "/usr/local/bin/mitub"
         }
       }
     ]
   };
   ```

### 🪟 Windows

1. OxideTube escucha en el puerto IPC local `127.0.0.1` de tu usuario.
2. Puedes asociar el protocolo `vnd.youtube:` o usar despachadores de URLs para Windows como **Browser Tamer** o **Junction** para reenviar los enlaces de YouTube directamente a `mitub.exe %1`.

---

## Compilación Rápida

Consulta [`BUILD.md`](file:///home/kes/Documentos/Programando/Mitub/BUILD.md) para la guía completa de dependencias por distribución.

```bash
# Compilar binario optimizado
cargo build --release

# Instalar localmente en ~/.local/bin
install -m 755 target/release/mitub ~/.local/bin/mitub
```

---

## Agradecimientos y Créditos

Este proyecto es posible gracias al trabajo y dedicación de las siguientes comunidades y proyectos de código abierto:

- **[iced](https://github.com/iced-rs/iced)**: Framework reactivo GUI en Rust que hace posible una interfaz moderna, limpia y ligera.
- **[mpv / libmpv](https://mpv.io/)**: El motor de reproducción de video más versátil y eficiente del ecosistema de software libre.
- **Comunidad InnerTube / [yt-dlp](https://github.com/yt-dlp/yt-dlp) / [NewPipe](https://github.com/TeamNewPipe/NewPipe)**: Por documentar y mantener accesible la API interna de YouTube de forma abierta y sin publicidad invasiva.
- **[Tokio](https://tokio.rs/) & [reqwest](https://github.com/seanmonstar/reqwest)**: Por el runtime asíncrono y la infraestructura de red de alto rendimiento en Rust.
- **[mimalloc](https://github.com/microsoft/mimalloc)**: Por su asignador de memoria ultrarrápido y con baja fragmentación.
- **Comunidad de Rust**: Por proporcionar un lenguaje con garantías de seguridad de memoria, concurrencia sin miedo y rendimiento bare-metal.

---

## Licencia

Distribuido bajo la Licencia MIT. Consulta el archivo `LICENSE` para más información.
