# Arquitectura del Sistema - Mitub

Este documento describe la arquitectura interna, el flujo de datos, la gestion de memoria y el modelo de concurrencia de Mitub.

---

## Vision General

Mitub sigue una arquitectura modular en Rust organizada en capas bien definidas:

1. Capa de Presentacion (iced 0.13 + iced_canvas):
   Gestiona el bucle de eventos, la interfaz grafica reactiva (Model-View-Update), el OSD flotante del reproductor y la sincronizacion de ventanas (principal y Picture-in-Picture).

2. Capa de Reproduccion Multimedia (src/player.rs - libmpv):
   Proporciona integracion FFI segura con la biblioteca nativa `libmpv`. Gestiona la inicializacion de subsuperficies Wayland, renderizado con aceleracion por hardware (hwdec), conmutacion dinamica de pistas de audio y video desacopladas (DASH) y control de buffering.

3. Capa de Servicios de Red y YouTube API (src/innertube.rs):
   Cliente asincrono sobre `reqwest` optimizado con HTTP/2, brotli, gzip y deserializacion acelerada con `simd-json` para comunicarse con los endpoints internos de YouTube InnerTube (Android/Web client mocks).

4. Capa de Autenticacion y Sesiones (src/auth.rs):
   Motor de importacion e interpretacion de cookies de navegadores locales. Soporta perfiles SQLite de Mozilla Firefox y navegadores Chromium (Google Chrome, Chromium, Brave, Microsoft Edge, Vivaldi) en Linux, Windows y macOS. Incluye generacion de cabeceras SAPISIDHASH requeridas para consultas autenticadas de YouTube.

5. Capa de Gestion de Cache y Recursos (src/cache_manager.rs):
   Controlador de almacenamiento persistente en disco (`~/.cache/mitub/video_cache`) con monitoreo dinamico del tamano y politica de desalojo LRU (Least Recently Used) basada en el limite maximo configurable.

---

## Flujo de Datos y Reproduccion

1. Solicitud de Video:
   El usuario selecciona un video o introduce un enlace. InnerTubeClient consulta el endpoint `player` y `next` de InnerTube, extrayendo metadatos, streams de video adaptativos, streams de audio independientes, pistas de subtitulos VTT y capitulos.

2. Seleccion Inteligente de Formato (DASH):
   La funcion `select_best_stream` evalua la resolucion preferida del usuario y el codec deseado (AV1, VP9 o H.264), verificando capacidades de decodificacion de hardware. Si el codec o resolucion no estan disponibles, desciende en cascada sin interrumpir la reproduccion.

3. Inicializacion del Reproductor:
   Se configura la URL del stream de video y el stream de audio opcional en libmpv (`audio-file`). libmpv establece la conexion directa por red, decodifica frames en GPU y sincroniza audio/video mediante reloj interno maestro de alta precision.

4. Interfaz OSD y Sincronizacion de Superficies:
   La interfaz iced dibuja una ventana translúcida o bajo-relieve donde libmpv renderiza directamente a la subsuperficie Wayland o mediante el buffer de frames compartido. El OSD (barra de progreso, controles, menus de calidad y subtitulos) se actualiza de manera reactiva mediante `Message::Tick`.

---

## Modelo de Concurrencia y Rendimiento

- Asignacion Global de Memoria (mimalloc):
  Mitub utiliza el asignador de memoria de alto rendimiento `mimalloc` de Microsoft en lugar del allocator por defecto del sistema, reduciendo la fragmentacion y acelerando las operaciones multihilo.

- Asincronia Tokio + Hilos Dedicados:
  Todas las peticiones de red (InnerTube, descarga de miniaturas, polling de chat en vivo) operan sobre un runtime asincrono de Tokio sin bloquear el hilo principal de la interfaz de usuario. Las operaciones bloqueantes de acceso a disco (SQLite de cookies, lectura/escritura de cache) se delegan explicitamente a `tokio::task::spawn_blocking`.

- Optimizacion de Memoria RAM en Pestanas y Reproductores:
  Solo la pestana activa mantiene la reproduccion en curso en el motor mpv. Cuando se cambia de pestana, la posicion de reproduccion se guarda y los buffers no esenciales se suspenden, evitando la acumulacion lineal de memoria por pestanas abiertas.

- Ahorro de Energia y Protector de Pantalla:
  `PowerManager` monitorea el estado del sistema y se comunica con D-Bus (`org.freedesktop.ScreenSaver` y `org.freedesktop.UPower`) para inhibir la suspension de pantalla exclusivamente mientras el video se encuentra en estado de reproduccion activa.
