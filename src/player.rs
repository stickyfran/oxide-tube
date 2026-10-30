use bytes::Bytes;
use crossbeam_channel::{bounded, Receiver, Sender};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU8, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use crate::power_manager::PowerProfile;

#[derive(Clone, Debug)]
pub struct VideoFrameBuffer {
    pub frame_id: u64,
    pub width: u32,
    pub height: u32,
    pub data: Bytes,
}

impl VideoFrameBuffer {
    #[allow(dead_code)]
    pub fn as_image_handle(&self) -> iced::widget::image::Handle {
        iced::widget::image::Handle::from_rgba(self.width, self.height, self.data.clone())
    }

    /// Downscales the current frame to a compact 480x270 RGBA image::Handle so it does not hold
    /// onto the recycled pool buffer and uses minimal RAM (~518 KB) when cached across tabs.
    pub fn to_cached_snapshot_handle(&self) -> Option<iced::widget::image::Handle> {
        let src_w = self.width as usize;
        let src_h = self.height as usize;
        if src_w == 0 || src_h == 0 || self.data.len() < src_w * src_h * 4 {
            return None;
        }
        let dst_w = 480usize.min(src_w);
        let dst_h = 270usize.min(src_h);
        let mut out = vec![0u8; dst_w * dst_h * 4];
        let src = self.data.as_ref();
        for dy in 0..dst_h {
            let sy = (dy * src_h) / dst_h;
            let src_row = sy * src_w * 4;
            let dst_row = dy * dst_w * 4;
            for dx in 0..dst_w {
                let sx = (dx * src_w) / dst_w;
                let s_idx = src_row + sx * 4;
                let d_idx = dst_row + dx * 4;
                out[d_idx..d_idx + 4].copy_from_slice(&src[s_idx..s_idx + 4]);
            }
        }
        Some(iced::widget::image::Handle::from_rgba(dst_w as u32, dst_h as u32, out))
    }
}

pub struct RecycledBuf {
    buf: Vec<u8>,
    pool_tx: Sender<Vec<u8>>,
}

impl AsRef<[u8]> for RecycledBuf {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        &self.buf
    }
}

impl Drop for RecycledBuf {
    fn drop(&mut self) {
        let returned_buf = std::mem::take(&mut self.buf);
        let _ = self.pool_tx.try_send(returned_buf);
    }
}

pub(crate) trait SafeLock<T> {
    fn safe_lock(&self) -> std::sync::MutexGuard<'_, T>;
}

impl<T> SafeLock<T> for Mutex<T> {
    #[inline]
    fn safe_lock(&self) -> std::sync::MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|e| e.into_inner())
    }
}

// -----------------------------------------------------------------------------
// libmpv FFI Bindings
// -----------------------------------------------------------------------------
pub type MpvHandle = std::ffi::c_void;
pub type MpvRenderContext = std::ffi::c_void;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct MpvEvent {
    pub event_id: i32,
    pub error: i32,
    pub reply_userdata: u64,
    pub data: *mut std::ffi::c_void,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct MpvEventProperty {
    pub name: *const c_char,
    pub format: i32,
    pub data: *mut std::ffi::c_void,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct MpvRenderParam {
    pub type_: i32,
    pub data: *mut std::ffi::c_void,
}

#[allow(dead_code)]
pub const MPV_EVENT_NONE: i32 = 0;
#[allow(dead_code)]
pub const MPV_EVENT_SHUTDOWN: i32 = 1;
#[allow(dead_code)]
pub const MPV_EVENT_START_FILE: i32 = 6;
#[allow(dead_code)]
pub const MPV_EVENT_END_FILE: i32 = 7;
#[allow(dead_code)]
pub const MPV_EVENT_FILE_LOADED: i32 = 8;
#[allow(dead_code)]
pub const MPV_EVENT_SEEK: i32 = 20;
#[allow(dead_code)]
pub const MPV_EVENT_PLAYBACK_RESTART: i32 = 21;
#[allow(dead_code)]
pub const MPV_EVENT_PROPERTY_CHANGE: i32 = 22;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct MpvEventEndFile {
    pub reason: i32,
    pub error: i32,
    pub playlist_entry_id: i64,
    pub playlist_insert_id: i64,
    pub playlist_insert_num_entries: i32,
}

pub const _MPV_END_FILE_REASON_EOF: i32 = 0;
pub const _MPV_END_FILE_REASON_STOP: i32 = 2;
pub const _MPV_END_FILE_REASON_QUIT: i32 = 3;
pub const MPV_END_FILE_REASON_ERROR: i32 = 4;
pub const _MPV_END_FILE_REASON_REDIRECT: i32 = 5;

#[allow(dead_code)]
pub const MPV_FORMAT_NONE: i32 = 0;
#[allow(dead_code)]
pub const MPV_FORMAT_STRING: i32 = 1;
pub const MPV_FORMAT_FLAG: i32 = 3;
pub const MPV_FORMAT_INT64: i32 = 4;
pub const MPV_FORMAT_DOUBLE: i32 = 5;

pub const MPV_RENDER_PARAM_INVALID: i32 = 0;
pub const MPV_RENDER_PARAM_API_TYPE: i32 = 1;
pub const MPV_RENDER_PARAM_OPENGL_INIT_PARAMS: i32 = 2;
pub const MPV_RENDER_PARAM_OPENGL_FBO: i32 = 3;
#[allow(dead_code)]
pub const MPV_RENDER_PARAM_FLIP_Y: i32 = 4;
pub const MPV_RENDER_PARAM_WL_DISPLAY: i32 = 9;
pub const MPV_RENDER_PARAM_SKIP_RENDERING: i32 = 13;
pub const MPV_RENDER_PARAM_SW_SIZE: i32 = 17;
pub const MPV_RENDER_PARAM_SW_FORMAT: i32 = 18;
pub const MPV_RENDER_PARAM_SW_STRIDE: i32 = 19;
pub const MPV_RENDER_PARAM_SW_POINTER: i32 = 20;

pub const MPV_RENDER_UPDATE_FRAME: u64 = 1 << 0;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct MpvOpenglInitParams {
    pub get_proc_address: Option<unsafe extern "C" fn(ctx: *mut std::ffi::c_void, name: *const c_char) -> *mut std::ffi::c_void>,
    pub get_proc_address_ctx: *mut std::ffi::c_void,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct MpvOpenglFbo {
    pub fbo: i32,
    pub w: i32,
    pub h: i32,
    pub internal_format: i32,
}

// -----------------------------------------------------------------------------
// EGL & OpenGL Desktop FFI
// -----------------------------------------------------------------------------
pub type EGLDisplay = *mut std::ffi::c_void;
pub type EGLConfig = *mut std::ffi::c_void;
pub type EGLContext = *mut std::ffi::c_void;
pub type EGLSurface = *mut std::ffi::c_void;
pub type EGLint = i32;
pub type EGLBoolean = u32;

pub const EGL_DEFAULT_DISPLAY: EGLDisplay = std::ptr::null_mut();
pub const EGL_NO_CONTEXT: EGLContext = std::ptr::null_mut();
pub const EGL_SURFACE_TYPE: EGLint = 0x3033;
pub const EGL_PBUFFER_BIT: EGLint = 0x0001;
pub const EGL_RENDERABLE_TYPE: EGLint = 0x3040;
pub const EGL_OPENGL_BIT: EGLint = 0x0008;
pub const EGL_NONE: EGLint = 0x3038;
pub const EGL_OPENGL_API: u32 = 0x30A2;
pub const EGL_WIDTH: EGLint = 0x3057;
pub const EGL_HEIGHT: EGLint = 0x3056;

#[cfg(target_os = "linux")]
#[link(name = "EGL")]
extern "C" {
    pub fn eglGetDisplay(display_id: EGLDisplay) -> EGLDisplay;
    pub fn eglInitialize(dpy: EGLDisplay, major: *mut EGLint, minor: *mut EGLint) -> EGLBoolean;
    pub fn eglChooseConfig(dpy: EGLDisplay, attrib_list: *const EGLint, configs: *mut EGLConfig, config_size: EGLint, num_config: *mut EGLint) -> EGLBoolean;
    pub fn eglBindAPI(api: u32) -> EGLBoolean;
    pub fn eglCreateContext(dpy: EGLDisplay, config: EGLConfig, share_context: EGLContext, attrib_list: *const EGLint) -> EGLContext;
    pub fn eglCreatePbufferSurface(dpy: EGLDisplay, config: EGLConfig, attrib_list: *const EGLint) -> EGLSurface;
    pub fn eglMakeCurrent(dpy: EGLDisplay, draw: EGLSurface, read: EGLSurface, ctx: EGLContext) -> EGLBoolean;
    pub fn eglGetProcAddress(procname: *const c_char) -> *mut std::ffi::c_void;
    pub fn eglDestroySurface(dpy: EGLDisplay, surface: EGLSurface) -> EGLBoolean;
    pub fn eglDestroyContext(dpy: EGLDisplay, ctx: EGLContext) -> EGLBoolean;
    pub fn eglTerminate(dpy: EGLDisplay) -> EGLBoolean;
}

pub type GLuint = u32;
pub type GLint = i32;
pub type GLsizei = i32;
pub type GLenum = u32;
pub type GLvoid = std::ffi::c_void;

pub const GL_TEXTURE_2D: GLenum = 0x0DE1;
pub const GL_RGBA8: GLenum = 0x8058;
pub const GL_RGBA: GLenum = 0x1908;
pub const GL_UNSIGNED_BYTE: GLenum = 0x1401;
pub const GL_FRAMEBUFFER: GLenum = 0x8D40;
pub const GL_COLOR_ATTACHMENT0: GLenum = 0x8CE0;

#[cfg(target_os = "linux")]
#[link(name = "GL")]
extern "C" {
    pub fn glGenTextures(n: GLsizei, textures: *mut GLuint);
    pub fn glBindTexture(target: GLenum, texture: GLuint);
    pub fn glTexImage2D(
        target: GLenum,
        level: GLint,
        internalformat: GLint,
        width: GLsizei,
        height: GLsizei,
        border: GLint,
        format: GLenum,
        type_: GLenum,
        pixels: *const GLvoid,
    );
    pub fn glDeleteTextures(n: GLsizei, textures: *const GLuint);
    pub fn glReadPixels(
        x: GLint,
        y: GLint,
        width: GLsizei,
        height: GLsizei,
        format: GLenum,
        type_: GLenum,
        pixels: *mut GLvoid,
    );
    pub fn glGetError() -> GLenum;
}

#[cfg(target_os = "linux")]
#[link(name = "wayland-client")]
extern "C" {
    pub fn wl_display_connect(name: *const c_char) -> *mut std::ffi::c_void;
    pub fn wl_display_disconnect(display: *mut std::ffi::c_void);
}

#[cfg(target_os = "linux")]
unsafe extern "C" fn get_proc_address_mpv(_ctx: *mut std::ffi::c_void, name: *const c_char) -> *mut std::ffi::c_void {
    eglGetProcAddress(name)
}

#[link(name = "mpv")]
#[allow(dead_code)]
extern "C" {
    pub fn mpv_create() -> *mut MpvHandle;
    pub fn mpv_initialize(ctx: *mut MpvHandle) -> i32;
    pub fn mpv_terminate_destroy(ctx: *mut MpvHandle);
    pub fn mpv_command(ctx: *mut MpvHandle, args: *const *const c_char) -> i32;
    pub fn mpv_set_option_string(ctx: *mut MpvHandle, name: *const c_char, data: *const c_char) -> i32;
    pub fn mpv_set_property_string(ctx: *mut MpvHandle, name: *const c_char, data: *const c_char) -> i32;
    pub fn mpv_get_property_string(ctx: *mut MpvHandle, name: *const c_char) -> *mut c_char;
    pub fn mpv_set_property(ctx: *mut MpvHandle, name: *const c_char, format: i32, data: *mut std::ffi::c_void) -> i32;
    pub fn mpv_get_property(ctx: *mut MpvHandle, name: *const c_char, format: i32, data: *mut std::ffi::c_void) -> i32;
    pub fn mpv_observe_property(ctx: *mut MpvHandle, reply_userdata: u64, name: *const c_char, format: i32) -> i32;
    pub fn mpv_wait_event(ctx: *mut MpvHandle, timeout: f64) -> *mut MpvEvent;
    pub fn mpv_error_string(error: i32) -> *const c_char;
    pub fn mpv_free(data: *mut std::ffi::c_void);

    pub fn mpv_render_context_create(res: *mut *mut MpvRenderContext, ctx: *mut MpvHandle, params: *const MpvRenderParam) -> i32;
    pub fn mpv_render_context_render(ctx: *mut MpvRenderContext, params: *const MpvRenderParam) -> i32;
    pub fn mpv_render_context_update(ctx: *mut MpvRenderContext) -> u64;
    pub fn mpv_render_context_free(ctx: *mut MpvRenderContext);
    pub fn mpv_render_context_set_update_callback(
        ctx: *mut MpvRenderContext,
        callback: Option<extern "C" fn(*mut std::ffi::c_void)>,
        callback_ctx: *mut std::ffi::c_void,
    );
    pub fn mpv_get_wakeup_pipe(ctx: *mut MpvHandle) -> i32;
}

extern "C" fn on_mpv_render_update(ctx: *mut std::ffi::c_void) {
    if ctx.is_null() {
        return;
    }
    #[cfg(unix)]
    {
        let write_fd = ctx as usize as i32;
        if write_fd >= 0 {
            let byte = 1u8;
            unsafe {
                libc::write(write_fd, &byte as *const _ as _, 1);
            }
        }
    }
    #[cfg(not(unix))]
    {
        unsafe {
            let thread = &*(ctx as *const std::thread::Thread);
            thread.unpark();
        }
    }
}

// -----------------------------------------------------------------------------

// Player Implementation
// -----------------------------------------------------------------------------
pub struct WaylandZeroCopyPlayer {
    mpv: Arc<Mutex<Option<usize>>>,
    render_ctx: Arc<Mutex<Option<usize>>>,
    display_addr: Arc<AtomicUsize>,
    surface_addr: Arc<AtomicUsize>,
    current_pos_ms: Arc<AtomicU64>,
    buffered_pos_ms: Arc<AtomicU64>,
    duration_ms: Arc<AtomicU64>,
    is_playing: Arc<AtomicBool>,
    is_buffering: Arc<AtomicBool>,
    buffering_percent: Arc<AtomicU8>,
    first_frame_rendered: Arc<AtomicBool>,
    eos_detected: Arc<AtomicBool>,
    has_error: Arc<AtomicBool>,
    last_error: Arc<Mutex<Option<String>>>,
    current_volume: Arc<Mutex<f64>>,
    current_rate: Arc<Mutex<f64>>,
    current_video_url: Arc<Mutex<String>>,
    current_audio_url: Arc<Mutex<Option<String>>>,
    current_frame: Arc<Mutex<Option<VideoFrameBuffer>>>,
    video_dims: Arc<Mutex<(u32, u32)>>,
    vp_x: Arc<AtomicI32>,
    vp_y: Arc<AtomicI32>,
    vp_w: Arc<AtomicI32>,
    vp_h: Arc<AtomicI32>,
    rendering_active: Arc<AtomicBool>,
    force_frame_refresh: Arc<AtomicBool>,
    underlay_mode: Arc<AtomicBool>,
    wakeup_tx: Arc<AtomicI32>,
    is_running: Arc<AtomicBool>,
}

/// Signalled by the render thread each time a new video frame is ready, so the UI can redraw
/// exactly at the video's frame rate (and not at all while paused) instead of a fixed 60 Hz clock.
pub static FRAME_NOTIFY: tokio::sync::Notify = tokio::sync::Notify::const_new();

/// Lightweight render-path counters (cheap relaxed atomics) used by benchmarks and MITUB_PROFILE.
pub static STAT_FRAMES: AtomicU64 = AtomicU64::new(0);
pub static STAT_RENDER_NS: AtomicU64 = AtomicU64::new(0);
pub static STAT_READ_NS: AtomicU64 = AtomicU64::new(0);
pub static STAT_BYTES: AtomicU64 = AtomicU64::new(0);

/// Snapshot of (frames, render_ns, readback_ns, bytes_copied).
pub fn render_stats() -> (u64, u64, u64, u64) {
    (
        STAT_FRAMES.load(Ordering::Relaxed),
        STAT_RENDER_NS.load(Ordering::Relaxed),
        STAT_READ_NS.load(Ordering::Relaxed),
        STAT_BYTES.load(Ordering::Relaxed),
    )
}

unsafe impl Send for WaylandZeroCopyPlayer {}
unsafe impl Sync for WaylandZeroCopyPlayer {}

impl WaylandZeroCopyPlayer {
    pub fn new<W>(_window: &W) -> Result<Self, Box<dyn std::error::Error + Send + Sync>>
    where
        W: HasWindowHandle + HasDisplayHandle,
    {
        Self::new_from_raw(0, 0)
    }

    pub fn new_from_raw(display_ptr: usize, surface_ptr: usize) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let player = Self {
            mpv: Arc::new(Mutex::new(None)),
            render_ctx: Arc::new(Mutex::new(None)),
            display_addr: Arc::new(AtomicUsize::new(display_ptr)),
            surface_addr: Arc::new(AtomicUsize::new(surface_ptr)),
            current_pos_ms: Arc::new(AtomicU64::new(0)),
            buffered_pos_ms: Arc::new(AtomicU64::new(0)),
            duration_ms: Arc::new(AtomicU64::new(0)),
            is_playing: Arc::new(AtomicBool::new(false)),
            is_buffering: Arc::new(AtomicBool::new(false)),
            buffering_percent: Arc::new(AtomicU8::new(100)),
            first_frame_rendered: Arc::new(AtomicBool::new(false)),
            eos_detected: Arc::new(AtomicBool::new(false)),
            has_error: Arc::new(AtomicBool::new(false)),
            last_error: Arc::new(Mutex::new(None)),
            current_volume: Arc::new(Mutex::new(1.0)),
            current_rate: Arc::new(Mutex::new(1.0)),
            current_video_url: Arc::new(Mutex::new(String::new())),
            current_audio_url: Arc::new(Mutex::new(None)),
            current_frame: Arc::new(Mutex::new(None)),
            video_dims: Arc::new(Mutex::new((1280, 720))),
            vp_x: Arc::new(AtomicI32::new(0)),
            vp_y: Arc::new(AtomicI32::new(0)),
            vp_w: Arc::new(AtomicI32::new(0)),
            vp_h: Arc::new(AtomicI32::new(0)),
            rendering_active: Arc::new(AtomicBool::new(true)),
            force_frame_refresh: Arc::new(AtomicBool::new(false)),
            underlay_mode: Arc::new(AtomicBool::new(true)),
            wakeup_tx: Arc::new(AtomicI32::new(-1)),
            is_running: Arc::new(AtomicBool::new(true)),
        };

        player.init_mpv()?;
        Ok(player)
    }

    fn init_mpv(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        unsafe {
            let ctx = mpv_create();
            if ctx.is_null() {
                return Err("Failed to create mpv context".into());
            }

            // High performance options for hardware decoding (VA-API on Intel Arc / Mesa)
            // Disable unused internal Lua scripts/OSC/OSD VMs to save ~15MB RAM and 6 threads
            let opts = [
                ("vo\0", "libmpv\0"),
                ("hwdec\0", "vaapi\0"),
                ("hwdec-codecs\0", "all\0"),
                ("keep-open\0", "yes\0"),
                ("ytdl\0", "no\0"),
                ("load-scripts\0", "no\0"),
                ("osc\0", "no\0"),
                ("osd-level\0", "0\0"),
                ("osd-bar\0", "no\0"),
                ("load-stats-overlay\0", "no\0"),
                ("load-osd-console\0", "no\0"),
                ("load-auto-profiles\0", "no\0"),
                ("sub-auto\0", "no\0"),
                ("audio-file-auto\0", "no\0"),
                ("network-timeout\0", "15\0"),
                ("demuxer-max-bytes\0", "25165824\0"),       // 24 MB forward buffer (<=1080p default; scales up dynamically for 1440p/4K)
                ("demuxer-max-back-bytes\0", "8388608\0"),   // 8 MB backward cache for instant reverse seek
                ("demuxer-readahead-secs\0", "20\0"),        // 20 seconds readahead
                ("cache-pause-wait\0", "2\0"),               // Resume after 2s buffered
                ("demuxer-seekable-cache\0", "yes\0"),       // Seek directly in RAM cache without HTTP reconnect
                ("force-seekable\0", "yes\0"),               // Force HTTP range-request seekability
                ("hr-seek\0", "default\0"),                  // Accurate seek with fast hardware keyframe decode
                ("hr-seek-framedrop\0", "yes\0"),            // Drop pre-target frames during seek for instantaneous resume
                ("vd-lavc-dr\0", "yes\0"),
                ("user-agent\0", "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36\0"),
            ];

            for (name, val) in opts {
                mpv_set_option_string(ctx, name.as_ptr() as _, val.as_ptr() as _);
            }

            let err = mpv_initialize(ctx);
            if err < 0 {
                let err_str = CStr::from_ptr(mpv_error_string(err)).to_string_lossy().into_owned();
                mpv_terminate_destroy(ctx);
                return Err(format!("mpv_initialize failed: {err_str}").into());
            }

            // Observe playback properties
            let props: &[(u64, &[u8], i32)] = &[
                (1u64, b"time-pos\0", MPV_FORMAT_DOUBLE),
                (2u64, b"duration\0", MPV_FORMAT_DOUBLE),
                (3u64, b"demuxer-cache-time\0", MPV_FORMAT_DOUBLE),
                (4u64, b"cache-buffering-state\0", MPV_FORMAT_INT64),
                (5u64, b"paused-for-cache\0", MPV_FORMAT_FLAG),
                (6u64, b"pause\0", MPV_FORMAT_FLAG),
                (7u64, b"dwidth\0", MPV_FORMAT_INT64),
                (8u64, b"dheight\0", MPV_FORMAT_INT64),
            ];

            for (id, name, fmt) in props {
                mpv_observe_property(ctx, *id, name.as_ptr() as _, *fmt);
            }

            *self.mpv.safe_lock() = Some(ctx as usize);

            // Spawn background event & render loop (initializes EGL/OpenGL on worker thread)
            self.spawn_event_loop(ctx as usize);
        }

        Ok(())
    }

    fn spawn_event_loop(&self, mpv_addr: usize) {
        let is_running = self.is_running.clone();
        let render_ctx = self.render_ctx.clone();
        let display_addr = self.display_addr.clone();
        let current_pos_ms = self.current_pos_ms.clone();
        let buffered_pos_ms = self.buffered_pos_ms.clone();
        let duration_ms = self.duration_ms.clone();
        let is_playing = self.is_playing.clone();
        let is_buffering = self.is_buffering.clone();
        let buffering_percent = self.buffering_percent.clone();
        let first_frame_rendered = self.first_frame_rendered.clone();
        let eos_detected = self.eos_detected.clone();
        let has_error = self.has_error.clone();
        let last_error = self.last_error.clone();
        let current_frame = self.current_frame.clone();
        let video_dims = self.video_dims.clone();
        let vp_w_atom = self.vp_w.clone();
        let vp_h_atom = self.vp_h.clone();
        let vp_y_atom = self.vp_y.clone();
        let rendering_active_atom = self.rendering_active.clone();
        let force_frame_refresh_atom = self.force_frame_refresh.clone();
        let wakeup_tx_atomic = self.wakeup_tx.clone();

        std::thread::Builder::new()
            .name("mitub-mpv-worker".into())
            .spawn(move || {
                let ctx = mpv_addr as *mut MpvHandle;

                // Connect to Wayland display to enable direct VA-API hardware decoding (iHD driver) on Linux
                let disp_ptr = display_addr.load(Ordering::SeqCst);
                #[allow(unused_mut)]
                let mut own_wl_dpy = false;
                #[allow(unused_variables)]
                let wl_dpy = if disp_ptr != 0 {
                    disp_ptr as *mut std::ffi::c_void
                } else {
                    #[cfg(target_os = "linux")]
                    {
                        let d = unsafe { wl_display_connect(std::ptr::null()) };
                        if !d.is_null() {
                            own_wl_dpy = true;
                        }
                        d
                    }
                    #[cfg(not(target_os = "linux"))]
                    {
                        std::ptr::null_mut()
                    }
                };

                // 1. Initialize EGL & OpenGL offscreen rendering context on this dedicated worker thread (Linux)
                #[allow(unused_mut)]
                let mut is_opengl = false;

                #[cfg(target_os = "linux")]
                let dpy = if !wl_dpy.is_null() {
                    let d = unsafe { eglGetDisplay(wl_dpy as _) };
                    if d.is_null() {
                        unsafe { eglGetDisplay(EGL_DEFAULT_DISPLAY) }
                    } else {
                        d
                    }
                } else {
                    unsafe { eglGetDisplay(EGL_DEFAULT_DISPLAY) }
                };
                #[cfg(target_os = "linux")]
                let mut egl_ctx = EGL_NO_CONTEXT;
                #[cfg(target_os = "linux")]
                let mut pbuf = std::ptr::null_mut();
                #[cfg(target_os = "linux")]
                let mut fbo = 0u32;
                #[cfg(target_os = "linux")]
                let mut tex = 0u32;
                #[cfg(target_os = "linux")]
                let mut gl_bind_fb: Option<unsafe extern "C" fn(GLenum, GLuint)> = None;
                #[cfg(target_os = "linux")]
                let mut gl_fb_tex: Option<unsafe extern "C" fn(GLenum, GLenum, GLenum, GLuint, GLint)> = None;
                #[cfg(target_os = "linux")]
                let mut gl_del_fb: Option<unsafe extern "C" fn(GLsizei, *const GLuint)> = None;

                #[cfg(target_os = "linux")]
                if !dpy.is_null() {
                    let mut maj = 0;
                    let mut min = 0;
                    if unsafe { eglInitialize(dpy, &mut maj, &mut min) } != 0 {
                        let config_attribs = [
                            EGL_SURFACE_TYPE, EGL_PBUFFER_BIT,
                            EGL_RENDERABLE_TYPE, EGL_OPENGL_BIT,
                            EGL_NONE,
                        ];
                        let mut cfg: EGLConfig = std::ptr::null_mut();
                        let mut num_configs = 0;
                        if unsafe { eglChooseConfig(dpy, config_attribs.as_ptr(), &mut cfg, 1, &mut num_configs) } != 0 && num_configs > 0 {
                            unsafe { eglBindAPI(EGL_OPENGL_API) };
                            egl_ctx = unsafe { eglCreateContext(dpy, cfg, EGL_NO_CONTEXT, std::ptr::null()) };
                            if !egl_ctx.is_null() {
                                let pbuf_attribs = [
                                    EGL_WIDTH, 16,
                                    EGL_HEIGHT, 16,
                                    EGL_NONE,
                                ];
                                pbuf = unsafe { eglCreatePbufferSurface(dpy, cfg, pbuf_attribs.as_ptr()) };
                                if !pbuf.is_null() && unsafe { eglMakeCurrent(dpy, pbuf, pbuf, egl_ctx) } != 0 {
                                    let c_gl_gen_fb = CString::new("glGenFramebuffers").unwrap();
                                    let c_gl_bind_fb = CString::new("glBindFramebuffer").unwrap();
                                    let c_gl_fb_tex = CString::new("glFramebufferTexture2D").unwrap();
                                    let c_gl_del_fb = CString::new("glDeleteFramebuffers").unwrap();

                                    let gl_gen_fb_ptr = unsafe { eglGetProcAddress(c_gl_gen_fb.as_ptr()) };
                                    let gl_bind_fb_ptr = unsafe { eglGetProcAddress(c_gl_bind_fb.as_ptr()) };
                                    let gl_fb_tex_ptr = unsafe { eglGetProcAddress(c_gl_fb_tex.as_ptr()) };
                                    let gl_del_fb_ptr = unsafe { eglGetProcAddress(c_gl_del_fb.as_ptr()) };

                                    if !gl_gen_fb_ptr.is_null() && !gl_bind_fb_ptr.is_null() && !gl_fb_tex_ptr.is_null() && !gl_del_fb_ptr.is_null() {
                                        let gl_gen_fb: unsafe extern "C" fn(GLsizei, *mut GLuint) = unsafe { std::mem::transmute(gl_gen_fb_ptr) };
                                        gl_bind_fb = Some(unsafe { std::mem::transmute(gl_bind_fb_ptr) });
                                        gl_fb_tex = Some(unsafe { std::mem::transmute(gl_fb_tex_ptr) });
                                        gl_del_fb = Some(unsafe { std::mem::transmute(gl_del_fb_ptr) });


                                        unsafe {
                                            glGenTextures(1, &mut tex);
                                            glBindTexture(GL_TEXTURE_2D, tex);
                                            glTexImage2D(GL_TEXTURE_2D, 0, GL_RGBA8 as i32, 1280, 720, 0, GL_RGBA, GL_UNSIGNED_BYTE, std::ptr::null());

                                            gl_gen_fb(1, &mut fbo);
                                            if let Some(bind_fb) = gl_bind_fb {
                                                bind_fb(GL_FRAMEBUFFER, fbo);
                                            }
                                            if let Some(fb_tex) = gl_fb_tex {
                                                fb_tex(GL_FRAMEBUFFER, GL_COLOR_ATTACHMENT0, GL_TEXTURE_2D, tex, 0);
                                            }
                                        }

                                        is_opengl = true;
                                    }
                                }
                            }
                        }
                    }
                }

                // 2. Create MPV render context (prefer hardware OpenGL with Wayland VA-API on Linux, fallback to software cross-platform)
                let mut r_ctx: *mut MpvRenderContext = std::ptr::null_mut();
                #[cfg(target_os = "linux")]
                let mut gl_init = MpvOpenglInitParams {
                    get_proc_address: Some(get_proc_address_mpv),
                    get_proc_address_ctx: std::ptr::null_mut(),
                };

                #[cfg(target_os = "linux")]
                let opengl_api = b"opengl\0";
                let sw_api = b"sw\0";

                #[cfg(target_os = "linux")]
                if is_opengl {
                    let mut params = Vec::with_capacity(4);
                    params.push(MpvRenderParam {
                        type_: MPV_RENDER_PARAM_API_TYPE,
                        data: opengl_api.as_ptr() as _,
                    });
                    params.push(MpvRenderParam {
                        type_: MPV_RENDER_PARAM_OPENGL_INIT_PARAMS,
                        data: &mut gl_init as *mut _ as _,
                    });
                    if !wl_dpy.is_null() {
                        params.push(MpvRenderParam {
                            type_: MPV_RENDER_PARAM_WL_DISPLAY,
                            data: wl_dpy,
                        });
                    }
                    params.push(MpvRenderParam {
                        type_: MPV_RENDER_PARAM_INVALID,
                        data: std::ptr::null_mut(),
                    });
                    let err = unsafe { mpv_render_context_create(&mut r_ctx, ctx, params.as_ptr()) };
                    if err < 0 {
                        eprintln!("[Mitub-Player] mpv_render_context_create OpenGL failed (code {err}), falling back to software");
                        is_opengl = false;
                    } else {
                        println!("[Mitub-Player] Hardware Zero-Copy OpenGL/VA-API render context initialized successfully");
                    }
                }

                if !is_opengl {
                    let params = [
                        MpvRenderParam {
                            type_: MPV_RENDER_PARAM_API_TYPE,
                            data: sw_api.as_ptr() as _,
                        },
                        MpvRenderParam {
                            type_: MPV_RENDER_PARAM_INVALID,
                            data: std::ptr::null_mut(),
                        },
                    ];
                    unsafe { mpv_render_context_create(&mut r_ctx, ctx, params.as_ptr()) };
                }

                *render_ctx.safe_lock() = Some(r_ctx as usize);

                #[cfg(unix)]
                let pipe_fd = unsafe { mpv_get_wakeup_pipe(ctx) };
                #[cfg(unix)]
                let mut wakeup_pipe = [-1i32; 2];
                #[cfg(unix)]
                let pipe_res = unsafe { libc::pipe(wakeup_pipe.as_mut_ptr()) };
                #[cfg(unix)]
                if pipe_res == 0 {
                    unsafe {
                        libc::fcntl(wakeup_pipe[0], libc::F_SETFL, libc::O_NONBLOCK);
                        libc::fcntl(wakeup_pipe[1], libc::F_SETFL, libc::O_NONBLOCK);
                    }
                }
                #[cfg(unix)]
                let wakeup_rx = wakeup_pipe[0];
                #[cfg(unix)]
                let wakeup_tx = wakeup_pipe[1];
                #[cfg(unix)]
                wakeup_tx_atomic.store(wakeup_tx, Ordering::SeqCst);

                #[cfg(unix)]
                if wakeup_tx >= 0 {
                    unsafe {
                        mpv_render_context_set_update_callback(
                            r_ctx,
                            Some(on_mpv_render_update),
                            wakeup_tx as usize as *mut std::ffi::c_void,
                        );
                    }
                }

                #[cfg(not(unix))]
                let worker_thread_handle = Box::into_raw(Box::new(std::thread::current()));
                #[cfg(not(unix))]
                unsafe {
                    mpv_render_context_set_update_callback(
                        r_ctx,
                        Some(on_mpv_render_update),
                        worker_thread_handle as *mut std::ffi::c_void,
                    );
                }

                let mut current_buf_w: u32 = 1280;
                let mut current_buf_h: u32 = 720;
                let fmt_rgba = CString::new("rgba").unwrap();

                // Triple-Buffer recyclable pool (3 slots: 1 in UI, 1 in current_frame, 1 being written by glReadPixels)
                // Initialized empty so idle startup uses 0 MB until video playback actually begins.
                let (pool_tx, pool_rx): (Sender<Vec<u8>>, Receiver<Vec<u8>>) = bounded(3);
                for _ in 0..3 {
                    let _ = pool_tx.send(Vec::new());
                }
                let mut frame_seq: u64 = 0;

                while is_running.load(Ordering::Relaxed) {
                    // 1. Drain MPV events
                    loop {
                        let ev_ptr = unsafe { mpv_wait_event(ctx, 0.0) };
                        if ev_ptr.is_null() {
                            break;
                        }
                        let ev = unsafe { *ev_ptr };
                        if ev.event_id == MPV_EVENT_NONE {
                            break;
                        }

                        match ev.event_id {
                            MPV_EVENT_PLAYBACK_RESTART => {
                                first_frame_rendered.store(true, Ordering::SeqCst);
                                has_error.store(false, Ordering::SeqCst);
                                *last_error.safe_lock() = None;
                                crate::diagnostics::session_first_frame();
                            }
                            MPV_EVENT_END_FILE => {
                                let mut is_err = false;
                                if !ev.data.is_null() {
                                    let end_file = unsafe { *(ev.data as *const MpvEventEndFile) };
                                    if end_file.reason == MPV_END_FILE_REASON_ERROR || end_file.error < 0 {
                                        is_err = true;
                                        let err_msg = unsafe {
                                            let p = mpv_error_string(end_file.error);
                                            if !p.is_null() {
                                                CStr::from_ptr(p).to_string_lossy().into_owned()
                                            } else {
                                                "Unknown MPV playback error".to_string()
                                            }
                                        };
                                        eprintln!("[mitub-mpv] Playback error: {err_msg} (code {})", end_file.error);
                                        *last_error.safe_lock() = Some(err_msg);
                                        has_error.store(true, Ordering::SeqCst);
                                    }
                                }
                                if !is_err {
                                    eos_detected.store(true, Ordering::SeqCst);
                                }
                                is_playing.store(false, Ordering::SeqCst);
                            }
                            MPV_EVENT_PROPERTY_CHANGE => {
                                if !ev.data.is_null() {
                                    let prop = unsafe { *(ev.data as *const MpvEventProperty) };
                                    if !prop.data.is_null() {
                                        match ev.reply_userdata {
                                            1 => { // time-pos
                                                let secs = unsafe { *(prop.data as *const f64) };
                                                if secs >= 0.0 {
                                                    let ms = (secs * 1000.0) as u64;
                                                    current_pos_ms.store(ms, Ordering::SeqCst);
                                                }
                                            }
                                            2 => { // duration
                                                let secs = unsafe { *(prop.data as *const f64) };
                                                if secs > 0.0 {
                                                    duration_ms.store((secs * 1000.0) as u64, Ordering::SeqCst);
                                                }
                                            }
                                            3 => { // demuxer-cache-time
                                                let cache_secs = unsafe { *(prop.data as *const f64) };
                                                let pos = current_pos_ms.load(Ordering::SeqCst);
                                                if cache_secs >= 0.0 {
                                                    let buf_ms = (cache_secs * 1000.0) as u64; // demuxer-cache-time is an absolute timestamp, not a duration
                                                    let buf_ms = buf_ms.max(pos);
                                                    buffered_pos_ms.store(buf_ms, Ordering::SeqCst);
                                                }
                                            }
                                            4 => { // cache-buffering-state
                                                let pct = unsafe { *(prop.data as *const i64) };
                                                let clamped_pct = pct.clamp(0, 100) as u8;
                                                buffering_percent.store(clamped_pct, Ordering::SeqCst);
                                                crate::diagnostics::log_buffering("mpv", pct as i32, clamped_pct);
                                            }
                                            5 => { // paused-for-cache
                                                let flag = unsafe { *(prop.data as *const i32) };
                                                is_buffering.store(flag != 0, Ordering::SeqCst);
                                            }
                                            6 => { // pause
                                                let flag = unsafe { *(prop.data as *const i32) };
                                                is_playing.store(flag == 0, Ordering::SeqCst);
                                            }
                                            7 => { // dwidth
                                                let w = unsafe { *(prop.data as *const i64) };
                                                if w > 0 {
                                                    video_dims.safe_lock().0 = w as u32;
                                                }
                                            }
                                            8 => { // dheight
                                                let h = unsafe { *(prop.data as *const i64) };
                                                if h > 0 {
                                                    video_dims.safe_lock().1 = h as u32;
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }

                    // 2. Check for new video frame to render
                    let flags = unsafe { mpv_render_context_update(r_ctx) };
                    let force_refresh = force_frame_refresh_atom.swap(false, Ordering::SeqCst)
                        && first_frame_rendered.load(Ordering::Relaxed);
                    if (flags & MPV_RENDER_UPDATE_FRAME != 0) || force_refresh {
                        let active = rendering_active_atom.load(Ordering::Relaxed);
                        let vp_w = vp_w_atom.load(Ordering::Relaxed);
                        let vp_h = vp_h_atom.load(Ordering::Relaxed);
                        let vp_y = vp_y_atom.load(Ordering::Relaxed);

                        // If window is minimized/hidden, video scrolled away, or inactive, skip GPU rendering completely!
                        // mpv updates playback clocks and keeps audio in perfect sync with ZERO GPU/RAM bandwidth.
                        let should_skip = !active || vp_w <= 16 || vp_h <= 16 || vp_y <= -5000;
                        if should_skip {
                            let mut skip: i32 = 1;
                            let render_params = [
                                MpvRenderParam {
                                    type_: MPV_RENDER_PARAM_SKIP_RENDERING,
                                    data: &mut skip as *mut i32 as *mut std::ffi::c_void,
                                },
                                MpvRenderParam {
                                    type_: MPV_RENDER_PARAM_INVALID,
                                    data: std::ptr::null_mut(),
                                },
                            ];
                            unsafe {
                                mpv_render_context_render(r_ctx, render_params.as_ptr());
                            }
                            continue;
                        }

                        let (vid_w, vid_h) = *video_dims.safe_lock();
                        // Render at the size the video is actually displayed at (x1.15 margin for subpixel sharpness),
                        // never above the source, while strictly preserving the exact video aspect ratio so libmpv
                        // never bakes letterbox or pillarbox black bars into the FBO texture.
                        let (target_w, target_h) = if vid_w > 0 && vid_h > 0 {
                            let max_scale = (1920.0 / vid_w as f32).min(1080.0 / vid_h as f32).min(1.0);
                            let min_scale = (240.0 / vid_w.max(vid_h) as f32).min(max_scale);
                            let vp_scale = if vp_w > 16 && vp_h > 16 {
                                let sx = (vp_w as f32 * 1.15) / vid_w as f32;
                                let sy = (vp_h as f32 * 1.15) / vid_h as f32;
                                sx.max(sy).clamp(min_scale, max_scale)
                            } else {
                                max_scale
                            };
                            let tw = ((vid_w as f32 * vp_scale).round() as u32 & !1).max(2);
                            let th = ((vid_h as f32 * vp_scale).round() as u32 & !1).max(2);
                            (tw, th)
                        } else {
                            (1280, 720)
                        };

                        if target_w != current_buf_w || target_h != current_buf_h {
                            current_buf_w = target_w;
                            current_buf_h = target_h;

                            #[cfg(target_os = "linux")]
                            if is_opengl {
                                unsafe {
                                    glBindTexture(GL_TEXTURE_2D, tex);
                                    glTexImage2D(GL_TEXTURE_2D, 0, GL_RGBA8 as i32, current_buf_w as i32, current_buf_h as i32, 0, GL_RGBA, GL_UNSIGNED_BYTE, std::ptr::null());
                                    if let Some(bind_fb) = gl_bind_fb {
                                        bind_fb(GL_FRAMEBUFFER, fbo);
                                    }
                                    if let Some(fb_tex) = gl_fb_tex {
                                        fb_tex(GL_FRAMEBUFFER, GL_COLOR_ATTACHMENT0, GL_TEXTURE_2D, tex, 0);
                                    }
                                }
                            }
                        }

                        let byte_size = (current_buf_w * current_buf_h * 4) as usize;

                        // 1. Get a recycled vector from the pool (Zero Heap Allocations in RAM)
                        let mut target_vec = match pool_rx.try_recv() {
                            Ok(mut v) => {
                                if v.len() != byte_size {
                                    v.resize(byte_size, 0);
                                }
                                v
                            }
                            Err(_) => vec![0u8; byte_size],
                        };

                        #[cfg(target_os = "linux")]
                        let mut gl_success = false;
                        #[cfg(target_os = "linux")]
                        if is_opengl {
                            let mut mpv_fbo = MpvOpenglFbo {
                                fbo: fbo as i32,
                                w: current_buf_w as i32,
                                h: current_buf_h as i32,
                                internal_format: GL_RGBA8 as i32,
                            };
                            let render_params = [
                                MpvRenderParam {
                                    type_: MPV_RENDER_PARAM_OPENGL_FBO,
                                    data: &mut mpv_fbo as *mut _ as _,
                                },
                                MpvRenderParam {
                                    type_: MPV_RENDER_PARAM_INVALID,
                                    data: std::ptr::null_mut(),
                                },
                            ];

                            let t_render = std::time::Instant::now();
                            let err = unsafe { mpv_render_context_render(r_ctx, render_params.as_ptr()) };
                            STAT_RENDER_NS.fetch_add(t_render.elapsed().as_nanos() as u64, Ordering::Relaxed);
                            if err >= 0 {
                                let t_read = std::time::Instant::now();
                                unsafe {
                                    if let Some(bind_fb) = gl_bind_fb {
                                        bind_fb(GL_FRAMEBUFFER, fbo);
                                    }
                                    glReadPixels(
                                        0,
                                        0,
                                        current_buf_w as i32,
                                        current_buf_h as i32,
                                        GL_RGBA,
                                        GL_UNSIGNED_BYTE,
                                        target_vec.as_mut_ptr() as _,
                                    );
                                }
                                STAT_READ_NS.fetch_add(t_read.elapsed().as_nanos() as u64, Ordering::Relaxed);
                                STAT_BYTES.fetch_add(byte_size as u64, Ordering::Relaxed);
                                STAT_FRAMES.fetch_add(1, Ordering::Relaxed);
                                gl_success = true;
                            }
                        }

                        #[cfg(target_os = "linux")]
                        let should_render_sw = !gl_success;
                        #[cfg(not(target_os = "linux"))]
                        let should_render_sw = true;

                        let render_ok = if should_render_sw {
                            // Software fallback render path with RecycledBuf
                            let mut size = [current_buf_w as i32, current_buf_h as i32];
                            let mut stride = (current_buf_w * 4) as usize;

                            let render_params = [
                                MpvRenderParam {
                                    type_: MPV_RENDER_PARAM_SW_SIZE,
                                    data: size.as_mut_ptr() as _,
                                },
                                MpvRenderParam {
                                    type_: MPV_RENDER_PARAM_SW_FORMAT,
                                    data: fmt_rgba.as_ptr() as _,
                                },
                                MpvRenderParam {
                                    type_: MPV_RENDER_PARAM_SW_STRIDE,
                                    data: &mut stride as *mut usize as _,
                                },
                                MpvRenderParam {
                                    type_: MPV_RENDER_PARAM_SW_POINTER,
                                    data: target_vec.as_mut_ptr() as _,
                                },
                                MpvRenderParam {
                                    type_: MPV_RENDER_PARAM_INVALID,
                                    data: std::ptr::null_mut(),
                                },
                            ];

                            let err = unsafe { mpv_render_context_render(r_ctx, render_params.as_ptr()) };
                            err >= 0
                        } else {
                            true
                        };

                        if render_ok {
                            frame_seq += 1;
                            let owner = RecycledBuf {
                                buf: target_vec,
                                pool_tx: pool_tx.clone(),
                            };
                            let byte_buf = Bytes::from_owner(owner);
                            let v_frame = VideoFrameBuffer {
                                frame_id: frame_seq,
                                width: current_buf_w,
                                height: current_buf_h,
                                data: byte_buf,
                            };
                            *current_frame.safe_lock() = Some(v_frame);
                            FRAME_NOTIFY.notify_one();
                            first_frame_rendered.store(true, Ordering::SeqCst);
                        }
                    }

                    // 3. Reactive sleep: wakes immediately on MPV core events or new render frame notification
                    let timeout_ms = if is_playing.load(Ordering::Relaxed) { 50 } else { 350 };
                    #[cfg(unix)]
                    {
                        let mut pfds = [
                            libc::pollfd {
                                fd: pipe_fd,
                                events: libc::POLLIN,
                                revents: 0,
                            },
                            libc::pollfd {
                                fd: wakeup_rx,
                                events: libc::POLLIN,
                                revents: 0,
                            },
                        ];

                        let poll_res = unsafe { libc::poll(pfds.as_mut_ptr(), if wakeup_rx >= 0 { 2 } else { 1 }, timeout_ms) };
                        if poll_res > 0 {
                            if (pfds[0].revents & libc::POLLIN) != 0 {
                                let mut drain_buf = [0u8; 256];
                                let _ = unsafe {
                                    libc::read(
                                        pipe_fd,
                                        drain_buf.as_mut_ptr() as *mut libc::c_void,
                                        drain_buf.len(),
                                    )
                                };
                            }
                            if wakeup_rx >= 0 && (pfds[1].revents & libc::POLLIN) != 0 {
                                let mut drain_buf = [0u8; 256];
                                let _ = unsafe {
                                    libc::read(
                                        wakeup_rx,
                                        drain_buf.as_mut_ptr() as *mut libc::c_void,
                                        drain_buf.len(),
                                    )
                                };
                            }
                        } else if pipe_fd < 0 && wakeup_rx < 0 {
                            std::thread::sleep(std::time::Duration::from_millis(timeout_ms as u64));
                        }
                    }

                    #[cfg(not(unix))]
                    {
                        std::thread::park_timeout(std::time::Duration::from_millis(timeout_ms as u64));
                    }
                }

                // Cleanup OpenGL & EGL resources on thread exit
                wakeup_tx_atomic.store(-1, Ordering::SeqCst);
                *render_ctx.safe_lock() = None;
                #[cfg(unix)]
                {
                    if wakeup_tx >= 0 {
                        unsafe {
                            mpv_render_context_set_update_callback(r_ctx, None, std::ptr::null_mut());
                            libc::close(wakeup_tx);
                        }
                    }
                    if wakeup_rx >= 0 {
                        unsafe {
                            libc::close(wakeup_rx);
                        }
                    }
                }
                #[cfg(not(unix))]
                unsafe {
                    mpv_render_context_set_update_callback(r_ctx, None, std::ptr::null_mut());
                    let _ = Box::from_raw(worker_thread_handle);
                }

                if !r_ctx.is_null() {
                    unsafe { mpv_render_context_free(r_ctx) };
                }
                #[cfg(target_os = "linux")]
                if is_opengl {
                    unsafe {
                        if tex != 0 {
                            glDeleteTextures(1, &tex);
                        }
                        if let Some(del_fb) = gl_del_fb {
                            if fbo != 0 {
                                del_fb(1, &fbo);
                            }
                        }
                        if !pbuf.is_null() {
                            eglDestroySurface(dpy, pbuf);
                        }
                        if !egl_ctx.is_null() {
                            eglDestroyContext(dpy, egl_ctx);
                        }
                        if !dpy.is_null() {
                            eglTerminate(dpy);
                        }
                    }
                }

                #[cfg(target_os = "linux")]
                if own_wl_dpy && !wl_dpy.is_null() {
                    unsafe { wl_display_disconnect(wl_dpy) };
                }
            })
            .expect("Failed to spawn mitub-mpv-worker thread");
    }

    pub fn play_stream(&self, video_url: &str, audio_url: Option<&str>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.play_stream_with_seek(video_url, audio_url, None)
    }

    pub fn play_stream_with_seek(
        &self,
        video_url: &str,
        audio_url: Option<&str>,
        initial_seek_ms: Option<u64>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.play_stream_internal(video_url, audio_url, initial_seek_ms, true)
    }

    pub fn play_stream_with_seek_paused(
        &self,
        video_url: &str,
        audio_url: Option<&str>,
        initial_seek_ms: Option<u64>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.play_stream_internal(video_url, audio_url, initial_seek_ms, false)
    }

    fn play_stream_internal(
        &self,
        video_url: &str,
        audio_url: Option<&str>,
        initial_seek_ms: Option<u64>,
        auto_play: bool,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let t_start = Instant::now();
        crate::diagnostics::stage_start("mpv_stream_setup");

        *self.current_video_url.safe_lock() = video_url.to_string();
        *self.current_audio_url.safe_lock() = audio_url.map(|s| s.to_string());

        self.first_frame_rendered.store(false, Ordering::SeqCst);
        self.eos_detected.store(false, Ordering::SeqCst);
        self.has_error.store(false, Ordering::SeqCst);
        *self.last_error.safe_lock() = None;

        let ctx_opt = *self.mpv.safe_lock();
        let ctx = match ctx_opt {
            Some(addr) => addr as *mut MpvHandle,
            None => return Err("MPV context not initialized".into()),
        };

        // Reset volume and playback rate
        let vol = *self.current_volume.safe_lock();
        let rate = *self.current_rate.safe_lock();

        let c_vol = CString::new(format!("{}", (vol * 100.0).clamp(0.0, 100.0)))?;
        let c_rate = CString::new(format!("{:.2}", rate))?;
        unsafe {
            mpv_set_property_string(ctx, b"volume\0".as_ptr() as _, c_vol.as_ptr());
            mpv_set_property_string(ctx, b"speed\0".as_ptr() as _, c_rate.as_ptr());
        }

        // Set start time if initial_seek requested
        if let Some(pos_ms) = initial_seek_ms {
            let start_sec = format!("{:.3}", pos_ms as f64 / 1000.0);
            let c_start = CString::new(start_sec)?;
            unsafe {
                mpv_set_property_string(ctx, b"start\0".as_ptr() as _, c_start.as_ptr());
            }
            self.current_pos_ms.store(pos_ms, Ordering::SeqCst);
        } else {
            unsafe {
                mpv_set_property_string(ctx, b"start\0".as_ptr() as _, b"0\0".as_ptr() as _);
            }
            self.current_pos_ms.store(0, Ordering::SeqCst);
        }

        // Load primary video stream and synchronized audio track
        let c_loadfile = CString::new("loadfile")?;
        let c_v_url = CString::new(video_url)?;
        let c_replace = CString::new("replace")?;
        let c_index = CString::new("-1")?;

        let err = if let Some(a_url) = audio_url {
            let opt = format!("audio-file=\"{}\"", a_url.replace('"', "\\\""));
            let c_opt = CString::new(opt)?;
            let cmd = [c_loadfile.as_ptr(), c_v_url.as_ptr(), c_replace.as_ptr(), c_index.as_ptr(), c_opt.as_ptr(), std::ptr::null()];
            unsafe { mpv_command(ctx, cmd.as_ptr()) }
        } else {
            let cmd = [c_loadfile.as_ptr(), c_v_url.as_ptr(), c_replace.as_ptr(), std::ptr::null()];
            unsafe { mpv_command(ctx, cmd.as_ptr()) }
        };
        if err < 0 {
            let err_str = unsafe { CStr::from_ptr(mpv_error_string(err)).to_string_lossy().into_owned() };
            return Err(format!("loadfile command failed: {err_str}").into());
        }

        self.has_error.store(false, Ordering::SeqCst);
        *self.last_error.safe_lock() = None;

        if auto_play {
            // Unpause to begin immediate playback
            unsafe {
                mpv_set_property_string(ctx, b"pause\0".as_ptr() as _, b"no\0".as_ptr() as _);
            }
            self.is_playing.store(true, Ordering::SeqCst);
        } else {
            // Keep paused to load first frame without autoplaying
            unsafe {
                mpv_set_property_string(ctx, b"pause\0".as_ptr() as _, b"yes\0".as_ptr() as _);
            }
            self.is_playing.store(false, Ordering::SeqCst);
        }

        crate::diagnostics::session_pipeline_built();
        crate::diagnostics::stage_end("mpv_stream_setup", Some(t_start));

        Ok(())
    }

    fn trigger_worker_wakeup(&self) {
        let fd = self.wakeup_tx.load(Ordering::Relaxed);
        if fd >= 0 {
            let byte = 1u8;
            unsafe {
                libc::write(fd, &byte as *const _ as _, 1);
            }
        }
    }

    pub fn play(&self) {
        if let Some(addr) = *self.mpv.safe_lock() {
            unsafe {
                let h = addr as *mut MpvHandle;
                mpv_set_property_string(h, b"pause\0".as_ptr() as _, b"no\0".as_ptr() as _);
            }
            self.is_playing.store(true, Ordering::SeqCst);
            self.trigger_worker_wakeup();
        }
    }

    pub fn pause(&self) {
        if let Some(addr) = *self.mpv.safe_lock() {
            unsafe {
                let h = addr as *mut MpvHandle;
                mpv_set_property_string(h, b"pause\0".as_ptr() as _, b"yes\0".as_ptr() as _);
            }
            self.is_playing.store(false, Ordering::SeqCst);
            self.trigger_worker_wakeup();
        }
    }

    pub fn toggle_playback(&self) -> bool {
        if self.is_playing.load(Ordering::SeqCst) {
            self.pause();
            false
        } else {
            self.play();
            true
        }
    }

    pub fn stop(&self) {
        if let Some(addr) = *self.mpv.safe_lock() {
            let c_stop = CString::new("stop").unwrap();
            let cmd = [c_stop.as_ptr(), std::ptr::null()];
            unsafe {
                mpv_command(addr as *mut MpvHandle, cmd.as_ptr());
            }
            self.is_playing.store(false, Ordering::SeqCst);
            self.first_frame_rendered.store(false, Ordering::SeqCst);
            *self.current_frame.safe_lock() = None;
            self.trigger_worker_wakeup();
            crate::power_manager::trim_process_memory();
        }
    }

    pub fn shutdown_sync(&self) {
        self.stop();
    }

    pub fn seek_ms(&self, target_ms: u64) -> u64 {
        let cur_ms = self.get_position_ms();
        crate::diagnostics::session_seek_start(cur_ms, target_ms);
        let sec_str = format!("{:.3}", target_ms as f64 / 1000.0);
        if let Some(addr) = *self.mpv.safe_lock() {
            let c_seek = CString::new("seek").unwrap();
            let c_pos = CString::new(sec_str).unwrap();
            let c_mode = CString::new("absolute").unwrap();
            let cmd = [c_seek.as_ptr(), c_pos.as_ptr(), c_mode.as_ptr(), std::ptr::null()];
            unsafe {
                mpv_command(addr as *mut MpvHandle, cmd.as_ptr());
            }
            self.current_pos_ms.store(target_ms, Ordering::SeqCst);
            self.trigger_worker_wakeup();
            crate::diagnostics::session_seek_end();
        }
        target_ms
    }

    pub fn seek_relative_secs(&self, offset_secs: i64) -> u64 {
        let cur = self.current_pos_ms.load(Ordering::SeqCst);
        let target = if offset_secs < 0 {
            cur.saturating_sub((-offset_secs as u64) * 1000)
        } else {
            cur.saturating_add((offset_secs as u64) * 1000)
        };
        self.seek_ms(target)
    }

    pub fn set_volume(&self, vol: f64) {
        *self.current_volume.safe_lock() = vol;
        if let Some(addr) = *self.mpv.safe_lock() {
            let c_vol = CString::new(format!("{}", (vol * 100.0).clamp(0.0, 100.0))).unwrap();
            unsafe {
                mpv_set_property_string(addr as *mut MpvHandle, b"volume\0".as_ptr() as _, c_vol.as_ptr());
            }
        }
    }

    pub fn set_playback_rate(&self, rate: f64) {
        *self.current_rate.safe_lock() = rate;
        if let Some(addr) = *self.mpv.safe_lock() {
            let c_rate = CString::new(format!("{:.2}", rate)).unwrap();
            unsafe {
                mpv_set_property_string(addr as *mut MpvHandle, b"speed\0".as_ptr() as _, c_rate.as_ptr());
            }
        }
    }

    pub fn set_power_profile(&self, profile: PowerProfile) {
        let (max_bytes, back_bytes, readahead) = match profile {
            PowerProfile::Battery => ("16777216\0", "6291456\0", "12\0"),
            PowerProfile::Ac => ("25165824\0", "8388608\0", "20\0"),
        };
        if let Some(addr) = *self.mpv.safe_lock() {
            unsafe {
                let h = addr as *mut MpvHandle;
                mpv_set_property_string(h, b"demuxer-max-bytes\0".as_ptr() as _, max_bytes.as_ptr() as _);
                mpv_set_property_string(h, b"demuxer-max-back-bytes\0".as_ptr() as _, back_bytes.as_ptr() as _);
                mpv_set_property_string(h, b"demuxer-readahead-secs\0".as_ptr() as _, readahead.as_ptr() as _);
            }
        }
    }

    /// Dynamically scales libmpv demuxer RAM cache according to the selected stream height:
    /// - <=1080p: 24 MB forward / 8 MB backward (~75s of 1080p AV1/VP9, saving 64 MB of RAM)
    /// - 1440p (2K): 36 MB forward / 12 MB backward
    /// - 2160p (4K): 56 MB forward / 16 MB backward (ensuring smooth 4K60 playback)
    pub fn set_stream_buffer_profile(&self, height: u32, profile: PowerProfile) {
        let (max_bytes, back_bytes, readahead) = if profile == PowerProfile::Battery {
            if height >= 2000 {
                ("41943040\0", "10485760\0", "18\0") // 40 MB / 10 MB on Battery 4K
            } else if height >= 1300 {
                ("25165824\0", "8388608\0", "15\0")  // 24 MB / 8 MB on Battery 1440p
            } else {
                ("16777216\0", "6291456\0", "12\0")  // 16 MB / 6 MB on Battery <=1080p
            }
        } else if height >= 2000 {
            ("58720256\0", "16777216\0", "25\0")     // 56 MB / 16 MB on AC 4K
        } else if height >= 1300 {
            ("37748736\0", "12582912\0", "22\0")     // 36 MB / 12 MB on AC 1440p
        } else {
            ("25165824\0", "8388608\0", "20\0")      // 24 MB / 8 MB on AC <=1080p
        };
        if let Some(addr) = *self.mpv.safe_lock() {
            unsafe {
                let h = addr as *mut MpvHandle;
                mpv_set_property_string(h, b"demuxer-max-bytes\0".as_ptr() as _, max_bytes.as_ptr() as _);
                mpv_set_property_string(h, b"demuxer-max-back-bytes\0".as_ptr() as _, back_bytes.as_ptr() as _);
                mpv_set_property_string(h, b"demuxer-readahead-secs\0".as_ptr() as _, readahead.as_ptr() as _);
            }
        }
    }

    /// Returns an mpv string property (e.g. "hwdec-current"), for diagnostics.
    pub fn get_property_string(&self, name: &str) -> Option<String> {
        let guard = self.mpv.safe_lock();
        let ctx = (*guard)?;
        let c_name = CString::new(name).ok()?;
        unsafe {
            let ptr = mpv_get_property_string(ctx as *mut MpvHandle, c_name.as_ptr());
            if ptr.is_null() {
                return None;
            }
            let v = CStr::from_ptr(ptr).to_string_lossy().into_owned();
            mpv_free(ptr as *mut std::ffi::c_void);
            Some(v)
        }
    }

    pub fn get_position_ms(&self) -> u64 {
        self.current_pos_ms.load(Ordering::SeqCst)
    }

    pub fn get_duration_ms(&self) -> u64 {
        self.duration_ms.load(Ordering::SeqCst)
    }

    pub fn get_buffered_pos_ms(&self) -> u64 {
        self.buffered_pos_ms.load(Ordering::SeqCst)
    }

    pub fn is_playing(&self) -> bool {
        self.is_playing.load(Ordering::SeqCst)
    }

    pub fn is_buffering(&self) -> bool {
        self.is_buffering.load(Ordering::SeqCst)
    }

    pub fn buffering_percent(&self) -> u8 {
        self.buffering_percent.load(Ordering::SeqCst)
    }

    pub fn is_first_frame_rendered(&self) -> bool {
        self.first_frame_rendered.load(Ordering::SeqCst)
    }

    pub fn is_eos(&self) -> bool {
        self.eos_detected.load(Ordering::SeqCst)
    }

    pub fn reset_eos(&self) {
        self.eos_detected.store(false, Ordering::SeqCst);
    }

    pub fn has_error(&self) -> bool {
        self.has_error.load(Ordering::SeqCst)
    }

    pub fn get_last_error(&self) -> Option<String> {
        self.last_error.safe_lock().clone()
    }

    pub fn get_current_frame(&self) -> Option<VideoFrameBuffer> {
        self.current_frame.safe_lock().clone()
    }

    pub fn get_video_dims(&self) -> (u32, u32) {
        *self.video_dims.safe_lock()
    }

    pub fn switch_surface(&self, _surf_ptr: usize, _explicit_pos_ms: Option<u64>) {
        // With MPV rendering directly into Iced's texture plane, surface swapping
        // is completely safe and requires zero pipeline teardown or recreation!
    }

    pub fn update_handles(&self, disp_ptr: usize, surf_ptr: usize) {
        if disp_ptr != 0 {
            self.display_addr.store(disp_ptr, Ordering::SeqCst);
        }
        if surf_ptr != 0 {
            self.surface_addr.store(surf_ptr, Ordering::SeqCst);
        }
    }

    pub fn set_underlay(&self, underlay: bool) {
        self.underlay_mode.store(underlay, Ordering::SeqCst);
    }

    pub fn set_viewport(&self, x: i32, y: i32, w: i32, h: i32) {
        let prev_w = self.vp_w.swap(w, Ordering::Relaxed);
        let prev_h = self.vp_h.swap(h, Ordering::Relaxed);
        let prev_y = self.vp_y.swap(y, Ordering::Relaxed);
        self.vp_x.store(x, Ordering::Relaxed);
        let was_hidden = prev_w <= 16 || prev_h <= 16 || prev_y <= -5000;
        let now_visible = w > 16 && h > 16 && y > -5000;
        if was_hidden && now_visible && self.rendering_active.load(Ordering::Relaxed) {
            self.force_frame_refresh.store(true, Ordering::SeqCst);
            self.trigger_worker_wakeup();
        }
    }

    pub fn set_rendering_active(&self, active: bool) {
        let was_active = self.rendering_active.swap(active, Ordering::SeqCst);
        if active && !was_active {
            self.force_frame_refresh.store(true, Ordering::SeqCst);
            self.trigger_worker_wakeup();
        }
    }

    pub fn is_rendering_active(&self) -> bool {
        self.rendering_active.load(Ordering::Relaxed)
    }

    pub fn switch_resolution(&self, video_url: &str, audio_url: Option<&str>) {
        let pos_ms = self.get_position_ms();
        let _ = self.play_stream_with_seek(video_url, audio_url, Some(pos_ms));
    }
}

impl Drop for WaylandZeroCopyPlayer {
    fn drop(&mut self) {
        self.is_running.store(false, Ordering::SeqCst);
        // Immediately wake up the worker thread from poll() so it doesn't wait up to 350ms
        self.trigger_worker_wakeup();

        // Give worker thread a moment to cleanly exit and free its OpenGL / EGL context
        for _ in 0..40 {
            if self.render_ctx.safe_lock().is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        if let Some(r_ctx) = self.render_ctx.safe_lock().take() {
            unsafe {
                mpv_render_context_free(r_ctx as *mut MpvRenderContext);
            }
        }
        if let Some(ctx) = self.mpv.safe_lock().take() {
            unsafe {
                mpv_terminate_destroy(ctx as *mut MpvHandle);
            }
        }
    }
}
