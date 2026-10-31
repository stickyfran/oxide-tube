# Auditoría Técnica y Especificación de Arquitectura: Rendimiento en Windows, Gestión de Memoria y Soporte 8K en OxideTube

Este documento proporciona un análisis exhaustivo, corregido y contextualizado sobre el comportamiento de **OxideTube** (`tube-oxide`) en Microsoft Windows frente a Linux. Aborda con rigor técnico el cuello de botella del rasterizador por software, la aceleración de video por hardware (GPU), la gestión del Working Set frente a Commit Charge en Windows, y la viabilidad arquitectónica para el soporte de resoluciones ultra-altas (8K / 4320p) sin violar el presupuesto de memoria RAM del proyecto.

---

## 1. Arquitectura Base y Presupuesto de Recursos

OxideTube es un cliente nativo de YouTube desarrollado 100% en Rust. Sus directrices fundamentales (`AGENTS.md`) establecen:
- **Cero Runtime Web**: Sin WebView, Chromium embebido ni Electron. Interfaz renderizada nativamente con `iced 0.13` sobre `wgpu` / DirectX / OpenGL.
- **Presupuesto Estricto de Memoria RAM**:
  - Reposo / Navegación: `< 150 MB`.
  - Reproducción activa (1080p / 4K): `< 350 MB`.
- **Motor Multimedia**: Integración con `libmpv` mediante FFI directo en [`src/player.rs`](file:///src/player.rs).
- **Asignador Global de Memoria**: `mimalloc` (`mimalloc::MiMalloc`), con políticas de purga inmediata de arenas libres.

---

## 2. Diagnóstico: Causas del Consumo Excesivo en Windows

En sistemas Microsoft Windows, la aplicación presenta consumos de RAM superiores a 500-800 MB y un uso de CPU desproporcionado (30% a 70%+). El análisis del código revela dos causas raíz interconectadas y una discrepancia en la medición de memoria del sistema operativo:

### 2.1. Causa Raíz 1: Hardcodeo de `hwdec = "vaapi"` (Inexistente en Windows)

**Ubicación del código:** [`src/player.rs`](file:///src/player.rs#L442)

```rust
let opts = [
    ("vo\0", "libmpv\0"),
    ("hwdec\0", "vaapi\0"),
    ("hwdec-codecs\0", "all\0"),
    // ...
];
```

#### Mecánica del Problema:
1. `vaapi` (**Video Acceleration API**) es una interfaz exclusiva de sistemas tipo Unix (Intel/AMD sobre Mesa). En Windows no existe implementación de VA-API.
2. Al ejecutar en Windows, `mpv_initialize()` falla al intentar vincular el decodificador VA-API.
3. Ante el fallo, `libmpv` recurre a su mecanismo de fallback: **Decodificación por Software en CPU (FFmpeg / libavcodec)**.
4. Múltiples hilos de la CPU se saturan descomprimiendo códecs de alta complejidad computacional (como AV1 o VP9), generando búferes continuos en la memoria RAM principal en vez de procesar el flujo en los ASICs dedicados de la GPU.

---

### 2.2. Causa Raíz 2: El Cuello de Botella del Rasterizador por Software (`sw`) en Windows

**Ubicación del código:** [`src/player.rs`](file:///src/player.rs#L600-L710)

El contexto de renderizado de `mpv` se inicializa discriminando plataformas:
```rust
#[cfg(target_os = "linux")]
if is_opengl {
    // Inicializa contexto EGL / Wayland con MPV_RENDER_PARAM_API_TYPE = "opengl"
}

if !is_opengl {
    let params = [
        MpvRenderParam {
            type_: MPV_RENDER_PARAM_API_TYPE,
            data: sw_api.as_ptr() as _, // "sw\0"
        },
        // ...
    ];
    unsafe { mpv_render_context_create(&mut r_ctx, ctx, params.as_ptr()) };
}
```

#### La Trampa del Doble Cuello de Botella:
- En Windows, `is_opengl` es siempre `false` al estar restringido por `#[cfg(target_os = "linux")]`. Por ende, Windows **siempre** instancia el contexto de renderizado `"sw"` (**Software Rasterizer** de `libmpv`).
- **Por qué cambiar solo `hwdec` a `"d3d11va-copy"` no soluciona el problema:**
  1. Si se configura `d3d11va-copy`, la GPU decodifica el video, pero luego copia cada fotograma de la VRAM a la memoria RAM del sistema.
  2. En la RAM del sistema, el contexto `"sw"` de `libmpv` convierte mediante CPU el formato de color YUV/NV12 a RGBA sin comprimir dentro de un `Vec<u8>`.
  3. Posteriormente, el backend gráfico de `iced` (`wgpu` sobre D3D12/Vulkan) toma ese `Vec<u8>` de la memoria RAM y **vuelve a subirlo a la VRAM** a través del bus PCIe en cada frame renderizado.
- **Tráfico en el Bus PCIe y RAM:**
  - A 1080p60 (8.3 MB/frame): ~500 MB/s de transferencias cruzadas VRAM $\leftrightarrow$ RAM.
  - A 4K60 (33.2 MB/frame): ~2.0 GB/s de transferencias continuas.
  - A 8K60 (132.7 MB/frame): **~8.0 GB/s en lectura + ~8.0 GB/s en escritura = ~16.0 GB/s** de ancho de banda inútil sobre el bus de memoria y PCIe, provocando ahogo térmico, saturación de RAM y micro-congelamientos.

---

### 2.3. Comportamiento del Asignador de Memoria (`mimalloc`): Working Set vs Commit Size

**Ubicación del código:** [`src/power_manager.rs`](file:///src/power_manager.rs#L296-L320)

`mimalloc` está configurado para purgar páginas liberadas de inmediato:
```rust
pub fn configure_allocator_low_memory() {
    unsafe {
        mi_option_set(3, 1);  // mi_option_page_reset
        mi_option_set(4, 1);  // mi_option_abandoned_page_purge
        mi_option_set(6, 0);  // mi_option_purge_delay (v1)
        mi_option_set(15, 0); // mi_option_purge_delay (v2)
    }
}
```

#### Discrepancia de Métricas en Windows:
- En Linux, `mi_collect(true)` ejecuta `madvise(MADV_DONTNEED)`, reduciendo inmediatamente el Resident Set Size (RSS) que reporta el monitor de recursos.
- En Windows, `mimalloc` invoca `VirtualFree(ptr, size, MEM_DECOMMIT)`. Las páginas quedan descomprometidas de la cuota de memoria física, pero el **Administrador de Tareas de Windows muestra por defecto la columna "Memoria" (Private Working Set)**. El Administrador de Memoria de Windows no remueve físicamente estas páginas del conjunto de trabajo del proceso hasta que haya presión global de memoria en el sistema.
- **Métrica Correcta de Auditoría:** Para medir el consumo de memoria real del proceso en Windows, se debe auditar la columna **Commit Size** (*Tamaño confirmado*) o **Private Bytes** (mediante Process Explorer / Process Hacker / Performance Monitor), no el Working Set básico.

---

## 3. Diagnóstico: Viabilidad y Estado del Soporte para 8K (4320p)

### 3.1. Estado Actual en el Código
1. **Enumerador [`PreferredResolution`](file:///src/config.rs#L6-L31):**
   Solo contempla `Res4K` (2160p), `Res1440p`, `Res1080p` y `Res720p`. No existe variante `Res8K` ni su integración en los selectores de la interfaz de Ajustes (`src/ui/mod.rs`).
2. **Buffer del Demuxer ([`src/player.rs`](file:///src/player.rs#L1400-L1424)):**
   El perfil máximo es 4K con 56 MB de búfer hacia adelante. Un flujo 8K AV1 en YouTube posee tasas de bits de 50 a 100 Mbps; un búfer de 56 MB solo amortigua ~4-8 segundos de reproducción, provocando pausas por buffering ante fluctuaciones de red.
3. **Tope de Escalado en [`src/player.rs`](file:///src/player.rs#L898-L915):**
   ```rust
   let max_scale = (1920.0 / vid_w as f32).min(1080.0 / vid_h as f32).min(1.0);
   ```
   El motor aplica un límite superior fijo de **1920x1080** al tamaño de los búferes que genera `libmpv`.

### 3.2. La Matemática del Búfer RGBA y el Presupuesto de 350 MB
- Un fotograma 8K (7680 $\times$ 4320) en formato de color RGBA de 32 bits (4 bytes por píxel) ocupa:
  $$7680 \times 4320 \times 4\text{ bytes} = 132{,}710{,}400\text{ bytes} \approx 132.7\text{ MB por fotograma}$$
- Con la arquitectura de triple-buffer reciclable (3 ranuras: 1 en renderizado mpv, 1 en tránsito UI, 1 en display wgpu):
  $$3 \times 132.7\text{ MB} = 398.1\text{ MB}$$
- **Conclusión Crítica:** Si el renderizado se realiza mediante Software Rasterizer (`sw` volcando a un `Vec<u8>` en RAM), **es matemáticamente imposible desbloquear 8K nativo sin violar el presupuesto de < 350 MB de RAM**. 8K nativo en pantalla completa **únicamente es admisible cuando el pipeline de video sea Zero-Copy puro en VRAM (GPU)**.

---

## 4. Plan de Acción y Soluciones Técnicas Corregidas

Este plan de acción sustituye enfoques superficiales por soluciones arquitectónicas completas:

### Paso 1: Configuración Heurística de `hwdec` con Literales CStr

En lugar de fijar nombres de backend por sistema operativo o usar cadenas manuales con `\0`, se debe utilizar la heurística interna de `libmpv` con literales de cadena `CStr` nativos de Rust 2021 (Rust 1.77+):

```rust
// src/player.rs
// Si se cuenta con contexto de renderizado directo en GPU (OpenGL/D3D11), usar c"auto-safe".
// Si temporalmente se opera sobre software context ("sw"), usar c"auto-copy".
let hwdec_mode = if is_gpu_zero_copy {
    c"auto-safe"
} else {
    c"auto-copy"
};

let opts = [
    (c"vo", c"libmpv"),
    (c"hwdec", hwdec_mode),
    (c"hwdec-codecs", c"all"),
    // ...
];
```
- `auto-safe`: `libmpv` negocia automáticamente Direct3D 11 (`d3d11va`) en Windows, VA-API en Linux y VideoToolbox en macOS sin extraer fotogramas a la RAM.
- `auto-copy`: La GPU decodifica por hardware y solo copia a RAM cuando el contexto receptor lo exige obligatoriamente.

---

### Paso 2: Eliminación del Software Rasterizer en Windows (Zero-Copy VRAM)

Para eliminar el cuello de botella en Windows, se debe proporcionar un contexto de renderizado GPU a `libmpv` mediante una de las dos vías soportadas:

#### Opción A (Recomendada): Contexto OpenGL vía WGL o ANGLE/EGL en Windows
Habilitar `is_opengl = true` también en Windows creando un contexto OpenGL invisible (PBuffer vía WGL o EGL con ANGLE). Esto permite usar:
```rust
let mut gl_init = MpvOpenglInitParams {
    get_proc_address: Some(get_proc_address_mpv),
    get_proc_address_ctx: std::ptr::null_mut(),
};
let params = [
    MpvRenderParam {
        type_: MPV_RENDER_PARAM_API_TYPE,
        data: c"opengl".as_ptr() as _,
    },
    MpvRenderParam {
        type_: MPV_RENDER_PARAM_OPENGL_INIT_PARAMS,
        data: &mut gl_init as *mut _ as _,
    },
    // ...
];
```
El fotograma se renderiza directo a una textura de GPU (`FBO`), eliminando la copia a memoria RAM del sistema.

#### Opción B: Texturas Compartidas DXGI (Direct3D 11 $\rightarrow$ wgpu)
Utilizar la API de renderizado D3D11 de `libmpv` exportando un recurso compartido hacia `wgpu`. 
> [!NOTE]
> En Direct3D 12 (el backend nativo de `wgpu` en Windows), los handles antiguos de `D3D11_RESOURCE_MISC_SHARED` están obsoletos y son rechazados por `ID3D12Device::OpenSharedHandle`. Para interoperar con D3D12, la textura D3D11 debe crearse con los flags `D3D11_RESOURCE_MISC_SHARED_NTHANDLE | D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX` y exportarse mediante `IDXGIResource1::CreateSharedHandle`. Como alternativa muy utilizada en clientes embebidos de Windows, `libmpv` puede apoyarse en ANGLE (EGL sobre D3D11) usando `MPV_RENDER_PARAM_API_TYPE = "opengl"` y la extensión `EGL_D3D_TEXTURE_2D_SHARE_HANDLE_ANGLE`. Es Zero-Copy absoluto en hardware de Windows.

---

### Paso 3: Purga Segura del Working Set en Windows (Sin Degradar Rendimiento)

> [!WARNING]
> Nunca debe ejecutarse `EmptyWorkingSet(GetCurrentProcess())` durante la reproducción activa de video o durante la navegación en la UI.
> Provocaría el vaciado forzoso de páginas calientes a la lista de espera de Windows, causando **soft page faults masivos** en el hilo de audio/video y en el ciclo de wgpu, produciendo micro-cortes y pérdida de fotogramas solo por maquillar el número del Administrador de Tareas.

#### Implementación Aislada por Estado:
En [`src/power_manager.rs`](file:///src/power_manager.rs), mantener `trim_process_memory()` para purga de arenas `mimalloc`, y crear una función dedicada para reposo prolongado:

```rust
// src/power_manager.rs

/// Purga agresiva de arenas mimalloc en heap (segura en cualquier momento).
pub fn trim_process_memory() {
    unsafe {
        mi_collect(true);
    }
}

/// Vaciado explícito del Working Set del sistema operativo.
/// EXCLUSIVAMENTE seguro al minimizar la ventana o detener la reproducción.
pub fn purge_working_set_on_idle() {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows_sys::Win32::System::ProcessStatus::EmptyWorkingSet;
        use windows_sys::Win32::System::Threading::GetCurrentProcess;
        EmptyWorkingSet(GetCurrentProcess());
    }
}
```

Invocar `purge_working_set_on_idle()` únicamente en:
1. `iced::window::Event::Unfocused` prolongado o ventana minimizada.
2. `player.stop()` tras retornar al Home y haber transcurrido más de 3 segundos de inactividad.

---

### Paso 4: Soporte 8K Condicionado a la Arquitectura de Renderizado

1. **Añadir `Res8K` en [`src/config.rs`](file:///src/config.rs):**
   ```rust
   #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
   pub enum PreferredResolution {
       Res8K,    // 4320p
       Res4K,    // 2160p
       Res1440p, // 1440p
       Res1080p, // 1080p
       Res720p,  // 720p
   }

   impl PreferredResolution {
       pub fn label(&self) -> &'static str {
           match self {
               PreferredResolution::Res8K => "8K (4320p)",
               PreferredResolution::Res4K => "4K (2160p)",
               // ...
           }
       }

       pub fn max_height(&self) -> u32 {
           match self {
               PreferredResolution::Res8K => 4320,
               PreferredResolution::Res4K => 2160,
               // ...
           }
       }
   }
   ```
2. **Buffer del Demuxer para 8K en [`src/player.rs`](file:///src/player.rs#L1400):**
   ```rust
   let (max_bytes, back_bytes, readahead) = if stream_height >= 4320 {
       if is_battery {
           (c"67108864", c"16777216", c"20")  // 64 MB forward / 16 MB backward
       } else {
           (c"100663296", c"33554432", c"30") // 96 MB forward / 32 MB backward
       }
   } else if stream_height >= 2160 {
       // ... perfiles de 4K existentes
   };
   ```
3. **Criterio Riguroso para el Límite de Escalado (`max_scale`):**
   En [`src/player.rs`](file:///src/player.rs#L898-L915):
   ```rust
   // Si el renderizado se realiza en CPU (Software Rasterizer a memoria RAM),
   // NUNCA permitir resoluciones superiores a 1080p para salvaguardar el límite de 350 MB de RAM.
   // Solo si el contexto es Zero-Copy directo en GPU (VRAM), escalar hasta la resolución física de pantalla.
   let (render_limit_w, render_limit_h) = if is_gpu_zero_copy {
       (vp_w.max(1920) as f32, vp_h.max(1080) as f32)
   } else {
       (1920.0, 1080.0)
   };

   let max_scale = (render_limit_w / vid_w as f32)
       .min(render_limit_h / vid_h as f32)
       .min(1.0);
   ```

---

## 5. Resumen de Métricas de Éxito para Validación

Cualquier auditoría o prueba de integración en Windows debe validar:
1. **Decodificación GPU:** `libmpv` debe reportar `hwdec-current != ""` (usando `d3d11va` o `nvdec`). El uso de CPU no debe exceder el 5-10% en reproducción 1080p60/4K60.
2. **Consumo de Memoria:** Medido en la columna **Commit Size** de Windows:
   - Reposo: `< 150 MB`.
   - Reproducción 1080p: `< 220 MB`.
   - Reproducción 4K: `< 330 MB`.
3. **Fluidez:** Cero micro-stuttering inducido por el asignador de memoria (sin llamadas a `EmptyWorkingSet` en caliente).
