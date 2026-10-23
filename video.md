# Arquitectura y Funcionamiento del Reproductor de Video en Mitub (`libmpv` + EGL/OpenGL + Wayland)

Este documento detalla en profundidad el diseño técnico, la arquitectura de renderizado, la aceleración por hardware (VA-API), la gestión de memoria y el ciclo de vida del reproductor de video nativo integrado en **Mitub**.

---

## 1. Contexto y Motivación del Cambio (GStreamer vs `libmpv`)

Originalmente, Mitub utilizaba un pipeline híbrido basado en **GStreamer** (`playbin3` / `appsink` / `waylandsink`):
- **Problema de CPU:** GStreamer instanciaba hasta **11 hilos de decodificación por software** (`dav1d-worker`), consumiendo entre un **25% y 35% de CPU** en videos AV1/VP9 a 1080p60.
- **Problema de Memoria (RAM):** La acumulación de colas internas de GStreamer (`multiqueue`, `appsrc`, búferes sin reciclar) disparaba el consumo de memoria hasta **524 MB RSS**.
- **Incompatibilidad Wayland Subsurfaces:** Al intentar hacer overlay de la interfaz Iced sobre una subsuperficie de Wayland, el compositor de KDE Plasma (KWin) generaba parpadeos, pérdidas de foco y desincronización de coordenadas.

### Solución adoptada
Se diseñó un motor basado en **`libmpv`** empotrado en Rust mediante FFI directa con renderizado **EGL/OpenGL FBO**, que:
1. Delega la decodificación directamente a la GPU Intel Arc integrada (Meteor Lake) vía **VA-API** (`iHD_drv_video.so`).
2. Elimina el 100% de los hilos de software `dav1d-worker`.
3. Reduce el uso de CPU a menos del **2%** y el consumo de memoria a **~310–340 MB**.

---

## 2. Diagrama de la Arquitectura de Renderizado

```
+-----------------------------------------------------------------------------------+
|                              PROCESO MITUB (RUST)                                 |
|                                                                                   |
|  +--------------------+        libmpv FFI         +----------------------------+  |
|  |   InnerTube API    | ------------------------> |        mpv_handle          |  |
|  | (URLs DASH / HTTPS)|                           | (Demuxer, Audio, VA-API)   |  |
|  +--------------------+                           +----------------------------+  |
|                                                                 |                 |
|                                                     Decodificación Hardware       |
|                                                     (Intel VA-API / iHD driver)   |
|                                                                 v                 |
|  +-----------------------------------------------------------------------------+  |
|  |                          HILO MPV WORKER DEDICADO                           |  |
|  |                                                                             |  |
|  |  1. Contexto EGL Headless (Pbuffer 16x16 dummy ligado a wl_display)         |  |
|  |  2. FBO Dedicado: glGenFramebuffers + Textura RGBA8 (1920x1080)             |  |
|  |  3. mpv_render_context_render() dibuja directamente al FBO en GPU           |  |
|  |  4. glBindFramebuffer(GL_FRAMEBUFFER, fbo) [Re-enlace crítico]              |  |
|  |  5. glReadPixels() lee textura a Ring Buffer atómico (3 ranuras Arc<[u8]>) |  |
|  +-----------------------------------------------------------------------------+  |
|                                         |                                         |
|                             Publicación atómica de frame                          |
|                                         v                                         |
|  +-----------------------------------------------------------------------------+  |
|  |                         HILO PRINCIPAL ICED (GUI)                           |  |
|  |                                                                             |  |
|  |  - iced::widget::image(Handle::from_rgba(w, h, bytes))                      |  |
|  |  - Canvas Stack Overlay (Controles OSD, Barra de progreso, Timeline, PiP)   |  |
|  |  - Renderizado compuesto unificado en GPU (WGPU / Wayland)                  |  |
|  +-----------------------------------------------------------------------------+  |
+-----------------------------------------------------------------------------------+
```

---

## 3. Inicialización del Pipeline EGL / OpenGL Headless

Para que `libmpv` active la aceleración por hardware VA-API sin requerir una ventana de MPV externa, creamos un contexto OpenGL sobre EGL en un hilo en segundo plano (`mitub-mpv-worker`):

1. **Conexión a Wayland:**
   ```c
   struct wl_display *wl_dpy = wl_display_connect(NULL);
   ```
   Pasar el puntero de `wl_display` a `libmpv` vía `MPV_RENDER_PARAM_WL_DISPLAY` es el requisito fundamental de Intel/Mesa para inicializar `vaGetDisplayWGL` / `vaGetDisplayDRM` y permitir el mapeo de texturas zero-copy en Linux.

2. **Inicialización EGL:**
   - Obtenemos el `EGLDisplay` a partir de `wl_dpy`.
   - Elegimos una configuración con soporte para `EGL_PBUFFER_BIT` y `EGL_OPENGL_BIT`.
   - Creamos un contexto OpenGL de escritorio (`EGL_OPENGL_API`) y una superficie pbuffer mínima (16×16) para volver el contexto activo (`eglMakeCurrent`).

3. **Frame Buffer Object (FBO) de Destino:**
   - Creamos una textura GL de tamaño dinámico (`current_buf_w × current_buf_h`) con formato `GL_RGBA8`.
   - Creamos un Framebuffer Object con `glGenFramebuffers`.
   - Asociamos la textura como color attachment 0:
     ```c
     glFramebufferTexture2D(GL_FRAMEBUFFER, GL_COLOR_ATTACHMENT0, GL_TEXTURE_2D, tex, 0);
     ```

---

## 4. Diagnóstico y Correcciones Clave

### A. La Pantalla Negra (The Framebuffer Unbind Gotcha)
- **Causa:** Al llamar a `mpv_render_context_render()`, MPV dibuja el cuadro de video en nuestro FBO, pero **por diseño restaura el estado previo de OpenGL**, dejando `GL_READ_FRAMEBUFFER = 0` (el pbuffer vacío de 16×16).
- **Efecto:** La posterior llamada a `glReadPixels(0, 0, w, h, ...)` leía píxeles fuera de rango del pbuffer 0, devolviendo únicamente ceros `[0, 0, 0, 255]` (imagen completamente negra).
- **Solución implementada:**
  Inmediatamente antes de `glReadPixels`, re-enlazamos de manera explícita nuestro FBO:
  ```rust
  if let Some(bind_fb) = gl_bind_fb {
      bind_fb(GL_FRAMEBUFFER, fbo);
  }
  glReadPixels(0, 0, current_buf_w as i32, current_buf_h as i32, GL_RGBA, GL_UNSIGNED_BYTE, target_ptr as _);
  ```

### B. Orientación del Video (Inversión y Espejado)
- **Comportamiento de MPV Render GL:**
  Cuando MPV renderiza a un FBO con `MPV_RENDER_PARAM_OPENGL_FBO`, proyecta el video de acuerdo al convenio habitual de memoria de lectura secuencial.
- **Por qué se veía al revés:**
  El uso del parámetro `MPV_RENDER_PARAM_FLIP_Y = 1` le ordenaba a MPV invertir la matriz de proyección Y, lo que generaba una rotación completa de 180° (upside down y texto invertido como espejo).
- **Solución implementada:**
  Se eliminó `MPV_RENDER_PARAM_FLIP_Y` y se mantiene la lectura directa de bytes. El fotograma resultante queda en orientación vertical correcta y con orientación de texto normal (izquierda a derecha).

---

## 5. Ring Buffer Atómico y Cero Contención

Para transferir los fotogramas decodificados desde el hilo de MPV hacia el motor de renderizado de Iced sin pausas de sincronización (lock contention):

1. **Estructura de 3 Ranuras:**
   ```rust
   let mut ring_buffers: [Arc<[u8]>; 3] = [
       vec![0u8; size].into(),
       vec![0u8; size].into(),
       vec![0u8; size].into(),
   ];
   ```
2. **Selección por Contador de Referencias:**
   El hilo de MPV busca una ranura cuyo `Arc::strong_count == 1` mediante `Arc::get_mut()`. Si la interfaz de usuario de Iced todavía está sosteniendo la textura del fotograma anterior, MPV escribe en una ranura libre sin bloquear a la interfaz ni esperar un mutex.
3. **Conversión a Handle de Iced:**
   ```rust
   let byte_buf = bytes::Bytes::copy_from_slice(b_slice);
   let handle = iced::widget::image::Handle::from_rgba(current_buf_w, current_buf_h, byte_buf);
   *current_frame.safe_lock() = Some(handle);
   ```

---

## 6. Eficiencia Energética y Gestión de Memoria

### A. Optimización Meteor Lake (Intel Core Ultra 7 155H)
El módulo `power_manager.rs` detecta la fuente de alimentación del sistema mediante `/sys/class/power_supply`:
- **Modo Batería:**
  - Aplica afinidad de hilos (thread pinning) para anclar la decodificación a los **LP E-Cores (núcleos 20 y 21)** de bajo consumo.
  - Ajusta la tasa de refresco del OSD en reproducción estable a 20 FPS (50 ms).
- **Modo Corriente AC:**
  - Desbloquea todos los núcleos para máxima respuesta y eleva el refresco OSD a 30 FPS (33 ms).

### B. Recolección de Memoria (`mimalloc`)
Para evitar que el cache de texturas y el desempacado de tramas acumulen fragmentación:
- Se invoca `mi_collect(true)` periódicamente cada 4 segundos durante `Message::Tick`.
- Se liberan los búferes de thumbnail viejos y se acotan los ring buffers al tamaño exacto de la resolución nativa del video (`clamp(320..=1920, 180..=1080)`).

---

## 7. Tabla Comparativa de Rendimiento

| Indicador | Reproductor Previo (GStreamer) | Reproductor Actual (libmpv + FBO) |
|---|---|---|
| **Motor de Reproducción** | GStreamer `playbin3` | `libmpv` C API (FFI nativa) |
| **Decodificación Hardware** | Inconsistente (Fallback a software) | Directa vía **VA-API** (`iHD_drv_video.so`) |
| **Hilos de decodificación CPU** | 11 hilos `dav1d-worker` | **0 hilos** (eliminados) |
| **Carga de CPU típica (1080p60)** | 25% – 35% | **0.9% – 2.4%** |
| **Memoria RSS en reproducción** | 524 MB | **~310 – 340 MB** |
| **Soporte PiP y OSD** | Subsuperficies complejas con parpadeo | Composición nativa Iced Stack (fluido) |

---

## 8. Roadmap Evolutivo de Optimización (Fases 1 a 4)

| Fase | Objetivo Principal | Tráfico en Bus RAM (1080p60) | Uso de CPU Estimado | Estado en Mitub |
|---|---|---|---|---|
| **Base** | `glReadPixels` síncrono + `copy_from_slice` | ~1.5 GB/s (GPU → Buf → Bytes → GPU) | 0.9% – 2.4% | Superada |
| **Fase 1** | Pool reciclable `RecycledBuf` (`Bytes::from_owner`) | ~1.0 GB/s (GPU → Buf → GPU) | ~0.5% – 1.2% | **Implementada** |
| **Fase 2** | Lectura DMA Asíncrona (Doble PBO Ping-Pong) | ~1.0 GB/s (Sin bloqueo de hilo GL) | ~0.3% – 0.8% | **Implementada** |
| **Fase 3** | Custom `VideoWidget` Shader (Textura Persistente WGPU) | ~370 MB/s (Cero recreación de texturas) | **0.2% – 0.4%** | **Implementada** |
| **Fase 4** | True Zero-Copy (`DMA-BUF` → `wgpu-hal` / Vulkan) | **0 MB/s** (100% interno en GPU Intel Arc) | **< 0.1%** | Próxima fase |

### Fase 1: Erradicación de Copias en RAM (Zero-Allocation Loop)
- **Problema actual:** `Bytes::copy_from_slice` reserva memoria nueva en cada frame (`~8 MB` a 1080p60, lo que genera una tasa de reserva de ~480 MB/s). Para mitigar la fragmentación, el sistema ejecutaba `mi_collect(true)` periódicamente.
- **Solución:** Implementar un pool de reciclaje con `Bytes::from_owner(RecycledFrame)`:
  ```rust
  struct RecycledFrame {
      data: Vec<u8>,
      return_tx: crossbeam_channel::Sender<Vec<u8>>,
  }
  impl AsRef<[u8]> for RecycledFrame {
      fn as_ref(&self) -> &[u8] { &self.data }
  }
  impl Drop for RecycledFrame {
      fn drop(&mut self) {
          let buf = std::mem::take(&mut self.data);
          let _ = self.return_tx.try_send(buf);
      }
  }
  ```
- **Resultado:** Las asignaciones en heap durante la reproducción caen a **cero absoluto**. Se elimina la necesidad de `mi_collect(true)`.

### Fase 2: Transferencia DMA Asíncrona con PBOs (Ping-Pong)
- **Problema actual:** `glReadPixels` a un puntero de CPU bloquea el hilo de ejecución hasta que la GPU finaliza el renderizado del fotograma.
- **Solución:** Crear 2 Pixel Buffer Objects (`GL_PIXEL_PACK_BUFFER`) con `GL_STREAM_READ`. En el frame $N$, se dispara la transferencia DMA asíncrona hacia `pbo[index]`, y se mapea con `glMapBufferRange` el buffer `pbo[1 - index]` del frame previo ($N-1$).
- **Resultado:** Cero esperas activas de CPU; los LP E-Cores pueden trabajar a frecuencias mínimas sin inducir microcortes.

### Fase 3: Widget Shader Custom en Iced (Bypass de `iced::widget::image`)
- **Problema actual:** `iced::widget::image` administra texturas estáticas, reconstruyendo descriptores y liberando texturas descartadas en cada frame.
- **Solución:** Crear un `VideoPrimitive` mediante `iced::widget::shader::Program`. Mantener una única `wgpu::Texture` persistente en el storage de Iced y actualizarla directamente con `queue.write_texture(...)`. Opcionalmente subir texturas directamente en formato **NV12** (12 bpp vs 32 bpp de RGBA8), reduciendo el tráfico de subida en un 62.5%.

### Fase 4: True Zero-Copy (`VA-API` / `EGLImage` → `DMA-BUF` → Vulkan `WGPU`)
- **Arquitectura Definitiva:** En procesadores Meteor Lake (Intel Core Ultra 7 155H), la decodificación VA-API, OpenGL y Vulkan comparten la misma memoria física unificada (UMA).
- **Mecanismo:**
  1. Exportar la textura del FBO de MPV como `DMA-BUF` usando `eglCreateImageKHR` y `eglExportDMABUFImageMESA`.
  2. Implementar triple-buffering de FBOs exportados al inicializar el reproductor.
  3. Importar los descriptores `dma_buf_fd` directamente en WGPU a través de `device.as_hal::<wgpu_hal::api::Vulkan>()` con la extensión `VK_EXT_external_memory_dma_buf`.
  4. Enviar únicamente un índice `u8` (0, 1 o 2) y un fence `EGLSyncKHR` a Iced por cada frame decodificado.
- **Resultado Final:** **0 bytes transitando por el bus de RAM**. 100% de la decodificación, escalado, conversión de color y composición ocurren en el silicio de la GPU Intel Arc. Consumo de CPU < 0.1% a 4K60.
