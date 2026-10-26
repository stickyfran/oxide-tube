# Arquitectura del Sistema - OxideTube (Mitub)

Este documento describe los componentes fundamentales, el modelo de memoria, la concurrencia y las decisiones de diseño arquitectónico de OxideTube.

---

## 1. Visión General de la Arquitectura

```
┌────────────────────────────────────────────────────────┐
│               Capa de Presentación (iced 0.13)         │
│  - Event Loop / MVU (Model-View-Update)                │
│  - Barra de Pestañas con Desplazamiento Zero-Shift     │
│  - Canvas de Glow Perimétrico en Pestaña Activa        │
│  - Ventana Principal + Picture-in-Picture Flotante     │
└──────────────────────────┬─────────────────────────────┘
                           │
         ┌─────────────────┴─────────────────┐
         │                                   │
┌────────▼────────────────┐         ┌────────▼────────────────┐
│   Capa de Reproducción  │         │   Capa de Red InnerTube │
│        (libmpv)         │         │  (reqwest + simd-json)  │
│  - hwdec (VA-API/NVDEC) │         │  - HTTP/2 + Brotli      │
│  - Triple-Buffering     │         │  - Deserialización SIMD │
│  - Demuxer Adaptativo   │         │  - Fallback de Códecs   │
│  - Suspensión Inactiva  │         │  - Extracción de Streams│
└─────────────────────────┘         └─────────────────────────┘
         │                                   │
         └─────────────────┬─────────────────┘
                           │
┌──────────────────────────▼─────────────────────────────┐
│           Gestión de Memoria y Sistema Operativo       │
│  - mimalloc (purge_delay = 0, mi_collect)              │
│  - Tokio Runtime acotado (4 workers)                   │
│  - Afinidad Dinámica de Cores de Bajo Consumo          │
│  - D-Bus ScreenSaver Inhibit                           │
└────────────────────────────────────────────────────────┘
```

---

## 2. Componentes Principales

### 2.1 Interfaz de Usuario y Pestañas (`src/ui/mod.rs`)
- **Arquitectura Zero-Shift en Pestañas**:
  El contenedor horizontal de pestañas (`tabs_scrollable`) ocupa de manera constante el 100% del ancho asignado en la capa base de un widget `stack!`. Los botones de navegación izquierda (`<`) y derecha (`>`) flotan en capas superiores como overlays cuando existe desbordamiento real. Esto garantiza que la aparición o desaparición de los botones no desplace el origen de coordenadas X del scrollable, eliminando cualquier stutter o saltos (*wiggle*) al usar la rueda del ratón.
- **Canvas de Glow Dinámico (`PlayingTabGlowBorder`)**:
  Dibuja un trazo perimétrico suave con degradado que recorre la pestaña en reproducción. Se actualiza mediante un timer dedicado de ~31 FPS en la suscripción de la aplicación, manteniéndose fluido tanto con el cursor estático como con la ventana sin foco.

### 2.2 Motor Multimedia (`src/player.rs`)
- **libmpv Nativo**: Integración directa mediante enlace FFI C a `libmpv.so.2`.
- **Triple-Buffering sin Preasignación**: Utiliza exactamente 3 slots de textura RGBA compartidos para presentación continua sin bloqueos de sincronización vertical, evitando consumo estático innecesario.
- **Caché de Demuxer Adaptativa**:
  El tamaño del buffer de red se modula dinámicamente según la resolución del video:
  - 4K / 1440p: 28 MB de demuxer (`demuxer-max-bytes = 28M`, `demuxer-readahead-secs = 15`) para absorber jitter de red de alta tasa de bits.
  - 1080p / 720p: 12 MB de demuxer (`demuxer-max-bytes = 12M`, `demuxer-readahead-secs = 8`).
  - < 720p: 4 MB de demuxer (`demuxer-max-bytes = 4M`, `demuxer-readahead-secs = 5`).
- **Eliminación de Máquinas Virtuales Innecesarias**:
  Se deshabilitan explícitamente los scripts Lua internos, el menú OSD por defecto y los reproductores de terminal de mpv (`load-scripts=no`, `osc=no`, `osd-bar=no`), reduciendo la huella base del proceso.

### 2.3 Servicios de Red y YouTube API (`src/innertube.rs`)
- Cliente HTTP asíncrono con compresión HTTP/2, Brotli y Gzip.
- Deserialización de JSON con aceleración SIMD vía `simd-json`.
- Búsqueda paralela y no bloqueante de dislikes (Return YouTube Dislike API) y detalles de canal concurrentemente con el manifiesto del reproductor.

### 2.4 Autenticación y Cookies (`src/auth.rs`)
- Extracción de cookies directamente de los perfiles SQLite de Mozilla Firefox (con lectura del journal WAL para tokens actualizados) y navegadores Chromium (Google Chrome, Brave, Chromium, Edge, Vivaldi).
- Generación de firma `SAPISIDHASH` en tiempo de ejecución para consultas autenticadas.
- Persistencia local de sesión en `~/.config/mitub/cookies.json`.

---

## 3. Estrategia de Optimización de Memoria

1. **mimalloc con Purga Inmediata**:
   Se configura `purge_delay = 0` para devolver páginas libres inmediatamente al kernel de Linux mediante `MADV_DONTNEED`. Se invoca `mi_collect(true)` periódicamente y al cerrar pestañas o pausar reproducción.
2. **Workers Tokio Acotados**:
   El runtime Tokio se limita a 4 hilos de trabajo (`TOKIO_WORKER_THREADS = 4`), evitando la creación excesiva de arenas de memoria locales por hilo en CPUs con alto número de núcleos.
3. **Caché de Miniaturas con Desalojo**:
   Las imágenes y portadas se descargan asíncronamente y se mantienen en una estructura LRU con reciclaje proactivo de handles de textura.
