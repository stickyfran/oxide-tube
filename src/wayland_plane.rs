//! Native Wayland video plane (zero-copy + HDR).
//!
//! The player renders MPV into an RGB10_A2 OpenGL texture, exports it as a
//! DMA-BUF and attaches it to a `wl_subsurface` placed below the iced window
//! surface. iced keeps compositing the UI (OSD, controls, overlays) on its own
//! surface, so the video never round-trips through system memory and can be
//! described to the compositor as PQ/BT.2020 for real HDR output.
//!
//! The subsurface lives on the same Wayland connection as the iced window. The
//! raw `wl_display`/`wl_surface` pointers come from `raw-window-handle`.
//!
//! This module only works on Wayland. `WaylandPlane::new` returns an error on
//! any other platform or when a required protocol is missing, and the caller
//! keeps the existing RGBA readback path as a fallback.

#[cfg(target_os = "linux")]
pub use linux::WaylandPlane;

#[cfg(not(target_os = "linux"))]
pub struct WaylandPlane;

#[cfg(not(target_os = "linux"))]
impl WaylandPlane {
    pub fn new(_display: *mut std::ffi::c_void, _surface: *mut std::ffi::c_void, _scale: f32) -> Result<Self, String> {
        Err("native Wayland plane is only available on Linux".to_owned())
    }

    pub fn supports_hdr(&self) -> bool {
        false
    }

    pub fn set_hdr(&mut self, _hdr: bool) {}

    pub fn set_rect(&mut self, _x: i32, _y: i32, _width: u32, _height: u32) {}

    pub fn present(&mut self, _texture: u32, _width: u32, _height: u32) -> Result<(), String> {
        Ok(())
    }

    pub fn hide(&mut self) {}
}

#[cfg(target_os = "linux")]
mod linux {
    use std::{
        collections::HashMap,
        ffi::{c_char, c_void},
        os::fd::{AsFd, FromRawFd, OwnedFd},
    };

    use wayland_client::{
        Connection, Dispatch, EventQueue, Proxy, QueueHandle,
        backend::{Backend, ObjectId},
        protocol::{
            wl_buffer::WlBuffer,
            wl_compositor::WlCompositor,
            wl_registry::WlRegistry,
            wl_subcompositor::WlSubcompositor,
            wl_subsurface::WlSubsurface,
            wl_surface::WlSurface,
        },
    };
    use wayland_protocols::wp::{
        color_management::v1::client::{
            wp_color_management_surface_v1::WpColorManagementSurfaceV1,
            wp_color_manager_v1::{Primaries, RenderIntent, TransferFunction, WpColorManagerV1},
            wp_image_description_creator_params_v1::WpImageDescriptionCreatorParamsV1,
            wp_image_description_v1::{self, WpImageDescriptionV1},
        },
        linux_dmabuf::zv1::client::{
            zwp_linux_buffer_params_v1::{Flags, ZwpLinuxBufferParamsV1},
            zwp_linux_dmabuf_v1::{self, ZwpLinuxDmabufV1},
        },
        viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter},
    };

    /// `wl_buffer` state for one exported GL texture.
    struct PlaneBuffer {
        width: u32,
        height: u32,
        buffer: WlBuffer,
    }

    pub struct WaylandPlane {
        connection: Connection,
        queue: EventQueue<PlaneState>,
        state: PlaneState,
        // Kept alive for the lifetime of the subsurface; the child surface is
        // what actually receives buffers.
        _parent: WlSurface,
        child: WlSurface,
        subsurface: WlSubsurface,
        viewport: WpViewport,
        dmabuf: ZwpLinuxDmabufV1,
        color_surface: Option<WpColorManagementSurfaceV1>,
        descriptions: Vec<WpImageDescriptionV1>,
        buffers: HashMap<u32, PlaneBuffer>,
        width: u32,
        height: u32,
        visible: bool,
        hdr: bool,
    }

    impl WaylandPlane {
        pub fn new(
            display: *mut c_void,
            surface: *mut c_void,
            _scale: f32,
        ) -> Result<Self, String> {
            if display.is_null() || surface.is_null() {
                return Err("missing Wayland display/surface handles".to_owned());
            }

            // SAFETY: the pointers belong to the iced/winit connection and stay
            // alive for the whole process; `from_foreign_display` does not
            // close the connection on drop.
            let backend = unsafe { Backend::from_foreign_display(display.cast()) };
            let connection = Connection::from_backend(backend);
            let mut queue = connection.new_event_queue::<PlaneState>();
            let qh = queue.handle();

            // Bind globals on a private event queue so we never dispatch the
            // queue owned by winit/iced.
            let _registry = connection.display().get_registry(&qh, ());
            let mut state = PlaneState::default();
            queue
                .roundtrip(&mut state)
                .map_err(|error| format!("wl_registry roundtrip: {error:?}"))?;
            let compositor = state
                .compositor
                .take()
                .ok_or_else(|| "wl_compositor is not available".to_owned())?;
            let subcompositor = state
                .subcompositor
                .take()
                .ok_or_else(|| "wl_subcompositor is not available".to_owned())?;
            let viewporter = state
                .viewporter
                .take()
                .ok_or_else(|| "wp_viewporter is not available".to_owned())?;
            let dmabuf = state
                .dmabuf
                .take()
                .ok_or_else(|| "zwp_linux_dmabuf_v1 is not available".to_owned())?;
            let color_manager = state.color_manager.clone();

            // Reconstruct the parent surface on the foreign connection.
            // SAFETY: `surface` is the live `wl_surface*` of the window and
            // `WlSurface::interface()` is the matching protocol interface.
            let surface_id = unsafe { ObjectId::from_ptr(WlSurface::interface(), surface.cast()) }
                .map_err(|error| format!("invalid wl_surface: {error:?}"))?;
            let parent = WlSurface::from_id(&connection, surface_id)
                .map_err(|error| format!("stale wl_surface: {error:?}"))?;

            let child = compositor.create_surface(&qh, ());
            let subsurface = subcompositor.get_subsurface(&child, &parent, &qh, ());
            // A new sub-surface is stacked above its parent by default, which
            // would cover the UI. Keep the video below the transparent hole.
            subsurface.place_below(&parent);
            let viewport = viewporter.get_viewport(&child, &qh, ());
            let color_surface = color_manager
                .as_ref()
                .map(|manager| manager.get_surface(&child, &qh, ()));

            subsurface.set_desync();
            if child.version() >= 3 {
                child.set_buffer_scale(1);
            }
            child.commit();
            connection
                .flush()
                .map_err(|error| format!("wayland flush: {error:?}"))?;

            Ok(Self {
                connection,
                queue,
                state,
                _parent: parent,
                child,
                subsurface,
                viewport,
                dmabuf,
                color_surface,
                descriptions: Vec::new(),
                buffers: HashMap::new(),
                width: 0,
                height: 0,
                visible: false,
                hdr: false,
            })
        }

        pub fn supports_hdr(&self) -> bool {
            self.color_surface.is_some()
        }

        /// Selects the color description the compositor should apply to the
        /// video surface. HDR uses PQ/BT.2020; SDR uses sRGB/BT.709.
        pub fn set_hdr(&mut self, hdr: bool) {
            if self.hdr == hdr && !self.descriptions.is_empty() {
                return;
            }
            self.hdr = hdr;
            let Some(color_surface) = self.color_surface.as_ref() else {
                return;
            };
            let Some(manager) = self.state.color_manager.as_ref() else {
                return;
            };
            let qh = self.queue.handle();
            let creator = manager.create_parametric_creator(&qh, ());
            if hdr {
                creator.set_tf_named(TransferFunction::St2084Pq);
                creator.set_primaries_named(Primaries::Bt2020);
                creator.set_luminances(0, 1000, 203);
            } else {
                creator.set_tf_named(TransferFunction::Srgb);
                creator.set_primaries_named(Primaries::Srgb);
                creator.set_luminances(0, 203, 203);
            }
            let description = creator.create(&qh, ());
            if let Err(error) = self.queue.roundtrip(&mut self.state) {
                eprintln!("[mitub-plane] color management roundtrip failed: {error:?}");
                return;
            }
            if !self.state.description_ready {
                eprintln!("[mitub-plane] compositor rejected the image description");
                return;
            }
            self.state.description_ready = false;
            color_surface.set_image_description(&description, RenderIntent::Perceptual);
            // Keep previously submitted descriptions alive: the compositor may
            // still reference them until the next surface commit.
            self.descriptions.push(description);
            if self.descriptions.len() > 4 {
                let stale = self.descriptions.remove(0);
                stale.destroy();
            }
            let _ = self.connection.flush();
        }

        /// Positions the plane inside the parent surface using logical
        /// (surface-local) coordinates.
        pub fn set_rect(&mut self, x: i32, y: i32, width: u32, height: u32) {
            if width == 0 || height == 0 {
                self.hide();
                return;
            }
            self.width = width;
            self.height = height;
            self.subsurface.set_position(x, y);
            self.viewport
                .set_destination(width as i32, height as i32);
            self.child.commit();
            let _ = self.connection.flush();
        }

        pub fn present(
            &mut self,
            texture: u32,
            width: u32,
            height: u32,
        ) -> Result<(), String> {
            if width == 0 || height == 0 {
                return Ok(());
            }
            let key = texture;
            // `glTexImage2D` reallocates the texture storage on resize/format
            // changes. A DMA-BUF exported from the previous storage keeps
            // pointing at freed memory, which compositors sample as black, so
            // a cached buffer is only reused when its dimensions still match.
            let stale = self
                .buffers
                .get(&key)
                .is_some_and(|buffer| buffer.width != width || buffer.height != height);
            if stale {
                if let Some(buffer) = self.buffers.remove(&key) {
                    buffer.buffer.destroy();
                }
            }
            if !self.buffers.contains_key(&key) {
                match self.export_buffer(texture, width, height) {
                    Ok(buffer) => {
                        self.buffers.insert(key, buffer);
                    }
                    Err(error) => {
                        eprintln!("[mitub-plane] could not export frame as DMA-BUF: {error}");
                        return Err(error);
                    }
                }
            }
            let Some(buffer) = self.buffers.get(&key) else {
                return Err(format!("no DMA-BUF buffer for texture {key}"));
            };
            self.child.attach(Some(&buffer.buffer), 0, 0);
            if self.child.version() >= 4 {
                self.child
                    .damage_buffer(0, 0, buffer.width as i32, buffer.height as i32);
            } else {
                self.child.damage(0, 0, buffer.width as i32, buffer.height as i32);
            }
            self.child.commit();
            let _ = self.connection.flush();
            let _ = self.queue.dispatch_pending(&mut self.state);
            self.visible = true;
            Ok(())
        }

        /// Drops every cached DMA-BUF. Must be called whenever the render
        /// texture storage is reallocated without changing its dimensions
        /// (for example, switching between 8-bit and 10-bit formats).
        pub fn invalidate_buffers(&mut self) {
            for (_, buffer) in self.buffers.drain() {
                buffer.buffer.destroy();
            }
        }

        pub fn hide(&mut self) {
            if !self.visible {
                return;
            }
            self.visible = false;
            self.child.attach(None, 0, 0);
            self.child.commit();
            let _ = self.connection.flush();
        }

        fn export_buffer(
            &mut self,
            texture: u32,
            width: u32,
            height: u32,
        ) -> Result<PlaneBuffer, String> {
            let export = export_dmabuf(texture)?;
            if !self.state.supports_format(export.fourcc, export.modifier) {
                return Err(format!(
                    "compositor does not advertise fourcc {:#x} modifier {:#x}",
                    export.fourcc, export.modifier
                ));
            }
            let qh = self.queue.handle();
            let params = self.dmabuf.create_params(&qh, ());
            params.add(
                export.fd.as_fd(),
                0,
                export.offset,
                export.stride,
                (export.modifier >> 32) as u32,
                (export.modifier & 0xffff_ffff) as u32,
            );
            let buffer = params.create_immed(
                width as i32,
                height as i32,
                export.fourcc,
                Flags::empty(),
                &qh,
                (),
            );
            params.destroy();
            Ok(PlaneBuffer {
                width,
                height,
                buffer,
            })
        }
    }

    #[derive(Default)]
    struct PlaneState {
        compositor: Option<WlCompositor>,
        subcompositor: Option<WlSubcompositor>,
        viewporter: Option<WpViewporter>,
        dmabuf: Option<ZwpLinuxDmabufV1>,
        color_manager: Option<WpColorManagerV1>,
        description_ready: bool,
        formats: Vec<u32>,
        modifiers: Vec<(u32, u64)>,
    }

    impl PlaneState {
        fn supports_format(&self, format: u32, modifier: u64) -> bool {
            const DRM_FORMAT_MOD_INVALID: u64 = 0x00ff_ffff_ffff_ffff;
            if self.formats.is_empty() && self.modifiers.is_empty() {
                // Older compositors only advertise `format` events; let them
                // validate the buffer themselves.
                return true;
            }
            self.formats.contains(&format)
                || self
                    .modifiers
                    .iter()
                    .any(|&(advertised, advertised_modifier)| {
                        advertised == format
                            && (advertised_modifier == modifier
                                || advertised_modifier == 0
                                || advertised_modifier == DRM_FORMAT_MOD_INVALID
                                || modifier == DRM_FORMAT_MOD_INVALID)
                    })
        }
    }

    macro_rules! ignore_events {
        ($($interface:ty),+ $(,)?) => {
            $(
                impl Dispatch<$interface, ()> for PlaneState {
                    fn event(
                        _: &mut Self,
                        _: &$interface,
                        _: <$interface as Proxy>::Event,
                        _: &(),
                        _: &Connection,
                        _: &QueueHandle<Self>,
                    ) {
                    }
                }
            )+
        };
    }

    ignore_events!(
        WlCompositor,
        WlSubcompositor,
        WlSurface,
        WlSubsurface,
        WlBuffer,
        WpViewporter,
        WpViewport,
        WpColorManagerV1,
        WpColorManagementSurfaceV1,
        WpImageDescriptionCreatorParamsV1,
        ZwpLinuxBufferParamsV1,
    );

    impl Dispatch<WlRegistry, ()> for PlaneState {
        fn event(
            state: &mut Self,
            registry: &WlRegistry,
            event: wayland_client::protocol::wl_registry::Event,
            _: &(),
            _: &Connection,
            qh: &QueueHandle<Self>,
        ) {
            let wayland_client::protocol::wl_registry::Event::Global {
                name,
                interface,
                version,
            } = event
            else {
                return;
            };
            match interface.as_str() {
                "wl_compositor" => {
                    state.compositor = Some(registry.bind::<WlCompositor, _, _>(
                        name,
                        version.min(6),
                        qh,
                        (),
                    ));
                }
                "wl_subcompositor" => {
                    state.subcompositor = Some(registry.bind::<WlSubcompositor, _, _>(
                        name,
                        version.min(1),
                        qh,
                        (),
                    ));
                }
                "wp_viewporter" => {
                    state.viewporter = Some(registry.bind::<WpViewporter, _, _>(
                        name,
                        version.min(1),
                        qh,
                        (),
                    ));
                }
                "zwp_linux_dmabuf_v1" if version >= 3 => {
                    state.dmabuf = Some(registry.bind::<ZwpLinuxDmabufV1, _, _>(
                        name,
                        version.min(4),
                        qh,
                        (),
                    ));
                }
                "wp_color_manager_v1" => {
                    state.color_manager = Some(registry.bind::<WpColorManagerV1, _, _>(
                        name,
                        version.min(3),
                        qh,
                        (),
                    ));
                }
                _ => {}
            }
        }
    }

    impl Dispatch<ZwpLinuxDmabufV1, ()> for PlaneState {
        fn event(
            state: &mut Self,
            proxy: &ZwpLinuxDmabufV1,
            event: zwp_linux_dmabuf_v1::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            match event {
                zwp_linux_dmabuf_v1::Event::Format { format } => {
                    if proxy.version() < 3 {
                        state.formats.push(format);
                    }
                }
                zwp_linux_dmabuf_v1::Event::Modifier {
                    format,
                    modifier_hi,
                    modifier_lo,
                } => {
                    state.modifiers.push((
                        format,
                        (u64::from(modifier_hi) << 32) | u64::from(modifier_lo),
                    ));
                }
                _ => {}
            }
        }
    }

    impl Dispatch<WpImageDescriptionV1, ()> for PlaneState {
        fn event(
            state: &mut Self,
            _: &WpImageDescriptionV1,
            event: wp_image_description_v1::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            match event {
                wp_image_description_v1::Event::Ready { .. }
                | wp_image_description_v1::Event::Ready2 { .. } => {
                    state.description_ready = true;
                }
                wp_image_description_v1::Event::Failed { cause, msg } => {
                    eprintln!("[mitub-plane] image description failed: {cause:?} {msg}");
                }
                _ => {}
            }
        }
    }

    // EGL DMA-BUF export (Mesa). Resolved through eglGetProcAddress because
    // they are extensions.
    #[link(name = "EGL")]
    unsafe extern "C" {
        fn eglGetCurrentDisplay() -> *mut c_void;
        fn eglGetCurrentContext() -> *mut c_void;
        fn eglGetProcAddress(name: *const c_char) -> *mut c_void;
    }

    const EGL_GL_TEXTURE_2D_KHR: u32 = 0x30B1;

    type EglCreateImage =
        unsafe extern "C" fn(*mut c_void, *mut c_void, u32, *mut c_void, *const i32) -> *mut c_void;
    type EglDestroyImage = unsafe extern "C" fn(*mut c_void, *mut c_void) -> u32;
    type EglExportQuery =
        unsafe extern "C" fn(*mut c_void, *mut c_void, *mut i32, *mut i32, *mut u64) -> u32;
    type EglExportImage =
        unsafe extern "C" fn(*mut c_void, *mut c_void, *mut i32, *mut i32, *mut i32) -> u32;

    struct ExportedDmabuf {
        fd: OwnedFd,
        fourcc: u32,
        modifier: u64,
        stride: u32,
        offset: u32,
    }

    fn export_dmabuf(texture: u32) -> Result<ExportedDmabuf, String> {
        // SAFETY: called from the player worker with its EGL context current.
        unsafe {
            let display = eglGetCurrentDisplay();
            let context = eglGetCurrentContext();
            if display.is_null() || context.is_null() {
                return Err("no current EGL display/context".to_owned());
            }
            let create = load_egl_proc::<EglCreateImage>("eglCreateImageKHR")?;
            let destroy = load_egl_proc::<EglDestroyImage>("eglDestroyImageKHR")?;
            let query = load_egl_proc::<EglExportQuery>("eglExportDMABUFImageQueryMESA")?;
            let export = load_egl_proc::<EglExportImage>("eglExportDMABUFImageMESA")?;

            // EGL_GL_TEXTURE_2D_KHR requires the current GL context; passing
            // EGL_NO_CONTEXT makes Mesa's eglCreateImageKHR dereference null.
            let image = create(
                display,
                context,
                EGL_GL_TEXTURE_2D_KHR,
                texture as usize as *mut c_void,
                std::ptr::null(),
            );
            if image.is_null() {
                return Err("eglCreateImageKHR failed".to_owned());
            }
            let mut fourcc: i32 = 0;
            let mut planes: i32 = 0;
            let mut modifiers = [0u64; 4];
            if query(
                display,
                image,
                &mut fourcc,
                &mut planes,
                modifiers.as_mut_ptr(),
            ) == 0
                || planes != 1
            {
                destroy(display, image);
                return Err("EGL DMA-BUF query failed (only single-plane RGB supported)".to_owned());
            }
            let mut fds = [-1i32; 4];
            let mut strides = [0i32; 4];
            let mut offsets = [0i32; 4];
            let ok = export(
                display,
                image,
                fds.as_mut_ptr(),
                strides.as_mut_ptr(),
                offsets.as_mut_ptr(),
            );
            destroy(display, image);
            if ok == 0 || fds[0] < 0 {
                return Err("EGL DMA-BUF export failed".to_owned());
            }
            for fd in &fds[1..] {
                if *fd >= 0 {
                    libc::close(*fd);
                }
            }
            Ok(ExportedDmabuf {
                // SAFETY: EGL transferred ownership of the fd to us.
                fd: OwnedFd::from_raw_fd(fds[0]),
                fourcc: fourcc as u32,
                modifier: modifiers[0],
                stride: strides[0] as u32,
                offset: offsets[0] as u32,
            })
        }
    }

    unsafe fn load_egl_proc<T>(name: &str) -> Result<T, String> {
        let name = std::ffi::CString::new(name).map_err(|error| error.to_string())?;
        // SAFETY: eglGetProcAddress is valid once EGL is initialized.
        let pointer = unsafe { eglGetProcAddress(name.as_ptr()) };
        if pointer.is_null() {
            return Err(format!(
                "EGL extension {} is unavailable",
                name.to_string_lossy()
            ));
        }
        // SAFETY: the caller names the correct Mesa EGL extension signature.
        Ok(unsafe { std::mem::transmute_copy(&pointer) })
    }
}
