use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Instant;

static DEBUG_ENABLED: AtomicBool = AtomicBool::new(false);
static INITIALIZED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Default)]
pub struct SessionMetrics {
    pub video_id: String,
    pub t_open: Option<Instant>,
    pub t_details: Option<Instant>,
    pub t_pipeline_built: Option<Instant>,
    pub t_first_frame: Option<Instant>,
    pub seek_count: u32,
    pub total_seek_time_ms: f64,
    pub current_seek_start: Option<Instant>,
    pub stall_count: u32,
    pub current_stall_start: Option<Instant>,
    pub total_stall_time_ms: f64,
}

static CURRENT_SESSION: Mutex<Option<SessionMetrics>> = Mutex::new(None);

#[allow(dead_code)]
pub fn get_session_metrics() -> Option<SessionMetrics> {
    CURRENT_SESSION.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[inline]
pub fn is_debug_enabled() -> bool {
    if !INITIALIZED.load(Ordering::Relaxed) {
        let val = std::env::var("MITUB_DEBUG").map(|v| v != "0").unwrap_or(false)
            || std::env::var("MITUB_PROFILE").map(|v| v != "0").unwrap_or(false);
        DEBUG_ENABLED.store(val, Ordering::Relaxed);
        INITIALIZED.store(true, Ordering::Relaxed);
    }
    DEBUG_ENABLED.load(Ordering::Relaxed)
}

#[allow(dead_code)]
pub fn set_debug_enabled(enabled: bool) {
    DEBUG_ENABLED.store(enabled, Ordering::Relaxed);
    INITIALIZED.store(true, Ordering::Relaxed);
}

#[allow(dead_code)]
#[inline]
pub fn stage_start(label: &'static str) -> Option<Instant> {
    if !is_debug_enabled() {
        return None;
    }
    eprintln!("[TELEMETRY::START] {label}");
    Some(Instant::now())
}

#[allow(dead_code)]
#[inline]
pub fn stage_end(label: &'static str, start_time: Option<Instant>) {
    if let Some(start) = start_time {
        let elapsed = start.elapsed();
        eprintln!("[TELEMETRY::END]   {label} => {:.2}ms ({:?})", elapsed.as_secs_f64() * 1000.0, elapsed);
    }
}

#[allow(dead_code)]
#[inline]
pub fn log_event(category: &'static str, message: &str) {
    if is_debug_enabled() {
        eprintln!("[TELEMETRY::{category}] {message}");
    }
}

#[allow(dead_code)]
#[inline]
pub fn log_buffering(src: &str, percent: i32, chosen_percent: u8) {
    if is_debug_enabled() {
        eprintln!("[TELEMETRY::BUFFER] from='{src}' raw={percent}% -> active={chosen_percent}%");
    }
}

#[allow(dead_code)]
#[inline]
pub fn log_seek(from_ms: u64, to_ms: u64, is_key_unit: bool) {
    if is_debug_enabled() {
        let mode = if is_key_unit { "KEY_UNIT (fast)" } else { "ACCURATE (exact)" };
        eprintln!("[TELEMETRY::SEEK] {from_ms}ms -> {to_ms}ms (mode: {mode})");
    }
}

#[allow(dead_code)]
#[inline]
pub fn log_pip(action: &'static str, surf_ptr: usize, details: &str) {
    if is_debug_enabled() {
        eprintln!("[TELEMETRY::PIP] action='{action}' surf=0x{surf_ptr:x} details='{details}'");
    }
}

// ==========================================
// Structured Session Telemetry Tracker
// ==========================================

#[allow(dead_code)]
pub fn session_start(video_id: &str) {
    if !is_debug_enabled() {
        return;
    }
    let now = Instant::now();
    let mut guard = CURRENT_SESSION.lock().unwrap_or_else(|e| e.into_inner());
    *guard = Some(SessionMetrics {
        video_id: video_id.to_string(),
        t_open: Some(now),
        ..Default::default()
    });
    eprintln!("\n=======================================================");
    eprintln!("[TELEMETRY::SESSION_START] video_id='{video_id}'");
    eprintln!("=======================================================");
}

#[allow(dead_code)]
pub fn session_details_loaded() {
    if !is_debug_enabled() {
        return;
    }
    let now = Instant::now();
    let mut guard = CURRENT_SESSION.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(ref mut session) = *guard {
        session.t_details = Some(now);
        if let Some(t0) = session.t_open {
            eprintln!("[TELEMETRY::SESSION] InnerTube API details loaded in {:.2}ms", t0.elapsed().as_secs_f64() * 1000.0);
        }
    }
}

#[allow(dead_code)]
pub fn session_pipeline_built() {
    if !is_debug_enabled() {
        return;
    }
    let now = Instant::now();
    let mut guard = CURRENT_SESSION.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(ref mut session) = *guard {
        session.t_pipeline_built = Some(now);
        if let Some(td) = session.t_details {
            eprintln!("[TELEMETRY::SESSION] GStreamer pipeline constructed in {:.2}ms", td.elapsed().as_secs_f64() * 1000.0);
        }
    }
}

#[allow(dead_code)]
pub fn session_first_frame() {
    if !is_debug_enabled() {
        return;
    }
    let now = Instant::now();
    let mut guard = CURRENT_SESSION.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(ref mut session) = *guard {
        if session.t_first_frame.is_none() {
            session.t_first_frame = Some(now);
            let total_startup = session.t_open.map(|t| t.elapsed().as_secs_f64() * 1000.0).unwrap_or(0.0);
            let preroll_time = session.t_pipeline_built.map(|t| t.elapsed().as_secs_f64() * 1000.0).unwrap_or(0.0);
            eprintln!("-------------------------------------------------------");
            eprintln!(
                "[TELEMETRY::REPORT] FIRST FRAME PRESENTED!\n  -> Total Startup Latency: {:.2}ms\n  -> Pipeline Preroll Time: {:.2}ms",
                total_startup, preroll_time
            );
            eprintln!("-------------------------------------------------------");
        }
    }
}

#[allow(dead_code)]
pub fn session_seek_start(from_ms: u64, to_ms: u64) {
    if !is_debug_enabled() {
        return;
    }
    let mut guard = CURRENT_SESSION.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(ref mut session) = *guard {
        session.current_seek_start = Some(Instant::now());
        session.seek_count += 1;
        eprintln!("[TELEMETRY::SEEK_START] #{}: {}ms -> {}ms", session.seek_count, from_ms, to_ms);
    }
}

#[allow(dead_code)]
pub fn session_seek_end() {
    if !is_debug_enabled() {
        return;
    }
    let mut guard = CURRENT_SESSION.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(ref mut session) = *guard {
        if let Some(t0) = session.current_seek_start.take() {
            let dur_ms = t0.elapsed().as_secs_f64() * 1000.0;
            session.total_seek_time_ms += dur_ms;
            eprintln!("[TELEMETRY::SEEK_DONE] Seek finished in {:.2}ms (avg: {:.2}ms)", dur_ms, session.total_seek_time_ms / (session.seek_count as f64));
        }
    }
}

#[allow(dead_code)]
pub fn session_stall(is_start: bool) {
    if !is_debug_enabled() {
        return;
    }
    let now = Instant::now();
    let mut guard = CURRENT_SESSION.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(ref mut session) = *guard {
        if is_start {
            session.stall_count += 1;
            session.current_stall_start = Some(now);
            eprintln!("[TELEMETRY::STALL_START] Playback starved / waiting for network buffer");
        } else if let Some(t0) = session.current_stall_start.take() {
            let dur_ms = t0.elapsed().as_secs_f64() * 1000.0;
            session.total_stall_time_ms += dur_ms;
            eprintln!("[TELEMETRY::STALL_END] Resumed after {:.2}ms stall", dur_ms);
        }
    }
}
