use std::sync::Arc;
use tokio::sync::RwLock;

#[tokio::test]
async fn test_fetch_watch_details() {
    if std::env::var("CI").is_ok() || std::env::var("GITHUB_ACTIONS").is_ok() {
        println!("Skipping test_fetch_watch_details in CI environment");
        return;
    }

    let session = mitub::auth::AuthSession::auto_load();
    let client = mitub::innertube::InnerTubeClient::new(Arc::new(RwLock::new(session)));

    // Cold-start test (without calling get_home_feed first) to verify visitorData bootstrap + WebM selection
    let details = client
        .get_watch_details("U-QiUAMda7Y")
        .await
        .expect("Failed to get watch details");

    assert!(!details.video_streams.is_empty(), "Expected video streams");
    assert!(!details.audio_streams.is_empty(), "Expected audio streams");

    let first_video = &details.video_streams[0];
    let first_audio = &details.audio_streams[0];

    println!(
        "Primary video: {} ({}, itag={})",
        first_video.quality_label, first_video.mime_type, first_video.itag
    );
    println!("Chapters found: {}", details.chapters.len());
    for ch in &details.chapters {
        println!(" - [{}s] {}", ch.start_secs, ch.title);
    }
    println!(
        "Primary audio: {} (itag={})",
        first_audio.mime_type, first_audio.itag
    );

    assert_eq!(
        first_video.mime_type, "video/webm",
        "Primary video stream must be video/webm for matroskademux HTTP byte-range seeking"
    );
    assert_eq!(
        first_audio.mime_type, "audio/webm",
        "Primary audio stream must be audio/webm for matroskademux HTTP byte-range seeking"
    );
}

#[tokio::test]
async fn test_user_profile() {
    if std::env::var("CI").is_ok() || std::env::var("GITHUB_ACTIONS").is_ok() {
        println!("Skipping test_user_profile in CI environment");
        return;
    }

    let session = mitub::auth::AuthSession::auto_load();
    let client = mitub::innertube::InnerTubeClient::new(Arc::new(RwLock::new(session)));
    let (avatar, name) = client.get_user_profile().await;
    println!("User profile: avatar={:?}, name={:?}", avatar, name);
}


