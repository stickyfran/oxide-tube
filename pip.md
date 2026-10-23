# Documentación Técnica Completa: Picture-in-Picture (PiP) en Mitub

Este documento detalla exhaustivamente todas las lógicas, comportamientos, arquitecturas e integraciones del sistema **Picture-in-Picture (PiP)** implementado en **Mitub**.

---

## 1. Arquitectura y Ciclo de Vida del PiP

El sistema de PiP en Mitub está diseñado como una ventana secundaria nativa e independiente administrada por el runtime multi-ventana de `iced` (0.13), acoplada directamente al reproductor de video de cero-copia (MPV / Wayland).

```
 ┌─────────────────────────────────────────────────────────┐
 │                   Ventana Principal                     │
 │  - Iced Main Loop                                       │
 │  - Barra de pestañas y navegación                       │
 │  - Watch View (reemplazada con tarjeta de estado PiP)   │
 └────────────────────────────┬────────────────────────────┘
                              │
             Disparo manual o automático (Alt+Tab / Tab Switch)
                              ▼
 ┌─────────────────────────────────────────────────────────┐
 │                 Ventana Flotante (PiP)                  │
 │  - Window ID independiente (`pip_window_id`)            │
 │  - Always-on-top & Sin decoraciones nativas             │
 │  - Composición directa de texturas GPU vía WGPU shader  │
 │  - OSD superpuesto con auto-hide                        │
 │  - KWin Scripting & Reglas de Compositor Wayland        │
 └─────────────────────────────────────────────────────────┘
```

### 1.1. Estado Interno en `MitubApp` (`src/main.rs`)
- `pip_window_id: Option<iced::window::Id>`: Identificador único asignado por el runtime de Iced a la ventana PiP abierta. Si es `None`, la ventana no existe.
- `pip_window_size: (f32, f32)`: Dimensiones actuales de la ventana PiP en píxeles de pantalla (por defecto `(480.0, 270.0)` para 16:9 o `(240.0, 426.0)` para videos verticales/Shorts).
- `pip_cursor_hovered: bool`: Rastrea si el cursor del mouse está dentro del perímetro del PiP para mostrar/ocultar el OSD y controles.
- `pip_watch_details: Option<WatchDetails>`: Metadatos del video que está reproduciéndose en PiP (conservados incluso si el usuario navega a otra vista o cierra la pestaña de origen).
- `auto_pip_triggered_by_unfocus: bool`: Bandera booleana que indica si el PiP fue desplegado automáticamente por un cambio de foco del sistema operativo (Alt+Tab) para saber si debe cerrarse y regresar a la app al retornar el foco.

---

## 2. Apertura y Creación de la Ventana PiP (`Message::TogglePip`)

Cuando se solicita entrar en modo PiP (vía botón en el reproductor, tecla de acceso directo, cambio de pestaña o evento de foco):

1. **Guardia de Estado**:
   - Si la ventana PiP ya está abierta (`self.pip_window_id.is_some()`), `TogglePip` delega directamente en `Message::ClosePip`.
   - Si no hay ningún video cargado (`self.watch_details.is_none()`), no realiza ninguna acción.
2. **Cálculo de Proporción (Aspect Ratio)**:
   - Se examina la resolución del stream de video seleccionado (`width` y `height`).
   - Si `ratio < 1.0` (formato vertical / Shorts / TikTok), la ventana se inicia con dimensiones adaptadas a teléfonos (`240.0 x 426.0`).
   - Si es panorámico tradicional (16:9), se calcula sobre una base de `480.0 x 270.0`.
   - Si el usuario previamente redimensionó el PiP, se respeta prioritariamente `self.config.pip_size` guardado en disco (`min_size`: 240x140).
3. **Cálculo de Posición Inicial**:
   - Se verifica si existe una posición guardada en `self.config.pip_position`.
   - Si no existe en la configuración, se consulta la última posición conocida en KWin vía `get_kwin_pip_position()`.
   - Si no hay historial previo, se posiciona por defecto en el centro o en la esquina inferior derecha mediante KWin Scripting.
4. **Configuración de Ventana en Iced (`window::Settings`)**:
   - `level: window::Level::AlwaysOnTop`: Ordena a la ventana mantenerse por encima de todas las demás aplicaciones.
   - `decorations: false`: Elimina las barras de título tradicionales del sistema operativo para un diseño ultra-minimalista.
   - `transparent: true`: Habilita transparencia alfa para bordes redondeados limpios.
   - `resizable: true`: Permite la interacción con el redimensionamiento.
5. **Enlace Wayland de la Superficie (`Message::PipHandleAcquired`)**:
   - Con la tarea asíncrona `window::run_with_handle`, se extrae el puntero crudo a la superficie Wayland (`wl_surface`).
   - El reproductor `WaylandZeroCopyPlayer` actualiza sus identificadores (`update_handles`), establece el modo de subventana (`set_underlay(true)`), ajusta el viewport al tamaño exacto de la ventana PiP y continúa la reproducción en el mismo milisegundo exacto (`player.get_position_ms()`).
6. **Manejo de Pantalla Completa**:
   - Si la ventana principal estaba en modo Fullscreen, se sale ordenadamente a modo ventana antes de abrir el PiP para evitar colisiones del compositor.

---

## 3. Cierre y Restauración

### 3.1. Cierre y Reacoplamiento (`Message::ClosePip`)
Al presionar el botón de cerrar `[X]`, la tecla `Esc` dentro del PiP o cambiar de vuelta a la pestaña del video:
1. Se limpia la bandera `auto_pip_triggered_by_unfocus = false`.
2. Se consulta la geometría final del PiP al compositor (`save_current_kwin_pip_position`) para recordar dónde quedó y su tamaño exacto.
3. Se toma el `pip_window_id` y se emite la orden de destrucción de ventana `window::close(pip_id)`.
4. Se reasigna el video en reproducción a la vista principal:
   - Se restablece `watch_details`.
   - Si la vista actual no es de reproducción, se conmuta a `ViewMode::Watch(video_id)`.
   - Se activa la pestaña correspondiente si aún existe en la barra superior.
5. Se reconecta el reproductor a la superficie Wayland de la ventana principal (`switch_surface(main_surf)`), sincronizando el viewport de la ventana madre sin pausar ni desincronizar audio.

### 3.2. Restauración Rápida con Doble Clic (`Message::RestoreFromPip`)
Hacer doble clic sobre la imagen del video en PiP no cierra el reproductor bruscamente, sino que:
1. Invoca `activate_kwin_main_window()` mediante KWin Scripting (desminimizando y activando la ventana `mitub`). Como guardia defensiva, solo invoca `wmctrl` si el binario existe en `/usr/bin/wmctrl`.
2. Solicita foco y desminimizado nativo en Iced (`window::minimize(main_id, false)` y `window::gain_focus(main_id)`).
3. Ejecuta `ClosePip`, produciendo una transición visual fluida desde el flotante directamente a la aplicación en pantalla principal.

---

## 4. Automatización Inteligente (Multitarea y Pestañas)

### 4.1. PiP Automático al Cambiar de Ventana (Alt+Tab)
- **Configuración**: Conmutable desde el menú de Ajustes (`self.config.auto_pip_on_unfocus`), guardada persistentemente en `config.json`.
- **Detección de Pérdida de Foco (`window::Event::Unfocused`)**:
  - Si el evento ocurre en la ventana principal, la opción está activa, no hay PiP abierto y hay un video activo reproduciéndose (`player.is_playing() || player.get_position_ms() > 0`), el sistema activa automáticamente `Message::TogglePip` y marca `auto_pip_triggered_by_unfocus = true`.
- **Detección de Retorno de Foco (`window::Event::Focused`)**:
  - Si la ventana principal vuelve a recibir foco y la bandera `auto_pip_triggered_by_unfocus` está encendida, Mitub interpreta que el usuario regresó a la app y cierra automáticamente el PiP reintegrando el video en la interfaz principal.

### 4.2. Cambio de Pestañas con Reproducción Activa
- Cuando el usuario tiene un video reproduciéndose y hace clic en otra pestaña (o abre una nueva pestaña vacía con `Ctrl+T` / botón `+`):
  - El video activo **no se detiene ni se pausa**.
  - Se genera automáticamente una tarea `self.update(Message::TogglePip)`, enviando el video a la esquina de la pantalla mientras la ventana principal muestra el inicio, suscripciones o el contenido de la nueva pestaña.
  - Al regresar a la pestaña del video original, el sistema detecta que el ID coincide con el video en PiP y lo devuelve de inmediato a la ventana principal (`ClosePip`).

### 4.3. Tarjeta Placeholder en la Vista de Reproducción
- Cuando el PiP está activo y el usuario se encuentra en la vista del video en la ventana principal, el área donde usualmente se dibuja el reproductor muestra una tarjeta elegante (`view_watch_page`):
  - Icono de Picture-in-Picture.
  - Mensaje descriptivo: *"Reproduciendo en ventana flotante (PiP)"*.
  - Botón directo *"Reacoplar video aquí"* con llamada a `Message::TogglePip`.
  - El fondo se vuelve opaco para evitar que el renderizado de la ventana principal interfiera con la ventana flotante.

---

## 5. Controles e Interacción en la Ventana PiP (`view_pip`)

La ventana flotante cuenta con una interfaz superpuesta completa renderizada en tiempo real mediante `iced::widget::stack`:

```
┌────────────────────────────────────────────────────────┐
│                                                  [ X ] │  <-- Top Bar Overlay (Close/Dock)
│                                                        │
│                                                        │
│                  ÁREA DE VIDEO (WGPU)                  │
│               - 1 Clic sostenido: Arrastrar Ventana    │
│               - Doble Clic: Restaurar App              │
│               - Rueda del Mouse: Zoom In / Zoom Out    │
│               - Arrastre en Bordes y Esquinas (8 ejes) │
│                                                        │
├────────────────────────────────────────────────────────┤
│ •  02. Introducción al proyecto...                     │  <-- Fila Dedicada de Capítulos
│ [███████████████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░] │  <-- Barra Scrubber
│ [-10s] [ ▶ / ⏸ ] [+10s]  02:15 / 14:30                 │  <-- Fila de Controles Multimedia
└────────────────────────────────────────────────────────┘
```

### 5.1. Barra Superior (Top Bar)
- **Botón `[ X ]`**: Cierra el PiP y reacopla el video a la app principal (`Message::ClosePip`). Estilo botón circular con fondo translúcido y resaltado rojo al pasar el cursor. (Los antiguos botones `[-]` y `[+]` fueron removidos para dar paso al redimensionamiento natural por bordes y rueda del ratón).

### 5.2. Visualización Inteligente y Dedicada de Capítulos
- **Fila Superior de Alto Contraste**: El capítulo activo cuenta con una fila dedicada de ancho completo justo arriba de la barra de progreso, evitando cualquier corte o solapamiento con los botones multimedia.
- **Interacción Dinámica con Hover**:
  - Mientras el cursor se desplaza por la barra de tiempo (`hover_seek_ratio`), el capítulo mostrado **cambia dinámicamente en tiempo real** para reflejar el capítulo exacto del segundo señalado.
  - Al estar señalando con el mouse, el texto del capítulo se resalta en blanco brillante de alto contraste (`#ffffff`); en reproducción normal permanece en tono secundario.
  - El título del capítulo se adapta de forma responsiva al ancho de la ventana PiP (`max_ch_len` entre 26 y 65 caracteres con elipsis `...`).

### 5.3. Barra de Progreso y Scrubber Interactivo
- **Canvas `ProgressBarProgram`**: Dibuja el progreso de reproducción actual, el búfer precargado y las marcas divisorias con gaps y relieves entre capítulos.
- **Hover Dinámico**: Al pasar el mouse sobre la barra, calcula `hover_seek_ratio`, mostrando el tiempo proyectado entre paréntesis: `02:15 (05:40) / 14:30`.
- **Arrastre de búsqueda (Seek Dragging)**: Hacer clic y deslizar sobre la barra busca de forma continua e instantánea en el reproductor MPV mediante `seek_ms(target_secs * 1000)`.

### 5.4. Botones Multimedia
- **`[-10s]`**: Retrocede 10 segundos en la reproducción.
- **`[Play / Pause]`**: Conmuta entre reproducción y pausa, actualizando el inhibidor de salvapantallas del sistema operativo (D-Bus).
- **`[+10s]`**: Adelanta 10 segundos en la reproducción.
- **Indicador de Tiempo**: Formato nítido `02:15 / 14:30` (o `02:15 (05:40) / 14:30` en hover).

### 5.5. Arrastre Directo de Ventana (Window Drag)
- **Capa `drag_area`**: Un widget `mouse_area` ubicado directamente sobre el cuerpo del video captura los clics izquierdos fuera de los botones u OSD e invoca de inmediato la orden nativa del protocolo Wayland `window::drag(pip_id)`, transmitiendo el serial del puntero al compositor para un desplazamiento nativo suave y sin latencia.
- **Doble Clic Rápido (<350ms)**: Restaura la ventana principal de Mitub a primer plano y cierra el PiP (`RestoreFromPip`).

### 5.6. Auto-Ocultación de Controles (Auto-Hide OSD)
- Los controles flotantes superior e inferior sólo se muestran si:
  - El cursor está sobre la ventana (`self.pip_cursor_hovered == true`).
  - El video se encuentra en pausa (`!self.osd_state.is_playing`).
  - Hay una interacción de OSD activa (`self.osd_state.is_visible`).
- Si el cursor abandona la ventana (`CursorLeft`), el OSD se desvanece de inmediato para dejar la visualización del video 100% limpia.

---

## 6. Redimensionamiento y Persistencia de Geometría

### 6.1. Métodos de Escalado Soportados
1. **Redimensionamiento Nativo en Bordes y Esquinas (8 Ejes)**:
   - Al posicionar el cursor sobre cualquiera de los 4 bordes (derecho, izquierdo, superior, inferior) o las 4 esquinas de la ventana PiP, el cursor del sistema cambia automáticamente al cursor nativo de redimensionamiento (`⬌`, `⬍`, `⤡`, `⤢`) mediante `iced::mouse::Interaction::Resizing*`.
   - Al hacer clic sostenido y arrastrar, la ventana se redimensiona en vivo preservando fielmente la relación de aspecto del video (sin bandas negras ni distorsión) en base al vector de movimiento de cada eje (`PipResizeEdge`).
2. **Rueda del Ratón (`WheelScrolled`)**: Al girar la rueda sobre cualquier sector de la ventana PiP, se aplica un escalado progresivo:
   - Rueda hacia arriba: Aumenta un 10% (`ScalePip(1.10)`).
   - Rueda hacia abajo: Reduce un 10% (`ScalePip(0.90)`).
3. **Persistencia Automática**: Al liberarse el clic (`ButtonReleased`) o redimensionar mediante el gestor de ventanas, se guardan las nuevas dimensiones en `config.pip_size` y en las reglas de KWin.

### 6.2. Arquitectura de Debouncing (Prevención de Micro-Tirones)
Para evitar degradación de rendimiento y micro-stuttering:
- Durante el escalado interactivo continuo (vía rueda del mouse o arrastre de bordes):
  - Las dimensiones se actualizan de forma instantánea en memoria (`self.pip_window_size` y `self.config.pip_size`).
  - El reproductor actualiza su viewport inmediatamente (`player.set_viewport`) y emite `window::resize`.
  - **No se escribe en disco ni se dispara `qdbus6 org.kde.KWin reconfigure` en cada muesca del ratón o píxel de arrastre**.
- La persistencia pesada hacia `.config/kwinrulesrc` y el reconfigurado de KWin se consolidan de manera diferida (*lazy persistence*) al soltar el botón del mouse (`ButtonReleased`) o al cerrar el PiP (`ClosePip`), protegiendo el disco y la fluidez del compositor.

---

## 7. Integración Profunda con KDE Plasma / KWin (Wayland)

Debido a que Wayland restringe estrictamente que los clientes decidan su posición absoluta o manipulen otras ventanas por motivos de seguridad, Mitub implementa una pasarela directa mediante **KWin Scripting** y el archivo de reglas del gestor de ventanas de KDE (`kwinrulesrc`).

### 7.1. Reglas en `.config/kwinrulesrc` (`ensure_kwin_pip_rule`)
Se escribe una regla específica para la ventana con título `Mitub - PiP` y clase de aplicación `mitub`:
```ini
[mitub-pip]
above=true
aboverule=3
acceptfocus=true
acceptfocusrule=3
decorationpolicy=none
decorationpolicyrule=3
description=Mitub PiP
desktops=
desktopsrule=3
focusstealing=0
focusstealingrule=3
position=1420,780
positionrule=2
size=480,270
sizerule=2
title=Mitub - PiP
titlematch=1
types=1
wmclass=mitub
wmclassmatch=1
```
Tras escribir el archivo de reglas, se emite una señal D-Bus para recargar KWin sin reiniciar la sesión:
```bash
qdbus6 org.kde.KWin /KWin org.kde.KWin.reconfigure
```

### 7.2. Scripting para Keep-Above y Geometría en Vivo (`enforce_kwin_pip_keep_above`)
Al crearse la superficie Wayland, se ejecuta un hilo en segundo plano que prueba a los 60ms, 220ms y 450ms un script ECMAScript en el motor interno de KWin:
- Localiza la ventana que cumple `resourceClass === "mitub"` y caption `"Mitub - PiP"`.
- Fuerza `w.keepAbove = true` (siempre encima de ventanas normales y maximizadas).
- Fuerza `w.onAllDesktops = true` (permanece visible al cambiar de escritorio virtual).
- Fuerza `w.noBorder = true`.
- Si no tenía posición previa guardada, calcula el área del monitor (`workspace.clientArea(0, w)`) y ubica la ventana exactamente en la **esquina inferior derecha** dejando 24 píxeles de margen respecto al panel de tareas.

### 7.3. Consulta de Posición y Tamaño al Cerrar (`query_kwin_pip_geometry`)
Dado que el protocolo Wayland no siempre notifica a la aplicación cliente dónde la movió el usuario mediante el compositor:
- Mitub inyecta temporalmente `/tmp/mitub_kwin_query_pip.js`.
- El script lee `w.frameGeometry.x`, `w.frameGeometry.y`, `w.frameGeometry.width`, `w.frameGeometry.height`.
- Escribe a la bitácora de usuario `console.info("MITUB_PIP_POS:x,y,w,h")`.
- Mitub lee la línea vía `journalctl --user -n 20 --no-pager`, actualizando la configuración para que la próxima apertura sea idéntica al píxel.

---

## 8. Optimización de Consumo de Energía y Ticks

### 8.1. Reloj de Presentación Adaptativo
- **Durante Reproducción Activa en PiP**:
  - Los frames de video se componen y presentan a través del shader personalizado WGPU (`video_widget::video_shader(frame)`).
  - Por esta razón, el reloj de presentación se mantiene a 16 ms (60 FPS) mientras `self.osd_state.is_playing == true`, asegurando que no haya judder de video ni caída de fluidez perceptible en la ventana flotante.
- **En Pausa o Estado Idle**:
  - Si el video en PiP está en pausa (`!self.osd_state.is_playing`) y no hay animaciones activas (ni arrastre de búsqueda ni animaciones de seek), el tick rate disminuye de inmediato a 250 ms (o 400 ms en modo Batería).
- **En Segundo Plano sin PiP**:
  - Si la ventana principal no tiene foco y el PiP está cerrado, el bucle entra en ahorro profundo a 2000 ms (2 segundos).

### 8.2. Eficiencia de Decodificación Cero-Copia
- La decodificación por hardware se realiza mediante libmpv con aceleración VA-API. Los buffers RGBA reciclables (pool estático sin asignaciones continuas en heap) transfieren las texturas al pipeline de WGPU con un uso global de CPU típicamente inferior al 1-2%.

---

## 9. Tabla Resumen de Atajos e Interacciones PiP

| Acción / Evento | Origen | Comportamiento |
| :--- | :--- | :--- |
| **Clic en Botón PiP** | OSD / Barra de controles | Abre o cierra la ventana flotante PiP. |
| **Doble Clic en Video** | Superficie de video en PiP | Desminimiza y enfoca la app principal cerrando el PiP. |
| **1 Clic Sostenido y Arrastre (Video)** | Superficie de video en PiP | Mueve la ventana libremente por la pantalla (Wayland native drag). |
| **Arrastre en Bordes o Esquinas** | Bordes de ventana PiP | Redimensiona interactivamente la ventana preservando el aspect ratio. |
| **Scroll Rueda Ratón Arriba** | Ventana PiP | Agranda la ventana un 10% de forma instantánea en memoria. |
| **Scroll Rueda Ratón Abajo** | Ventana PiP | Achica la ventana un 10% de forma instantánea en memoria. |
| **Botón `[X]` o tecla `Esc`** | Ventana PiP | Cierra el PiP, sincroniza KWin y reacopla el video a la ventana principal. |
| **Alt + Tab (Desenfocar app)** | Sistema Operativo | Si está activo en Ajustes, envía automáticamente el video a PiP. |
| **Regresar a Mitub tras Alt+Tab** | Sistema Operativo | Si se abrió por Alt+Tab, se cierra el PiP y regresa a la app. |
| **Cambiar a otra Pestaña** | Barra de pestañas | Si hay reproducción activa, se abre PiP para continuar viendo el video. |
| **Hover sobre Barra Scrubber** | Barra de progreso en PiP | Muestra tiempo objetivo e ilumina el nombre del capítulo señalado. |

---

## 10. Análisis de Auditoría y Mitigaciones Implementadas

A partir de la auditoría técnica de arquitectura, se identificaron y resolvieron los siguientes puntos críticos:

### 10.1. Eliminación de Micro-Tirones por Escritura en Disco y D-Bus
- **Problema previo**: Se invocaba `ensure_kwin_pip_rule` (reescritura de `.config/kwinrulesrc` + llamada a `qdbus6 org.kde.KWin.reconfigure`) en cada muesca del scroll (`WheelScrolled`) y en cada píxel del evento `Resized`.
- **Mitigación aplicada**: Se desacopló la persistencia de reglas de KWin del ciclo interactivo. El escalado en vivo opera exclusivamente en memoria y en el reproductor. La sincronización con KWin ocurre una única vez al cerrar el PiP (`Message::ClosePip`) o al inicializarlo.

### 10.2. Protección de Reglas de Ventana contra Colisiones
- **Problema previo**: La regla de KWin dependía exclusivamente del título (`title=Mitub - PiP`), lo que podía generar falsos positivos con navegadores o ventanas de terceros con títulos similares.
- **Mitigación aplicada**: Se agregó la especificación estricta de clase de ventana `wmclass=mitub` con `wmclassmatch=1`, restringiendo la regla exclusivamente a las ventanas generadas por el proceso binario de Mitub.

### 10.3. Seguridad en Entornos Wayland Puros
- **Problema previo**: Invocación incondicional de `wmctrl` (herramienta dependiente de X11/XWayland).
- **Mitigación aplicada**: Se encapsuló la invocación detrás de la verificación de existencia del binario en `/usr/bin/wmctrl`, priorizando KWin Scripting nativo para desminimizado y foco.

### 10.4. Hoja de Ruta Futura (Arquitectura Nivel Industrial)
1. **IPC Nativo D-Bus para Geometría**: Reemplazar la lectura de `journalctl` por una señal D-Bus emitida directamente desde el script KWin (`callDBus("org.mitub.App", "UpdateGeometry", ...)`).
2. **Abstracción Multi-Compositor**: Encapsular la lógica de gestión de ventanas detrás de un trait `WindowManagerIntegration` con conectores para KWin (KDE), Ext-Foreign-Toplevel / IPC (Sway, Hyprland) y degradación básica estándar (GNOME).
