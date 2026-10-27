use std::sync::Arc;
use tokio::sync::RwLock;
use mitub::diagnostics;
use mitub::player::WaylandZeroCopyPlayer;
use std::time::Instant;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::test]
async fn test_benchmark_full_playback_lifecycle() {
    if std::env::var("CI").is_ok() || std::env::var("GITHUB_ACTIONS").is_ok() {
        println!("Skipping test_benchmark_full_playback_lifecycle in CI environment");
        return;
    }

    diagnostics::set_debug_enabled(true);

    let test_video_id = "jNQXAC9IVRw";
    diagnostics::session_start(test_video_id);

    // 1. Measure API fetch latency
    let t_api_start = Instant::now();
    let session = mitub::auth::AuthSession::auto_load();
    let client = mitub::innertube::InnerTubeClient::new(Arc::new(RwLock::new(session)));
    let details = client.get_watch_details(test_video_id).await.expect("Failed to get watch details");
    let api_elapsed = t_api_start.elapsed();
    diagnostics::session_details_loaded();
    println!("\n>>> BENCHMARK 1: InnerTube API details took: {:.2}ms", api_elapsed.as_secs_f64() * 1000.0);

    let v_stream = details.video_streams.first().expect("No video stream");
    let a_stream = details.audio_streams.first();

    // 2. Measure MPV Player initialization and stream loading
    let t_pipe_start = Instant::now();
    let player = WaylandZeroCopyPlayer::new_from_raw(0, 0).expect("Failed to initialize MPV Player");
    let init_elapsed = t_pipe_start.elapsed();
    println!(">>> BENCHMARK 2: MPV Player initialization took: {:.2}ms", init_elapsed.as_secs_f64() * 1000.0);

    // 3. Play stream with separate audio
    let t_preroll_start = Instant::now();
    player.play_stream(&v_stream.url, a_stream.map(|s| s.url.as_str())).expect("Failed to play stream");

    // Wait for first frame
    let mut waited = 0;
    while !player.is_first_frame_rendered() && waited < 100 {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        waited += 1;
    }
    let preroll_elapsed = t_preroll_start.elapsed();
    println!(">>> BENCHMARK 3: Preroll to first frame took: {:.2}ms (rendered={})",
             preroll_elapsed.as_secs_f64() * 1000.0, player.is_first_frame_rendered());

    // 4. Measure seek latency
    let t_seek_start = Instant::now();
    player.seek_ms(5000);
    let seek_elapsed = t_seek_start.elapsed();
    println!(">>> BENCHMARK 4: Fast KEY_UNIT seek took: {:.2}ms", seek_elapsed.as_secs_f64() * 1000.0);

    // Verify player frame acquisition
    let frame_opt = player.get_current_frame();
    println!(">>> BENCHMARK 5: Current video frame handle available: {}", frame_opt.is_some());

    if let Some(metrics) = diagnostics::get_session_metrics() {
        println!("\n==========================================");
        println!("=== MPV PLAYBACK METRICS SUMMARY ===");
        println!("==========================================");
        println!("  Video ID:              {}", metrics.video_id);
        println!("  Seek Count:            {}", metrics.seek_count);
        println!("  Average Seek Latency:  {:.2}ms", if metrics.seek_count > 0 { metrics.total_seek_time_ms / metrics.seek_count as f64 } else { 0.0 });
        println!("==========================================\n");
    }

    player.stop();
}
