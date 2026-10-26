# Guía de Desarrollo para Agentes de IA (AGENTS.md)

Este documento establece las directrices arquitectónicas, restricciones de rendimiento y convenciones de código para cualquier agente de IA o desarrollador que modifique el código de **OxideTube** (`mitub`).

---

## 1. Principios Fundamentales del Proyecto

1. **Cero Runtime Web**:
   Bajo ninguna circunstancia se debe integrar WebView, Electron, componentes Chromium embebidos o motores JavaScript en la interfaz. Toda la UI es 100% Rust nativo renderizado mediante `iced 0.13` sobre `wgpu` / OpenGL.
2. **Presupuesto Estricto de Memoria RAM**:
   - Consumo objetivo en reposo/navegación: **< 150 MB**.
   - Consumo objetivo en reproducción activa 1080p/4K: **< 350 MB**.
   - Cualquier cambio que aumente la retención de memoria de forma estática o desactive las políticas de purga de `mimalloc` será considerado una regresión crítica.
3. **Fluidez de Renderizado e Interactividad**:
   - El hilo principal de la interfaz jamás debe ser bloqueado por llamadas síncronas de red, SQLite o decodificación de video.
   - El desplazamiento de pestañas debe mantener la arquitectura *Zero Layout Shift*: el contenedor base de pestañas nunca debe cambiar sus coordenadas o dimensiones al mostrar u ocultar botones de navegación.

---

## 2. Convenciones de Concurrencia y Asincronía

- **Runtime de Tokio**:
  - Tokio está configurado con 4 workers (`TOKIO_WORKER_THREADS = 4`). No aumentes este valor a ciegas, ya que cada worker thread crea un set de arenas de memoria en `mimalloc`.
  - Las tareas de red (InnerTube, descarga de miniaturas, chat) deben ejecutarse mediante `Task::perform` o `tokio::spawn`.
  - Las operaciones de disco o SQLite (como la lectura de cookies de Firefox/Chrome) deben encapsularse obligatoriamente en `tokio::task::spawn_blocking`.

---

## 3. Integración con libmpv (`src/player.rs`)

- **Buffers de Video**:
  - El motor utiliza triple-buffering sin preasignación (`3 slots`). No agregues buffers adicionales a menos que se justifique con benchmarks.
  - Para ajustes de caché de red, utiliza la función centralizada `set_stream_buffer_profile(is_high_res, is_low_bandwidth)`. No fijes valores estáticos de cientos de megabytes en `demuxer-max-bytes`.
- **Scripts y Módulos Internos**:
  - Mantén deshabilitados los scripts Lua internos (`load-scripts=no`, `osc=no`, `osd-bar=no`) para preservar la memoria del proceso.

---

## 4. Estilo de Código y Reglas de Sanitización

- **Privacidad y Datos de Usuario**:
  - No incluyas rutas absolutas que apunten a directorios personales (ej. `/home/<usuario>`). Utiliza siempre `dirs::home_dir()` o `dirs::config_dir()`.
  - No incluyas identificadores específicos de hardware personal (ej. modelos de procesador concretos o drivers forzados sin detección).
  - No añadas credenciales, tokens o archivos `.cookies` / `.sqlite` al repositorio.
- **Compilación y Reemplazo de Binarios**:
  - Al reemplazar el binario compilado en directorios locales (`~/.local/bin/mitub`), utiliza siempre `install -m 755` en lugar de `cp`. Esto evita errores de "fichero de texto ocupado" (*ETXTBSY*) si la aplicación se encuentra en ejecución.

---

## 5. Verificación de Cambios

Antes de dar por completada una tarea:
1. Ejecuta `cargo check --release` para comprobar tipos y compilación limpia.
2. Ejecuta `cargo test --release` para validar la suite de pruebas.
3. Verifica que `git status` no contenga archivos temporales, logs o volcados de base de datos sin rastrear.
