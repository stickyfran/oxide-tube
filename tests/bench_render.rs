//! Render-path benchmark: plays a real stream headless and reports CPU time, frames,
//! per-frame render/readback cost and bytes copied. Run with:
//!   MITUB_BENCH_VP=0x0 cargo test --release --test bench_render -- --nocapture
//! (VP = viewport WxH the player is told it is displayed in; 0x0 = unknown).
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use mitub::player::{render_stats, WaylandZeroCopyPlayer};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn cpu_secs() -> f64 {
    let s = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
    let rest = s.rsplit(')').next().unwrap_or("");
    let f: Vec<&str> = rest.split_whitespace().collect();
    let ut: f64 = f.get(11).and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let st: f64 = f.get(12).and_then(|v| v.parse().ok()).unwrap_or(0.0);
    (ut + st) / 100.0
}

#[tokio::test]
async fn bench_render_path() {
    if std::env::var("CI").is_ok() || std::env::var("GITHUB_ACTIONS").is_ok() {
        println!("Skipping bench_render_path in CI environment");
        return;
    }

    let vp = std::env::var("MITUB_BENCH_VP").unwrap_or_else(|_| "0x0".into());
    let (vw, vh) = vp.split_once('x').map(|(a, b)| (a.parse().unwrap_or(0), b.parse().unwrap_or(0))).unwrap_or((0, 0));
    let secs: u64 = std::env::var("MITUB_BENCH_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(20);
    let vid = std::env::var("MITUB_BENCH_VIDEO").unwrap_or_else(|_| "U-QiUAMda7Y".into());

    let session = mitub::auth::AuthSession::auto_load();
    let client = mitub::innertube::InnerTubeClient::new(Arc::new(RwLock::new(session)));
    let d = match client.get_watch_details(&vid).await {
        Ok(details) => details,
        Err(e) => {
            eprintln!("Skipping bench_render_path: failed to fetch watch details: {e}");
            return;
        }
    };
    // prefer a 1080p webm/mp4 stream
    let v = match d.video_streams.iter().find(|s| s.height == 1080).or(d.video_streams.first()) {
        Some(s) => s,
        None => {
            eprintln!("Skipping bench_render_path: no video stream found");
            return;
        }
    };
    let a = d.audio_streams.first();
    println!("STREAM: {}x{} {} itag={}", v.width, v.height, v.mime_type, v.itag);

    let p = WaylandZeroCopyPlayer::new_from_raw(0, 0).expect("player");
    p.set_viewport(0, 0, vw, vh);
    p.play_stream(&v.url, a.map(|s| s.url.as_str())).expect("play");
    let mut w = 0;
    while !p.is_first_frame_rendered() && w < 200 { tokio::time::sleep(Duration::from_millis(50)).await; w += 1; }
    tokio::time::sleep(Duration::from_secs(3)).await; // warmup

    let (f0, r0, rd0, b0) = render_stats();
    let c0 = cpu_secs();
    let t0 = Instant::now();
    tokio::time::sleep(Duration::from_secs(secs)).await;
    let wall = t0.elapsed().as_secs_f64();
    let (f1, r1, rd1, b1) = render_stats();
    let cpu = cpu_secs() - c0;
    let frames = (f1 - f0).max(1) as f64;
    println!("=== BENCH vp={vp} wall={wall:.1}s ===");
    println!("CPU: {:.1}% of one core ({cpu:.2}s)", cpu / wall * 100.0);
    println!("frames: {} ({:.1} fps)", f1 - f0, frames / wall);
    println!("mpv render: {:.3} ms/frame", (r1 - r0) as f64 / frames / 1e6);
    println!("glReadPixels: {:.3} ms/frame", (rd1 - rd0) as f64 / frames / 1e6);
    println!("copied: {:.1} MB/s ({:.2} MB/frame)", (b1 - b0) as f64 / wall / 1e6, (b1 - b0) as f64 / frames / 1e6);
    println!("hwdec-current={:?} video-codec={:?}", p.get_property_string("hwdec-current"), p.get_property_string("video-codec"));
    println!("frame-drop={:?} decoder-drop={:?}", p.get_property_string("frame-drop-count"), p.get_property_string("decoder-frame-drop-count"));
    p.stop();
}
