mod auth;
mod cache_manager;
mod config;
mod innertube;
mod live_chat;
mod player;
mod power_manager;
mod ui;
mod diagnostics;

use auth::AuthSession;
use config::{AppConfig, PreferredCodec, PreferredResolution};
use innertube::{ChannelDetails, InnerTubeClient, VideoCard, WatchDetails};
use live_chat::{ChatMessage, LiveChatManager};
use player::WaylandZeroCopyPlayer;
use power_manager::{PowerManager, PowerProfile};
use ui::channel_view::view_channel_page;
use ui::home_view::view_home_grid;
use ui::icons::icon_check_circle;
use ui::osd_controls::OsdState;
use ui::theme::*;
use ui::watch_view::view_watch_page;
use ui::{view_auth_modal, view_header_bar, view_suggestions_overlay};

use iced::keyboard::key::Named;
use iced::keyboard::Key;
use iced::widget::{column, container, image, row, stack, text, Space};
use iced::window::{self, Id, Mode, Settings};
use iced::{Alignment, Element, Length, Size, Subscription, Task};
use raw_window_handle::RawWindowHandle;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

// Global Allocator with aggressive memory trimming
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewMode {
    Home,
    Subscriptions,
    History,
    Playlists,
    Watch(String),
    Channel(String),
    VideoFullscreen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipResizeEdge {
    Right,
    Bottom,
    BottomRight,
    Left,
    Top,
    TopLeft,
    TopRight,
    BottomLeft,
}

#[derive(Debug, Clone)]
pub enum Message {
    Tick,
    VideoFrameReady,
    PowerChanged(PowerProfile),
    WindowHandleAcquired(usize, usize),
    PipHandleAcquired(usize, usize),
    ClosePip,
    RestoreFromPip,
    OpenVideo(String),
    WatchDetailsLoaded(u64, Result<Arc<WatchDetails>, String>),
    OpenChannel(String),
    ChannelLoaded(Result<ChannelDetails, String>),
    SelectChannelTab(usize),
    GoHome,
    GoBack,
    OpenSubscriptions,
    SubscriptionsLoaded(Result<Vec<VideoCard>, String>),
    OpenHistory,
    HistoryLoaded(Result<Vec<VideoCard>, String>),
    OpenPlaylists,
    PlaylistsLoaded(Result<Vec<VideoCard>, String>),
    UserProfileLoaded(Option<String>, Option<String>),
    UserAvatarDownloaded(image::Handle),
    AddToQueue(VideoCard),
    RemoveFromQueue(usize),
    ClearQueue,
    ToggleAutoplay,
    SearchInputChanged(String),
    SearchSubmitted,
    SuggestionsLoaded(Vec<String>),
    SearchResultsLoaded(Result<Vec<VideoCard>, String>),
    SelectSuggestion(String),
    VideoHitboxClicked,
    PlayPauseToggle,
    Seek(u64),
    SeekRelative(i64),
    WindowMinimizedStatus(bool),
    VolumeChanged(f32),
    ToggleMute,
    ToggleQualityMenu,
    SelectQuality(u32),
    ToggleSaveDefaultQuality,
    CloseOsdMenus,
    StartPipDrag,
    StartPipEdgeResize(PipResizeEdge),
    ToggleAudioMenu,
    SelectAudioTrack(usize),
    ToggleSpeedMenu,
    ChangePlaybackRate(f64),
    ToggleCaptions,
    SelectCaptionTrack(Option<usize>),
    CaptionsLoaded(String, Result<Vec<innertube::SubtitleCue>, String>),
    ToggleTheater,
    ToggleFullscreen,
    TogglePip,
    OpenUrl(String),
    IpcUrlReceived(String),
    CopyToClipboard(String),
    CopyShareUrl,
    CopyDescription,
    CopyComment(usize),
    ToggleDescription,
    DescriptionAction(iced::widget::text_editor::Action),
    CommentAction(usize, iced::widget::text_editor::Action),
    RetryPlayback,
    ToggleChat,
    ToggleSubscribe,
    SubscribedResult(Result<bool, String>),
    LikeVideo,
    DislikeVideo,
    LikeStatusResult(Result<String, String>),
    OpenVideoInNewTab(String),
    SelectTab(usize),
    CloseTab(usize),
    RestoreClosedTab,
    ScrollTabsLeft,
    ScrollTabsRight,
    OpenEmptyTab,
    ToggleLoginModal,
    CookieInputChanged(String),
    SaveRawCookie,
    ImportFirefoxCookies,
    FirefoxCookiesImported(Result<AuthSession, String>),
    ImportChromeCookies,
    ChromeCookiesImported(Result<AuthSession, String>),
    Logout,
    SetPreferredResolution(PreferredResolution),
    SetPreferredCodec(PreferredCodec),
    SetPreferredAudioLanguage(crate::config::PreferredAudioLanguage),
    ToggleAutoPipOnUnfocus,
    ScalePip(f32),
    WatchDetailsLoadedPassive(Result<Arc<innertube::WatchDetails>, String>),
    ReloadFeed,
    HomeFeedLoaded(Result<Vec<VideoCard>, String>),
    ChatMessagesBatch(Vec<ChatMessage>),
    ThumbnailDownloaded(String, image::Handle, Option<image::Handle>),
    BlurredPosterReady(String, image::Handle),
    ThumbnailFailed(String),
    WatchScrolled(iced::widget::scrollable::Viewport),
    HomeScrolled(iced::widget::scrollable::Viewport),
    MoreFeedLoaded(Result<Vec<VideoCard>, String>),
    TabsScrolled(iced::widget::scrollable::Viewport),
    TriggerFullBuffer(String),
    FullBufferProgress(String, u8),
    FullBufferComplete(String, PathBuf, Option<PathBuf>),
    FullBufferError(String, String),
    ToggleFullBufferTabs,
    SetCacheLimitIndex(usize),
    ClearCache,
    CacheCleared(u64),
    CacheSizeRefreshed(u64),
    EventOccurred(iced::Event, Id),
    WindowIdFound(Option<Id>),
    WindowMaximized(bool),
}

#[derive(Debug, Clone)]
pub struct BufferDownloadState {
    pub percent: u8,
    pub is_complete: bool,
    pub video_path: Option<PathBuf>,
    pub audio_path: Option<PathBuf>,
}

#[allow(dead_code)]
pub struct MitubApp {
    config: AppConfig,
    auth_session: Arc<RwLock<AuthSession>>,
    innertube: Arc<InnerTubeClient>,
    buffer_downloads: HashMap<String, BufferDownloadState>,
    power_manager: Arc<PowerManager>,
    power_profile: PowerProfile,
    player: Option<Arc<WaylandZeroCopyPlayer>>,
    live_chat: Arc<LiveChatManager>,
    current_view: ViewMode,
    previous_view: ViewMode,
    navigation_history: Vec<ViewMode>,
    search_query: String,
    search_input_focused: bool,
    last_cursor_pos: Option<iced::Point>,
    suggestions: Vec<String>,
    show_suggestions: bool,
    tabs: Vec<ui::VideoTab>,
    active_tab_idx: Option<usize>,
    playback_queue: Vec<VideoCard>,
    autoplay_next: bool,
    home_videos: Vec<VideoCard>,
    cached_home_feed: Vec<VideoCard>,
    watch_details: Option<Arc<WatchDetails>>,
    channel_details: Option<ChannelDetails>,
    selected_channel_tab: usize,
    osd_state: OsdState,
    theater_mode: bool,
    is_fullscreen: bool,
    description_expanded: bool,
    chat_collapsed: bool,
    show_auth_modal: bool,
    auth_cookie_input: String,
    auth_status: String,
    user_avatar_url: Option<String>,
    user_avatar_handle: Option<image::Handle>,
    user_display_name: Option<String>,
    live_messages: Vec<ChatMessage>,
    thumbnail_cache: HashMap<String, image::Handle>,
    blurred_thumbnails: HashMap<String, image::Handle>,
    blurred_posters: HashMap<String, image::Handle>,
    tab_cached_frames: HashMap<String, image::Handle>,
    tab_details_cache: HashMap<String, Arc<WatchDetails>>,
    is_loading_more_feed: bool,
    thumbnail_order: std::collections::VecDeque<String>,
    in_flight_thumbnails: std::collections::HashSet<String>,
    active_subtitles: Vec<innertube::SubtitleCue>,
    watch_scroll_y: f32,
    saved_volume: f32,
    main_window_id: Option<Id>,
    pip_window_id: Option<Id>,
    pip_watch_details: Option<Arc<innertube::WatchDetails>>,
    pip_window_size: (f32, f32),
    last_pip_resize_edge: Option<PipResizeEdge>,
    pip_cursor_hovered: bool,
    pip_cursor_pos: Option<iced::Point>,
    main_window_handles: (usize, usize),
    seek_suppress_until: Option<std::time::Instant>,
    is_seeking_drag: bool,
    window_size: (f32, f32),
    is_maximized: bool,
    is_window_focused: bool,
    focused_window: Option<Id>,
    current_request_id: u64,
    view_transition_start: Option<std::time::Instant>,
    view_transition_alpha: f32,
    fullscreen_anim: Option<(std::time::Instant, bool, (f32, f32, f32, f32))>,
    is_page_loading: bool,
    live_chat_poll_tick: u32,
    playing_video_id: Option<String>,
    video_started_at: Option<std::time::Instant>,
    recently_autoplayed: Vec<String>,
    description_content: iced::widget::text_editor::Content,
    last_snapshot_frame_ids: HashMap<String, u64>,
    last_progress_pos_sec: u64,
    last_session_save_secs: u64,
    toast_message: Option<(String, std::time::Instant)>,
    tab_opened_anim: Option<(usize, std::time::Instant)>,
    tab_shake_anim: Option<(usize, std::time::Instant)>,
    current_cache_size_bytes: u64,
    closed_tabs_history: std::collections::VecDeque<ui::VideoTab>,
    tabs_scroll_x: f32,
    tabs_max_scroll_x: f32,
    last_memory_trim: std::time::Instant,
    inactive_tab_osd_state: OsdState,
    auto_pip_triggered_by_unfocus: bool,
    is_window_minimized: bool,
}

static IPC_RX: std::sync::OnceLock<tokio::sync::Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<String>>>> = std::sync::OnceLock::new();

fn get_ipc_port() -> u16 {
    use std::hash::{Hash, Hasher};
    let username = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "default".to_string());
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    username.hash(&mut hasher);
    let hash = hasher.finish();
    45000 + (hash % 10000) as u16
}

fn try_send_to_existing_instance(arg: &str) -> bool {
    use std::io::Write;
    let port = get_ipc_port();
    if let Ok(mut stream) = std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        std::time::Duration::from_millis(300),
    ) {
        let msg = format!("{}\n", arg.trim());
        if stream.write_all(msg.as_bytes()).is_ok() {
            let _ = stream.flush();
            return true;
        }
    }
    false
}

fn ipc_url_stream() -> impl iced::futures::Stream<Item = Message> {
    iced::futures::stream::unfold((), |_| async {
        let mutex = IPC_RX.get_or_init(|| tokio::sync::Mutex::new(None));
        let mut lock = mutex.lock().await;
        if let Some(rx) = lock.as_mut() {
            if let Some(url) = rx.recv().await {
                return Some((Message::IpcUrlReceived(url), ()));
            }
        }
        None
    })
}

fn frame_ready_stream() -> impl iced::futures::Stream<Item = Message> {
    iced::futures::stream::unfold((), |_| async {
        crate::player::FRAME_NOTIFY.notified().await;
        Some((Message::VideoFrameReady, ()))
    })
}

fn ensure_kwin_pip_rule(saved_pos: Option<(f32, f32)>, _saved_size: Option<(f32, f32)>) {
    let Some(home) = dirs::home_dir() else { return };
    let path = home.join(".config/kwinrulesrc");
    let content = fs::read_to_string(&path).unwrap_or_default();

    let rule_section = "[mitub-pip]";
    let pos_line = match saved_pos {
        Some((x, y)) => format!("position={},{}\npositionrule=3\n", x.round() as i32, y.round() as i32),
        None => "positionrule=3\n".to_string(),
    };
    let size_line = "";

    let rule_body = format!("\
[mitub-pip]
above=true
aboverule=3
acceptfocus=true
acceptfocusrule=3
description=Oxide tube PiP
desktops=
desktopsrule=3
focusstealing=0
focusstealingrule=3
{pos_line}{size_line}title=PiP
titlematch=2
types=1
wmclass=mitub
wmclassmatch=1");

    let mut new_content = String::new();
    let mut general_found = false;
    let mut order_updated = false;

    if content.contains(rule_section) {
        let mut in_pip_section = false;
        let mut pip_written = false;
        for line in content.lines() {
            let t = line.trim();
            if t.starts_with('[') {
                if in_pip_section {
                    in_pip_section = false;
                }
                if t.eq_ignore_ascii_case("[mitub-pip]") {
                    in_pip_section = true;
                    new_content.push_str(&rule_body);
                    new_content.push('\n');
                    pip_written = true;
                    continue;
                }
            }
            if !in_pip_section {
                new_content.push_str(line);
                new_content.push('\n');
            }
        }
        if !pip_written {
            new_content.push_str(&rule_body);
            new_content.push('\n');
        }
    } else {
        for line in content.lines() {
            if line.trim() == "[General]" {
                general_found = true;
                new_content.push_str(line);
                new_content.push('\n');
                continue;
            }
            if general_found && (line.starts_with("Order=") || line.starts_with("order=")) && !order_updated {
                let (_, order_val) = line.split_once('=').unwrap_or(("", ""));
                let mut items: Vec<&str> = order_val.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
                if !items.contains(&"mitub-pip") {
                    items.insert(0, "mitub-pip");
                }
                new_content.push_str(&format!("Order={}\n", items.join(",")));
                order_updated = true;
                continue;
            }
            new_content.push_str(line);
            new_content.push('\n');
        }

        if !general_found {
            new_content.push_str("\n[General]\nOrder=mitub-pip\n");
        } else if !order_updated {
            new_content.push_str("Order=mitub-pip\n");
        }

        new_content.push('\n');
        new_content.push_str(&rule_body);
        new_content.push('\n');
    }

    if fs::write(&path, new_content).is_ok() {
        let qdbus_bin = if std::path::Path::new("/usr/bin/qdbus6").exists() {
            "/usr/bin/qdbus6"
        } else {
            "qdbus"
        };
        let _ = std::process::Command::new(qdbus_bin)
            .args(["org.kde.KWin", "/KWin", "org.kde.KWin.reconfigure"])
            .output();
    }
}

fn get_kwin_pip_position() -> Option<(f32, f32)> {
    let home = dirs::home_dir()?;
    let content = fs::read_to_string(home.join(".config/kwinrulesrc")).ok()?;
    let mut in_mitub_pip = false;
    for line in content.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_mitub_pip = t.eq_ignore_ascii_case("[mitub-pip]");
        } else if in_mitub_pip && t.starts_with("position=") {
            let (_, val) = t.split_once('=')?;
            let (x_s, y_s) = val.split_once(',')?;
            let x: f32 = x_s.trim().parse().ok()?;
            let y: f32 = y_s.trim().parse().ok()?;
            return Some((x, y));
        }
    }
    None
}

fn query_kwin_pip_geometry() -> Option<((f32, f32), (f32, f32))> {
    let js = r#"
const clients = workspace.windowList();
for (let i = 0; i < clients.length; i++) {
    const w = clients[i];
    const isMitub = (w.resourceClass === "mitub" || (w.caption && (w.caption.indexOf("Mitub") !== -1 || w.caption.indexOf("Oxide") !== -1)));
    const isPip = (w.caption && (w.caption.indexOf("PiP") !== -1))
        || (isMitub && w.frameGeometry.width < 1000 && w.frameGeometry.height < 700);
    if (isPip) {
        console.info("MITUB_PIP_POS:" + Math.round(w.frameGeometry.x) + "," + Math.round(w.frameGeometry.y) + "," + Math.round(w.frameGeometry.width) + "," + Math.round(w.frameGeometry.height));
        break;
    }
}
"#;
    let tmp_path = "/tmp/mitub_kwin_query_pip.js";
    if std::fs::write(tmp_path, js).is_ok() {
        let qdbus_bin = if std::path::Path::new("/usr/bin/qdbus6").exists() {
            "/usr/bin/qdbus6"
        } else {
            "qdbus"
        };
        let _ = std::process::Command::new(qdbus_bin)
            .args(["org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting.unloadScript", "mitub_query_pip"])
            .output();
        if let Ok(out) = std::process::Command::new(qdbus_bin)
            .args(["org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting.loadScript", tmp_path, "mitub_query_pip"])
            .output()
        {
            let id_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !id_str.is_empty() {
                let script_path = format!("/Scripting/Script{id_str}");
                let _ = std::process::Command::new(qdbus_bin)
                    .args(["org.kde.KWin", &script_path, "org.kde.kwin.Script.run"])
                    .output();
                let _ = std::process::Command::new(qdbus_bin)
                    .args(["org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting.unloadScript", "mitub_query_pip"])
                    .output();

                if let Ok(journal) = std::process::Command::new("journalctl")
                    .args(["--user", "-n", "20", "--no-pager"])
                    .output()
                {
                    let log_str = String::from_utf8_lossy(&journal.stdout);
                    for line in log_str.lines().rev() {
                        if let Some(pos_idx) = line.find("MITUB_PIP_POS:") {
                            let coords = &line[pos_idx + "MITUB_PIP_POS:".len()..];
                            let parts: Vec<&str> = coords.trim().split(',').collect();
                            if parts.len() >= 4 {
                                if let (Ok(x), Ok(y), Ok(w), Ok(h)) = (
                                    parts[0].trim().parse::<f32>(),
                                    parts[1].trim().parse::<f32>(),
                                    parts[2].trim().parse::<f32>(),
                                    parts[3].trim().parse::<f32>(),
                                ) {
                                    return Some(((x, y), (w, h)));
                                }
                            } else if parts.len() >= 2 {
                                if let (Ok(x), Ok(y)) = (parts[0].trim().parse::<f32>(), parts[1].trim().parse::<f32>()) {
                                    return Some(((x, y), (480.0, 270.0)));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

fn save_current_kwin_pip_position(config: &mut crate::config::AppConfig) {
    if let Some((pos, size)) = query_kwin_pip_geometry() {
        config.pip_position = Some(pos);
        if size.0 >= 240.0 && size.1 >= 135.0 {
            config.pip_size = Some(size);
        }
        config.save_async();
        ensure_kwin_pip_rule(Some(pos), Some(size));
    }
}

fn activate_kwin_main_window() {
    std::thread::Builder::new()
        .name("kwin-activate-main".into())
        .spawn(|| {
            for delay in [0u64, 150u64] {
                if delay > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(delay));
                }
                let js = r#"
const clients = workspace.windowList();
for (let i = 0; i < clients.length; i++) {
    const w = clients[i];
    const isMitub = (w.resourceClass === "mitub" || (w.caption && (w.caption.indexOf("Mitub") !== -1 || w.caption.indexOf("Oxide") !== -1)));
    const isPip = w.caption && w.caption.indexOf("PiP") !== -1;
    if (isMitub && !isPip) {
        try { w.minimized = false; } catch(e) {}
        try { workspace.activeWindow = w; } catch(e) {}
        try { if (workspace.raiseWindow) workspace.raiseWindow(w); } catch(e) {}
    }
}
"#;
                let tmp_path = "/tmp/mitub_kwin_activate.js";
                if std::fs::write(tmp_path, js).is_ok() {
                    let qdbus_bin = if std::path::Path::new("/usr/bin/qdbus6").exists() {
                        "/usr/bin/qdbus6"
                    } else {
                        "qdbus"
                    };
                    let _ = std::process::Command::new(qdbus_bin)
                        .args(["org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting.unloadScript", "mitub_activate_helper"])
                        .output();
                    if let Ok(out) = std::process::Command::new(qdbus_bin)
                        .args(["org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting.loadScript", tmp_path, "mitub_activate_helper"])
                        .output()
                    {
                        let id_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
                        if !id_str.is_empty() {
                            let script_path = format!("/Scripting/Script{id_str}");
                            let _ = std::process::Command::new(qdbus_bin)
                                .args(["org.kde.KWin", &script_path, "org.kde.kwin.Script.run"])
                                .output();
                            let _ = std::process::Command::new(qdbus_bin)
                                .args(["org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting.unloadScript", "mitub_activate_helper"])
                                .output();
                        }
                    }
                }
            }
            // Fallback for X11 environments if wmctrl exists
            if std::path::Path::new("/usr/bin/wmctrl").exists() {
                let _ = std::process::Command::new("/usr/bin/wmctrl")
                    .args(["-x", "-a", "mitub"])
                    .output();
            }
        })
        .ok();
}

fn enforce_kwin_pip_keep_above(saved_pos: Option<(f32, f32)>, saved_size: Option<(f32, f32)>) {
    std::thread::Builder::new()
        .name("kwin-pip-top".into())
        .spawn(move || {
            ensure_kwin_pip_rule(saved_pos, saved_size);

            let (target_x_str, target_y_str) = match saved_pos {
                Some((x, y)) => (format!("{x}"), format!("{y}")),
                None => ("null".to_string(), "null".to_string()),
            };
            let (target_w_str, target_h_str) = match saved_size {
                Some((w, h)) => (format!("{w}"), format!("{h}")),
                None => ("null".to_string(), "null".to_string()),
            };

            // Wait briefly for the Wayland surface to map in KWin, then enforce keepAbove & onAllDesktops & noBorder & geometry
            for (i, delay_ms) in [60u64, 220u64, 450u64].iter().enumerate() {
                std::thread::sleep(std::time::Duration::from_millis(*delay_ms));
                let apply_position = i == 0;
                let js = format!(r#"
const clients = workspace.windowList();
for (let i = 0; i < clients.length; i++) {{
    const w = clients[i];
    const isMitub = (w.resourceClass === "mitub" || (w.caption && (w.caption.indexOf("Mitub") !== -1 || w.caption.indexOf("Oxide") !== -1)));
    const isPip = (w.caption && (w.caption.indexOf("PiP") !== -1))
        || (isMitub && w.frameGeometry.width < 1000 && w.frameGeometry.height < 700);
    if (isPip) {{
        try {{ w.keepAbove = true; }} catch(e) {{}}
        try {{ w.onAllDesktops = true; }} catch(e) {{}}
        if ({apply_position}) {{
            try {{
                const ca = workspace.clientArea(0, w);
                let targetW = {target_w_str};
                let targetH = {target_h_str};
                if (targetW === null || targetH === null) {{
                    targetW = w.frameGeometry.width;
                    targetH = w.frameGeometry.height;
                }}
                let targetX = {target_x_str};
                let targetY = {target_y_str};
                if (targetX === null || targetY === null) {{
                    targetX = Math.round(ca.x + ca.width - targetW - 24);
                    targetY = Math.round(ca.y + ca.height - targetH - 24);
                }}
                w.frameGeometry = {{
                    x: targetX,
                    y: targetY,
                    width: targetW,
                    height: targetH
                }};
            }} catch(e) {{}}
        }}
    }}
}}
"#);
                let tmp_path = "/tmp/mitub_kwin_pip.js";
                if std::fs::write(tmp_path, js).is_ok() {
                    let qdbus_bin = if std::path::Path::new("/usr/bin/qdbus6").exists() {
                        "/usr/bin/qdbus6"
                    } else {
                        "qdbus"
                    };
                    let _ = std::process::Command::new(qdbus_bin)
                        .args(["org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting.unloadScript", "mitub_pip_helper"])
                        .output();
                    if let Ok(out) = std::process::Command::new(qdbus_bin)
                        .args(["org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting.loadScript", tmp_path, "mitub_pip_helper"])
                        .output()
                    {
                        let id_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
                        if !id_str.is_empty() {
                            let script_path = format!("/Scripting/Script{id_str}");
                            let _ = std::process::Command::new(qdbus_bin)
                                .args(["org.kde.KWin", &script_path, "org.kde.kwin.Script.run"])
                                .output();
                            let _ = std::process::Command::new(qdbus_bin)
                                .args(["org.kde.KWin", "/Scripting", "org.kde.kwin.Scripting.unloadScript", "mitub_pip_helper"])
                                .output();
                        }
                    }
                }
            }
        })
        .ok();
}

fn is_hw_codec_supported(_codec: PreferredCodec) -> bool {
    // Under MPV, hardware decoding (VA-API / auto-safe) natively supports AV1, VP9, and H.264
    true
}

fn select_best_stream<'a>(streams: &'a [innertube::StreamQuality], config: &AppConfig) -> Option<&'a innertube::StreamQuality> {
    if streams.is_empty() {
        return None;
    }

    let max_target_height = config.preferred_resolution.max_height();

    // If preferred codec is AV1 but the system GPU lacks hardware AV1 decode,
    // automatically prefer VP9 to prevent stutter and software decoding OOM
    let effective_preferred = if config.preferred_codec == PreferredCodec::Av1 && !is_hw_codec_supported(PreferredCodec::Av1) {
        PreferredCodec::Vp9
    } else {
        config.preferred_codec
    };

    let matches_codec = |s: &innertube::StreamQuality, codec: PreferredCodec| -> bool {
        match codec {
            PreferredCodec::Av1 => s.codecs.starts_with("av01"),
            PreferredCodec::Vp9 => s.codecs.starts_with("vp9") || s.codecs.starts_with("vp09"),
            PreferredCodec::H264 => s.codecs.starts_with("avc1"),
        }
    };

    // 1. Preferred codec <= target resolution (highest resolution first)
    if let Some(stream) = streams
        .iter()
        .filter(|s| matches_codec(s, effective_preferred) && s.height <= max_target_height)
        .max_by_key(|s| (s.height, s.fps, s.bitrate))
    {
        return Some(stream);
    }

    // 2. Preferred codec any resolution (if only higher resolution available)
    if let Some(stream) = streams
        .iter()
        .filter(|s| matches_codec(s, effective_preferred))
        .max_by_key(|s| (s.height, s.fps, s.bitrate))
    {
        return Some(stream);
    }

    // 3. Fallback to alternative codecs <= target resolution
    let fallback_codecs = match effective_preferred {
        PreferredCodec::Av1 => [PreferredCodec::Vp9, PreferredCodec::H264],
        PreferredCodec::Vp9 => [PreferredCodec::H264, PreferredCodec::Av1],
        PreferredCodec::H264 => [PreferredCodec::Vp9, PreferredCodec::Av1],
    };

    for fb in fallback_codecs {
        if let Some(stream) = streams
            .iter()
            .filter(|s| matches_codec(s, fb) && s.height <= max_target_height)
            .max_by_key(|s| (s.height, s.fps, s.bitrate))
        {
            return Some(stream);
        }
    }

    // 4. Fallback to any codec <= target resolution
    if let Some(stream) = streams
        .iter()
        .filter(|s| s.height <= max_target_height)
        .max_by_key(|s| (s.height, s.fps, s.bitrate))
    {
        return Some(stream);
    }

    // 5. Ultimate fallback: best stream available
    streams.iter().max_by_key(|s| (s.height, s.fps, s.bitrate))
}

fn select_best_audio_stream<'a>(
    streams: &'a [innertube::StreamQuality],
    pref: crate::config::PreferredAudioLanguage,
) -> Option<&'a innertube::StreamQuality> {
    if streams.is_empty() {
        return None;
    }

    match pref {
        crate::config::PreferredAudioLanguage::Original => {
            streams
                .iter()
                .find(|s| s.is_default_audio || s.quality_label.to_lowercase().contains("original") || s.quality_label.to_lowercase().contains("default"))
                .or_else(|| streams.first())
        }
        crate::config::PreferredAudioLanguage::Spanish => {
            streams
                .iter()
                .find(|s| {
                    let id = s.audio_track_id.to_lowercase();
                    let ql = s.quality_label.to_lowercase();
                    id.starts_with("es") || ql.contains("español") || ql.contains("spanish")
                })
                .or_else(|| {
                    streams
                        .iter()
                        .find(|s| s.is_default_audio || s.quality_label.to_lowercase().contains("original") || s.quality_label.to_lowercase().contains("default"))
                })
                .or_else(|| streams.first())
        }
        crate::config::PreferredAudioLanguage::English => {
            streams
                .iter()
                .find(|s| {
                    let id = s.audio_track_id.to_lowercase();
                    let ql = s.quality_label.to_lowercase();
                    id.starts_with("en") || ql.contains("english") || ql.contains("inglés") || ql.contains("ingles")
                })
                .or_else(|| {
                    streams
                        .iter()
                        .find(|s| s.is_default_audio || s.quality_label.to_lowercase().contains("original") || s.quality_label.to_lowercase().contains("default"))
                })
                .or_else(|| streams.first())
        }
    }
}

fn extract_youtube_video_id(input: &str) -> Option<String> {
    let s = input.trim();
    if s.contains("youtube.com/watch") {
        if let Some(pos) = s.find("v=") {
            let rest = &s[pos + 2..];
            let id: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
            if id.len() >= 8 {
                return Some(id);
            }
        }
    } else if s.contains("youtu.be/") {
        if let Some(pos) = s.find("youtu.be/") {
            let rest = &s[pos + 9..];
            let id: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
            if id.len() >= 8 {
                return Some(id);
            }
        }
    } else if s.contains("youtube.com/shorts/") {
        if let Some(pos) = s.find("shorts/") {
            let rest = &s[pos + 7..];
            let id: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
            if id.len() >= 8 {
                return Some(id);
            }
        }
    }
    None
}

fn start_full_buffer_task(
    innertube: Arc<InnerTubeClient>,
    video_id: String,
    known_streams: Option<(String, Option<String>)>,
) -> Task<Message> {
    Task::run(
        iced::stream::channel(100, move |mut output: iced::futures::channel::mpsc::Sender<Message>| async move {
            use iced::futures::SinkExt;

            let cache_dir = crate::cache_manager::video_cache_dir();
            let _ = tokio::fs::create_dir_all(&cache_dir).await;

            let v_dest = cache_dir.join(format!("{video_id}_video.mp4"));
            let a_dest = cache_dir.join(format!("{video_id}_audio.mp4"));
            let v_part = cache_dir.join(format!("{video_id}_video.part"));
            let a_part = cache_dir.join(format!("{video_id}_audio.part"));

            // If already complete on disk and non-empty:
            if v_dest.exists() && std::fs::metadata(&v_dest).map(|m| m.len() > 1024).unwrap_or(false) {
                let has_audio = a_dest.exists() && std::fs::metadata(&a_dest).map(|m| m.len() > 1024).unwrap_or(false);
                let _ = output.send(Message::FullBufferProgress(video_id.clone(), 100)).await;
                let _ = output.send(Message::FullBufferComplete(
                    video_id,
                    v_dest,
                    if has_audio { Some(a_dest) } else { None },
                )).await;
                return;
            }

            // Get streams
            let (v_url, a_url) = if let Some(streams) = known_streams {
                streams
            } else {
                match innertube.get_watch_details(&video_id).await {
                    Ok(details) => {
                        let v_stream = details.video_streams.iter().find(|s| s.height <= 1080).or_else(|| details.video_streams.first());
                        let a_stream = select_best_audio_stream(&details.audio_streams, crate::config::PreferredAudioLanguage::Original);
                        match v_stream {
                            Some(v) => (v.url.clone(), a_stream.map(|a| a.url.clone())),
                            None => {
                                let _ = output.send(Message::FullBufferError(video_id, "No se encontraron streams disponibles".to_string())).await;
                                return;
                            }
                        }
                    }
                    Err(e) => {
                        let _ = output.send(Message::FullBufferError(video_id, e.to_string())).await;
                        return;
                    }
                }
            };

            const UA: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:132.0) Gecko/20100101 Firefox/132.0";
            let client = reqwest::Client::builder()
                .user_agent(UA)
                .timeout(Duration::from_secs(180))
                .tcp_nodelay(true)
                .build()
                .unwrap_or_default();

            let _ = output.send(Message::FullBufferProgress(video_id.clone(), 3)).await;

            let has_separate_audio = a_url.is_some();
            let v_range = if has_separate_audio { (3u8, 75u8) } else { (3u8, 98u8) };

            let dl_video = async {
                use iced::futures::StreamExt;
                use tokio::io::AsyncWriteExt;

                let resp = client
                    .get(&v_url)
                    .header(reqwest::header::REFERER, "https://www.youtube.com/")
                    .header(reqwest::header::ORIGIN, "https://www.youtube.com")
                    .send()
                    .await
                    .map_err(|e| e.to_string())?;

                if !resp.status().is_success() {
                    return Err(format!("HTTP {}", resp.status()));
                }
                let total_size = resp.content_length().unwrap_or(0);
                let mut file = tokio::fs::File::create(&v_part).await.map_err(|e| e.to_string())?;
                let mut stream = resp.bytes_stream();
                let mut downloaded: u64 = 0;
                let mut last_pct = v_range.0;

                while let Some(chunk_res) = stream.next().await {
                    let chunk = chunk_res.map_err(|e| e.to_string())?;
                    file.write_all(&chunk).await.map_err(|e| e.to_string())?;
                    downloaded += chunk.len() as u64;

                    if total_size > 0 {
                        let p = v_range.0 as f32
                            + (downloaded as f32 / total_size as f32)
                                * (v_range.1 - v_range.0) as f32;
                        let p_u8 = p.clamp(v_range.0 as f32, v_range.1 as f32) as u8;
                        if p_u8 > last_pct {
                            last_pct = p_u8;
                            let _ = output.send(Message::FullBufferProgress(video_id.clone(), p_u8)).await;
                        }
                    }
                }
                file.flush().await.map_err(|e| e.to_string())?;
                tokio::fs::rename(&v_part, &v_dest).await.map_err(|e| e.to_string())?;
                Ok::<(), String>(())
            };

            if let Err(e) = dl_video.await {
                eprintln!("[Mitub Full Buffer Download Error for {video_id}]: {e}");
                let _ = tokio::fs::remove_file(&v_part).await;
                let _ = output.send(Message::FullBufferError(video_id, format!("Error descargando video: {e}"))).await;
                return;
            }

            let final_a_dest = if let Some(audio_url) = a_url {
                let _ = output.send(Message::FullBufferProgress(video_id.clone(), 78)).await;

                let a_range = (78u8, 98u8);
                let dl_audio = async {
                    use iced::futures::StreamExt;
                    use tokio::io::AsyncWriteExt;

                    let resp = client
                        .get(&audio_url)
                        .header(reqwest::header::REFERER, "https://www.youtube.com/")
                        .header(reqwest::header::ORIGIN, "https://www.youtube.com")
                        .send()
                        .await
                        .map_err(|e| e.to_string())?;

                    if !resp.status().is_success() {
                        return Err(format!("HTTP {}", resp.status()));
                    }
                    let total_size = resp.content_length().unwrap_or(0);
                    let mut file = tokio::fs::File::create(&a_part).await.map_err(|e| e.to_string())?;
                    let mut stream = resp.bytes_stream();
                    let mut downloaded: u64 = 0;
                    let mut last_pct = a_range.0;

                    while let Some(chunk_res) = stream.next().await {
                        let chunk = chunk_res.map_err(|e| e.to_string())?;
                        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
                        downloaded += chunk.len() as u64;

                        if total_size > 0 {
                            let p = a_range.0 as f32
                                + (downloaded as f32 / total_size as f32)
                                    * (a_range.1 - a_range.0) as f32;
                            let p_u8 = p.clamp(a_range.0 as f32, a_range.1 as f32) as u8;
                            if p_u8 > last_pct {
                                last_pct = p_u8;
                                let _ = output.send(Message::FullBufferProgress(video_id.clone(), p_u8)).await;
                            }
                        }
                    }
                    file.flush().await.map_err(|e| e.to_string())?;
                    tokio::fs::rename(&a_part, &a_dest).await.map_err(|e| e.to_string())?;
                    Ok::<(), String>(())
                };

                if let Err(e) = dl_audio.await {
                    eprintln!("[Mitub Full Buffer Audio Download Error for {video_id}]: {e}");
                    let _ = tokio::fs::remove_file(&a_part).await;
                    let _ = output.send(Message::FullBufferError(video_id, format!("Error descargando audio: {e}"))).await;
                    return;
                }
                Some(a_dest)
            } else {
                None
            };

            let _ = output.send(Message::FullBufferProgress(video_id.clone(), 100)).await;
            let _ = output.send(Message::FullBufferComplete(video_id, v_dest, final_a_dest)).await;
        }),
        std::convert::identity,
    )
}

impl MitubApp {
    pub fn new() -> (Self, Task<Message>) {
        // Auto-load cookies from disk cache or Firefox PSD profile
        let initial_session = AuthSession::auto_load();
        let is_authed = initial_session.is_authenticated();

        let auth_session = Arc::new(RwLock::new(initial_session));
        let innertube = Arc::new(InnerTubeClient::new(auth_session.clone()));
        let power_manager = Arc::new(PowerManager::new());
        let live_chat = Arc::new(LiveChatManager::new(auth_session.clone()));

        power_manager.start_listener();

        let ((saved_w, saved_h), saved_maximized) = load_window_state();
        let config = AppConfig::load();
        let saved_tabs = config.saved_tabs.clone();
        let saved_active_idx = config.active_tab_idx;
        let saved_pip_size = config.pip_size;

        let window_settings = Settings {
            size: Size::new(saved_w, saved_h),
            min_size: Some(Size::new(800.0, 600.0)),
            transparent: true,
            decorations: true,
            icon: load_app_icon(),
            ..Settings::default()
        };

        let (main_id, open_task) = window::open(window_settings);

        let mut osd_state = OsdState::default();
        osd_state.preferred_codec = config.preferred_codec;
        osd_state.preferred_resolution = config.preferred_resolution;
        if let Some(vol) = config.saved_volume {
            osd_state.volume = vol;
        }
        if let Some(true) = config.is_muted {
            osd_state.volume = 0.0;
        }

        let cached_avatar_url = config.cached_avatar_url.clone();
        let cached_user_name = config.cached_user_name.clone();

        let player = match WaylandZeroCopyPlayer::new_from_raw(0, 0) {
            Ok(p) => {
                println!("[MPV] Initialized native MPV player engine successfully");
                p.set_volume(osd_state.volume as f64);
                p.set_power_profile(PowerProfile::Ac);
                Some(Arc::new(p))
            }
            Err(e) => {
                eprintln!("[MPV] Error initializing player: {e}");
                None
            }
        };

        let app_autoplay = config.autoplay_next;
        let app_theater = config.theater_mode;
        let app_volume = config.saved_volume.unwrap_or(1.0);

        let mut app = Self {
            config,
            auth_session,
            innertube: innertube.clone(),
            power_manager,
            power_profile: PowerProfile::Ac,
            player,
            live_chat,
            current_view: ViewMode::Home,
            previous_view: ViewMode::Home,
            navigation_history: Vec::new(),
            search_query: String::new(),
            search_input_focused: false,
            last_cursor_pos: None,
            suggestions: Vec::new(),
            show_suggestions: false,
            buffer_downloads: HashMap::new(),
            tabs: saved_tabs.clone(),
            active_tab_idx: saved_active_idx,
            playback_queue: Vec::new(),
            autoplay_next: app_autoplay,
            home_videos: Vec::new(),
            cached_home_feed: Vec::new(),
            watch_details: None,
            channel_details: None,
            selected_channel_tab: 0,
            osd_state,
            theater_mode: app_theater,
            is_fullscreen: false,
            description_expanded: false,
            chat_collapsed: false,
            show_auth_modal: false,
            auth_cookie_input: String::new(),
            auth_status: if is_authed {
                "[OK] Sesion activa de YouTube".to_string()
            } else {
                "Modo invitado".to_string()
            },
            user_avatar_url: cached_avatar_url,
            user_avatar_handle: None,
            user_display_name: cached_user_name,
            live_messages: Vec::new(),
            thumbnail_cache: HashMap::new(),
            blurred_thumbnails: HashMap::new(),
            blurred_posters: HashMap::new(),
            tab_cached_frames: HashMap::new(),
            tab_details_cache: HashMap::new(),
            is_loading_more_feed: false,
            thumbnail_order: std::collections::VecDeque::new(),
            in_flight_thumbnails: std::collections::HashSet::new(),
            active_subtitles: Vec::new(),
            watch_scroll_y: 0.0,
            saved_volume: app_volume,
            main_window_id: Some(main_id),
            pip_window_id: None,
            pip_watch_details: None,
            pip_window_size: saved_pip_size.unwrap_or((480.0, 270.0)),
            last_pip_resize_edge: None,
            pip_cursor_hovered: true,
            pip_cursor_pos: None,
            main_window_handles: (0, 0),
            seek_suppress_until: None,
            is_seeking_drag: false,
            window_size: (saved_w, saved_h),
            is_maximized: saved_maximized,
            is_window_focused: true,
            focused_window: Some(main_id),
            current_request_id: 0,
            view_transition_start: Some(std::time::Instant::now()),
            view_transition_alpha: 1.0,
            fullscreen_anim: None,
            is_page_loading: true,
            live_chat_poll_tick: 0,
            playing_video_id: None,
            video_started_at: None,
            recently_autoplayed: Vec::new(),
            description_content: iced::widget::text_editor::Content::new(),
            last_snapshot_frame_ids: HashMap::new(),
            last_progress_pos_sec: 0,
            last_session_save_secs: 0,
            toast_message: None,
            tab_opened_anim: None,
            tab_shake_anim: None,
            current_cache_size_bytes: crate::cache_manager::get_cache_size_bytes(),
            closed_tabs_history: std::collections::VecDeque::with_capacity(5),
            tabs_scroll_x: 0.0,
            tabs_max_scroll_x: 0.0,
            last_memory_trim: std::time::Instant::now(),
            inactive_tab_osd_state: OsdState::default(),
            auto_pip_triggered_by_unfocus: false,
            is_window_minimized: false,
        };

        // Initial tasks: fetch home feed & bind Wayland window pointers
        let it = innertube.clone();
        let home_task = Task::perform(
            async move { it.get_home_feed().await.map_err(|e| e.to_string()) },
            Message::HomeFeedLoaded,
        );

        let handle_task = window::run_with_handle(main_id, move |handle| {
            let surf_addr = match handle.as_raw() {
                RawWindowHandle::Wayland(h) => h.surface.as_ptr() as usize,
                _ => 0usize,
            };
            let disp_addr = if surf_addr != 0 {
                #[link(name = "wayland-client")]
                extern "C" {
                    fn wl_proxy_get_display(proxy: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
                }
                unsafe {
                    wl_proxy_get_display(surf_addr as *mut std::ffi::c_void) as usize
                }
            } else {
                0usize
            };
            (disp_addr, surf_addr)
        })
        .map(|(disp, surf)| Message::WindowHandleAcquired(disp, surf));

        let open_task = open_task.map(|id| Message::WindowIdFound(Some(id)));

        let it_profile = innertube.clone();
        let profile_task = Task::perform(
            async move { it_profile.get_user_profile().await },
            |(url, name)| Message::UserProfileLoaded(url, name),
        );

        let mut initial_tasks = vec![open_task, home_task, handle_task, profile_task];
        if let Some(cached_avatar) = &app.config.cached_avatar_url {
            let it = innertube.clone();
            let url = cached_avatar.clone();
            let avatar_task = Task::perform(
                async move {
                    if let Some(bytes) = it.fetch_thumbnail_bytes(&url).await {
                        tokio::task::spawn_blocking(move || {
                            if let Ok(img) = image_crate::load_from_memory(&bytes) {
                                let resized = img.thumbnail(96, 96);
                                let rgba = resized.to_rgba8();
                                let (w, h) = rgba.dimensions();
                                Some(image::Handle::from_rgba(w, h, rgba.into_raw()))
                            } else {
                                None
                            }
                        })
                        .await
                        .ok()
                        .flatten()
                    } else {
                        None
                    }
                },
                |opt_handle| {
                    if let Some(handle) = opt_handle {
                        Message::UserAvatarDownloaded(handle)
                    } else {
                        Message::Tick
                    }
                },
            );
            initial_tasks.push(avatar_task);
        }
        if saved_maximized {
            initial_tasks.push(window::maximize(main_id, true));
        }

        if !saved_tabs.is_empty() {
            let active_idx = saved_active_idx.unwrap_or(0).min(saved_tabs.len().saturating_sub(1));
            app.active_tab_idx = Some(active_idx);
            let active_tab = &saved_tabs[active_idx];
            let vid = active_tab.video_id.clone();
            let saved_ms = active_tab.saved_pos_ms;

            app.current_view = ViewMode::Watch(vid.clone());
            app.osd_state.current_pos_secs = saved_ms / 1000;
            app.watch_details = Some(Arc::new(WatchDetails {
                video_id: vid.clone(),
                title: active_tab.title.clone(),
                channel_name: active_tab.channel_name.clone(),
                channel_id: String::new(),
                channel_avatar: String::new(),
                subscriber_count: String::new(),
                likes_str: String::new(),
                dislikes_str: String::new(),
                description: String::new(),
                published_date: String::new(),
                views_str: String::new(),
                is_live: false,
                is_subscribed: false,
                like_status: "INDIFFERENT".to_string(),
                live_chat_continuation: None,
                recommended_videos: Vec::new(),
                comments: Vec::new(),
                video_streams: Vec::new(),
                audio_streams: Vec::new(),
                caption_tracks: Vec::new(),
                chapters: Vec::new(),
                parsed_desc_urls: Vec::new(),
                parsed_desc_timestamps: Vec::new(),
            }));

            let it_watch = innertube.clone();
            let vid_copy = vid.clone();
            let req_id = app.current_request_id;
            initial_tasks.push(Task::perform(
                async move { it_watch.get_watch_details(&vid_copy).await.map_err(|e| e.to_string()) },
                move |res| Message::WatchDetailsLoaded(req_id, res),
            ));
            let poster_url = format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", &vid);
            if let Some(poster_task) = app.queue_thumbnail_download(&poster_url) {
                initial_tasks.push(poster_task);
            }
            if let Some(blur_task) = app.queue_blurred_poster(&vid) {
                initial_tasks.push(blur_task);
            }
        }

        // Check if a direct video URL or ID was passed as a CLI argument on launch
        let cli_video_id = std::env::args().nth(1).and_then(|arg| {
            extract_youtube_video_id(&arg).or_else(|| {
                let trimmed = arg.trim();
                if trimmed.len() >= 10 && trimmed.len() <= 12 && trimmed.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                    Some(trimmed.to_string())
                } else {
                    None
                }
            })
        });

        if let Some(vid) = cli_video_id {
            let new_tab = ui::VideoTab {
                video_id: vid.clone(),
                title: "Cargando...".to_string(),
                channel_name: String::new(),
                saved_pos_ms: 0,
                thumbnail_url: format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", &vid),
            };
            app.tabs.push(new_tab);
            let new_idx = app.tabs.len() - 1;
            app.active_tab_idx = Some(new_idx);
            app.current_view = ViewMode::Watch(vid.clone());
            app.save_session();

            let it_watch = innertube.clone();
            let vid_copy = vid.clone();
            app.current_request_id = app.current_request_id.wrapping_add(1);
            let req_id = app.current_request_id;
            initial_tasks.push(Task::perform(
                async move { it_watch.get_watch_details(&vid_copy).await.map_err(|e| e.to_string()) },
                move |res| Message::WatchDetailsLoaded(req_id, res),
            ));
            let poster_url = format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", &vid);
            if let Some(poster_task) = app.queue_thumbnail_download(&poster_url) {
                initial_tasks.push(poster_task);
            }
        }

        (app, Task::batch(initial_tasks))
    }

    /// Starts the blur-in phase when navigating to a new page whose content is loading.
    fn trigger_blur_loading(&mut self) {
        self.is_page_loading = true;
        self.view_transition_start = Some(std::time::Instant::now());
        self.view_transition_alpha = self.view_transition_alpha.max(0.38);
    }

    /// Starts the unblur-out phase (1.0 -> 0.0) when page content is ready to focus.
    fn trigger_unblur_transition(&mut self) {
        self.is_page_loading = false;
        self.view_transition_start = Some(std::time::Instant::now());
        self.view_transition_alpha = 1.0;
    }

    /// Captures a lightweight 480x270 snapshot of the currently playing video frame
    /// so that switching away to another tab preserves the exact last frame on return.
    fn snapshot_current_playing_frame(&mut self) {
        if let (Some(ref vid), Some(ref player)) = (&self.playing_video_id, &self.player) {
            if player.is_first_frame_rendered() {
                if let Some(frame) = player.get_current_frame() {
                    if let Some(&last_id) = self.last_snapshot_frame_ids.get(vid) {
                        if last_id == frame.frame_id && self.tab_cached_frames.contains_key(vid) {
                            return;
                        }
                    }
                    if let Some(handle) = frame.to_cached_snapshot_handle() {
                        if self.tab_cached_frames.len() >= 15 && !self.tab_cached_frames.contains_key(vid) {
                            let open_vids: std::collections::HashSet<&str> =
                                self.tabs.iter().map(|t| t.video_id.as_str()).collect();
                            self.tab_cached_frames.retain(|k, _| open_vids.contains(k.as_str()));
                        }
                        self.last_snapshot_frame_ids.insert(vid.clone(), frame.frame_id);
                        self.tab_cached_frames.insert(vid.clone(), handle);
                    }
                }
            }
        }
    }

    fn save_session(&mut self) {
        if let (Some(cur_idx), Some(player)) = (self.active_tab_idx, &self.player) {
            if cur_idx < self.tabs.len()
                && player.is_first_frame_rendered()
                && self.playing_video_id.as_deref() == Some(self.tabs[cur_idx].video_id.as_str())
            {
                let pos = player.get_position_ms();
                let dur = player.get_duration_ms();
                if pos > 1000 && (dur == 0 || pos + 5000 < dur) {
                    self.tabs[cur_idx].saved_pos_ms = pos;
                }
            }
        }
        self.config.saved_tabs = self.tabs.clone();
        self.config.active_tab_idx = self.active_tab_idx;
        let config_clone = self.config.clone();
        std::thread::Builder::new()
            .name("session-save".into())
            .spawn(move || {
                config_clone.save();
            })
            .ok();
    }

    fn queue_blurred_poster(&mut self, video_id: &str) -> Option<Task<Message>> {
        if video_id.is_empty() || self.blurred_posters.contains_key(video_id) {
            return None;
        }
        let vid = video_id.to_string();
        let poster_url = format!("https://i.ytimg.com/vi/{vid}/hqdefault.jpg");
        let it = self.innertube.clone();
        Some(Task::perform(
            async move {
                let bytes = it.fetch_thumbnail_bytes(&poster_url).await?;
                tokio::task::spawn_blocking(move || {
                    let img = image_crate::load_from_memory(&bytes).ok()?;
                    // Downscale to 96x54 + Gaussian blur(3.5) -> <0.8ms CPU & visually identical when bilinearly upscaled
                    let blurred = img.thumbnail(96, 54).blur(3.5).to_rgba8();
                    let (w, h) = blurred.dimensions();
                    Some((vid, image::Handle::from_rgba(w, h, blurred.into_raw())))
                })
                .await
                .ok()
                .flatten()
            },
            |opt| match opt {
                Some((vid, h)) => Message::BlurredPosterReady(vid, h),
                None => Message::ThumbnailFailed(String::new()),
            },
        ))
    }

    fn queue_thumbnail_download(&mut self, url: &str) -> Option<Task<Message>> {
        self.queue_thumbnail_download_delayed(url, 0)
    }

    fn queue_thumbnail_download_delayed(&mut self, url: &str, delay_ms: u64) -> Option<Task<Message>> {
        static THUMB_DECODE_SEM: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(3);
        if url.is_empty() || self.thumbnail_cache.contains_key(url) || self.in_flight_thumbnails.contains(url) {
            return None;
        }
        self.in_flight_thumbnails.insert(url.to_string());
        let it = self.innertube.clone();
        let target_url = url.to_string();
        Some(Task::perform(
            async move {
                if delay_ms > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                }
                let bytes = it.fetch_thumbnail_bytes(&target_url).await;
                let is_avatar = target_url.contains("yt3.ggpht.com")
                    || target_url.contains("=s")
                    || target_url.contains("/channel/");
                if let Some(b) = bytes {
                    // For tiny avatars (< 16 KB), pass compressed bytes directly to avoid Intermediate DynamicImage allocations
                    if is_avatar && b.len() <= 16_384 {
                        return (target_url, Some((image::Handle::from_bytes(b), None)));
                    }
                    let _permit = THUMB_DECODE_SEM.acquire().await.ok();
                    let handles = tokio::task::spawn_blocking(move || {
                        let res = if let Ok(img) = image_crate::load_from_memory(&b) {
                            let (orig_w, orig_h) = (img.width(), img.height());
                            let is_square_avatar = is_avatar || (orig_w == orig_h && orig_w > 0);

                            let processed = if is_square_avatar {
                                if orig_w > 72 || orig_h > 72 {
                                    img.thumbnail(72, 72)
                                } else {
                                    img
                                }
                            } else if orig_w > 360 || orig_h > 202 {
                                img.thumbnail(360, 202)
                            } else {
                                img
                            };
                            let rgba = processed.to_rgba8();
                            let (w, h) = rgba.dimensions();
                            drop(b);
                            let sharp_handle = image::Handle::from_rgba(w, h, rgba.into_raw());
                            (sharp_handle, None)
                        } else {
                            (image::Handle::from_bytes(b), None)
                        };
                        crate::power_manager::trim_process_memory();
                        res
                    })
                    .await
                    .ok();
                    (target_url, handles)
                } else {
                    (target_url, None)
                }
            },
            |(url, handles)| {
                if let Some((sharp, blurred)) = handles {
                    Message::ThumbnailDownloaded(url, sharp, blurred)
                } else {
                    Message::ThumbnailFailed(url)
                }
            },
        ))
    }

    fn current_video_aspect_ratio(&self) -> f32 {
        if let Some(player) = &self.player {
            let (vw, vh) = player.get_video_dims();
            if vw > 0 && vh > 0 {
                let r = vw as f32 / vh as f32;
                if r.is_finite() && r > 0.1 && r < 10.0 {
                    return r;
                }
            }
        }
        if let Some(details) = self.pip_watch_details.as_ref().or(self.watch_details.as_ref()) {
            let stream_opt = details
                .video_streams
                .iter()
                .find(|s| s.itag == self.osd_state.selected_quality_itag && s.width > 0 && s.height > 0)
                .or_else(|| details.video_streams.iter().find(|s| s.width > 0 && s.height > 0));
            if let Some(stream) = stream_opt {
                let r = stream.width as f32 / stream.height as f32;
                if r.is_finite() && r > 0.1 && r < 10.0 {
                    return r;
                }
            }
        }
        16.0 / 9.0
    }

    fn compute_pip_aspect_size(&self, raw_w: f32, raw_h: f32, edge: Option<PipResizeEdge>) -> (f32, f32) {
        let ratio = self.current_video_aspect_ratio();
        let min_short = 135.0_f32;
        let max_w = 1920.0_f32;
        let max_h = 1080.0_f32;

        let drive_by_height = matches!(edge, Some(PipResizeEdge::Top | PipResizeEdge::Bottom));
        let (mut w, mut h) = if drive_by_height {
            let h = raw_h.max(min_short).min(max_h);
            let w = h * ratio;
            (w, h)
        } else {
            let min_w = if ratio < 1.0 { min_short } else { (min_short * ratio).max(220.0) };
            let w = raw_w.max(min_w).min(max_w);
            let h = w / ratio;
            (w, h)
        };

        if w > max_w {
            w = max_w;
            h = w / ratio;
        }
        if h > max_h {
            h = max_h;
            w = h * ratio;
        }
        let min_h = if ratio >= 1.0 { min_short } else { (min_short / ratio).max(220.0) };
        if h < min_h {
            h = min_h;
            w = h * ratio;
        }
        (w.round().max(1.0), h.round().max(1.0))
    }

    fn reset_watch_scroll_task(&mut self) -> Task<Message> {
        self.watch_scroll_y = 0.0;
        iced::widget::scrollable::scroll_to(
            iced::widget::scrollable::Id::new("watch_scrollable"),
            iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: 0.0 },
        )
    }

    fn reset_home_scroll_task(&self) -> Task<Message> {
        iced::widget::scrollable::scroll_to(
            iced::widget::scrollable::Id::new("home_scrollable"),
            iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: 0.0 },
        )
    }

    pub fn title(&self, id: Id) -> String {
        if Some(id) == self.pip_window_id || (self.main_window_id.is_some() && Some(id) != self.main_window_id) {
            return "Oxide tube - PiP".to_string();
        }
        match &self.current_view {
            ViewMode::Watch(_) => {
                if let Some(details) = &self.watch_details {
                    if !details.title.is_empty() {
                        return format!("{} - Oxide tube", details.title);
                    }
                }
                "Oxide tube".to_string()
            }
            ViewMode::Channel(_) => {
                if let Some(channel) = &self.channel_details {
                    if !channel.title.is_empty() {
                        return format!("{} - Oxide tube", channel.title);
                    }
                }
                "Oxide tube".to_string()
            }
            ViewMode::Home => "Oxide tube".to_string(),
            ViewMode::Subscriptions => "Suscripciones - Oxide tube".to_string(),
            ViewMode::History => "Historial de reproducciones - Oxide tube".to_string(),
            ViewMode::Playlists => "Playlists - Oxide tube".to_string(),
            _ => "Oxide tube".to_string(),
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::WindowIdFound(opt_id) => {
                if let Some(id) = opt_id {
                    self.main_window_id = Some(id);
                }
                Task::none()
            }

            Message::WindowHandleAcquired(disp_ptr, surf_ptr) => {
                if surf_ptr != 0 {
                    self.main_window_handles = (disp_ptr, surf_ptr);
                    if let Some(player) = &self.player {
                        player.update_handles(disp_ptr, surf_ptr);
                    } else {
                        match WaylandZeroCopyPlayer::new_from_raw(disp_ptr, surf_ptr) {
                            Ok(p) => {
                                let p_arc = Arc::new(p);
                                self.player = Some(p_arc.clone());
                                println!("[WaylandZeroCopy] libmpv Hardware Zero-Copy bound to wl_surface 0x{surf_ptr:x}, wl_display 0x{disp_ptr:x}");

                                if let Some(details) = &self.watch_details {
                                    if let Some(stream) = select_best_stream(&details.video_streams, &self.config) {
                                        let v_uri = stream.playback_uri("v");
                                        let a_uri = select_best_audio_stream(&details.audio_streams, self.config.preferred_audio_lang).map(|a| a.playback_uri("a"));
                                        let mut seek_target = None;
                                        if let Some(tab_idx) = self.active_tab_idx {
                                            if let Some(tab) = self.tabs.get(tab_idx) {
                                                if tab.video_id == details.video_id
                                                    && tab.saved_pos_ms > 0
                                                    && (stream.approx_duration_ms == 0
                                                        || tab.saved_pos_ms + 5000 < stream.approx_duration_ms)
                                                {
                                                    seek_target = Some(tab.saved_pos_ms);
                                                }
                                            }
                                        }
                                        self.playing_video_id = Some(details.video_id.clone());
                                        self.video_started_at = Some(std::time::Instant::now());
                                        self.osd_state.is_playing = true;
                                        let _ = p_arc.play_stream_with_seek(&v_uri, a_uri.as_deref(), seek_target);
                                        self.sync_player_viewport();
                                    }
                                }
                            }
                            Err(e) => eprintln!("[WaylandZeroCopy] Error initializing player: {e}"),
                        }
                    }
                }
                Task::none()
            }

            Message::PipHandleAcquired(disp_ptr, surf_ptr) => {
                crate::diagnostics::log_pip("Acquired", surf_ptr, "Wayland surface resolved for PiP window");
                if let Some(player) = &self.player {
                    let (pw, ph) = self.pip_window_size;
                    player.set_underlay(true);
                    player.set_viewport(0, 0, pw as i32, ph as i32);
                    player.set_rendering_active(true);
                    if surf_ptr != 0 {
                        if disp_ptr != 0 {
                            player.update_handles(disp_ptr, surf_ptr);
                        }
                        let target_pos = if player.get_position_ms() > 0 {
                            player.get_position_ms()
                        } else {
                            self.osd_state.current_pos_secs * 1000
                        };
                        player.switch_surface(surf_ptr, Some(target_pos));
                    }
                }
                if surf_ptr != 0 {
                    enforce_kwin_pip_keep_above(self.config.pip_position, Some(self.pip_window_size));
                }
                Task::none()
            }

            Message::ClosePip => {
                self.auto_pip_triggered_by_unfocus = false;
                save_current_kwin_pip_position(&mut self.config);
                if let Some(pip_id) = self.pip_window_id.take() {
                    crate::diagnostics::log_pip("Close", 0, "Closing PiP and returning to main window");
                    let pip_vid = self.playing_video_id.clone();
                    if let Some(details) = self.pip_watch_details.take().or_else(|| self.watch_details.clone()) {
                        self.watch_details = Some(details.clone());
                        if !matches!(self.current_view, ViewMode::Watch(_) | ViewMode::VideoFullscreen) {
                            self.current_view = ViewMode::Watch(details.video_id.clone());
                        }
                    } else if let Some(ref vid) = pip_vid {
                        if !matches!(self.current_view, ViewMode::Watch(_) | ViewMode::VideoFullscreen) {
                            self.current_view = ViewMode::Watch(vid.clone());
                        }
                    }
                    if let Some(ref vid) = pip_vid {
                        if let Some(idx) = self.tabs.iter().position(|t| &t.video_id == vid) {
                            self.active_tab_idx = Some(idx);
                        }
                    }
                    if let Some(player) = &self.player {
                        let target_pos = if player.get_position_ms() > 0 {
                            player.get_position_ms()
                        } else {
                            self.osd_state.current_pos_secs * 1000
                        };
                        let (_main_disp, main_surf) = self.main_window_handles;
                        if main_surf != 0 {
                            self.sync_player_viewport();
                            player.switch_surface(main_surf, Some(target_pos));
                        } else {
                            player.shutdown_sync();
                        }
                    }
                    return window::close(pip_id);
                }
                Task::none()
            }

            Message::RestoreFromPip => {
                crate::diagnostics::log_pip("Restore", 0, "Restoring main window from PiP via double-click");
                activate_kwin_main_window();
                let mut tasks = Vec::new();
                if let Some(main_id) = self.main_window_id {
                    self.focused_window = Some(main_id);
                    self.is_window_focused = true;
                    tasks.push(window::minimize(main_id, false));
                    tasks.push(window::gain_focus(main_id));
                }
                tasks.push(self.update(Message::ClosePip));
                return Task::batch(tasks);
            }

            Message::VideoFrameReady => {
                if !self.osd_state.first_frame_rendered {
                    if let Some(player) = &self.player {
                        if player.is_first_frame_rendered() {
                            self.osd_state.first_frame_rendered = true;
                        }
                    }
                }
                if let Some(pip_id) = self.pip_window_id {
                    let (cur_w, cur_h) = self.pip_window_size;
                    let (tw, th) = self.compute_pip_aspect_size(cur_w, cur_h, None);
                    if (cur_w - tw).abs() > 1.5 || (cur_h - th).abs() > 1.5 {
                        self.pip_window_size = (tw, th);
                        if let Some(player) = &self.player {
                            player.set_viewport(0, 0, tw as i32, th as i32);
                        }
                        return window::resize(pip_id, Size::new(tw, th));
                    }
                }
                Task::none()
            }

            Message::Tick => {

                if let Some((start, _)) = self.osd_state.play_pause_anim {
                    if start.elapsed().as_secs_f32() >= 0.50 {
                        self.osd_state.play_pause_anim = None;
                    }
                }

                if let Some(anim) = self.osd_state.seek_anim {
                    if anim.last_tap_time.elapsed().as_secs_f32() >= 0.65 {
                        self.osd_state.seek_anim = None;
                    }
                }

                if let Some((_, instant)) = &self.toast_message {
                    if instant.elapsed().as_secs_f32() >= 3.0 {
                        self.toast_message = None;
                    }
                }

                if let Some(start) = self.view_transition_start {
                    let elapsed = start.elapsed().as_secs_f32();
                    if self.is_page_loading {
                        // Phase 1: Blur-in while page data is being fetched (smooth ramp 0.38 -> 1.0 over 140ms)
                        let ramp = (elapsed / 0.14_f32).clamp(0.0, 1.0);
                        // Gentle breathing between 0.88 and 1.0 while loading
                        let pulse = if elapsed > 0.14 {
                            0.94 + 0.06 * ((elapsed - 0.14) * 5.0).cos()
                        } else {
                            0.38 + 0.62 * (1.0 - (1.0 - ramp).powi(2))
                        };
                        self.view_transition_alpha = pulse.clamp(0.35, 1.0);
                        // Safety timeout so UI never stays blurred if a request hangs
                        if elapsed > 4.0 {
                            self.is_page_loading = false;
                            self.view_transition_start = Some(std::time::Instant::now());
                        }
                    } else {
                        // Phase 2: Unblur-out once page data arrives (smooth cubic decay 1.0 -> 0.0 over 280ms)
                        let duration = 0.28_f32;
                        if elapsed >= duration {
                            self.view_transition_alpha = 0.0;
                            self.view_transition_start = None;
                        } else {
                            let t = (elapsed / duration).clamp(0.0, 1.0);
                            self.view_transition_alpha = (1.0 - t).powi(3);
                        }
                    }
                }

                if let Some((start, _, _)) = self.fullscreen_anim {
                    if start.elapsed().as_secs_f32() >= 0.22 {
                        self.fullscreen_anim = None;
                    }
                    self.sync_player_viewport();
                }

                self.osd_state.tick_visibility();

                let player_opt = self.player.clone();
                if let Some(player) = player_opt {
                    let pos_ms = player.get_position_ms();
                    let dur_ms = player.get_duration_ms();
                    let buf_ms = player.get_buffered_pos_ms();
                    let seek_unsuppressed = match self.seek_suppress_until {
                        Some(until) => {
                            let now = std::time::Instant::now();
                            if now >= until {
                                self.seek_suppress_until = None;
                                true
                            } else {
                                let cur_sec = pos_ms / 1000;
                                if cur_sec.abs_diff(self.osd_state.current_pos_secs) <= 1 {
                                    self.seek_suppress_until = None;
                                    true
                                } else {
                                    false
                                }
                            }
                        }
                        None => true,
                    };
                    if seek_unsuppressed {
                        self.osd_state.current_pos_secs = pos_ms / 1000;
                    }
                    if dur_ms > 0 {
                        self.osd_state.duration_secs = dur_ms / 1000;
                    }
                    self.osd_state.buffered_pos_secs = (buf_ms / 1000).max(self.osd_state.current_pos_secs);
                    let is_playing = player.is_playing();
                    self.osd_state.is_playing = is_playing;
                    self.osd_state.first_frame_rendered = player.is_first_frame_rendered();
                    self.osd_state.is_buffering = player.is_buffering();
                    self.osd_state.buffering_percent = player.buffering_percent();
                    let in_watch = matches!(self.current_view, ViewMode::Watch(_) | ViewMode::VideoFullscreen);
                    let pip_active = self.pip_window_id.is_some();
                    self.power_manager.set_screensaver_inhibit((in_watch || pip_active) && is_playing);

                    // Watchdog for video load stall / error: prevent staying stuck on "Preparando video..."
                    if in_watch && !self.osd_state.first_frame_rendered {
                        if let Some(started_at) = self.video_started_at {
                            let elapsed = started_at.elapsed();
                            let has_err = player.has_error();
                            let is_stalled = elapsed > Duration::from_secs(8)
                                && (!player.is_buffering() || player.buffering_percent() == 0);

                            if (has_err || is_stalled) && !self.osd_state.has_playback_error {
                                if let Some(details) = &self.watch_details {
                                    let cur_itag = self.osd_state.selected_quality_itag;
                                    let is_av1_or_vp9 = details.video_streams.iter().any(|s| {
                                        s.itag == cur_itag
                                            && (s.codecs.starts_with("av01")
                                                || s.codecs.starts_with("vp9")
                                                || s.codecs.starts_with("vp09"))
                                    });
                                    let h264_stream = details
                                        .video_streams
                                        .iter()
                                        .find(|s| s.codecs.starts_with("avc1") && s.height <= 1080);
                                    if is_av1_or_vp9 && h264_stream.is_some() {
                                        let stream = h264_stream.unwrap();
                                        self.osd_state.selected_quality_itag = stream.itag;
                                        self.osd_state.selected_quality_label = format!("{} (H264)", stream.quality_label);
                                        let v_uri = stream.playback_uri("v");
                                        let a_uri = select_best_audio_stream(&details.audio_streams, self.config.preferred_audio_lang).map(|s| s.playback_uri("a"));
                                        self.video_started_at = Some(std::time::Instant::now());
                                        let seek_pos = if self.osd_state.current_pos_secs >= 1 {
                                            Some(self.osd_state.current_pos_secs * 1000)
                                        } else {
                                            None
                                        };
                                        if let Err(e) = player.play_stream_with_seek(&v_uri, a_uri.as_deref(), seek_pos) {
                                            eprintln!("[Watchdog] failed to play stream: {e}");
                                            self.osd_state.has_playback_error = true;
                                        }
                                    } else {
                                        self.osd_state.has_playback_error = true;
                                    }
                                } else {
                                    self.osd_state.has_playback_error = true;
                                }
                            }
                        }
                    } else if self.osd_state.first_frame_rendered {
                        self.osd_state.has_playback_error = false;
                    }

                    let active_matches_playing = self
                        .watch_details
                        .as_ref()
                        .map(|d| self.playing_video_id.as_deref() == Some(d.video_id.as_str()))
                        .unwrap_or(false);

                    let cur_sec = pos_ms / 1000;
                    let last_sec = self.last_progress_pos_sec;
                    if self.osd_state.first_frame_rendered
                        && pos_ms > 1000
                        && seek_unsuppressed
                        && cur_sec.abs_diff(last_sec) >= 2
                    {
                        self.last_progress_pos_sec = cur_sec;
                        if let Some(ref playing_vid) = self.playing_video_id {
                            for tab in &mut self.tabs {
                                if &tab.video_id == playing_vid {
                                    if dur_ms > 5000 && pos_ms + 5000 >= dur_ms {
                                        tab.saved_pos_ms = 0;
                                    } else {
                                        tab.saved_pos_ms = pos_ms;
                                    }
                                }
                            }
                            if let Some(details) = self
                                .watch_details
                                .as_ref()
                                .filter(|d| &d.video_id == playing_vid)
                                .or_else(|| self.tab_details_cache.get(playing_vid))
                            {
                                let dur_str = if self.osd_state.duration_secs > 0 {
                                    ui::osd_controls::format_duration(self.osd_state.duration_secs)
                                } else {
                                    "00:00".to_string()
                                };
                                self.config.record_playback_progress(
                                    &details.video_id,
                                    &details.title,
                                    &details.channel_name,
                                    &details.channel_id,
                                    &details.channel_avatar,
                                    &dur_str,
                                    pos_ms,
                                );
                            }
                        }
                    }

                    let now_secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
                    let prev = self.last_session_save_secs;
                    if now_secs.saturating_sub(prev) >= 10 {
                        self.last_session_save_secs = now_secs;
                        self.save_session();
                    }

                    // Synchronize subtitles
                    if self.osd_state.selected_caption_lang.is_some() && !self.active_subtitles.is_empty() {
                        if let Some(cue) = self.active_subtitles.iter().find(|c| c.start_ms <= pos_ms && pos_ms <= c.start_ms + c.duration_ms) {
                            if self.osd_state.current_subtitle_text != cue.text {
                                self.osd_state.current_subtitle_text = cue.text.clone();
                            }
                        } else if !self.osd_state.current_subtitle_text.is_empty() {
                            self.osd_state.current_subtitle_text.clear();
                        }
                    }

                    // Continuous Autoplay / Queue Advance on Video Completion
                    // Strict guards prevent false triggers during initial loading, seek transitions, or stale timestamps
                    let is_eos = player.is_eos();
                    let played_long_enough = self
                        .video_started_at
                        .map(|t| t.elapsed() >= Duration::from_secs(5))
                        .unwrap_or(false);

                    if is_eos && self.pip_window_id.is_some() {
                        player.reset_eos();
                        player.pause();
                        self.osd_state.is_playing = false;
                    }

                    if self.autoplay_next
                        && self.pip_window_id.is_none()
                        && active_matches_playing
                        && played_long_enough
                        && self.osd_state.first_frame_rendered
                        && pos_ms > 3000
                        && dur_ms > 5000
                        && (is_eos || (dur_ms > 10_000 && pos_ms + 400 >= dur_ms && pos_ms <= dur_ms + 3000 && seek_unsuppressed && !player.is_buffering()))
                    {
                        player.reset_eos();
                        self.osd_state.first_frame_rendered = false;

                        // Reset saved position of the completed video so reopening it starts from 00:00
                        if let Some(finished_vid) = self.playing_video_id.clone() {
                            self.config.saved_positions.remove(&finished_vid);
                            for t in &mut self.tabs {
                                if t.video_id == finished_vid {
                                    t.saved_pos_ms = 0;
                                }
                            }
                            if !self.recently_autoplayed.contains(&finished_vid) {
                                self.recently_autoplayed.push(finished_vid);
                                if self.recently_autoplayed.len() > 20 {
                                    self.recently_autoplayed.remove(0);
                                }
                            }
                        }

                        if !self.playback_queue.is_empty() {
                            let next_vid = self.playback_queue.remove(0);
                            return self.update(Message::OpenVideo(next_vid.video_id));
                        } else if let Some(details) = &self.watch_details {
                            if let Some(next_rec) = details
                                .recommended_videos
                                .iter()
                                .find(|r| r.video_id != details.video_id && !self.recently_autoplayed.contains(&r.video_id))
                                .or_else(|| details.recommended_videos.iter().find(|r| r.video_id != details.video_id))
                            {
                                let next_id = next_rec.video_id.clone();
                                return self.update(Message::OpenVideo(next_id));
                            }
                        }
                    }
                }

                if let Some(details) = &self.watch_details {
                    if details.is_live || details.live_chat_continuation.is_some() {
                        self.live_chat_poll_tick = self.live_chat_poll_tick.wrapping_add(1);
                        // Throttle live chat snapshot reads to ~1.5s intervals instead of every 100ms
                        if self.live_chat_poll_tick % 6 == 0 {
                            let chat_mgr = self.live_chat.clone();
                            return Task::perform(
                                async move { chat_mgr.get_messages().await },
                                Message::ChatMessagesBatch,
                            );
                        }
                    }
                }
                Task::none()
            }

            Message::ChatMessagesBatch(msgs) => {
                self.live_messages = msgs;
                Task::none()
            }

            Message::PowerChanged(profile) => {
                self.power_profile = profile;
                if let Some(player) = &self.player {
                    player.set_power_profile(profile);
                }
                Task::none()
            }

            Message::WatchDetailsLoadedPassive(res) => {
                if let Ok(details) = res {
                    self.tab_details_cache.insert(details.video_id.clone(), Arc::clone(&details));
                    if matches!(self.current_view, ViewMode::Watch(ref v) if v == &details.video_id) {
                        self.description_content = iced::widget::text_editor::Content::with_text(&details.description);

                        if let Some(idx) = self.active_tab_idx {
                            if idx < self.tabs.len() && self.tabs[idx].video_id == details.video_id {
                                if self.tabs[idx].title.is_empty() || self.tabs[idx].title == "Video" {
                                    self.tabs[idx].title = details.title.clone();
                                }
                                if self.tabs[idx].channel_name.is_empty() {
                                    self.tabs[idx].channel_name = details.channel_name.clone();
                                }
                            }
                        }

                        let mut rec_tasks = Vec::new();
                        for rec in &details.recommended_videos {
                            if let Some(task) = self.queue_thumbnail_download(&rec.thumbnail_url) {
                                rec_tasks.push(task);
                            }
                        }

                        if self.pip_window_id.is_some() && self.playing_video_id.as_deref() == Some(&details.video_id) {
                            self.pip_watch_details = Some(Arc::clone(&details));
                        }
                        self.watch_details = Some(details);
                        if !rec_tasks.is_empty() {
                            return Task::batch(rec_tasks);
                        }
                    }
                }
                Task::none()
            }

            Message::OpenVideo(video_id) => {
                crate::diagnostics::session_start(&video_id);
                if self.pip_window_id.is_some() {
                    save_current_kwin_pip_position(&mut self.config);
                }
                // Save previous video's playback position and frame snapshot BEFORE mutating tab state or stopping player!
                self.snapshot_current_playing_frame();
                if let (Some(cur_idx), Some(player)) = (self.active_tab_idx, &self.player) {
                    if cur_idx < self.tabs.len()
                        && player.is_first_frame_rendered()
                        && self.playing_video_id.as_deref() == Some(self.tabs[cur_idx].video_id.as_str())
                    {
                        let pos = player.get_position_ms();
                        let dur = player.get_duration_ms();
                        if pos > 1000 && (dur == 0 || pos + 5000 < dur) {
                            self.tabs[cur_idx].saved_pos_ms = pos;
                            self.config
                                .saved_positions
                                .insert(self.tabs[cur_idx].video_id.clone(), pos);
                        }
                    }
                }

                // Stop the player and live chat FIRST so state is immediately reset
                if let Some(player) = &self.player {
                    if self.pip_window_id.is_some() {
                        player.shutdown_sync();
                    } else {
                        player.stop();
                    }
                }
                self.live_chat.stop();
                self.live_messages.clear();
                self.playing_video_id = None;
                self.video_started_at = None;
                self.last_progress_pos_sec = 0;

                self.previous_view = self.current_view.clone();
                self.current_view = ViewMode::Watch(video_id.clone());
                self.trigger_blur_loading();
                self.show_suggestions = false;
                self.watch_scroll_y = 0.0;
                self.active_subtitles.clear();
                self.osd_state.current_subtitle_text.clear();
                self.osd_state.selected_caption_lang = None;

                // Look up clicked VideoCard to pre-populate WatchDetails on frame 0 (0ms layout transition)
                let matched_card = self
                    .home_videos
                    .iter()
                    .find(|v| v.video_id == video_id)
                    .or_else(|| {
                        self.cached_home_feed
                            .iter()
                            .find(|v| v.video_id == video_id)
                    })
                    .or_else(|| {
                        self.watch_details
                            .as_ref()
                            .and_then(|d| d.recommended_videos.iter().find(|v| v.video_id == video_id))
                    })
                    .or_else(|| {
                        self.channel_details.as_ref().and_then(|ch| {
                            ch.tabs
                                .iter()
                                .flat_map(|t| t.videos.iter())
                                .find(|v| v.video_id == video_id)
                        })
                    })
                    .cloned();

                let mut existing_recs = self
                    .watch_details
                    .as_ref()
                    .map(|d| d.recommended_videos.clone())
                    .filter(|r| !r.is_empty())
                    .unwrap_or_else(|| self.home_videos.iter().take(10).cloned().collect());
                existing_recs.retain(|v| v.video_id != video_id);

                let existing_tab = self.tabs.iter().find(|t| t.video_id == video_id).cloned();

                self.watch_details = Some(Arc::new(WatchDetails {
                    video_id: video_id.clone(),
                    title: matched_card
                        .as_ref()
                        .map(|c| c.title.clone())
                        .or_else(|| existing_tab.as_ref().map(|t| t.title.clone()))
                        .unwrap_or_default(),
                    channel_name: matched_card
                        .as_ref()
                        .map(|c| c.channel_name.clone())
                        .or_else(|| existing_tab.as_ref().map(|t| t.channel_name.clone()))
                        .unwrap_or_default(),
                    channel_id: matched_card.as_ref().map(|c| c.channel_id.clone()).unwrap_or_default(),
                    channel_avatar: matched_card.as_ref().map(|c| c.channel_avatar.clone()).unwrap_or_default(),
                    subscriber_count: String::new(),
                    likes_str: String::new(),
                    dislikes_str: String::new(),
                    description: String::new(),
                    published_date: String::new(),
                    views_str: String::new(),
                    is_live: matched_card.as_ref().map(|c| c.is_live).unwrap_or(false),
                    is_subscribed: false,
                    like_status: "INDIFFERENT".to_string(),
                    live_chat_continuation: None,
                    recommended_videos: existing_recs,
                    comments: Vec::new(),
                    video_streams: Vec::new(),
                    audio_streams: Vec::new(),
                    caption_tracks: Vec::new(),
                    chapters: Vec::new(),
                    parsed_desc_urls: Vec::new(),
                    parsed_desc_timestamps: Vec::new(),
                }));

                let mut target_saved_pos_ms = self.config.saved_positions.get(&video_id).copied().unwrap_or(0);
                // Validate saved position against known card duration if available
                if let Some(card) = &matched_card {
                    if let Some(dur_secs) = innertube::InnerTubeClient::parse_timestamp_secs(&card.duration_str) {
                        let card_dur_ms = dur_secs * 1000;
                        if card_dur_ms > 5000 && target_saved_pos_ms + 5000 >= card_dur_ms {
                            target_saved_pos_ms = 0;
                            self.config.saved_positions.remove(&video_id);
                        }
                    }
                }

                if let Some(tab_idx) = self.tabs.iter().position(|t| t.video_id == video_id) {
                    self.active_tab_idx = Some(tab_idx);
                    if let Some(card) = &matched_card {
                        if let Some(dur_secs) = innertube::InnerTubeClient::parse_timestamp_secs(&card.duration_str) {
                            let card_dur_ms = dur_secs * 1000;
                            if card_dur_ms > 5000 && self.tabs[tab_idx].saved_pos_ms + 5000 >= card_dur_ms {
                                self.tabs[tab_idx].saved_pos_ms = 0;
                            }
                        }
                    }
                    if self.tabs[tab_idx].saved_pos_ms > 0 {
                        target_saved_pos_ms = self.tabs[tab_idx].saved_pos_ms;
                    } else if target_saved_pos_ms > 0 {
                        self.tabs[tab_idx].saved_pos_ms = target_saved_pos_ms;
                    }
                } else if let Some(cur_idx) = self.active_tab_idx {
                    if cur_idx < self.tabs.len() {
                        self.tabs[cur_idx].video_id = video_id.clone();
                        self.tabs[cur_idx].title = matched_card.as_ref().map(|c| c.title.clone()).unwrap_or_else(|| "Cargando...".to_string());
                        self.tabs[cur_idx].channel_name = matched_card.as_ref().map(|c| c.channel_name.clone()).unwrap_or_default();
                        self.tabs[cur_idx].saved_pos_ms = target_saved_pos_ms;
                        if let Some(c) = &matched_card {
                            self.tabs[cur_idx].thumbnail_url = c.thumbnail_url.clone();
                        }
                    }
                } else {
                    self.tabs.push(ui::VideoTab {
                        video_id: video_id.clone(),
                        title: matched_card.as_ref().map(|c| c.title.clone()).unwrap_or_else(|| "Cargando...".to_string()),
                        channel_name: matched_card.as_ref().map(|c| c.channel_name.clone()).unwrap_or_default(),
                        saved_pos_ms: target_saved_pos_ms,
                        thumbnail_url: matched_card.as_ref().map(|c| c.thumbnail_url.clone()).unwrap_or_default(),
                    });
                    self.active_tab_idx = Some(self.tabs.len() - 1);
                }
                self.save_session();

                self.osd_state.current_pos_secs = target_saved_pos_ms / 1000;
                self.osd_state.buffered_pos_secs = target_saved_pos_ms / 1000;
                self.osd_state.duration_secs = matched_card
                    .as_ref()
                    .and_then(|c| innertube::InnerTubeClient::parse_timestamp_secs(&c.duration_str))
                    .unwrap_or(0);
                self.osd_state.first_frame_rendered = false;
                self.osd_state.is_buffering = false;
                self.osd_state.buffering_percent = 100;
                self.osd_state.is_playing = true;
                self.osd_state.show_speed_menu = false;
                self.osd_state.show_qualities = false;
                self.osd_state.show_audio_menu = false;
                self.osd_state.show_subtitles_menu = false;
                self.osd_state.on_mouse_move();

                self.seek_suppress_until = Some(std::time::Instant::now() + Duration::from_millis(2000));
                self.sync_player_viewport();
                power_manager::trim_process_memory();

                // Record watched video in the user's YouTube Account History asynchronously
                let it_hist = self.innertube.clone();
                let vid_hist = video_id.clone();
                tokio::spawn(async move {
                    it_hist.record_watch_history(&vid_hist).await;
                });

                self.current_request_id = self.current_request_id.wrapping_add(1);
                let req_id = self.current_request_id;

                let it = self.innertube.clone();
                let vid_for_task = video_id.clone();
                let watch_task = Task::perform(
                    async move { it.get_watch_details(&vid_for_task).await.map_err(|e| e.to_string()) },
                    move |res| Message::WatchDetailsLoaded(req_id, res),
                );

                let mut tasks = vec![watch_task, self.reset_watch_scroll_task()];
                if let Some(pip_id) = self.pip_window_id.take() {
                    self.pip_watch_details = None;
                    let (_main_disp, main_surf) = self.main_window_handles;
                    if let Some(player) = &self.player {
                        if main_surf != 0 {
                            player.update_handles(0, main_surf);
                        }
                    }
                    tasks.push(window::close(pip_id));
                }
                let poster_url = format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", &video_id);
                if let Some(poster_task) = self.queue_thumbnail_download(&poster_url) {
                    tasks.push(poster_task);
                }
                if let Some(blur_task) = self.queue_blurred_poster(&video_id) {
                    tasks.push(blur_task);
                }
                Task::batch(tasks)
            }

            Message::WatchDetailsLoaded(req_id, res) => {
                if req_id != self.current_request_id {
                    return Task::none();
                }
                self.trigger_unblur_transition();
                match res {
                    Ok(mut details) => {
                        crate::diagnostics::session_details_loaded();
                        // Preserve pre-populated title/channel_name if API fallback was generic
                        if let Some(prev) = &self.watch_details {
                            if prev.video_id == details.video_id {
                                if (details.title.is_empty() || details.title == "Sin título") && !prev.title.is_empty() {
                                    Arc::make_mut(&mut details).title = prev.title.clone();
                                }
                                if (details.channel_name.is_empty() || details.channel_name == "Canal de YouTube")
                                    && !prev.channel_name.is_empty()
                                {
                                    Arc::make_mut(&mut details).channel_name = prev.channel_name.clone();
                                }
                                if details.channel_id.is_empty() && !prev.channel_id.is_empty() {
                                    Arc::make_mut(&mut details).channel_id = prev.channel_id.clone();
                                }
                            }
                        }

                        self.watch_details = Some(Arc::clone(&details));
                        self.tab_details_cache.insert(details.video_id.clone(), Arc::clone(&details));
                        if self.tab_details_cache.len() > self.tabs.len() + 2 {
                            let open_vids: std::collections::HashSet<&str> =
                                self.tabs.iter().map(|t| t.video_id.as_str()).collect();
                            let cur_vid = details.video_id.as_str();
                            let play_vid = self.playing_video_id.as_deref();
                            self.tab_details_cache.retain(|k, _| {
                                k.as_str() == cur_vid || play_vid == Some(k.as_str()) || open_vids.contains(k.as_str())
                            });
                        }
                        if self.pip_window_id.is_some() && self.playing_video_id.as_deref() == Some(&details.video_id) {
                            self.pip_watch_details = Some(Arc::clone(&details));
                        }

                        let best_stream = select_best_stream(&details.video_streams, &self.config);
                        let approx_dur_ms = best_stream.map(|s| s.approx_duration_ms).unwrap_or(0);
                        if approx_dur_ms > 0 {
                            self.osd_state.duration_secs = approx_dur_ms / 1000;
                        }

                        // Sanitize any stale or finished saved_pos_ms against the actual stream duration
                        let mut initial_seek_ms: Option<u64> = None;
                        if let Some(tab_idx) = self.tabs.iter().position(|t| t.video_id == details.video_id) {
                            self.tabs[tab_idx].title = details.title.clone();
                            self.tabs[tab_idx].channel_name = details.channel_name.clone();
                            let saved_ms = self.tabs[tab_idx].saved_pos_ms;
                            if saved_ms > 0 {
                                if approx_dur_ms > 5000 && saved_ms + 5000 >= approx_dur_ms {
                                    // Stale / near-end timestamp: reset to 0 so video starts cleanly from the beginning
                                    self.tabs[tab_idx].saved_pos_ms = 0;
                                    self.config.saved_positions.remove(&details.video_id);
                                    self.osd_state.current_pos_secs = 0;
                                    self.osd_state.buffered_pos_secs = 0;
                                } else {
                                    initial_seek_ms = Some(saved_ms);
                                }
                            }
                            self.save_session();
                        }

                        // Record in local history immediately so History tab reflects the video right away
                        let dur_str = if self.osd_state.duration_secs > 0 {
                            ui::osd_controls::format_duration(self.osd_state.duration_secs)
                        } else {
                            "00:00".to_string()
                        };
                        self.config.record_playback_progress(
                            &details.video_id,
                            &details.title,
                            &details.channel_name,
                            &details.channel_id,
                            &details.channel_avatar,
                            &dur_str,
                            initial_seek_ms.unwrap_or(0),
                        );

                        if details.is_live || details.live_chat_continuation.is_some() {
                            let chat = self.live_chat.clone();
                            let cont = details.live_chat_continuation.clone();
                            tokio::spawn(async move {
                                chat.set_continuation(cont).await;
                                chat.start_polling();
                            });
                        }

                        if let Some(stream) = best_stream {
                            let codec_tag = if stream.codecs.starts_with("av01") {
                                "AV1"
                            } else if stream.codecs.starts_with("vp9") || stream.codecs.starts_with("vp09") {
                                "VP9"
                            } else {
                                "H264"
                            };
                            self.osd_state.selected_quality_itag = stream.itag;
                            self.osd_state.selected_quality_label = format!("{} ({})", stream.quality_label, codec_tag);
                        }

                        if let Some(first_audio) = select_best_audio_stream(&details.audio_streams, self.config.preferred_audio_lang) {
                            self.osd_state.selected_audio_itag = first_audio.itag;
                            self.osd_state.selected_audio_track_id = first_audio.audio_track_id.clone();
                            self.osd_state.selected_audio_label = first_audio.audio_track_name.clone();
                        }

                        let desc_text = if self.description_expanded || details.description.chars().count() <= 240 {
                            details.description.clone()
                        } else {
                            let truncated: String = details.description.chars().take(240).collect();
                            format!("{truncated}...")
                        };
                        self.description_content = iced::widget::text_editor::Content::with_text(&desc_text);

                        let is_still_active_tab = self
                            .active_tab_idx
                            .and_then(|i| self.tabs.get(i))
                            .map(|t| t.video_id == details.video_id)
                            .unwrap_or(true);
                        let another_video_already_playing = self
                            .playing_video_id
                            .as_ref()
                            .map(|pid| pid != &details.video_id)
                            .unwrap_or(false)
                            && self.player.as_ref().map(|p| p.is_playing()).unwrap_or(false);

                        if is_still_active_tab && !another_video_already_playing {
                            if let Some(player) = &self.player {
                                let stream_height = best_stream.map(|s| s.height).unwrap_or(1080);
                                player.set_stream_buffer_profile(stream_height, self.power_profile);
                                let (video_uri, audio_uri) = if let Some(buf_state) = self.buffer_downloads.get(&details.video_id) {
                                    if buf_state.is_complete && buf_state.video_path.is_some() {
                                        let v_p = buf_state.video_path.as_ref().unwrap();
                                        let v_str = format!("file://{}", v_p.to_string_lossy());
                                        let a_str = buf_state.audio_path.as_ref().map(|p| format!("file://{}", p.to_string_lossy()));
                                        (v_str, a_str)
                                    } else {
                                        let v = best_stream.map(|s| s.playback_uri("v")).unwrap_or_default();
                                        let a = select_best_audio_stream(&details.audio_streams, self.config.preferred_audio_lang).map(|s| s.playback_uri("a"));
                                        (v, a)
                                    }
                                } else {
                                    let v = best_stream.map(|s| s.playback_uri("v")).unwrap_or_default();
                                    let a = select_best_audio_stream(&details.audio_streams, self.config.preferred_audio_lang).map(|s| s.playback_uri("a"));
                                    (v, a)
                                };

                                if !video_uri.is_empty() {
                                    crate::diagnostics::log_event("STREAM", &format!("Launching playback for: {}", details.video_id));
                                    self.osd_state.has_playback_error = false;
                                    self.osd_state.is_playing = true;
                                    self.playing_video_id = Some(details.video_id.clone());
                                    self.video_started_at = Some(std::time::Instant::now());
                                    if let Err(e) = player.play_stream_with_seek(&video_uri, audio_uri.as_deref(), initial_seek_ms) {
                                        eprintln!("[Playback Error] failed to launch stream: {e}");
                                        self.osd_state.has_playback_error = true;
                                    }
                                } else {
                                    self.osd_state.has_playback_error = true;
                                }

                                self.sync_player_viewport();
                            }
                        }

                        // Trigger thumbnail downloads: prioritize channel avatar and top 4 recommendations immediately;
                        // delay subsequent off-screen thumbnails to prevent CPU & network starvation during video startup.
                        let mut tasks = Vec::new();
                        if self.config.subtitles_enabled && !details.caption_tracks.is_empty() {
                            let track_opt = if let Some(pref_lang) = &self.config.preferred_caption_lang {
                                details.caption_tracks.iter().find(|t| t.language_code == *pref_lang || t.language_code.starts_with(pref_lang))
                                    .or_else(|| details.caption_tracks.iter().find(|t| t.language_code.starts_with("es")))
                                    .or_else(|| details.caption_tracks.first())
                            } else {
                                details.caption_tracks.iter().find(|t| t.language_code.starts_with("es"))
                                    .or_else(|| details.caption_tracks.first())
                            };

                            if let Some(track) = track_opt {
                                self.osd_state.selected_caption_lang = Some(track.name.clone());
                                let base_url = track.base_url.clone();
                                let it = self.innertube.clone();
                                let vid = details.video_id.clone();
                                tasks.push(Task::perform(
                                    async move {
                                        let cues = it.fetch_captions(&base_url).await.map_err(|e| e.to_string());
                                        (vid, cues)
                                    },
                                    |(vid, res)| Message::CaptionsLoaded(vid, res),
                                ));
                            }
                        }
                        if self.config.full_buffer_tabs
                            && !self.buffer_downloads.get(&details.video_id).map(|s| s.is_complete).unwrap_or(false)
                        {
                            tasks.push(self.update(Message::TriggerFullBuffer(details.video_id.clone())));
                        }
                        if !details.channel_avatar.is_empty() {
                            if let Some(t) = self.queue_thumbnail_download(&details.channel_avatar) {
                                tasks.push(t);
                            }
                        }
                        for (idx, video) in details.recommended_videos.iter().enumerate() {
                            let delay = if idx < 4 { 0 } else { 450 + (idx as u64) * 30 };
                            if let Some(t) = self.queue_thumbnail_download_delayed(&video.thumbnail_url, delay) {
                                tasks.push(t);
                            }
                            if !video.channel_avatar.is_empty() {
                                if let Some(t) = self.queue_thumbnail_download_delayed(&video.channel_avatar, delay) {
                                    tasks.push(t);
                                }
                            }
                        }
                        for (idx, comment) in details.comments.iter().enumerate() {
                            let delay = 600 + (idx as u64) * 20;
                            if !comment.author_avatar.is_empty() {
                                if let Some(t) = self.queue_thumbnail_download_delayed(&comment.author_avatar, delay) {
                                    tasks.push(t);
                                }
                            }
                        }

                        self.watch_details = Some(details);
                        if !tasks.is_empty() {
                            return Task::batch(tasks);
                        }
                    }
                    Err(e) => {
                        eprintln!("[InnerTube] Error loading video watch details: {e}");
                        self.osd_state.has_playback_error = true;
                    }
                }
                Task::none()
            }

            Message::OpenChannel(channel_id) => {
                if channel_id.is_empty() {
                    return Task::none();
                }
                self.previous_view = self.current_view.clone();
                self.current_view = ViewMode::Channel(channel_id.clone());
                self.trigger_blur_loading();
                self.selected_channel_tab = 0;
                self.show_suggestions = false;

                let known_name = self
                    .watch_details
                    .as_ref()
                    .filter(|d| d.channel_id == channel_id)
                    .map(|d| d.channel_name.clone())
                    .or_else(|| {
                        self.home_videos
                            .iter()
                            .find(|v| v.channel_id == channel_id)
                            .map(|v| v.channel_name.clone())
                    })
                    .unwrap_or_default();

                let known_subscribed = self
                    .watch_details
                    .as_ref()
                    .filter(|d| d.channel_id == channel_id)
                    .map(|d| d.is_subscribed)
                    .unwrap_or(false);

                self.channel_details = Some(ChannelDetails {
                    channel_id: channel_id.clone(),
                    title: known_name,
                    handle: String::new(),
                    subscriber_count: String::new(),
                    is_subscribed: known_subscribed,
                    banner_url: None,
                    avatar_url: String::new(),
                    tabs: Vec::new(),
                });

                self.snapshot_current_playing_frame();
                if let Some(details) = &self.watch_details {
                    if !details.video_streams.is_empty() {
                        self.tab_details_cache.insert(details.video_id.clone(), details.clone());
                    }
                }
                self.sync_player_viewport();
                power_manager::trim_process_memory();

                let scroll_top = iced::widget::scrollable::scroll_to(
                    iced::widget::scrollable::Id::new("channel_scrollable"),
                    iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: 0.0 },
                );
                let it = self.innertube.clone();
                let ch_task = Task::perform(
                    async move { it.get_channel(&channel_id).await.map_err(|e| e.to_string()) },
                    Message::ChannelLoaded,
                );
                Task::batch([ch_task, scroll_top])
            }

            Message::ChannelLoaded(res) => {
                self.trigger_unblur_transition();
                match res {
                    Ok(channel) => {
                        let mut tasks = Vec::new();
                        if !channel.avatar_url.is_empty() {
                            if let Some(t) = self.queue_thumbnail_download(&channel.avatar_url) {
                                tasks.push(t);
                            }
                        }
                        if let Some(banner) = &channel.banner_url {
                            if let Some(t) = self.queue_thumbnail_download(banner) {
                                tasks.push(t);
                            }
                        }
                        if let Some(tab) = channel.tabs.get(self.selected_channel_tab) {
                            for v in &tab.videos {
                                if let Some(t) = self.queue_thumbnail_download(&v.thumbnail_url) {
                                    tasks.push(t);
                                }
                                if !v.channel_avatar.is_empty() {
                                    if let Some(t) = self.queue_thumbnail_download(&v.channel_avatar) {
                                        tasks.push(t);
                                    }
                                }
                            }
                        }
                        self.channel_details = Some(channel);
                        if !tasks.is_empty() {
                            return Task::batch(tasks);
                        }
                    }
                    Err(e) => eprintln!("[InnerTube] Error loading channel: {e}"),
                }
                Task::none()
            }

            Message::SelectChannelTab(idx) => {
                self.selected_channel_tab = idx;
                self.trigger_unblur_transition();
                let mut tasks = Vec::new();
                if let Some(channel) = &self.channel_details {
                    if let Some(tab) = channel.tabs.get(idx) {
                        let urls: Vec<(String, String)> = tab
                            .videos
                            .iter()
                            .map(|v| (v.thumbnail_url.clone(), v.channel_avatar.clone()))
                            .collect();
                        for (t_url, a_url) in urls {
                            if let Some(t) = self.queue_thumbnail_download(&t_url) {
                                tasks.push(t);
                            }
                            if !a_url.is_empty() {
                                if let Some(t) = self.queue_thumbnail_download(&a_url) {
                                    tasks.push(t);
                                }
                            }
                        }
                    }
                }
                if !tasks.is_empty() {
                    return Task::batch(tasks);
                }
                Task::none()
            }

            Message::GoHome => {
                self.snapshot_current_playing_frame();
                if let Some(details) = &self.watch_details {
                    if !details.video_streams.is_empty() {
                        self.tab_details_cache.insert(details.video_id.clone(), details.clone());
                    }
                }
                self.previous_view = self.current_view.clone();
                self.current_view = ViewMode::Home;
                self.active_tab_idx = None;
                self.show_suggestions = false;
                self.show_auth_modal = false;

                self.sync_player_viewport();
                power_manager::trim_process_memory();

                let scroll_top = self.reset_home_scroll_task();
                if !self.cached_home_feed.is_empty() {
                    self.home_videos = self.cached_home_feed.clone();
                    self.trigger_unblur_transition();
                    let urls: Vec<(String, String)> = self
                        .home_videos
                        .iter()
                        .map(|v| (v.thumbnail_url.clone(), v.channel_avatar.clone()))
                        .collect();
                    let mut tasks = vec![scroll_top];
                    for (t_url, a_url) in urls {
                        if let Some(t) = self.queue_thumbnail_download(&t_url) {
                            tasks.push(t);
                        }
                        if !a_url.is_empty() {
                            if let Some(t) = self.queue_thumbnail_download(&a_url) {
                                tasks.push(t);
                            }
                        }
                    }
                    return Task::batch(tasks);
                } else {
                    self.home_videos.clear();
                    self.trigger_blur_loading();
                    let it = self.innertube.clone();
                    let feed_task = Task::perform(
                        async move { it.get_home_feed().await.map_err(|e| e.to_string()) },
                        Message::HomeFeedLoaded,
                    );
                    return Task::batch([feed_task, scroll_top]);
                }
            }

            Message::GoBack => {
                let target = self.previous_view.clone();
                self.previous_view = ViewMode::Home;
                match target {
                    ViewMode::Home => self.update(Message::GoHome),
                    ViewMode::Subscriptions => self.update(Message::OpenSubscriptions),
                    ViewMode::Watch(v) => self.update(Message::OpenVideo(v)),
                    ViewMode::Channel(c) => self.update(Message::OpenChannel(c)),
                    ViewMode::VideoFullscreen => self.update(Message::ToggleFullscreen),
                    ViewMode::History => self.update(Message::OpenHistory),
                    ViewMode::Playlists => self.update(Message::OpenPlaylists),
                }
            }

            Message::SearchInputChanged(query) => {
                self.search_input_focused = true;
                self.search_query = query.clone();
                if query.trim().len() >= 2 {
                    self.show_suggestions = true;
                    let it = self.innertube.clone();
                    return Task::perform(
                        async move { it.get_search_suggestions(&query).await },
                        Message::SuggestionsLoaded,
                    );
                } else {
                    self.show_suggestions = false;
                    self.suggestions.clear();
                }
                Task::none()
            }

            Message::SuggestionsLoaded(suggs) => {
                self.suggestions = suggs;
                Task::none()
            }

            Message::SelectSuggestion(query) => {
                self.search_input_focused = false;
                self.search_query = query;
                self.show_suggestions = false;
                self.update(Message::SearchSubmitted)
            }

            Message::SearchSubmitted => {
                self.search_input_focused = false;
                let query = self.search_query.trim().to_string();
                if query.is_empty() {
                    return Task::none();
                }
                self.show_suggestions = false;

                // Check if query is a direct YouTube link or video ID
                if let Some(vid) = extract_youtube_video_id(&query) {
                    if let Some(active_idx) = self.active_tab_idx {
                        if active_idx < self.tabs.len() && self.tabs[active_idx].video_id.is_empty() {
                            self.tabs[active_idx].video_id = vid.clone();
                            self.tabs[active_idx].title = "Cargando...".to_string();
                            self.save_session();
                        }
                    }
                    self.search_query.clear();
                    return self.update(Message::OpenVideo(vid));
                }

                self.snapshot_current_playing_frame();
                if let Some(details) = &self.watch_details {
                    if !details.video_streams.is_empty() {
                        self.tab_details_cache.insert(details.video_id.clone(), details.clone());
                    }
                }
                self.previous_view = self.current_view.clone();
                self.current_view = ViewMode::Home;
                self.trigger_blur_loading();
                self.sync_player_viewport();

                let scroll_top = self.reset_home_scroll_task();
                let it = self.innertube.clone();
                let search_task = Task::perform(
                    async move { it.search(&query).await.map_err(|e| e.to_string()) },
                    Message::SearchResultsLoaded,
                );
                Task::batch([search_task, scroll_top])
            }

            Message::SearchResultsLoaded(res) => {
                self.trigger_unblur_transition();
                match res {
                    Ok(videos) => {
                        let mut tasks = Vec::new();
                        for v in &videos {
                            if let Some(t) = self.queue_thumbnail_download(&v.thumbnail_url) {
                                tasks.push(t);
                            }
                            if !v.channel_avatar.is_empty() {
                                if let Some(t) = self.queue_thumbnail_download(&v.channel_avatar) {
                                    tasks.push(t);
                                }
                            }
                        }
                        self.home_videos = videos;
                        if !tasks.is_empty() {
                            return Task::batch(tasks);
                        }
                    }
                    Err(e) => eprintln!("[InnerTube] Search error: {e}"),
                }
                Task::none()
            }

            Message::HomeFeedLoaded(res) => {
                self.trigger_unblur_transition();
                if let Ok(r) = self.auth_session.try_read() {
                    if r.is_authenticated() {
                        self.auth_status = "[OK] Sesion activa de YouTube".to_string();
                    }
                }
                match res {
                    Ok(videos) => {
                        let mut tasks = Vec::new();
                        for v in &videos {
                            if let Some(t) = self.queue_thumbnail_download(&v.thumbnail_url) {
                                tasks.push(t);
                            }
                            if !v.channel_avatar.is_empty() {
                                if let Some(t) = self.queue_thumbnail_download(&v.channel_avatar) {
                                    tasks.push(t);
                                }
                            }
                        }
                        self.cached_home_feed = videos.clone();
                        if matches!(self.current_view, ViewMode::Home) {
                            self.home_videos = videos;
                            self.trigger_unblur_transition();
                        }
                        if !tasks.is_empty() {
                            return Task::batch(tasks);
                        }
                    }
                    Err(e) => {
                        eprintln!("[InnerTube] Error loading feed: {e} — retrying in 3s");
                        if self.home_videos.is_empty() {
                            let it = self.innertube.clone();
                            return Task::perform(
                                async move {
                                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                                    it.get_home_feed().await.map_err(|e| e.to_string())
                                },
                                Message::HomeFeedLoaded,
                            );
                        }
                    }
                }
                Task::none()
            }

            Message::ThumbnailDownloaded(url, handle, blurred_opt) => {
                self.in_flight_thumbnails.remove(&url);
                // Deduplicate LRU queue so the same URL never appears twice
                self.thumbnail_order.retain(|k| k != &url);

                // Each video card uses 2 slots (1 thumbnail + 1 avatar) and Watch view uses ~82 slots.
                // Setting 220 (Battery) / 320 (AC) holds ~110-160 cards + avatars without evicting visible items
                // or thrashing iced_wgpu texture atlas Handle IDs.
                let limit = if self.power_profile == PowerProfile::Battery { 220 } else { 320 };
                let mut evicted = false;
                while self.thumbnail_cache.len() >= limit {
                    if let Some(first_key) = self.thumbnail_order.pop_front() {
                        self.thumbnail_cache.remove(&first_key);
                        self.blurred_thumbnails.remove(&first_key);
                        evicted = true;
                    } else {
                        break;
                    }
                }
                if evicted && self.last_memory_trim.elapsed().as_secs() >= 5 {
                    self.last_memory_trim = std::time::Instant::now();
                    crate::power_manager::trim_process_memory();
                }

                if let Some(blurred) = blurred_opt {
                    self.blurred_thumbnails.insert(url.clone(), blurred);
                }
                self.thumbnail_cache.insert(url.clone(), handle);
                self.thumbnail_order.push_back(url);
                Task::none()
            }

            Message::BlurredPosterReady(video_id, handle) => {
                if !video_id.is_empty() {
                    if self.blurred_posters.len() > 20 {
                        self.blurred_posters.clear();
                    }
                    self.blurred_posters.insert(video_id, handle);
                }
                Task::none()
            }

            Message::ThumbnailFailed(url) => {
                self.in_flight_thumbnails.remove(&url);
                Task::none()
            }

            Message::WatchScrolled(viewport) => {
                self.watch_scroll_y = viewport.absolute_offset().y;
                self.sync_player_viewport();
                Task::none()
            }

            Message::HomeScrolled(viewport) => {
                let rel_y = viewport.relative_offset().y;
                let mut tasks = Vec::new();

                // Re-queue any evicted thumbnails around the current scroll region so cards never stay blank
                let total = self.home_videos.len();
                if total > 0 {
                    let center_idx = ((rel_y.clamp(0.0, 1.0) * (total as f32)) as usize).min(total.saturating_sub(1));
                    let start_idx = center_idx.saturating_sub(18);
                    let end_idx = (center_idx + 24).min(total);
                    let slice_urls: Vec<(String, String)> = self.home_videos[start_idx..end_idx]
                        .iter()
                        .map(|v| (v.thumbnail_url.clone(), v.channel_avatar.clone()))
                        .collect();
                    for (t_url, a_url) in slice_urls {
                        if let Some(t) = self.queue_thumbnail_download(&t_url) {
                            tasks.push(t);
                        }
                        if !a_url.is_empty() {
                            if let Some(t) = self.queue_thumbnail_download(&a_url) {
                                tasks.push(t);
                            }
                        }
                    }
                }

                if rel_y > 0.72 && !self.is_loading_more_feed && !self.home_videos.is_empty() {
                    self.is_loading_more_feed = true;
                    let it = self.innertube.clone();
                    tasks.push(Task::perform(
                        async move { it.fetch_more_feed().await.map_err(|e| e.to_string()) },
                        Message::MoreFeedLoaded,
                    ));
                }
                if !tasks.is_empty() {
                    return Task::batch(tasks);
                }
                Task::none()
            }

            Message::MoreFeedLoaded(res) => {
                self.is_loading_more_feed = false;
                if let Ok(new_videos) = res {
                    if !new_videos.is_empty() {
                        let existing_ids: std::collections::HashSet<String> =
                            self.home_videos.iter().map(|v| v.video_id.clone()).collect();
                        let mut added = Vec::new();
                        for v in new_videos {
                            if !existing_ids.contains(&v.video_id) && !added.iter().any(|a: &VideoCard| a.video_id == v.video_id) {
                                added.push(v);
                            }
                        }
                        if !added.is_empty() {
                            let mut tasks = Vec::new();
                            for v in &added {
                                if let Some(t) = self.queue_thumbnail_download(&v.thumbnail_url) {
                                    tasks.push(t);
                                }
                                if !v.channel_avatar.is_empty() {
                                    if let Some(t) = self.queue_thumbnail_download(&v.channel_avatar) {
                                        tasks.push(t);
                                    }
                                }
                            }
                            self.home_videos.extend(added.clone());
                            if matches!(self.current_view, ViewMode::Home) && self.search_query.trim().is_empty() {
                                self.cached_home_feed.extend(added);
                            }
                            if !tasks.is_empty() {
                                return Task::batch(tasks);
                            }
                        }
                    }
                }
                Task::none()
            }

            Message::TabsScrolled(viewport) => {
                self.tabs_scroll_x = viewport.absolute_offset().x;
                let max_offset = (viewport.content_bounds().width - viewport.bounds().width).max(0.0);
                self.tabs_max_scroll_x = max_offset;
                Task::none()
            }

            Message::VideoHitboxClicked => {
                if self.osd_state.close_all_menus() {
                    if self.is_fullscreen {
                        self.sync_player_viewport();
                    }
                    return Task::none();
                }
                let now = std::time::Instant::now();
                if let Some(prev) = self.osd_state.last_click_time {
                    if now.duration_since(prev).as_millis() < 320 {
                        self.osd_state.last_click_time = None;
                        // Revert the first click's play/pause toggle
                        let _ = self.update(Message::PlayPauseToggle);
                        return self.update(Message::ToggleFullscreen);
                    }
                }
                self.osd_state.last_click_time = Some(now);
                self.update(Message::PlayPauseToggle)
            }

            Message::CloseOsdMenus => {
                if self.osd_state.close_all_menus() && self.is_fullscreen {
                    self.sync_player_viewport();
                }
                Task::none()
            }

            Message::ToggleSaveDefaultQuality => {
                self.osd_state.set_as_default_quality = !self.osd_state.set_as_default_quality;
                self.osd_state.on_mouse_move();
                Task::none()
            }

            Message::StartPipDrag => {
                self.last_pip_resize_edge = None;
                let now = std::time::Instant::now();
                if let Some(prev) = self.osd_state.last_click_time {
                    if now.duration_since(prev).as_millis() < 350 {
                        self.osd_state.last_click_time = None;
                        return self.update(Message::RestoreFromPip);
                    }
                }
                self.osd_state.last_click_time = Some(now);
                if let Some(pip_id) = self.pip_window_id {
                    return window::drag(pip_id);
                }
                Task::none()
            }

            Message::StartPipEdgeResize(edge) => {
                // Native compositor-driven resize (xdg_toplevel.resize), like Firefox PiP.
                self.last_pip_resize_edge = Some(edge);
                if let Some(pip_id) = self.pip_window_id {
                    use iced::window::ResizeEdge as E;
                    let e = match edge {
                        PipResizeEdge::Right => E::East,
                        PipResizeEdge::Left => E::West,
                        PipResizeEdge::Top => E::North,
                        PipResizeEdge::Bottom => E::South,
                        PipResizeEdge::TopRight => E::NorthEast,
                        PipResizeEdge::TopLeft => E::NorthWest,
                        PipResizeEdge::BottomRight => E::SouthEast,
                        PipResizeEdge::BottomLeft => E::SouthWest,
                    };
                    return window::drag_resize(pip_id, e);
                }
                Task::none()
            }

            Message::PlayPauseToggle => {
                if let ViewMode::Watch(ref view_vid) = self.current_view {
                    if self.playing_video_id.as_deref() != Some(view_vid.as_str()) {
                        let vid = view_vid.clone();
                        let close_pip_task = if self.pip_window_id.is_some() {
                            self.update(Message::ClosePip)
                        } else {
                            Task::none()
                        };

                        let details_opt = self
                            .watch_details
                            .as_ref()
                            .filter(|d| &d.video_id == &vid && !d.video_streams.is_empty())
                            .cloned()
                            .or_else(|| {
                                self.tab_details_cache
                                    .get(&vid)
                                    .filter(|d| !d.video_streams.is_empty())
                                    .cloned()
                            });

                        if let (Some(details), Some(player)) = (details_opt, self.player.clone()) {
                            self.snapshot_current_playing_frame();
                            if let Some(prev_vid) = self.playing_video_id.clone() {
                                let pos = player.get_position_ms();
                                let dur = player.get_duration_ms();
                                if pos > 1000 && (dur == 0 || pos + 5000 < dur) {
                                    for tab in &mut self.tabs {
                                        if tab.video_id == prev_vid {
                                            tab.saved_pos_ms = pos;
                                        }
                                    }
                                    self.config.saved_positions.insert(prev_vid, pos);
                                }
                            }

                            if let Some(stream) = select_best_stream(&details.video_streams, &self.config) {
                                let v_uri = stream.playback_uri("v");
                                let a_uri = select_best_audio_stream(&details.audio_streams, self.config.preferred_audio_lang).map(|s| s.playback_uri("a"));
                                let seek_target = self
                                    .tabs
                                    .iter()
                                    .find(|t| t.video_id == vid)
                                    .map(|t| t.saved_pos_ms)
                                    .filter(|&p| p > 0);

                                self.watch_details = Some(details);
                                self.playing_video_id = Some(vid.clone());
                                self.video_started_at = Some(std::time::Instant::now());
                                self.osd_state.is_playing = true;
                                self.osd_state.first_frame_rendered = false;
                                self.osd_state.has_playback_error = false;
                                self.seek_suppress_until = Some(std::time::Instant::now() + Duration::from_millis(800));

                                let _ = player.play_stream_with_seek(&v_uri, a_uri.as_deref(), seek_target);
                                self.sync_player_viewport();
                                return close_pip_task;
                            }
                        }

                        return Task::batch([close_pip_task, self.update(Message::OpenVideo(vid))]);
                    }
                }

                if let Some(player) = &self.player {
                    let is_play = player.toggle_playback();
                    self.osd_state.is_playing = is_play;
                    self.osd_state.play_pause_anim = Some((std::time::Instant::now(), is_play));
                    self.power_manager.set_screensaver_inhibit(is_play);
                }
                Task::none()
            }

            Message::Seek(target_secs) => {
                crate::diagnostics::session_seek_start(self.osd_state.current_pos_secs * 1000, target_secs * 1000);
                self.osd_state.close_all_menus();
                if let Some(player) = &self.player {
                    player.seek_ms(target_secs * 1000);
                    self.osd_state.current_pos_secs = target_secs;
                    self.seek_suppress_until = Some(std::time::Instant::now() + Duration::from_millis(800));
                }
                Task::none()
            }

            Message::SeekRelative(delta) => {
                if let Some(player) = &self.player {
                    let target_ms = player.seek_relative_secs(delta);
                    self.osd_state.current_pos_secs = target_ms / 1000;
                    self.seek_suppress_until = Some(std::time::Instant::now() + Duration::from_millis(800));
                    self.osd_state.trigger_seek_feedback(delta);
                }
                Task::none()
            }

            Message::VolumeChanged(vol) => {
                self.osd_state.volume = vol;
                self.config.saved_volume = Some(vol);
                self.config.is_muted = Some(vol <= 0.001);
                self.config.save_async();
                if let Some(player) = &self.player {
                    player.set_volume(vol as f64);
                }
                Task::none()
            }

            Message::ToggleMute => {
                if self.osd_state.volume > 0.001 {
                    self.saved_volume = self.osd_state.volume;
                    self.update(Message::VolumeChanged(0.0))
                } else {
                    let restore = if self.saved_volume > 0.001 { self.saved_volume } else { 1.0 };
                    self.update(Message::VolumeChanged(restore))
                }
            }

            Message::ToggleQualityMenu => {
                self.osd_state.show_qualities = !self.osd_state.show_qualities;
                if self.osd_state.show_qualities {
                    self.osd_state.show_audio_menu = false;
                    self.osd_state.show_speed_menu = false;
                    self.osd_state.show_subtitles_menu = false;
                }
                self.osd_state.on_mouse_move();
                if self.is_fullscreen {
                    self.sync_player_viewport();
                }
                Task::none()
            }

            Message::SelectQuality(itag) => {
                self.osd_state.selected_quality_itag = itag;
                self.osd_state.show_qualities = false;
                self.osd_state.first_frame_rendered = false;

                if let (Some(player), Some(details)) = (&self.player, &self.watch_details) {
                    if let Some(quality) = details.video_streams.iter().find(|s| s.itag == itag) {
                        let (codec_tag, pref_codec) = if quality.codecs.starts_with("av01") {
                            ("AV1", PreferredCodec::Av1)
                        } else if quality.codecs.starts_with("vp9") || quality.codecs.starts_with("vp09") {
                            ("VP9", PreferredCodec::Vp9)
                        } else {
                            ("H264", PreferredCodec::H264)
                        };
                        self.osd_state.selected_quality_label = format!("{} ({})", quality.quality_label, codec_tag);

                        if self.osd_state.set_as_default_quality {
                            let pref_res = if quality.height >= 2000 {
                                PreferredResolution::Res4K
                            } else if quality.height >= 1300 {
                                PreferredResolution::Res1440p
                            } else if quality.height >= 950 {
                                PreferredResolution::Res1080p
                            } else {
                                PreferredResolution::Res720p
                            };
                            self.config.preferred_codec = pref_codec;
                            self.config.preferred_resolution = pref_res;
                            self.osd_state.preferred_codec = pref_codec;
                            self.osd_state.preferred_resolution = pref_res;
                            let config_clone = self.config.clone();
                            std::thread::Builder::new()
                                .name("save-default-qual".into())
                                .spawn(move || config_clone.save())
                                .ok();
                        }

                        let video_uri = quality.playback_uri("v");
                        let audio_uri = details
                            .audio_streams
                            .iter()
                            .find(|a| a.itag == self.osd_state.selected_audio_itag && a.audio_track_id == self.osd_state.selected_audio_track_id)
                            .or_else(|| select_best_audio_stream(&details.audio_streams, self.config.preferred_audio_lang))
                            .map(|s| s.playback_uri("a"));
                        player.set_stream_buffer_profile(quality.height, self.power_profile);
                        let _ = player.switch_resolution(&video_uri, audio_uri.as_deref());
                        self.sync_player_viewport();
                    }
                }
                Task::none()
            }

            Message::ToggleAudioMenu => {
                self.osd_state.show_audio_menu = !self.osd_state.show_audio_menu;
                if self.osd_state.show_audio_menu {
                    self.osd_state.show_qualities = false;
                    self.osd_state.show_speed_menu = false;
                    self.osd_state.show_subtitles_menu = false;
                }
                self.osd_state.on_mouse_move();
                if self.is_fullscreen {
                    self.sync_player_viewport();
                }
                Task::none()
            }

            Message::SelectAudioTrack(idx) => {
                self.osd_state.show_audio_menu = false;

                if let (Some(player), Some(details)) = (&self.player, &self.watch_details) {
                    if let Some(audio) = details.audio_streams.get(idx) {
                        self.osd_state.selected_audio_itag = audio.itag;
                        self.osd_state.selected_audio_track_id = audio.audio_track_id.clone();
                        self.osd_state.selected_audio_label = audio.audio_track_name.clone();

                        let audio_uri = audio.playback_uri("a");
                        let video_uri = details
                            .video_streams
                            .iter()
                            .find(|v| v.itag == self.osd_state.selected_quality_itag)
                            .or_else(|| details.video_streams.first())
                            .map(|v| v.playback_uri("v"))
                            .unwrap_or_default();

                        if !video_uri.is_empty() {
                            let _ = player.switch_resolution(&video_uri, Some(&audio_uri));
                            self.sync_player_viewport();
                        }
                    }
                }
                Task::none()
            }

            Message::ToggleSpeedMenu => {
                self.osd_state.show_speed_menu = !self.osd_state.show_speed_menu;
                if self.osd_state.show_speed_menu {
                    self.osd_state.show_qualities = false;
                    self.osd_state.show_audio_menu = false;
                    self.osd_state.show_subtitles_menu = false;
                }
                self.osd_state.on_mouse_move();
                if self.is_fullscreen {
                    self.sync_player_viewport();
                }
                Task::none()
            }

            Message::ChangePlaybackRate(rate) => {
                self.osd_state.playback_rate = rate;
                self.osd_state.show_speed_menu = false;
                if let Some(player) = &self.player {
                    player.set_playback_rate(rate);
                }
                if self.is_fullscreen {
                    self.sync_player_viewport();
                }
                Task::none()
            }

            Message::ToggleCaptions => {
                self.osd_state.show_subtitles_menu = !self.osd_state.show_subtitles_menu;
                if self.osd_state.show_subtitles_menu {
                    self.osd_state.show_qualities = false;
                    self.osd_state.show_audio_menu = false;
                    self.osd_state.show_speed_menu = false;
                }
                self.osd_state.on_mouse_move();
                if self.is_fullscreen {
                    self.sync_player_viewport();
                }
                Task::none()
            }

            Message::SelectCaptionTrack(opt_idx) => {
                self.osd_state.show_subtitles_menu = false;
                if self.is_fullscreen {
                    self.sync_player_viewport();
                }

                match opt_idx {
                    None => {
                        self.osd_state.selected_caption_lang = None;
                        self.osd_state.current_subtitle_text.clear();
                        self.active_subtitles.clear();
                        self.config.subtitles_enabled = false;
                        self.config.save_async();
                        Task::none()
                    }
                    Some(idx) => {
                        if let Some(details) = &self.watch_details {
                            if let Some(track) = details.caption_tracks.get(idx) {
                                self.osd_state.selected_caption_lang = Some(track.name.clone());
                                self.config.subtitles_enabled = true;
                                self.config.preferred_caption_lang = Some(track.language_code.clone());
                                self.config.save_async();
                                let base_url = track.base_url.clone();
                                let it = self.innertube.clone();
                                let vid = details.video_id.clone();
                                return Task::perform(
                                    async move {
                                        let cues = it.fetch_captions(&base_url).await.map_err(|e| e.to_string());
                                        (vid, cues)
                                    },
                                    |(vid, res)| Message::CaptionsLoaded(vid, res),
                                );
                            }
                        }
                        Task::none()
                    }
                }
            }

            Message::CaptionsLoaded(vid, res) => {
                if self.playing_video_id.as_deref() != Some(&vid) {
                    return Task::none();
                }
                match res {
                    Ok(cues) => {
                        self.active_subtitles = cues;
                    }
                    Err(e) => {
                        eprintln!("[Captions] Error loading captions: {e}");
                        self.osd_state.selected_caption_lang = None;
                    }
                }
                Task::none()
            }

            Message::ToggleTheater => {
                self.theater_mode = !self.theater_mode;
                self.config.theater_mode = self.theater_mode;
                self.config.save_async();
                self.sync_player_viewport();
                Task::none()
            }

            Message::ToggleFullscreen => {
                let was_fullscreen = self.is_fullscreen;
                let initial_rect = if was_fullscreen {
                    (0.0, 0.0, self.window_size.0, self.window_size.1)
                } else {
                    self.get_windowed_player_rect()
                };

                self.is_fullscreen = !self.is_fullscreen;
                self.osd_state.on_mouse_move();

                self.fullscreen_anim = Some((
                    std::time::Instant::now(),
                    self.is_fullscreen,
                    initial_rect,
                ));

                if self.is_fullscreen {
                    self.current_view = ViewMode::VideoFullscreen;
                    self.sync_player_viewport();
                    if let Some(id) = self.main_window_id {
                        window::change_mode(id, Mode::Fullscreen)
                    } else {
                        Task::none()
                    }
                } else {
                    if let ViewMode::VideoFullscreen = self.current_view {
                        if let Some(details) = &self.watch_details {
                            self.current_view = ViewMode::Watch(details.video_id.clone());
                        } else {
                            self.current_view = ViewMode::Home;
                        }
                    }
                    self.sync_player_viewport();
                    if let Some(id) = self.main_window_id {
                        window::change_mode(id, Mode::Windowed)
                    } else {
                        Task::none()
                    }
                }
            }

            Message::TogglePip => {
                if self.pip_window_id.is_some() {
                    return self.update(Message::ClosePip);
                }

                let pip_details = self
                    .playing_video_id
                    .as_ref()
                    .and_then(|vid| {
                        self.watch_details
                            .as_ref()
                            .filter(|d| &d.video_id == vid)
                            .cloned()
                            .or_else(|| self.tab_details_cache.get(vid).cloned())
                    })
                    .or_else(|| self.watch_details.clone());

                let Some(details) = pip_details else {
                    return Task::none();
                };

                self.pip_watch_details = Some(details.clone());
                self.last_pip_resize_edge = None;

                let ratio = self.current_video_aspect_ratio();
                let (base_w, base_h) = if let Some((saved_w, saved_h)) = self.config.pip_size {
                    if ratio < 1.0 {
                        let h = saved_h.max(saved_w).clamp(240.0, 1080.0);
                        (h * ratio, h)
                    } else {
                        let w = saved_w.max(saved_h).clamp(240.0, 1920.0);
                        (w, w / ratio)
                    }
                } else if ratio < 1.0 {
                    (240.0, (240.0 / ratio).round())
                } else {
                    (480.0, (480.0 / ratio).round())
                };

                let (final_pip_w, final_pip_h) = self.compute_pip_aspect_size(base_w, base_h, None);

                self.pip_window_size = (final_pip_w, final_pip_h);
                self.pip_cursor_hovered = true;

                if let Some(player) = &self.player {
                    player.set_underlay(true);
                    player.set_viewport(0, 0, final_pip_w as i32, final_pip_h as i32);
                    player.set_rendering_active(true);
                }

                let initial_pos = self.config.pip_position.or_else(get_kwin_pip_position);
                let pip_settings = Settings {
                    position: if let Some((x, y)) = initial_pos {
                        window::Position::Specific(iced::Point::new(x, y))
                    } else {
                        window::Position::Centered
                    },
                    size: Size::new(final_pip_w, final_pip_h),
                    min_size: Some(Size::new(135.0, 135.0)),
                    transparent: true,
                    resizable: true,
                    decorations: false,
                    level: window::Level::AlwaysOnTop,
                    icon: load_app_icon(),
                    ..Settings::default()
                };

                let (pip_id, open_task) = window::open(pip_settings);
                self.pip_window_id = Some(pip_id);
                self.focused_window = Some(pip_id);
                self.is_window_focused = true;

                let disp_addr = self.main_window_handles.0;
                let handle_task = open_task
                    .then(move |pip_id| {
                        window::run_with_handle(pip_id, move |handle| {
                            let surf_addr = match handle.as_raw() {
                                RawWindowHandle::Wayland(h) => h.surface.as_ptr() as usize,
                                _ => 0usize,
                            };
                            (disp_addr, surf_addr)
                        })
                    })
                    .map(|(disp, surf)| Message::PipHandleAcquired(disp, surf));

                let focus_task = window::gain_focus(pip_id);
                let mut tasks = vec![handle_task, focus_task];
                if self.is_fullscreen {
                    self.is_fullscreen = false;
                    if let Some(details) = &self.watch_details {
                        self.current_view = ViewMode::Watch(details.video_id.clone());
                    }
                    if let Some(id) = self.main_window_id {
                        tasks.push(window::change_mode(id, Mode::Windowed));
                    }
                }
                return Task::batch(tasks);
            }

            Message::OpenSubscriptions => {
                self.snapshot_current_playing_frame();
                if let Some(details) = &self.watch_details {
                    if !details.video_streams.is_empty() {
                        self.tab_details_cache.insert(details.video_id.clone(), details.clone());
                    }
                }
                self.show_auth_modal = false;
                self.previous_view = self.current_view.clone();
                self.current_view = ViewMode::Subscriptions;
                self.active_tab_idx = None;
                self.trigger_blur_loading();
                self.show_suggestions = false;
                self.sync_player_viewport();

                let scroll_top = self.reset_home_scroll_task();
                let it = self.innertube.clone();
                let sub_task = Task::perform(
                    async move { it.get_subscriptions_feed().await.map_err(|e| e.to_string()) },
                    Message::SubscriptionsLoaded,
                );
                return Task::batch([sub_task, scroll_top]);
            }

            Message::SubscriptionsLoaded(res) => {
                self.trigger_unblur_transition();
                if let Ok(r) = self.auth_session.try_read() {
                    if r.is_authenticated() {
                        self.auth_status = "[OK] Sesion activa de YouTube".to_string();
                    }
                }
                match res {
                    Ok(videos) => {
                        if !matches!(self.current_view, ViewMode::Subscriptions) {
                            return Task::none();
                        }
                        self.trigger_unblur_transition();
                        let urls: Vec<(String, String)> = videos
                            .iter()
                            .map(|v| (v.thumbnail_url.clone(), v.channel_avatar.clone()))
                            .collect();
                        self.home_videos = videos;
                        let mut tasks = Vec::new();
                        for (t_url, a_url) in urls {
                            if let Some(t) = self.queue_thumbnail_download(&t_url) {
                                tasks.push(t);
                            }
                            if !a_url.is_empty() {
                                if let Some(t) = self.queue_thumbnail_download(&a_url) {
                                    tasks.push(t);
                                }
                            }
                        }
                        if !tasks.is_empty() {
                            return Task::batch(tasks);
                        }
                    }
                    Err(err) => {
                        eprintln!("[Mitub] Error al cargar suscripciones de YouTube: {err}");
                    }
                }
                Task::none()
            }

            Message::OpenHistory => {
                self.snapshot_current_playing_frame();
                if let Some(details) = &self.watch_details {
                    if !details.video_streams.is_empty() {
                        self.tab_details_cache.insert(details.video_id.clone(), details.clone());
                    }
                }
                self.show_auth_modal = false;
                self.previous_view = self.current_view.clone();
                self.current_view = ViewMode::History;
                self.active_tab_idx = None;
                self.trigger_blur_loading();
                self.show_suggestions = false;
                self.sync_player_viewport();

                let scroll_top = self.reset_home_scroll_task();
                let it = self.innertube.clone();
                let hist_task = Task::perform(
                    async move { it.get_history().await.map_err(|e| e.to_string()) },
                    Message::HistoryLoaded,
                );
                return Task::batch([hist_task, scroll_top]);
            }

            Message::HistoryLoaded(res) => {
                self.trigger_unblur_transition();
                if let Ok(r) = self.auth_session.try_read() {
                    if r.is_authenticated() {
                        self.auth_status = "[OK] Sesion activa de YouTube".to_string();
                    }
                }
                match res {
                    Ok(mut videos) => {
                        if !matches!(self.current_view, ViewMode::History) {
                            return Task::none();
                        }
                        self.trigger_unblur_transition();
                        // Merge recent local history items at the top if not yet indexed by YouTube FEhistory
                        let mut prepended = Vec::new();
                        for h in self.config.local_history.iter().take(15) {
                            if !videos.iter().any(|v| v.video_id == h.video_id) {
                                prepended.push(VideoCard {
                                    video_id: h.video_id.clone(),
                                    title: h.title.clone(),
                                    channel_name: h.channel_name.clone(),
                                    channel_id: h.channel_id.clone(),
                                    channel_avatar: h.channel_avatar.clone(),
                                    views_str: "Visto recientemente".to_string(),
                                    published_str: String::new(),
                                    duration_str: h.duration_str.clone(),
                                    thumbnail_url: h.thumbnail_url.clone(),
                                    is_live: false,
                                    meta_suffix: " • Visto recientemente".to_string(),
                                });
                            }
                        }
                        if !prepended.is_empty() {
                            prepended.append(&mut videos);
                            videos = prepended;
                        }

                        if !videos.is_empty() {
                            let urls: Vec<(String, String)> = videos
                                .iter()
                                .map(|v| (v.thumbnail_url.clone(), v.channel_avatar.clone()))
                                .collect();
                            self.home_videos = videos;
                            let mut tasks = Vec::new();
                            for (t_url, a_url) in urls {
                                if let Some(t) = self.queue_thumbnail_download(&t_url) {
                                    tasks.push(t);
                                }
                                if !a_url.is_empty() {
                                    if let Some(t) = self.queue_thumbnail_download(&a_url) {
                                        tasks.push(t);
                                    }
                                }
                            }
                            if !tasks.is_empty() {
                                return Task::batch(tasks);
                            }
                        }
                    }
                    Err(err) => {
                        eprintln!("[Mitub] Error al cargar historial de la cuenta de YouTube (usando respaldo local): {err}");
                        if !self.config.local_history.is_empty() {
                            self.home_videos = self
                                .config
                                .local_history
                                .iter()
                                .map(|h| VideoCard {
                                    video_id: h.video_id.clone(),
                                    title: h.title.clone(),
                                    channel_name: h.channel_name.clone(),
                                    channel_id: h.channel_id.clone(),
                                    channel_avatar: h.channel_avatar.clone(),
                                    views_str: String::new(),
                                    published_str: String::new(),
                                    duration_str: h.duration_str.clone(),
                                    thumbnail_url: h.thumbnail_url.clone(),
                                    is_live: false,
                                    meta_suffix: String::new(),
                                })
                                .collect();
                        }
                    }
                }
                Task::none()
            }

            Message::OpenPlaylists => {
                self.snapshot_current_playing_frame();
                if let Some(details) = &self.watch_details {
                    if !details.video_streams.is_empty() {
                        self.tab_details_cache.insert(details.video_id.clone(), details.clone());
                    }
                }
                self.show_auth_modal = false;
                self.previous_view = self.current_view.clone();
                self.current_view = ViewMode::Playlists;
                self.active_tab_idx = None;
                self.trigger_blur_loading();
                self.show_suggestions = false;
                self.sync_player_viewport();

                let scroll_top = self.reset_home_scroll_task();
                let it = self.innertube.clone();
                let pl_task = Task::perform(
                    async move { it.get_playlists_feed().await.map_err(|e| e.to_string()) },
                    Message::PlaylistsLoaded,
                );
                return Task::batch([pl_task, scroll_top]);
            }

            Message::PlaylistsLoaded(res) => {
                self.trigger_unblur_transition();
                if let Ok(r) = self.auth_session.try_read() {
                    if r.is_authenticated() {
                        self.auth_status = "[OK] Sesion activa de YouTube".to_string();
                    }
                }
                match res {
                    Ok(videos) => {
                        if !matches!(self.current_view, ViewMode::Playlists) {
                            return Task::none();
                        }
                        self.trigger_unblur_transition();
                        let urls: Vec<(String, String)> = videos
                            .iter()
                            .map(|v| (v.thumbnail_url.clone(), v.channel_avatar.clone()))
                            .collect();
                        self.home_videos = videos;
                        let mut tasks = Vec::new();
                        for (t_url, a_url) in urls {
                            if let Some(t) = self.queue_thumbnail_download(&t_url) {
                                tasks.push(t);
                            }
                            if !a_url.is_empty() {
                                if let Some(t) = self.queue_thumbnail_download(&a_url) {
                                    tasks.push(t);
                                }
                            }
                        }
                        if !tasks.is_empty() {
                            return Task::batch(tasks);
                        }
                    }
                    Err(err) => {
                        eprintln!("[Mitub] Error al cargar playlists de YouTube: {err}");
                    }
                }
                Task::none()
            }

            Message::UserProfileLoaded(url_opt, name_opt) => {
                if let Some(name) = name_opt {
                    self.user_display_name = Some(name.clone());
                    self.config.cached_user_name = Some(name);
                    self.config.save_async();
                }
                if let Some(url) = url_opt {
                    self.user_avatar_url = Some(url.clone());
                    self.config.cached_avatar_url = Some(url.clone());
                    self.config.save_async();
                    let it = self.innertube.clone();
                    return Task::perform(
                        async move {
                            if let Some(bytes) = it.fetch_thumbnail_bytes(&url).await {
                                tokio::task::spawn_blocking(move || {
                                    if let Ok(img) = image_crate::load_from_memory(&bytes) {
                                        let resized = img.thumbnail(96, 96);
                                        let rgba = resized.to_rgba8();
                                        let (w, h) = rgba.dimensions();
                                        Some(image::Handle::from_rgba(w, h, rgba.into_raw()))
                                    } else {
                                        None
                                    }
                                })
                                .await
                                .ok()
                                .flatten()
                            } else {
                                None
                            }
                        },
                        |opt_handle| {
                            if let Some(handle) = opt_handle {
                                Message::UserAvatarDownloaded(handle)
                            } else {
                                Message::Tick
                            }
                        },
                    );
                }
                Task::none()
            }

            Message::UserAvatarDownloaded(handle) => {
                self.user_avatar_handle = Some(handle);
                Task::none()
            }

            Message::AddToQueue(video) => {
                if !self.playback_queue.iter().any(|v| v.video_id == video.video_id) {
                    self.playback_queue.push(video);
                }
                Task::none()
            }

            Message::RemoveFromQueue(idx) => {
                if idx < self.playback_queue.len() {
                    self.playback_queue.remove(idx);
                }
                Task::none()
            }

            Message::ClearQueue => {
                self.playback_queue.clear();
                Task::none()
            }

            Message::ToggleAutoplay => {
                self.autoplay_next = !self.autoplay_next;
                self.config.autoplay_next = self.autoplay_next;
                self.config.save_async();
                Task::none()
            }

            Message::SetPreferredResolution(res) => {
                self.config.preferred_resolution = res;
                self.osd_state.preferred_resolution = res;
                self.config.save_async();
                if let Some(best_itag) = self.watch_details.as_ref().and_then(|details| {
                    select_best_stream(&details.video_streams, &self.config).map(|s| s.itag)
                }) {
                    return self.update(Message::SelectQuality(best_itag));
                }
                Task::none()
            }

            Message::SetPreferredCodec(codec) => {
                self.config.preferred_codec = codec;
                self.osd_state.preferred_codec = codec;
                self.config.save_async();
                if let Some(best_itag) = self.watch_details.as_ref().and_then(|details| {
                    select_best_stream(&details.video_streams, &self.config).map(|s| s.itag)
                }) {
                    return self.update(Message::SelectQuality(best_itag));
                }
                Task::none()
            }

            Message::SetPreferredAudioLanguage(pref) => {
                self.config.preferred_audio_lang = pref;
                self.config.save_async();
                if let Some(details) = &self.watch_details {
                    if let Some(best) = select_best_audio_stream(&details.audio_streams, pref) {
                        if let Some(idx) = details.audio_streams.iter().position(|s| s.audio_track_id == best.audio_track_id && s.itag == best.itag) {
                            return self.update(Message::SelectAudioTrack(idx));
                        }
                    }
                }
                Task::none()
            }

            Message::ToggleAutoPipOnUnfocus => {
                self.config.auto_pip_on_unfocus = !self.config.auto_pip_on_unfocus;
                self.config.save_async();
                Task::none()
            }

            Message::ScalePip(scale) => {
                if let Some(pip_id) = self.pip_window_id {
                    self.last_pip_resize_edge = None;
                    let (cur_w, cur_h) = self.pip_window_size;
                    let (new_w, new_h) = self.compute_pip_aspect_size(cur_w * scale, cur_h * scale, None);
                    self.pip_window_size = (new_w, new_h);
                    self.config.pip_size = Some((new_w, new_h));
                    self.config.save_async();
                    if let Some(player) = &self.player {
                        player.set_underlay(true);
                        player.set_viewport(0, 0, new_w as i32, new_h as i32);
                    }
                    return window::resize(pip_id, Size::new(new_w, new_h));
                }
                Task::none()
            }

            Message::OpenUrl(url) => {
                let video_id_opt = extract_youtube_video_id(&url);

                if let Some(vid) = video_id_opt {
                    if !vid.is_empty() {
                        return self.update(Message::OpenVideo(vid));
                    }
                }

                let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
                Task::none()
            }

            Message::IpcUrlReceived(raw_url) => {
                let video_id_opt = extract_youtube_video_id(&raw_url).or_else(|| {
                    let trimmed = raw_url.trim();
                    if trimmed.len() >= 10 && trimmed.len() <= 12 && trimmed.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                        Some(trimmed.to_string())
                    } else {
                        None
                    }
                });

                let mut tasks = Vec::new();
                if let Some(main_id) = self.main_window_id {
                    self.focused_window = Some(main_id);
                    self.is_window_focused = true;
                    self.is_window_minimized = false;
                    tasks.push(window::minimize(main_id, false));
                    tasks.push(window::gain_focus(main_id));
                }

                if let Some(vid) = video_id_opt {
                    tasks.push(self.update(Message::OpenVideoInNewTab(vid)));
                }
                Task::batch(tasks)
            }

            Message::CopyToClipboard(text) => {
                let text_clone = text.clone();
                tokio::task::spawn_blocking(move || {
                    use std::io::Write;
                    if let Ok(mut child) = std::process::Command::new("wl-copy")
                        .stdin(std::process::Stdio::piped())
                        .spawn()
                    {
                        if let Some(mut stdin) = child.stdin.take() {
                            let _ = stdin.write_all(text_clone.as_bytes());
                        }
                        let _ = child.wait();
                    }
                });
                let toast_text = if text.contains("youtu.be") || text.contains("youtube.com") {
                    "Enlace copiado al portapapeles".to_string()
                } else {
                    "Copiado al portapapeles".to_string()
                };
                self.toast_message = Some((toast_text, std::time::Instant::now()));
                iced::clipboard::write(text)
            }

            Message::CopyShareUrl => {
                if let Some(details) = &self.watch_details {
                    let url = format!("https://youtu.be/{}", details.video_id);
                    return self.update(Message::CopyToClipboard(url));
                }
                Task::none()
            }

            Message::CopyDescription => {
                if let Some(details) = &self.watch_details {
                    let desc = details.description.clone();
                    if !desc.is_empty() {
                        return self.update(Message::CopyToClipboard(desc));
                    }
                }
                Task::none()
            }

            Message::CopyComment(idx) => {
                if let Some(details) = &self.watch_details {
                    if let Some(c) = details.comments.get(idx) {
                        let text = c.text.clone();
                        if !text.is_empty() {
                            return self.update(Message::CopyToClipboard(text));
                        }
                    }
                }
                Task::none()
            }

            Message::ToggleDescription => {
                self.description_expanded = !self.description_expanded;
                if let Some(details) = &self.watch_details {
                    let desc_text = if self.description_expanded || details.description.chars().count() <= 240 {
                        details.description.clone()
                    } else {
                        let truncated: String = details.description.chars().take(240).collect();
                        format!("{truncated}...")
                    };
                    self.description_content = iced::widget::text_editor::Content::with_text(&desc_text);
                }
                Task::none()
            }

            Message::DescriptionAction(action) => {
                match action {
                    iced::widget::text_editor::Action::Scroll { lines } => {
                        return iced::widget::scrollable::scroll_by(
                            iced::widget::scrollable::Id::new("watch_scrollable"),
                            iced::widget::scrollable::AbsoluteOffset {
                                x: 0.0,
                                y: (lines as f32) * 28.0,
                            },
                        );
                    }
                    action => {
                        if !action.is_edit() {
                            self.description_content.perform(action);
                        }
                    }
                }
                Task::none()
            }

            Message::CommentAction(_idx, _action) => {
                Task::none()
            }

            Message::RetryPlayback => {
                self.osd_state.has_playback_error = false;
                self.osd_state.first_frame_rendered = false;
                self.osd_state.is_buffering = false;
                if let (Some(details), Some(player)) = (&self.watch_details, &self.player) {
                    let stream = details
                        .video_streams
                        .iter()
                        .find(|s| s.codecs.starts_with("avc1") && s.height <= 1080)
                        .or_else(|| details.video_streams.first());

                    if let Some(stream) = stream {
                        let v_uri = stream.playback_uri("v");
                        let a_uri = select_best_audio_stream(&details.audio_streams, self.config.preferred_audio_lang).map(|s| s.playback_uri("a"));
                        self.video_started_at = Some(std::time::Instant::now());
                        let seek_pos = if self.osd_state.current_pos_secs >= 1 {
                            Some(self.osd_state.current_pos_secs * 1000)
                        } else {
                            None
                        };
                        if let Err(e) = player.play_stream_with_seek(&v_uri, a_uri.as_deref(), seek_pos) {
                            eprintln!("[RetryPlayback Error] failed to launch stream: {e}");
                            self.osd_state.has_playback_error = true;
                        }
                    }
                }
                Task::none()
            }

            Message::ToggleChat => {
                self.chat_collapsed = !self.chat_collapsed;
                Task::none()
            }

            Message::ToggleSubscribe => {
                let target = if matches!(self.current_view, ViewMode::Channel(_)) {
                    self.channel_details.as_mut().map(|c| {
                        let new_state = !c.is_subscribed;
                        c.is_subscribed = new_state;
                        (c.channel_id.clone(), new_state)
                    })
                } else {
                    self.watch_details.as_mut().map(|d| {
                        let mut_d = Arc::make_mut(d);
                        let new_state = !mut_d.is_subscribed;
                        mut_d.is_subscribed = new_state;
                        (mut_d.channel_id.clone(), new_state)
                    })
                };

                if let Some((channel_id, new_state)) = target {
                    if let Some(d) = &mut self.watch_details {
                        if d.channel_id == channel_id {
                            Arc::make_mut(d).is_subscribed = new_state;
                        }
                    }
                    if let Some(c) = &mut self.channel_details {
                        if c.channel_id == channel_id {
                            c.is_subscribed = new_state;
                        }
                    }
                    if !channel_id.is_empty() {
                        let it = self.innertube.clone();
                        return Task::perform(
                            async move {
                                if new_state {
                                    it.subscribe_channel(&channel_id).await.map(|_| true).map_err(|e| e.to_string())
                                } else {
                                    it.unsubscribe_channel(&channel_id).await.map(|_| false).map_err(|e| e.to_string())
                                }
                            },
                            Message::SubscribedResult,
                        );
                    }
                }
                Task::none()
            }

            Message::SubscribedResult(res) => {
                match res {
                    Ok(state) => {
                        if matches!(self.current_view, ViewMode::Channel(_)) {
                            if let Some(c) = &mut self.channel_details {
                                c.is_subscribed = state;
                            }
                        } else if let Some(details) = &mut self.watch_details {
                            Arc::make_mut(details).is_subscribed = state;
                        }
                    }
                    Err(e) => {
                        eprintln!("[Mitub] Error al cambiar suscripcion: {e}");
                        if matches!(self.current_view, ViewMode::Channel(_)) {
                            if let Some(c) = &mut self.channel_details {
                                c.is_subscribed = !c.is_subscribed;
                            }
                        } else if let Some(details) = &mut self.watch_details {
                            let mut_d = Arc::make_mut(details);
                            mut_d.is_subscribed = !mut_d.is_subscribed;
                        }
                    }
                }
                Task::none()
            }

            Message::LikeVideo => {
                if let Some(details) = &mut self.watch_details {
                    let vid_id = details.video_id.clone();
                    let current = details.like_status.clone();
                    let (new_status, action_type) = if current == "LIKE" {
                        ("INDIFFERENT".to_string(), 0)
                    } else {
                        ("LIKE".to_string(), 1)
                    };
                    Arc::make_mut(details).like_status = new_status;
                    let it = self.innertube.clone();
                    return Task::perform(
                        async move {
                            it.invalidate_watch_cache(&vid_id).await;
                            if action_type == 0 {
                                it.remove_like_video(&vid_id).await.map(|_| "INDIFFERENT".to_string()).map_err(|e| e.to_string())
                            } else {
                                it.like_video(&vid_id).await.map(|_| "LIKE".to_string()).map_err(|e| e.to_string())
                            }
                        },
                        Message::LikeStatusResult,
                    );
                }
                Task::none()
            }

            Message::DislikeVideo => {
                if let Some(details) = &mut self.watch_details {
                    let vid_id = details.video_id.clone();
                    let current = details.like_status.clone();
                    let (new_status, action_type) = if current == "DISLIKE" {
                        ("INDIFFERENT".to_string(), 0)
                    } else {
                        ("DISLIKE".to_string(), 2)
                    };
                    Arc::make_mut(details).like_status = new_status;
                    let it = self.innertube.clone();
                    return Task::perform(
                        async move {
                            it.invalidate_watch_cache(&vid_id).await;
                            if action_type == 0 {
                                it.remove_like_video(&vid_id).await.map(|_| "INDIFFERENT".to_string()).map_err(|e| e.to_string())
                            } else {
                                it.dislike_video(&vid_id).await.map(|_| "DISLIKE".to_string()).map_err(|e| e.to_string())
                            }
                        },
                        Message::LikeStatusResult,
                    );
                }
                Task::none()
            }

            Message::LikeStatusResult(res) => {
                match res {
                    Ok(st) => {
                        if let Some(details) = &mut self.watch_details {
                            Arc::make_mut(details).like_status = st;
                        }
                    }
                    Err(e) => {
                        eprintln!("[Mitub] Error al cambiar estado de like: {e}");
                    }
                }
                Task::none()
            }

            Message::OpenVideoInNewTab(video_id) => {
                if let Some(pos) = self.tabs.iter().position(|t| t.video_id == video_id) {
                    self.tab_shake_anim = Some((pos, std::time::Instant::now()));
                    let tab_x = pos as f32 * 145.0;
                    let target_scroll = (tab_x - 100.0).max(0.0);
                    self.tabs_scroll_x = target_scroll;
                    return iced::widget::scrollable::scroll_to(
                        iced::widget::scrollable::Id::new("tabs_scrollable"),
                        iced::widget::scrollable::AbsoluteOffset { x: target_scroll, y: 0.0 },
                    );
                }
                let matched_card = self.home_videos.iter()
                    .find(|v| v.video_id == video_id)
                    .or_else(|| {
                        self.watch_details.as_ref().and_then(|d| {
                            d.recommended_videos.iter().find(|v| v.video_id == video_id)
                        })
                    })
                    .or_else(|| {
                        self.channel_details.as_ref().and_then(|ch| {
                            ch.tabs.iter().flat_map(|t| t.videos.iter()).find(|v| v.video_id == video_id)
                        })
                    });

                let title = matched_card
                    .map(|v| v.title.clone())
                    .unwrap_or_else(|| "Video".to_string());

                let chan = matched_card
                    .map(|v| v.channel_name.clone())
                    .unwrap_or_default();

                let thumb_url = matched_card
                    .map(|v| v.thumbnail_url.clone())
                    .unwrap_or_default();

                let new_idx = self.tabs.len();
                self.tabs.push(ui::VideoTab {
                    video_id: video_id.clone(),
                    title,
                    channel_name: chan,
                    saved_pos_ms: 0,
                    thumbnail_url: thumb_url,
                });
                self.tab_opened_anim = Some((new_idx, std::time::Instant::now()));
                self.save_session();
                let target_x = (new_idx as f32 * 145.0).max(0.0);
                self.tabs_scroll_x = target_x;
                let scroll_task = iced::widget::scrollable::scroll_to(
                    iced::widget::scrollable::Id::new("tabs_scrollable"),
                    iced::widget::scrollable::AbsoluteOffset { x: target_x, y: 0.0 },
                );
                let it = self.innertube.clone();
                let prefetch_vid = video_id.clone();
                let prefetch_task = Task::perform(
                    async move { it.get_watch_details(&prefetch_vid).await.map_err(|e| e.to_string()) },
                    Message::WatchDetailsLoadedPassive,
                );

                if self.config.full_buffer_tabs {
                    Task::batch([self.update(Message::TriggerFullBuffer(video_id)), scroll_task, prefetch_task])
                } else {
                    Task::batch([scroll_task, prefetch_task])
                }
            }

            Message::SelectTab(idx) => {
                if idx < self.tabs.len() {
                    if let Some(cur_idx) = self.active_tab_idx {
                        if cur_idx == idx {
                            if let ViewMode::Watch(ref cur_vid) = self.current_view {
                                if cur_vid == &self.tabs[idx].video_id {
                                    if self.pip_window_id.is_some() {
                                        return self.update(Message::ClosePip);
                                    }
                                    return Task::none();
                                }
                            }
                        }
                    }

                    // Guardar posición de reproducción y snapshot del frame de la pestaña actual si estaba activa
                    self.snapshot_current_playing_frame();
                    if let Some(details) = &self.watch_details {
                        if !details.video_streams.is_empty() {
                            self.tab_details_cache.insert(details.video_id.clone(), details.clone());
                        }
                    }
                    if let (Some(cur_idx), Some(player)) = (self.active_tab_idx, &self.player) {
                        if cur_idx < self.tabs.len()
                            && player.is_first_frame_rendered()
                            && self.playing_video_id.as_deref() == Some(self.tabs[cur_idx].video_id.as_str())
                        {
                            let pos = player.get_position_ms();
                            let dur = player.get_duration_ms();
                            if pos > 1000 && (dur == 0 || pos + 5000 < dur) {
                                self.tabs[cur_idx].saved_pos_ms = pos;
                                self.config
                                    .saved_positions
                                    .insert(self.tabs[cur_idx].video_id.clone(), pos);
                            }
                        }
                    }

                    let is_currently_playing = self.osd_state.is_playing
                        && self.player.as_ref().map(|p| p.is_playing()).unwrap_or(false)
                        && matches!(self.current_view, ViewMode::Watch(_) | ViewMode::VideoFullscreen)
                        && self.watch_details.is_some();

                    self.active_tab_idx = Some(idx);
                    self.save_session();
                    let vid = self.tabs[idx].video_id.clone();
                    if vid.is_empty() {
                        let auto_pip_task = if is_currently_playing && self.pip_window_id.is_none() {
                            self.update(Message::TogglePip)
                        } else {
                            Task::none()
                        };

                        self.current_view = ViewMode::Home;
                        self.search_query.clear();
                        self.search_input_focused = true;
                        self.show_suggestions = false;
                        self.sync_player_viewport();
                        let focus_task = iced::widget::text_input::focus(iced::widget::text_input::Id::new("main_search_input"));
                        let scroll_top = self.reset_home_scroll_task();
                        return Task::batch([auto_pip_task, focus_task, scroll_top]);
                    } else {
                        // Si es el video que ya está reproduciéndose (ej. en background o PiP), retornar fluidamente
                        if self.playing_video_id.as_deref() == Some(vid.as_str()) {
                            let scroll_reset_task = self.reset_watch_scroll_task();
                            if self.pip_window_id.is_some() {
                                return Task::batch([self.update(Message::ClosePip), scroll_reset_task]);
                            } else {
                                self.previous_view = self.current_view.clone();
                                self.current_view = ViewMode::Watch(vid.clone());
                                if let Some(cached) = self.tab_details_cache.get(&vid) {
                                    self.watch_details = Some(Arc::clone(cached));
                                }
                                self.sync_player_viewport();
                                return scroll_reset_task;
                            }
                        } else if let Some(cached) = self.tab_details_cache.get(&vid).cloned() {
                            // Video en caché: restaurar de forma 100% síncrona en el mismo frame (0ms)
                            self.previous_view = self.current_view.clone();
                            self.current_view = ViewMode::Watch(vid.clone());
                            self.show_suggestions = false;
                            let scroll_reset_task = self.reset_watch_scroll_task();
                            self.active_subtitles.clear();

                            let desc_text = if self.description_expanded || cached.description.chars().count() <= 240 {
                                cached.description.clone()
                            } else {
                                let truncated: String = cached.description.chars().take(240).collect();
                                format!("{truncated}...")
                            };
                            self.description_content = iced::widget::text_editor::Content::with_text(&desc_text);

                            let best_stream = select_best_stream(&cached.video_streams, &self.config);
                            if let Some(stream) = best_stream {
                                let codec_tag = if stream.codecs.starts_with("av01") {
                                    "AV1"
                                } else if stream.codecs.starts_with("vp9") || stream.codecs.starts_with("vp09") {
                                    "VP9"
                                } else {
                                    "H264"
                                };
                                self.inactive_tab_osd_state.selected_quality_itag = stream.itag;
                                self.inactive_tab_osd_state.selected_quality_label = format!("{} ({})", stream.quality_label, codec_tag);
                                if stream.approx_duration_ms > 0 {
                                    self.inactive_tab_osd_state.duration_secs = stream.approx_duration_ms / 1000;
                                }
                            }
                            self.inactive_tab_osd_state.current_pos_secs = self.tabs[idx].saved_pos_ms / 1000;
                            self.inactive_tab_osd_state.volume = self.osd_state.volume;
                            self.inactive_tab_osd_state.is_playing = false;
                            self.inactive_tab_osd_state.first_frame_rendered = false;
                            self.inactive_tab_osd_state.is_buffering = false;
                            self.inactive_tab_osd_state.has_playback_error = false;

                            self.watch_details = Some(cached);
                            self.sync_player_viewport();
                            return scroll_reset_task;
                        } else {
                            // Video diferente: NO poner pausa al anterior ni reproducir automáticamente este,
                            // ni abrir ventana PiP forzada al cambiar de pestañas.
                            self.previous_view = self.current_view.clone();
                            self.current_view = ViewMode::Watch(vid.clone());
                            self.show_suggestions = false;
                            let scroll_reset_task = self.reset_watch_scroll_task();
                            self.active_subtitles.clear();

                            // Restaurar o preparar detalles para esta pestaña pasivamente
                            let tab_info = self.tabs[idx].clone();
                            let existing_recs = self
                                .cached_home_feed
                                .iter()
                                .take(10)
                                .cloned()
                                .collect();

                            self.inactive_tab_osd_state.is_playing = false;
                            self.inactive_tab_osd_state.first_frame_rendered = false;
                            self.inactive_tab_osd_state.is_buffering = false;
                            self.inactive_tab_osd_state.has_playback_error = false;
                            self.inactive_tab_osd_state.current_pos_secs = tab_info.saved_pos_ms / 1000;
                            self.inactive_tab_osd_state.volume = self.osd_state.volume;

                            self.watch_details = Some(Arc::new(WatchDetails {
                                video_id: vid.clone(),
                                title: tab_info.title.clone(),
                                channel_name: tab_info.channel_name.clone(),
                                channel_id: String::new(),
                                channel_avatar: String::new(),
                                subscriber_count: String::new(),
                                likes_str: String::new(),
                                dislikes_str: String::new(),
                                description: String::new(),
                                published_date: String::new(),
                                views_str: String::new(),
                                is_live: false,
                                is_subscribed: false,
                                like_status: "INDIFFERENT".to_string(),
                                live_chat_continuation: None,
                                recommended_videos: existing_recs,
                                comments: Vec::new(),
                                video_streams: Vec::new(),
                                audio_streams: Vec::new(),
                                caption_tracks: Vec::new(),
                                chapters: Vec::new(),
                                parsed_desc_urls: Vec::new(),
                                parsed_desc_timestamps: Vec::new(),
                            }));

                            let mut tasks = vec![scroll_reset_task];
                            if !tab_info.thumbnail_url.is_empty() {
                                if let Some(tt) = self.queue_thumbnail_download(&tab_info.thumbnail_url) {
                                    tasks.push(tt);
                                }
                            }
                            if let Some(tt) = self.queue_thumbnail_download(&format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", vid)) {
                                tasks.push(tt);
                            }

                            let it = self.innertube.clone();
                            let fetch_vid = vid.clone();
                            let fetch_task = Task::perform(
                                async move {
                                    it.get_watch_details(&fetch_vid).await.map_err(|e| e.to_string())
                                },
                                Message::WatchDetailsLoadedPassive,
                            );
                            tasks.push(fetch_task);

                            self.sync_player_viewport();
                            return Task::batch(tasks);
                        }
                    }
                }
                Task::none()
            }

            Message::ScrollTabsLeft => {
                self.tabs_scroll_x = (self.tabs_scroll_x - 160.0).max(0.0);
                iced::widget::scrollable::scroll_to(
                    iced::widget::scrollable::Id::new("tabs_scrollable"),
                    iced::widget::scrollable::AbsoluteOffset { x: self.tabs_scroll_x, y: 0.0 },
                )
            }

            Message::ScrollTabsRight => {
                let max_scroll = self.tabs_max_scroll_x.max(self.tabs.len() as f32 * 145.0);
                self.tabs_scroll_x = (self.tabs_scroll_x + 160.0).min(max_scroll);
                iced::widget::scrollable::scroll_to(
                    iced::widget::scrollable::Id::new("tabs_scrollable"),
                    iced::widget::scrollable::AbsoluteOffset { x: self.tabs_scroll_x, y: 0.0 },
                )
            }

            Message::RestoreClosedTab => {
                if let Some(tab) = self.closed_tabs_history.pop_back() {
                    let new_idx = self.tabs.len();
                    let vid = tab.video_id.clone();
                    self.tabs.push(tab);
                    self.active_tab_idx = Some(new_idx);
                    self.tab_opened_anim = Some((new_idx, std::time::Instant::now()));
                    self.save_session();

                    let target_x = (new_idx as f32 * 145.0).max(0.0);
                    self.tabs_scroll_x = target_x;
                    let scroll_task = iced::widget::scrollable::scroll_to(
                        iced::widget::scrollable::Id::new("tabs_scrollable"),
                        iced::widget::scrollable::AbsoluteOffset { x: target_x, y: 0.0 },
                    );

                    if !vid.is_empty() {
                        return Task::batch([self.update(Message::OpenVideo(vid)), scroll_task]);
                    } else {
                        return Task::batch([self.update(Message::GoHome), scroll_task]);
                    }
                }
                Task::none()
            }

            Message::OpenEmptyTab => {
                let is_currently_playing = self.osd_state.is_playing
                    && self.player.as_ref().map(|p| p.is_playing()).unwrap_or(false)
                    && matches!(self.current_view, ViewMode::Watch(_) | ViewMode::VideoFullscreen)
                    && self.watch_details.is_some();

                let pip_task = if is_currently_playing && self.pip_window_id.is_none() {
                    self.update(Message::TogglePip)
                } else {
                    Task::none()
                };

                let new_tab = ui::VideoTab {
                    video_id: String::new(),
                    title: "Nueva pestaña".to_string(),
                    channel_name: String::new(),
                    saved_pos_ms: 0,
                    thumbnail_url: String::new(),
                };
                self.tabs.push(new_tab);
                let new_idx = self.tabs.len() - 1;
                self.active_tab_idx = Some(new_idx);
                self.tab_opened_anim = Some((new_idx, std::time::Instant::now()));
                self.save_session();

                self.current_view = ViewMode::Home;
                self.search_query.clear();
                self.search_input_focused = true;
                self.show_suggestions = false;
                if self.pip_window_id.is_none() && !is_currently_playing {
                    self.watch_details = None;
                    if let Some(player) = &self.player {
                        player.stop();
                    }
                    self.power_manager.set_screensaver_inhibit(false);
                }
                self.sync_player_viewport();

                let focus_task = iced::widget::text_input::focus(iced::widget::text_input::Id::new("main_search_input"));
                let target_x = (new_idx as f32 * 145.0).max(0.0);
                self.tabs_scroll_x = target_x;
                let scroll_task = iced::widget::scrollable::scroll_to(
                    iced::widget::scrollable::Id::new("tabs_scrollable"),
                    iced::widget::scrollable::AbsoluteOffset { x: target_x, y: 0.0 },
                );
                return Task::batch([pip_task, focus_task, scroll_task]);
            }

            Message::WindowMaximized(is_max) => {
                self.is_maximized = is_max;
                if !self.is_fullscreen && self.pip_window_id.is_none() {
                    save_window_state(self.window_size.0, self.window_size.1, is_max);
                }
                Task::none()
            }

            Message::WindowMinimizedStatus(is_min) => {
                self.is_window_minimized = is_min;
                self.sync_player_viewport();
                Task::none()
            }

            Message::CloseTab(idx) => {
                if idx < self.tabs.len() {
                    let removed = self.tabs.remove(idx);
                    if self.playing_video_id.as_deref() == Some(removed.video_id.as_str()) {
                        if let Some(player) = &self.player {
                            player.stop();
                        }
                        self.playing_video_id = None;
                        self.video_started_at = None;
                        self.osd_state.is_playing = false;
                        self.power_manager.set_screensaver_inhibit(false);
                    }
                    // Prune cached frames, details, and posters if video is no longer open in any tab
                    let vid = &removed.video_id;
                    let is_still_in_open_tabs = self.tabs.iter().any(|t| &t.video_id == vid);
                    if !is_still_in_open_tabs {
                        self.tab_details_cache.remove(vid);
                        self.tab_cached_frames.remove(vid);
                        self.blurred_posters.remove(vid);
                        self.last_snapshot_frame_ids.remove(vid);
                    }
                    if self.tab_details_cache.len() > self.tabs.len() + 8 {
                        let open_vids: std::collections::HashSet<&str> =
                            self.tabs.iter().map(|t| t.video_id.as_str()).collect();
                        self.tab_details_cache.retain(|k, _| open_vids.contains(k.as_str()));
                    }

                    // Remember up to 5 recently closed tabs
                    if self.closed_tabs_history.len() >= 5 {
                        self.closed_tabs_history.pop_front();
                    }
                    self.closed_tabs_history.push_back(removed);

                    if self.tabs.is_empty() {
                        self.active_tab_idx = None;
                        self.save_session();
                        return self.update(Message::GoHome);
                    } else if let Some(active) = self.active_tab_idx {
                        if active == idx {
                            let new_idx = idx.min(self.tabs.len() - 1);
                            self.save_session();
                            return self.update(Message::SelectTab(new_idx));
                        } else if active > idx {
                            self.active_tab_idx = Some(active - 1);
                        }
                    }
                    self.save_session();
                    crate::power_manager::trim_process_memory();
                }
                Task::none()
            }

            Message::ToggleLoginModal => {
                self.show_auth_modal = !self.show_auth_modal;
                if self.show_auth_modal {
                    self.current_cache_size_bytes = crate::cache_manager::get_cache_size_bytes();
                }
                Task::none()
            }

            Message::CookieInputChanged(raw) => {
                self.auth_cookie_input = raw;
                Task::none()
            }

            Message::SaveRawCookie => {
                let session = AuthSession::from_raw_cookie_header(&self.auth_cookie_input);
                if session.is_authenticated() {
                    self.auth_status = "[OK] Autenticado con exito".to_string();
                    self.innertube.set_session(session);
                    self.show_auth_modal = false;
                    let it = self.innertube.clone();
                    let profile_task = Task::perform(
                        async move { it.get_user_profile().await },
                        |(url, name)| Message::UserProfileLoaded(url, name),
                    );
                    return Task::batch([self.update(Message::ReloadFeed), profile_task]);
                } else {
                    self.auth_status = "[Error] No se detecto SAPISID o cookies validas".to_string();
                }
                Task::none()
            }

            Message::ImportFirefoxCookies => {
                self.auth_status = "Importando sesión de Firefox...".to_string();
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(AuthSession::from_firefox)
                            .await
                            .map_err(|e| e.to_string())
                            .and_then(|res| res.map_err(|e| e.to_string()))
                    },
                    Message::FirefoxCookiesImported,
                )
            }

            Message::FirefoxCookiesImported(res) => {
                match res {
                    Ok(session) => {
                        self.auth_status = "[OK] Sesión importada desde Firefox exitosamente".to_string();
                        self.innertube.set_session(session);
                        self.show_auth_modal = false;
                        let it = self.innertube.clone();
                        let profile_task = Task::perform(
                            async move { it.get_user_profile().await },
                            |(url, name)| Message::UserProfileLoaded(url, name),
                        );
                        return Task::batch([self.update(Message::ReloadFeed), profile_task]);
                    }
                    Err(e) => {
                        self.auth_status = format!("Error al leer Firefox: {e}");
                    }
                }
                Task::none()
            }

            Message::ImportChromeCookies => {
                self.auth_status = "Importando sesión de Chrome / Chromium...".to_string();
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(AuthSession::from_chrome)
                            .await
                            .map_err(|e| e.to_string())
                            .and_then(|res| res.map_err(|e| e.to_string()))
                    },
                    Message::ChromeCookiesImported,
                )
            }

            Message::ChromeCookiesImported(res) => {
                match res {
                    Ok(session) => {
                        self.auth_status = "[OK] Sesión importada desde Chrome exitosamente".to_string();
                        self.innertube.set_session(session);
                        self.show_auth_modal = false;
                        let it = self.innertube.clone();
                        let profile_task = Task::perform(
                            async move { it.get_user_profile().await },
                            |(url, name)| Message::UserProfileLoaded(url, name),
                        );
                        return Task::batch([self.update(Message::ReloadFeed), profile_task]);
                    }
                    Err(e) => {
                        self.auth_status = format!("Error al leer Chrome: {e}");
                    }
                }
                Task::none()
            }

            Message::Logout => {
                let _ = AuthSession::clear_from_disk();
                let guest_session = AuthSession::default();
                self.innertube.set_session(guest_session);
                self.auth_status = "Sesión cerrada. Modo invitado.".to_string();
                self.user_display_name = None;
                self.user_avatar_handle = None;
                self.user_avatar_url = None;
                self.config.cached_user_name = None;
                self.config.cached_avatar_url = None;
                self.config.save_async();
                self.toast_message = Some(("Sesión cerrada exitosamente".to_string(), std::time::Instant::now()));
                return self.update(Message::ReloadFeed);
            }

            Message::ReloadFeed => {
                self.trigger_blur_loading();
                match self.current_view.clone() {
                    ViewMode::Subscriptions => return self.update(Message::OpenSubscriptions),
                    ViewMode::History => return self.update(Message::OpenHistory),
                    ViewMode::Playlists => return self.update(Message::OpenPlaylists),
                    ViewMode::Channel(cid) => return self.update(Message::OpenChannel(cid)),
                    ViewMode::Watch(vid) => return self.update(Message::OpenVideo(vid)),
                    _ => {
                        let it = self.innertube.clone();
                        Task::perform(
                            async move { it.get_home_feed().await.map_err(|e| e.to_string()) },
                            Message::HomeFeedLoaded,
                        )
                    }
                }
            }

            Message::EventOccurred(event, win_id) => {
                match event {
                    iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                        let prev_pos = self.last_cursor_pos;
                        self.last_cursor_pos = Some(position);
                        let dx = prev_pos.map(|p| (p.x - position.x).abs()).unwrap_or(100.0);
                        let dy = prev_pos.map(|p| (p.y - position.y).abs()).unwrap_or(100.0);
                        let significant_move = dx >= 2.0 || dy >= 2.0;

                        if Some(win_id) == self.pip_window_id {
                            self.pip_cursor_hovered = true;
                            self.pip_cursor_pos = Some(position);
                            if significant_move {
                                self.osd_state.on_mouse_move();
                            }

                            let (pip_w, pip_h) = self.pip_window_size;
                            let pad_x = 10.0;
                            let seek_w = (pip_w - 20.0).max(1.0);
                            let seek_y_min = pip_h - 62.0;
                            let seek_y_max = pip_h - 30.0;
                            let in_seek_x = position.x >= pad_x && position.x <= pad_x + seek_w;
                            let in_seek_y = position.y >= seek_y_min && position.y <= seek_y_max;

                            let effective_dur = if self.osd_state.duration_secs > 0 {
                                self.osd_state.duration_secs
                            } else {
                                self.pip_watch_details
                                    .as_ref()
                                    .or(self.watch_details.as_ref())
                                    .and_then(|d| d.video_streams.first())
                                    .map(|s| s.approx_duration_ms / 1000)
                                    .unwrap_or(1)
                                    .max(1)
                            };

                            if (in_seek_x && in_seek_y) || self.is_seeking_drag {
                                let rel_x = (position.x - pad_x) / seek_w;
                                let ratio = rel_x.clamp(0.0, 1.0);
                                self.osd_state.hover_seek_ratio = Some(ratio);

                                if self.is_seeking_drag {
                                    let target_secs = (ratio * effective_dur as f32) as u64;
                                    self.osd_state.current_pos_secs = target_secs;
                                    if let Some(player) = &self.player {
                                        player.seek_ms(target_secs * 1000);
                                    }
                                    self.seek_suppress_until = Some(std::time::Instant::now() + Duration::from_millis(500));
                                }
                            } else {
                                self.osd_state.hover_seek_ratio = None;
                            }
                        } else if matches!(self.current_view, ViewMode::Watch(_) | ViewMode::VideoFullscreen) {
                            let was_visible = self.osd_state.is_visible;
                            self.osd_state.on_mouse_move();
                            if self.is_fullscreen && !was_visible {
                                self.sync_player_viewport();
                            }

                            // Calculate exact seek bar geometry so hovering/clicking bottom buttons never triggers seeks!
                            let (win_w, win_h) = self.window_size;
                            let (px, py, pw, ph) = if self.is_fullscreen {
                                (0.0, 0.0, win_w, win_h)
                            } else {
                                self.get_windowed_player_rect()
                            };

                            let pad_x = px + 12.0;
                            let seek_w = (pw - 24.0).max(1.0);
                            let player_bottom = py + ph;
                            let seek_y_min = player_bottom - 80.0;
                            let seek_y_max = player_bottom - 34.0;
                            let in_seek_x = position.x >= pad_x && position.x <= pad_x + seek_w;
                            let in_seek_y = position.y >= seek_y_min && position.y <= seek_y_max;

                            let effective_dur = if self.osd_state.duration_secs > 0 {
                                self.osd_state.duration_secs
                            } else {
                                self.watch_details
                                    .as_ref()
                                    .and_then(|d| d.video_streams.first())
                                    .map(|s| s.approx_duration_ms / 1000)
                                    .unwrap_or(1)
                                    .max(1)
                            };

                            if (in_seek_x && in_seek_y) || self.is_seeking_drag {
                                let rel_x = (position.x - pad_x) / seek_w;
                                let ratio = rel_x.clamp(0.0, 1.0);
                                self.osd_state.hover_seek_ratio = Some(ratio);

                                if self.is_seeking_drag {
                                    let target_secs = (ratio * effective_dur as f32) as u64;
                                    self.osd_state.current_pos_secs = target_secs;
                                    if let Some(player) = &self.player {
                                        player.seek_ms(target_secs * 1000);
                                    }
                                    self.seek_suppress_until = Some(std::time::Instant::now() + Duration::from_millis(500));
                                }
                            } else {
                                self.osd_state.hover_seek_ratio = None;
                            }
                        }
                    }
                    iced::Event::Mouse(iced::mouse::Event::ButtonPressed(button)) => {
                        let is_pip = Some(win_id) == self.pip_window_id;
                        if is_pip {
                            self.search_input_focused = false;
                            self.focused_window = Some(win_id);
                            self.is_window_focused = true;

                            if let iced::mouse::Button::Left = button {
                                if let Some(ratio) = self.osd_state.hover_seek_ratio {
                                    let effective_dur = if self.osd_state.duration_secs > 0 {
                                        self.osd_state.duration_secs
                                    } else {
                                        self.pip_watch_details
                                            .as_ref()
                                            .or(self.watch_details.as_ref())
                                            .and_then(|d| d.video_streams.first())
                                            .map(|s| s.approx_duration_ms / 1000)
                                            .unwrap_or(1)
                                            .max(1)
                                    };
                                    self.is_seeking_drag = true;
                                    let target_secs = (ratio * effective_dur as f32) as u64;
                                    self.osd_state.current_pos_secs = target_secs;
                                    if let Some(player) = &self.player {
                                        player.seek_ms(target_secs * 1000);
                                    }
                                    self.seek_suppress_until = Some(std::time::Instant::now() + Duration::from_millis(500));
                                }
                            }
                        } else if matches!(self.current_view, ViewMode::Watch(_) | ViewMode::VideoFullscreen) {
                            let was_visible = self.osd_state.is_visible;
                            self.osd_state.on_mouse_move();
                            if self.is_fullscreen && !was_visible {
                                self.sync_player_viewport();
                            }

                            if let iced::mouse::Button::Left = button {
                                if !self.osd_state.has_open_menu() {
                                    if let Some(ratio) = self.osd_state.hover_seek_ratio {
                                        let effective_dur = if self.osd_state.duration_secs > 0 {
                                            self.osd_state.duration_secs
                                        } else {
                                            self.watch_details
                                                .as_ref()
                                                .and_then(|d| d.video_streams.first())
                                                .map(|s| s.approx_duration_ms / 1000)
                                                .unwrap_or(1)
                                                .max(1)
                                        };
                                        self.is_seeking_drag = true;
                                        let target_secs = (ratio * effective_dur as f32) as u64;
                                        self.osd_state.current_pos_secs = target_secs;
                                        if let Some(player) = &self.player {
                                            player.seek_ms(target_secs * 1000);
                                        }
                                        self.seek_suppress_until = Some(std::time::Instant::now() + Duration::from_millis(500));
                                    }
                                }
                            }
                        }
                        if let Some(pos) = self.last_cursor_pos {
                            if pos.y > 55.0 || pos.x < 70.0 || pos.x > 380.0 {
                                self.search_input_focused = false;
                            }
                        }
                    }
                    iced::Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => {
                        self.is_seeking_drag = false;
                    }
                    iced::Event::Mouse(iced::mouse::Event::CursorEntered) => {
                        if Some(win_id) == self.pip_window_id {
                            self.pip_cursor_hovered = true;
                            self.osd_state.on_mouse_move();
                        } else if matches!(self.current_view, ViewMode::Watch(_) | ViewMode::VideoFullscreen) {
                            self.osd_state.on_mouse_move();
                        }
                    }
                    iced::Event::Mouse(iced::mouse::Event::CursorLeft) => {
                        if Some(win_id) == self.pip_window_id {
                            self.pip_cursor_hovered = false;
                            self.pip_cursor_pos = None;
                        }
                        self.osd_state.hover_seek_ratio = None;
                        self.is_seeking_drag = false;
                    }
                    iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                        if self.show_auth_modal {
                            if let Key::Named(Named::Escape) = key {
                                self.show_auth_modal = false;
                                return Task::none();
                            }
                        } else {
                            let has_ctrl = modifiers.control() || modifiers.command();
                            if has_ctrl {
                                match &key {
                                    Key::Named(Named::Tab) => {
                                        if !self.tabs.is_empty() {
                                            let next = match self.active_tab_idx {
                                                Some(cur) => {
                                                    if modifiers.shift() {
                                                        if cur == 0 { self.tabs.len() - 1 } else { cur - 1 }
                                                    } else {
                                                        (cur + 1) % self.tabs.len()
                                                    }
                                                }
                                                None => 0,
                                            };
                                            return self.update(Message::SelectTab(next));
                                        }
                                    }
                                    Key::Character(c) if c == "w" || c == "W" || c == "\u{17}" => {
                                        if let Some(active) = self.active_tab_idx {
                                            return self.update(Message::CloseTab(active));
                                        }
                                    }
                                    Key::Character(c) if c == "t" || c == "T" || c == "\u{14}" => {
                                        if modifiers.shift() {
                                            return self.update(Message::RestoreClosedTab);
                                        } else {
                                            return self.update(Message::OpenEmptyTab);
                                        }
                                    }
                                    _ => {}
                                }
                            } else if self.search_input_focused {
                                if let Key::Named(Named::Escape) = key {
                                    self.search_input_focused = false;
                                    self.show_suggestions = false;
                                    return Task::none();
                                }
                            } else {
                                let is_watching = matches!(self.current_view, ViewMode::Watch(_) | ViewMode::VideoFullscreen)
                                    || self.pip_window_id.is_some();
                                match key {
                                    Key::Named(Named::Space) if is_watching => return self.update(Message::PlayPauseToggle),
                                    Key::Named(Named::Escape) if self.show_suggestions => {
                                        self.show_suggestions = false;
                                        return Task::none();
                                    }
                                    Key::Named(Named::Escape) if self.osd_state.has_open_menu() => {
                                        return self.update(Message::CloseOsdMenus);
                                    }
                                    Key::Named(Named::Escape) if self.is_fullscreen => {
                                        return self.update(Message::ToggleFullscreen);
                                    }
                                    Key::Named(Named::Escape) if Some(win_id) == self.pip_window_id => {
                                        return self.update(Message::ClosePip);
                                    }
                                    Key::Named(Named::ArrowLeft) if is_watching => {
                                        return self.update(Message::SeekRelative(-5));
                                    }
                                    Key::Named(Named::ArrowRight) if is_watching => {
                                        return self.update(Message::SeekRelative(5));
                                    }
                                    Key::Named(Named::ArrowUp) if is_watching => {
                                        let new_vol = (self.osd_state.volume + 0.05).min(1.0);
                                        return self.update(Message::VolumeChanged(new_vol));
                                    }
                                    Key::Named(Named::ArrowDown) if is_watching => {
                                        let new_vol = (self.osd_state.volume - 0.05).max(0.0);
                                        return self.update(Message::VolumeChanged(new_vol));
                                    }
                                    Key::Character(c) if is_watching && (c == "k" || c == "K") => {
                                        return self.update(Message::PlayPauseToggle);
                                    }
                                    Key::Character(c) if is_watching && (c == "j" || c == "J") => {
                                        return self.update(Message::SeekRelative(-10));
                                    }
                                    Key::Character(c) if is_watching && (c == "l" || c == "L") => {
                                        return self.update(Message::SeekRelative(10));
                                    }
                                    Key::Character(c) if is_watching && (c == "m" || c == "M") => {
                                        return self.update(Message::ToggleMute);
                                    }
                                    Key::Character(c) if is_watching && (c == "t" || c == "T") => {
                                        return self.update(Message::ToggleTheater);
                                    }
                                    Key::Character(c) if is_watching && (c == "c" || c == "C") => {
                                        return self.update(Message::ToggleCaptions);
                                    }
                                    Key::Character(c) if is_watching && (c == "f" || c == "F") => {
                                        return self.update(Message::ToggleFullscreen);
                                    }
                                    Key::Character(c) if is_watching && (c == "i" || c == "I" || c == "p" || c == "P") => {
                                        return self.update(Message::TogglePip);
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    iced::Event::Window(iced::window::Event::Closed)
                    | iced::Event::Window(iced::window::Event::CloseRequested) => {
                        if Some(win_id) == self.pip_window_id {
                            return self.update(Message::ClosePip);
                        } else if Some(win_id) == self.main_window_id {
                            if self.pip_window_id.is_some() {
                                save_current_kwin_pip_position(&mut self.config);
                            }
                            self.save_session();
                            self.config.save();
                            if !self.is_fullscreen && self.pip_window_id.is_none() {
                                save_window_state(self.window_size.0, self.window_size.1, self.is_maximized);
                            }
                            // Release the Wayland video subsurface before the display dies, then exit
                            // directly: wgpu's eglTerminate on a closed wl_display segfaults on drop.
                            if let Some(player) = self.player.as_ref() {
                                player.shutdown_sync();
                            }
                            std::process::exit(0);
                        }
                    }
                    iced::Event::Window(iced::window::Event::Focused) => {
                        self.focused_window = Some(win_id);
                        self.is_window_focused = true;
                        if Some(win_id) == self.main_window_id {
                            self.is_window_minimized = false;
                            if self.pip_window_id.is_none() {
                                self.sync_player_viewport();
                            }
                        }
                        if Some(win_id) == self.main_window_id && self.auto_pip_triggered_by_unfocus && self.pip_window_id.is_some() {
                            self.auto_pip_triggered_by_unfocus = false;
                            return self.update(Message::ClosePip);
                        }
                    }
                    iced::Event::Window(iced::window::Event::Unfocused) => {
                        if self.focused_window == Some(win_id) {
                            self.focused_window = None;
                            self.is_window_focused = false;
                        }
                        if Some(win_id) == self.main_window_id
                            && self.config.auto_pip_on_unfocus
                            && self.pip_window_id.is_none()
                            && (self.watch_details.is_some() || self.playing_video_id.is_some())
                        {
                            let is_active = self.player.as_ref().map(|p| p.is_playing() || p.get_position_ms() > 0).unwrap_or(false);
                            if is_active {
                                self.auto_pip_triggered_by_unfocus = true;
                                return self.update(Message::TogglePip);
                            }
                        } else if Some(win_id) == self.main_window_id && self.pip_window_id.is_none() {
                            return window::get_minimized(win_id).map(|min| Message::WindowMinimizedStatus(min.unwrap_or(false)));
                        }
                    }
                    iced::Event::Window(iced::window::Event::Moved(p)) => {
                        if Some(win_id) == self.pip_window_id {
                            self.config.pip_position = Some((p.x, p.y));
                            self.config.save_async();
                        }
                    }
                    iced::Event::Window(iced::window::Event::Resized(size)) => {
                        if size.width < 10.0 || size.height < 10.0 {
                            if Some(win_id) == self.main_window_id {
                                self.is_window_minimized = true;
                                if let Some(player) = &self.player {
                                    if self.pip_window_id.is_none() {
                                        player.set_rendering_active(false);
                                    }
                                }
                            }
                            return Task::none();
                        } else if Some(win_id) == self.main_window_id {
                            self.is_window_minimized = false;
                        }
                        if Some(win_id) == self.pip_window_id {
                            let (target_w, target_h) =
                                self.compute_pip_aspect_size(size.width, size.height, self.last_pip_resize_edge);
                            self.pip_window_size = (target_w, target_h);
                            self.config.pip_size = Some((target_w, target_h));
                            self.config.save_async();
                            if let Some(player) = &self.player {
                                player.set_underlay(true);
                                player.set_viewport(0, 0, target_w as i32, target_h as i32);
                                player.set_rendering_active(true);
                            }
                            if (size.width - target_w).abs() > 1.0 || (size.height - target_h).abs() > 1.0 {
                                return window::resize(win_id, Size::new(target_w, target_h));
                            }
                            return Task::none();
                        } else if Some(win_id) == self.main_window_id || self.main_window_id.is_none() {
                            self.window_size = (size.width, size.height);
                            if self.pip_window_id.is_none() {
                                self.sync_player_viewport();
                            }
                            if !self.is_fullscreen && self.pip_window_id.is_none() {
                                return window::get_maximized(win_id).map(Message::WindowMaximized);
                            } else {
                                return Task::none();
                            }
                        } else {
                            return Task::none();
                        }
                    }
                    iced::Event::Mouse(iced::mouse::Event::WheelScrolled { delta }) => {
                        if Some(win_id) == self.pip_window_id {
                            let lines = match delta {
                                iced::mouse::ScrollDelta::Lines { y, .. } => y,
                                iced::mouse::ScrollDelta::Pixels { y, .. } => y / 20.0,
                            };
                            if lines.abs() > 0.05 {
                                let scale = if lines > 0.0 { 1.10 } else { 0.90 };
                                return self.update(Message::ScalePip(scale));
                            }
                        }
                    }
                    _ => {}
                }
                Task::none()
            }

            Message::TriggerFullBuffer(video_id) => {
                if let Some(state) = self.buffer_downloads.get(&video_id) {
                    if state.is_complete || (state.percent > 0 && state.percent < 100) {
                        return Task::none();
                    }
                }
                self.buffer_downloads.insert(
                    video_id.clone(),
                    BufferDownloadState {
                        percent: 1,
                        is_complete: false,
                        video_path: None,
                        audio_path: None,
                    },
                );

                let known_streams = if let Some(details) = &self.watch_details {
                    if details.video_id == video_id {
                        let best = select_best_stream(&details.video_streams, &self.config);
                        let audio = select_best_audio_stream(&details.audio_streams, self.config.preferred_audio_lang);
                        best.map(|v| (v.url.clone(), audio.map(|a| a.url.clone())))
                    } else {
                        None
                    }
                } else {
                    None
                };

                start_full_buffer_task(self.innertube.clone(), video_id, known_streams)
            }

            Message::FullBufferProgress(video_id, pct) => {
                if let Some(state) = self.buffer_downloads.get_mut(&video_id) {
                    state.percent = pct;
                } else {
                    self.buffer_downloads.insert(
                        video_id,
                        BufferDownloadState {
                            percent: pct,
                            is_complete: pct >= 100,
                            video_path: None,
                            audio_path: None,
                        },
                    );
                }
                Task::none()
            }

            Message::FullBufferComplete(video_id, v_path, a_path) => {
                self.buffer_downloads.insert(
                    video_id.clone(),
                    BufferDownloadState {
                        percent: 100,
                        is_complete: true,
                        video_path: Some(v_path.clone()),
                        audio_path: a_path.clone(),
                    },
                );

                if self.playing_video_id.as_deref() == Some(&video_id) {
                    if let Some(player) = &self.player {
                        let v_uri = format!("file://{}", v_path.to_string_lossy());
                        let a_uri = a_path.as_ref().map(|p| format!("file://{}", p.to_string_lossy()));
                        let _ = player.switch_resolution(&v_uri, a_uri.as_deref());
                    }
                }

                // Prune cache if exceeding configured limit and refresh size
                let max_bytes = self.config.max_cache_size_bytes;
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            crate::cache_manager::prune_cache_if_needed(max_bytes);
                            crate::cache_manager::get_cache_size_bytes()
                        })
                        .await
                        .unwrap_or(0)
                    },
                    Message::CacheSizeRefreshed,
                )
            }

            Message::FullBufferError(video_id, err) => {
                eprintln!("[Mitub Full Buffer Error for {video_id}]: {err}");
                if let Some(state) = self.buffer_downloads.get_mut(&video_id) {
                    state.percent = 0;
                    state.is_complete = false;
                }
                Task::none()
            }

            Message::ToggleFullBufferTabs => {
                self.config.full_buffer_tabs = !self.config.full_buffer_tabs;
                self.config.save();
                Task::none()
            }

            Message::SetCacheLimitIndex(idx) => {
                self.config.max_cache_size_bytes = crate::cache_manager::index_to_limit(idx);
                self.config.save();
                let max_bytes = self.config.max_cache_size_bytes;
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            crate::cache_manager::prune_cache_if_needed(max_bytes);
                            crate::cache_manager::get_cache_size_bytes()
                        })
                        .await
                        .unwrap_or(0)
                    },
                    Message::CacheSizeRefreshed,
                )
            }

            Message::ClearCache => {
                self.buffer_downloads.clear();
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(crate::cache_manager::clear_all_cache)
                            .await
                            .unwrap_or(0)
                    },
                    Message::CacheCleared,
                )
            }

            Message::CacheCleared(freed_bytes) => {
                self.current_cache_size_bytes = crate::cache_manager::get_cache_size_bytes();
                self.toast_message = Some((
                    format!(
                        "Caché de video vaciada ({} liberados)",
                        crate::cache_manager::format_size(freed_bytes)
                    ),
                    std::time::Instant::now(),
                ));
                crate::power_manager::trim_process_memory();
                Task::none()
            }

            Message::CacheSizeRefreshed(new_size) => {
                self.current_cache_size_bytes = new_size;
                Task::none()
            }
        }
    }

pub fn calculate_player_geometry(win_w: f32, win_h: f32, theater_mode: bool, scroll_y: f32) -> (f32, f32, f32, f32) {
    if theater_mode {
        let h = ((win_w * 9.0 / 16.0).min(win_h * 0.7)).round();
        let initial_y = 56.0;
        let y = initial_y - scroll_y;
        (0.0, y, win_w, h)
    } else {
        let col_w = (((win_w - 56.0) * 0.75).max(480.0)).round();
        let max_h = (win_h - 140.0).max(360.0);
        let col_h = (((col_w * 9.0 / 16.0).min(max_h))).round();
        let initial_y = 56.0 + 12.0;
        let y = initial_y - scroll_y;
        (24.0, y, col_w, col_h)
    }
}

    fn get_windowed_player_rect(&self) -> (f32, f32, f32, f32) {
        Self::calculate_player_geometry(self.window_size.0, self.window_size.1, self.theater_mode, self.watch_scroll_y)
    }

    fn sync_player_viewport(&self) {
        if self.pip_window_id.is_some() {
            if let Some(player) = &self.player {
                player.set_rendering_active(true);
            }
            return;
        }
        if let Some(player) = &self.player {
            let is_showing_playing_video = match &self.current_view {
                ViewMode::VideoFullscreen => true,
                ViewMode::Watch(vid) => self.playing_video_id.as_deref() == Some(vid.as_str()),
                _ => false,
            };

            if !is_showing_playing_video {
                player.set_viewport(0, 0, 0, 0);
                player.set_rendering_active(false);
                return;
            }

            if self.is_window_minimized {
                player.set_rendering_active(false);
                return;
            }

            let (win_w, win_h) = self.window_size;
            player.set_underlay(true);

            if let Some((start, is_entering, initial_rect)) = self.fullscreen_anim {
                let elapsed = start.elapsed().as_secs_f32();
                let t = (elapsed / 0.22_f32).clamp(0.0, 1.0);
                let progress = 1.0 - (1.0 - t).powi(3);

                let (start_r, target_r) = if is_entering {
                    (initial_rect, (0.0, 0.0, win_w, win_h))
                } else {
                    ((0.0, 0.0, win_w, win_h), initial_rect)
                };

                let cur_x = start_r.0 + (target_r.0 - start_r.0) * progress;
                let cur_y = start_r.1 + (target_r.1 - start_r.1) * progress;
                let cur_w = (start_r.2 + (target_r.2 - start_r.2) * progress).max(1.0);
                let cur_h = (start_r.3 + (target_r.3 - start_r.3) * progress).max(1.0);

                player.set_rendering_active(true);
                player.set_viewport(cur_x as i32, cur_y as i32, cur_w as i32, cur_h as i32);
            } else if self.is_fullscreen {
                player.set_rendering_active(true);
                player.set_viewport(0, 0, win_w as i32, win_h as i32);
            } else if self.theater_mode {
                let (x, y, w, h) = self.get_windowed_player_rect();
                if (y as i32) + (h as i32) <= 56 {
                    // Completely scrolled off above the 56px header bar
                    player.set_viewport(0, -10000, 1, 1);
                    player.set_rendering_active(false);
                } else {
                    player.set_rendering_active(true);
                    player.set_viewport(x as i32, y as i32, w as i32, h as i32);
                }
            } else {
                let (x, y, w, h) = self.get_windowed_player_rect();
                if (y as i32) + (h as i32) <= 56 {
                    // Completely scrolled off above the 56px header bar
                    player.set_viewport(0, -10000, 1, 1);
                    player.set_rendering_active(false);
                } else {
                    player.set_rendering_active(true);
                    player.set_viewport(x as i32, y as i32, w as i32, h as i32);
                }
            }
        }
    }

    pub fn view(&self, id: Id) -> Element<'_, Message> {
        if Some(id) == self.pip_window_id {
            return self.view_pip();
        }
        self.view_main()
    }

    fn view_pip(&self) -> Element<'_, Message> {
        let effective_dur = if self.osd_state.duration_secs > 0 {
            self.osd_state.duration_secs
        } else {
            self.watch_details
                .as_ref()
                .and_then(|d| d.video_streams.first())
                .map(|s| s.approx_duration_ms / 1000)
                .unwrap_or(0)
        };

        let progress_ratio = if effective_dur > 0 {
            (self.osd_state.current_pos_secs as f32 / effective_dur as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let buffered_ratio = if effective_dur > 0 {
            (self.osd_state.buffered_pos_secs as f32 / effective_dur as f32).clamp(progress_ratio, 1.0)
        } else {
            0.0
        };

        let target_secs = if let Some(ratio) = self.osd_state.hover_seek_ratio {
            ((ratio.clamp(0.0, 1.0)) * (effective_dur.max(1) as f32)) as u64
        } else {
            self.osd_state.current_pos_secs
        };

        let chapters = self
            .pip_watch_details
            .as_ref()
            .or(self.watch_details.as_ref())
            .map(|d| d.chapters.as_slice())
            .unwrap_or(&[]);

        let chapter_starts: Vec<u64> = chapters.iter().map(|c| c.start_secs).collect();

        let pb_canvas = iced::widget::canvas(ui::osd_controls::ProgressBarProgram {
            progress_ratio,
            buffered_ratio,
            hover_ratio: self.osd_state.hover_seek_ratio,
            chapter_starts,
            duration_secs: effective_dur,
        })
        .width(Length::Fill)
        .height(Length::Fixed(14.0));

        let interactive_seek_bar: Element<'_, Message> = iced::widget::mouse_area(
            container(pb_canvas).width(Length::Fill).padding([2, 0]),
        )
        .interaction(iced::mouse::Interaction::Pointer)
        .into();

        let time_label = if let Some(ratio) = self.osd_state.hover_seek_ratio {
            let hover_secs = ((ratio.clamp(0.0, 1.0)) * (effective_dur.max(1) as f32)) as u64;
            format!(
                "{} ({}) / {}",
                ui::osd_controls::format_duration(self.osd_state.current_pos_secs),
                ui::osd_controls::format_duration(hover_secs),
                ui::osd_controls::format_duration(effective_dur)
            )
        } else {
            format!(
                "{} / {}",
                ui::osd_controls::format_duration(self.osd_state.current_pos_secs),
                ui::osd_controls::format_duration(effective_dur)
            )
        };

        let max_ch_len = if self.pip_window_size.0 < 340.0 {
            26
        } else if self.pip_window_size.0 < 440.0 {
            40
        } else {
            65
        };
        let chapter_label: Option<String> = if !chapters.is_empty() {
            chapters
                .iter()
                .rev()
                .find(|c| c.start_secs <= target_secs)
                .or_else(|| chapters.first())
                .map(|active_ch| {
                    let title = if active_ch.title.chars().count() > max_ch_len {
                        let mut s: String = active_ch.title.chars().take(max_ch_len.saturating_sub(3)).collect();
                        s.push_str("...");
                        s
                    } else {
                        active_ch.title.clone()
                    };
                    format!("•  {title}")
                })
        } else {
            None
        };

        let back_10_btn = iced::widget::button(
            iced::widget::text("-10s").size(11).color(COLOR_TEXT_PRIMARY),
        )
        .style(ghost_button)
        .padding([4, 7])
        .on_press(Message::SeekRelative(-10));

        let play_btn = iced::widget::button(if self.osd_state.is_playing {
            ui::icons::icon_pause(16.0)
        } else {
            ui::icons::icon_play(16.0)
        })
        .style(ghost_button)
        .padding([5, 8])
        .on_press(Message::PlayPauseToggle);

        let fwd_10_btn = iced::widget::button(
            iced::widget::text("+10s").size(11).color(COLOR_TEXT_PRIMARY),
        )
        .style(ghost_button)
        .padding([4, 7])
        .on_press(Message::SeekRelative(10));

        let controls_row = iced::widget::row![
            back_10_btn,
            play_btn,
            fwd_10_btn,
            iced::widget::text(time_label).size(11).color(COLOR_TEXT_SECONDARY),
            iced::widget::Space::with_width(Length::Fill),
        ]
        .spacing(4)
        .align_y(iced::Alignment::Center);

        let mut bottom_col = iced::widget::column![].spacing(3).padding([6, 10]);

        if let Some(ch) = chapter_label {
            bottom_col = bottom_col.push(
                iced::widget::row![
                    iced::widget::text(ch)
                        .size(11)
                        .color(if self.osd_state.hover_seek_ratio.is_some() {
                            iced::Color::from_rgb8(0xff, 0xff, 0xff)
                        } else {
                            COLOR_TEXT_SECONDARY
                        })
                ]
                .width(Length::Fill)
            );
        }

        bottom_col = bottom_col.push(interactive_seek_bar);
        bottom_col = bottom_col.push(controls_row);

        let bottom_controls = container(bottom_col)
            .style(|_t| container::Style {
                background: Some(iced::Color::from_rgba(0.04, 0.04, 0.04, 0.85).into()),
                ..container::Style::default()
            })
            .width(Length::Fill);

        // Top-right in-video Close/Reattach button ("X" like Android YouTube & Firefox PiP)
        let top_right_close_btn: Element<'_, Message> = container(
            iced::widget::button(
                container(ui::icons::icon_close(14.0))
                    .align_x(iced::alignment::Horizontal::Center)
                    .align_y(iced::alignment::Vertical::Center),
            )
            .style(|_t, status| match status {
                iced::widget::button::Status::Hovered => iced::widget::button::Style {
                    background: Some(iced::Color::from_rgba8(0xee, 0x22, 0x22, 0.90).into()),
                    border: iced::border::rounded(14),
                    ..iced::widget::button::Style::default()
                },
                _ => iced::widget::button::Style {
                    background: Some(iced::Color::from_rgba8(0x11, 0x11, 0x11, 0.75).into()),
                    border: iced::border::rounded(14),
                    ..iced::widget::button::Style::default()
                },
            })
            .padding(6)
            .on_press(Message::ClosePip),
        )
        .padding(8)
        .into();

        let top_bar_overlay = container(
            iced::widget::row![
                iced::widget::Space::with_width(Length::Fill),
                top_right_close_btn,
            ]
            .width(Length::Fill)
            .align_y(iced::Alignment::Center),
        )
        .width(Length::Fill)
        .align_y(iced::alignment::Vertical::Top)
        .style(ui::theme::transparent_player_container);

        let show_pip_osd = self.pip_cursor_hovered || !self.osd_state.is_playing || self.osd_state.is_visible;

        let drag_area: Element<'_, Message> = iced::widget::mouse_area(
            iced::widget::Space::new(Length::Fill, Length::Fill),
        )
        .on_press(Message::StartPipDrag)
        .into();

        let border_size = 7.0;
        let corner_size = 14.0;

        let right_handle: Element<'_, Message> = container(
            iced::widget::mouse_area(iced::widget::Space::new(Length::Fixed(border_size), Length::Fill))
                .interaction(iced::mouse::Interaction::ResizingHorizontally)
                .on_press(Message::StartPipEdgeResize(PipResizeEdge::Right)),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Right)
        .into();

        let left_handle: Element<'_, Message> = container(
            iced::widget::mouse_area(iced::widget::Space::new(Length::Fixed(border_size), Length::Fill))
                .interaction(iced::mouse::Interaction::ResizingHorizontally)
                .on_press(Message::StartPipEdgeResize(PipResizeEdge::Left)),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Left)
        .into();

        let bottom_handle: Element<'_, Message> = container(
            iced::widget::mouse_area(iced::widget::Space::new(Length::Fill, Length::Fixed(border_size)))
                .interaction(iced::mouse::Interaction::ResizingVertically)
                .on_press(Message::StartPipEdgeResize(PipResizeEdge::Bottom)),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(iced::alignment::Vertical::Bottom)
        .into();

        let top_handle: Element<'_, Message> = container(
            iced::widget::mouse_area(iced::widget::Space::new(Length::Fill, Length::Fixed(border_size)))
                .interaction(iced::mouse::Interaction::ResizingVertically)
                .on_press(Message::StartPipEdgeResize(PipResizeEdge::Top)),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(iced::alignment::Vertical::Top)
        .into();

        let br_corner: Element<'_, Message> = container(
            iced::widget::mouse_area(iced::widget::Space::new(Length::Fixed(corner_size), Length::Fixed(corner_size)))
                .interaction(iced::mouse::Interaction::ResizingDiagonallyDown)
                .on_press(Message::StartPipEdgeResize(PipResizeEdge::BottomRight)),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Right)
        .align_y(iced::alignment::Vertical::Bottom)
        .into();

        let bl_corner: Element<'_, Message> = container(
            iced::widget::mouse_area(iced::widget::Space::new(Length::Fixed(corner_size), Length::Fixed(corner_size)))
                .interaction(iced::mouse::Interaction::ResizingDiagonallyUp)
                .on_press(Message::StartPipEdgeResize(PipResizeEdge::BottomLeft)),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Left)
        .align_y(iced::alignment::Vertical::Bottom)
        .into();

        let tl_corner: Element<'_, Message> = container(
            iced::widget::mouse_area(iced::widget::Space::new(Length::Fixed(corner_size), Length::Fixed(corner_size)))
                .interaction(iced::mouse::Interaction::ResizingDiagonallyDown)
                .on_press(Message::StartPipEdgeResize(PipResizeEdge::TopLeft)),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Left)
        .align_y(iced::alignment::Vertical::Top)
        .into();

        let tr_corner: Element<'_, Message> = container(
            iced::widget::mouse_area(iced::widget::Space::new(Length::Fixed(corner_size), Length::Fixed(corner_size)))
                .interaction(iced::mouse::Interaction::ResizingDiagonallyUp)
                .on_press(Message::StartPipEdgeResize(PipResizeEdge::TopRight)),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Right)
        .align_y(iced::alignment::Vertical::Top)
        .into();

        let video_frame = self.player.as_ref().and_then(|p| p.get_current_frame());
        let mut layers: Vec<Element<'_, Message>> = Vec::new();

        if let Some(frame) = video_frame {
            layers.push(
                crate::ui::video_widget::video_shader_pip(frame),
            );
        }

        // Layer 1: drag_area directly over video
        layers.push(drag_area);

        // Layer 2: seek feedback animation
        if let Some(anim) = self.osd_state.seek_anim {
            if anim.last_tap_time.elapsed().as_secs_f32() < 0.65 {
                layers.push(
                    container(
                        iced::widget::canvas(ui::osd_controls::SeekFeedbackProgram { anim })
                            .width(Length::Fill)
                            .height(Length::Fill),
                    )
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(ui::theme::transparent_player_container)
                    .into(),
                );
            }
        }

        // Layer 3: OSD controls (top close button & bottom controls)
        if show_pip_osd {
            layers.push(top_bar_overlay.into());
            layers.push(
                container(bottom_controls)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_y(iced::alignment::Vertical::Bottom)
                    .style(ui::theme::transparent_player_container)
                    .into(),
            );
        }

        // Layer 4: Native border resize handles with proper directional cursor icons
        layers.push(right_handle);
        layers.push(left_handle);
        layers.push(bottom_handle);
        layers.push(top_handle);
        layers.push(br_corner);
        layers.push(bl_corner);
        layers.push(tl_corner);
        layers.push(tr_corner);

        container(iced::widget::stack(layers).width(Length::Fill).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_t| container::Style {
                background: Some(iced::Color::TRANSPARENT.into()),
                ..container::Style::default()
            })
            .into()
    }

    fn view_main(&self) -> Element<'_, Message> {
        let blur_amount = self.view_transition_alpha.clamp(0.0, 1.0);
        let is_pip_active = self.pip_window_id.is_some();
        if self.is_fullscreen {
            if let Some(details) = &self.watch_details {
                let fs_viewport_w = self.window_size.0 as u16;
                let fs_viewport_h = self.window_size.1 as u16;
                let is_active_video = self.playing_video_id.as_deref() == Some(&details.video_id);
                let is_this_video_in_pip = self.pip_window_id.is_some() && is_active_video;
                let blurred_poster = self.blurred_posters.get(&details.video_id);
                let sharp_poster_override = self
                    .tab_cached_frames
                    .get(&details.video_id)
                    .or_else(|| {
                        self.tabs
                            .iter()
                            .find(|t| t.video_id == details.video_id && !t.thumbnail_url.is_empty())
                            .and_then(|t| self.thumbnail_cache.get(&t.thumbnail_url))
                    });
                let buffer_status = self.buffer_downloads.get(&details.video_id).map(|s| (s.percent, s.is_complete));
                let video_frame = if is_active_video {
                    self.player.as_ref().and_then(|p| p.get_current_frame())
                } else {
                    None
                };

                let osd_ref = if is_active_video {
                    &self.osd_state
                } else {
                    &self.inactive_tab_osd_state
                };

                return view_watch_page(
                    details,
                    osd_ref,
                    &self.live_messages,
                    &self.playback_queue,
                    self.autoplay_next,
                    &self.thumbnail_cache,
                    &self.blurred_thumbnails,
                    blurred_poster,
                    sharp_poster_override,
                    video_frame,
                    0.0,
                    self.theater_mode,
                    self.is_fullscreen,
                    is_this_video_in_pip,
                    self.description_expanded,
                    self.chat_collapsed,
                    &self.description_content,
                    fs_viewport_w,
                    fs_viewport_h,
                    self.window_size.1,
                    Message::VideoHitboxClicked,
                    Message::CloseOsdMenus,
                    Message::PlayPauseToggle,
                    Message::RetryPlayback,
                    Message::Seek,
                    Message::VolumeChanged,
                    Message::ToggleMute,
                    Message::ToggleQualityMenu,
                    Message::SelectQuality,
                    Message::ToggleSaveDefaultQuality,
                    Message::SetPreferredCodec,
                    Message::ToggleAudioMenu,
                    Message::SelectAudioTrack,
                    Message::ToggleSpeedMenu,
                    Message::ChangePlaybackRate,
                    Message::ToggleCaptions,
                    Message::SelectCaptionTrack,
                    Message::TogglePip,
                    Message::ToggleTheater,
                    Message::ToggleFullscreen,
                    Message::ToggleSubscribe,
                    Message::LikeVideo,
                    Message::DislikeVideo,
                    buffer_status,
                    Message::TriggerFullBuffer(details.video_id.clone()),
                    Message::OpenVideo,
                    Message::OpenVideoInNewTab,
                    Message::OpenChannel,
                    Message::AddToQueue,
                    Message::RemoveFromQueue,
                    Message::ClearQueue,
                    Message::ToggleAutoplay,
                    Message::OpenUrl,
                    Message::CopyShareUrl,
                    Message::CopyDescription,
                    Message::CopyComment,
                    Message::ToggleDescription,
                    Message::ToggleChat,
                    Message::DescriptionAction,
                    Message::WatchScrolled,
                );
            }
        }

        let is_authed = self.auth_status.starts_with("[OK]");
        let is_subscriptions = matches!(self.current_view, ViewMode::Subscriptions);
        let is_history = matches!(self.current_view, ViewMode::History);
        let is_home = matches!(self.current_view, ViewMode::Home);
        let newly_opened = self.tab_opened_anim.and_then(|(idx, inst)| {
            let el = inst.elapsed().as_secs_f32();
            if el < 1.5 {
                Some((idx, el))
            } else {
                None
            }
        });
        let shaking_tab = self.tab_shake_anim.and_then(|(idx, inst)| {
            let el = inst.elapsed().as_secs_f32();
            if el < 0.6 {
                Some((idx, el))
            } else {
                None
            }
        });
        let is_video_playing = self.osd_state.is_playing
            && self.player.as_ref().map(|p| p.is_playing()).unwrap_or(false);

        let header = view_header_bar(
            &self.search_query,
            &self.suggestions,
            self.show_suggestions,
            self.power_profile,
            is_authed,
            self.user_display_name.as_deref(),
            self.user_avatar_handle.as_ref(),
            is_subscriptions,
            is_history,
            is_home,
            &self.tabs,
            self.active_tab_idx,
            self.playing_video_id.as_deref(),
            is_video_playing,
            newly_opened,
            shaking_tab,
            self.window_size.0,
            self.tabs_scroll_x,
            self.tabs_max_scroll_x,
            Message::SearchInputChanged,
            Message::SearchSubmitted,
            Message::SelectSuggestion,
            Message::GoHome,
            Message::GoBack,
            Message::OpenSubscriptions,
            Message::OpenHistory,
            Message::ToggleLoginModal,
            Message::ReloadFeed,
            Message::SelectTab,
            Message::CloseTab,
            Message::TabsScrolled,
            Message::ScrollTabsLeft,
            Message::ScrollTabsRight,
        );

        let content: Element<'_, Message> = match &self.current_view {
            ViewMode::Home => view_home_grid(
                &self.home_videos,
                &self.thumbnail_cache,
                &self.blurred_thumbnails,
                blur_amount,
                self.window_size.0,
                self.is_loading_more_feed,
                Message::OpenVideo,
                Message::OpenVideoInNewTab,
                Message::OpenChannel,
                Message::HomeScrolled,
            ),
            ViewMode::Subscriptions => view_home_grid(
                &self.home_videos,
                &self.thumbnail_cache,
                &self.blurred_thumbnails,
                blur_amount,
                self.window_size.0,
                self.is_loading_more_feed,
                Message::OpenVideo,
                Message::OpenVideoInNewTab,
                Message::OpenChannel,
                Message::HomeScrolled,
            ),
            ViewMode::History => view_home_grid(
                &self.home_videos,
                &self.thumbnail_cache,
                &self.blurred_thumbnails,
                blur_amount,
                self.window_size.0,
                self.is_loading_more_feed,
                Message::OpenVideo,
                Message::OpenVideoInNewTab,
                Message::OpenChannel,
                Message::HomeScrolled,
            ),
            ViewMode::Playlists => view_home_grid(
                &self.home_videos,
                &self.thumbnail_cache,
                &self.blurred_thumbnails,
                blur_amount,
                self.window_size.0,
                false,
                Message::OpenVideo,
                Message::OpenVideoInNewTab,
                Message::OpenChannel,
                Message::HomeScrolled,
            ),
            ViewMode::Watch(_) | ViewMode::VideoFullscreen => {
                let (_, _, col_w, col_h) = if self.is_fullscreen {
                    (0.0, 0.0, self.window_size.0, self.window_size.1)
                } else {
                    Self::calculate_player_geometry(self.window_size.0, self.window_size.1, self.theater_mode, 0.0)
                };
                let viewport_w = col_w as u16;
                let viewport_h = col_h as u16;

                if let Some(details) = &self.watch_details {
                    let is_active_video = self.playing_video_id.as_deref() == Some(&details.video_id);
                    let is_this_video_in_pip = self.pip_window_id.is_some() && is_active_video;
                    let blurred_poster = self.blurred_posters.get(&details.video_id);
                    let sharp_poster_override = self
                        .tab_cached_frames
                        .get(&details.video_id)
                        .or_else(|| {
                            self.tabs
                                .iter()
                                .find(|t| t.video_id == details.video_id && !t.thumbnail_url.is_empty())
                                .and_then(|t| self.thumbnail_cache.get(&t.thumbnail_url))
                        });
                    let buffer_status = self.buffer_downloads.get(&details.video_id).map(|s| (s.percent, s.is_complete));
                    let video_frame = if is_active_video {
                        self.player.as_ref().and_then(|p| p.get_current_frame())
                    } else {
                        None
                    };

                    let osd_ref = if is_active_video {
                        &self.osd_state
                    } else {
                        &self.inactive_tab_osd_state
                    };

                    view_watch_page(
                        details,
                        osd_ref,
                        &self.live_messages,
                        &self.playback_queue,
                        self.autoplay_next,
                        &self.thumbnail_cache,
                        &self.blurred_thumbnails,
                        blurred_poster,
                        sharp_poster_override,
                        video_frame,
                        blur_amount,
                        self.theater_mode,
                        self.is_fullscreen,
                        is_this_video_in_pip,
                        self.description_expanded,
                        self.chat_collapsed,
                        &self.description_content,
                        viewport_w,
                        viewport_h,
                        self.window_size.1,
                        Message::VideoHitboxClicked,
                        Message::CloseOsdMenus,
                        Message::PlayPauseToggle,
                        Message::RetryPlayback,
                        Message::Seek,
                        Message::VolumeChanged,
                        Message::ToggleMute,
                        Message::ToggleQualityMenu,
                        Message::SelectQuality,
                        Message::ToggleSaveDefaultQuality,
                        Message::SetPreferredCodec,
                        Message::ToggleAudioMenu,
                        Message::SelectAudioTrack,
                        Message::ToggleSpeedMenu,
                        Message::ChangePlaybackRate,
                        Message::ToggleCaptions,
                        Message::SelectCaptionTrack,
                        Message::TogglePip,
                        Message::ToggleTheater,
                        Message::ToggleFullscreen,
                        Message::ToggleSubscribe,
                        Message::LikeVideo,
                        Message::DislikeVideo,
                        buffer_status,
                        Message::TriggerFullBuffer(details.video_id.clone()),
                        Message::OpenVideo,
                        Message::OpenVideoInNewTab,
                        Message::OpenChannel,
                        Message::AddToQueue,
                        Message::RemoveFromQueue,
                        Message::ClearQueue,
                        Message::ToggleAutoplay,
                        Message::OpenUrl,
                        Message::CopyShareUrl,
                        Message::CopyDescription,
                        Message::CopyComment,
                        Message::ToggleDescription,
                        Message::ToggleChat,
                        Message::DescriptionAction,
                        Message::WatchScrolled,
                    )
                } else {
                    container(view_home_grid(
                        &self.home_videos,
                        &self.thumbnail_cache,
                        &self.blurred_thumbnails,
                        blur_amount,
                        self.window_size.0,
                        self.is_loading_more_feed,
                        Message::OpenVideo,
                        Message::OpenVideoInNewTab,
                        Message::OpenChannel,
                        Message::HomeScrolled,
                    ))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(main_container)
                    .into()
                }
            }
            ViewMode::Channel(_) => {
                if let Some(channel) = &self.channel_details {
                    view_channel_page(
                        channel,
                        self.selected_channel_tab,
                        &self.thumbnail_cache,
                        &self.blurred_thumbnails,
                        blur_amount,
                        self.window_size.0,
                        Message::SelectChannelTab,
                        Message::ToggleSubscribe,
                        Message::OpenVideo,
                        Message::OpenVideoInNewTab,
                        Message::OpenChannel,
                    )
                } else {
                    view_home_grid(
                        &self.home_videos,
                        &self.thumbnail_cache,
                        &self.blurred_thumbnails,
                        blur_amount,
                        self.window_size.0,
                        false,
                        Message::OpenVideo,
                        Message::OpenVideoInNewTab,
                        Message::OpenChannel,
                        Message::HomeScrolled,
                    )
                }
            }
        };

        let is_video_view = matches!(self.current_view, ViewMode::Watch(_) | ViewMode::VideoFullscreen);
        let content_with_transition: Element<'_, Message> = if !is_video_view && blur_amount > 0.015 {
            // Soft frosted glass structural diffusion layer over grid views during blur/unblur transition
            let alpha = (blur_amount * 0.45).clamp(0.0, 0.55);
            let fade_veil = container(iced::widget::Space::with_width(Length::Fill).height(Length::Fill))
                .width(Length::Fill)
                .height(Length::Fill)
                .style(move |_t| container::Style {
                    background: Some(
                        iced::Color {
                            a: alpha,
                            ..COLOR_BG
                        }
                        .into(),
                    ),
                    ..container::Style::default()
                });
            stack![content, fade_veil]
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        } else {
            content
        };

        let clipped_content = container(content_with_transition)
            .width(Length::Fill)
            .height(Length::Fill)
            .clip(true);

        let base_view: Element<'_, Message> = match &self.current_view {
            ViewMode::Watch(_) if !is_pip_active && self.watch_details.is_some() => {
                container(
                    stack![
                        iced::widget::column![
                            iced::widget::Space::with_height(Length::Fixed(56.0)),
                            clipped_content
                        ]
                        .width(Length::Fill)
                        .height(Length::Fill),
                        header,
                    ]
                    .width(Length::Fill)
                    .height(Length::Fill),
                )
                .width(Length::Fill)
                .height(Length::Fill)
                .style(main_container)
                .into()
            }
            ViewMode::VideoFullscreen if !is_pip_active => {
                container(clipped_content)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(main_container)
                    .into()
            }
            _ => {
                container(
                    stack![
                        iced::widget::column![
                            iced::widget::Space::with_height(Length::Fixed(56.0)),
                            clipped_content
                        ]
                        .width(Length::Fill)
                        .height(Length::Fill),
                        header,
                    ]
                    .width(Length::Fill)
                    .height(Length::Fill),
                )
                .width(Length::Fill)
                .height(Length::Fill)
                .style(main_container)
                .into()
            }
        };

        let mut layers: Vec<Element<'_, Message>> = vec![base_view];

        if let Some((ref msg, instant)) = &self.toast_message {
            let elapsed = instant.elapsed().as_secs_f32();
            if elapsed < 3.0 {
                let alpha = if elapsed > 2.6 {
                    ((3.0 - elapsed) / 0.4).clamp(0.0, 1.0)
                } else if elapsed < 0.2 {
                    (elapsed / 0.2).clamp(0.0, 1.0)
                } else {
                    1.0
                };

                let toast_widget = container(
                    row![
                        icon_check_circle(16.0),
                        text(msg.clone())
                            .size(13)
                            .color(COLOR_TEXT_PRIMARY),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .padding([10, 18])
                .style(move |_t| container::Style {
                    background: Some(iced::Color::from_rgba(0.12, 0.12, 0.14, 0.95 * alpha).into()),
                    border: iced::border::rounded(20)
                        .color(iced::Color::from_rgba(0.35, 0.35, 0.38, alpha))
                        .width(1.0),
                    shadow: iced::Shadow {
                        color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.45 * alpha),
                        offset: iced::Vector::new(0.0, 4.0),
                        blur_radius: 12.0,
                    },
                    ..container::Style::default()
                });

                let toast_overlay = container(
                    column![
                        Space::with_height(Length::Fill),
                        row![
                            Space::with_width(Length::Fill),
                            toast_widget,
                            Space::with_width(Length::Fill),
                        ],
                        Space::with_height(Length::Fixed(32.0)),
                    ]
                )
                .width(Length::Fill)
                .height(Length::Fill)
                .into();

                layers.push(toast_overlay);
            }
        }

        if self.show_suggestions && !self.suggestions.is_empty() {
            layers.push(view_suggestions_overlay(
                &self.suggestions,
                Message::SelectSuggestion,
            ));
        }

        if self.show_auth_modal {
            layers.push(view_auth_modal(
                &self.auth_cookie_input,
                &self.auth_status,
                self.user_display_name.as_deref(),
                self.user_avatar_handle.as_ref(),
                self.config.preferred_resolution,
                self.config.preferred_codec,
                self.config.preferred_audio_lang,
                self.config.auto_pip_on_unfocus,
                self.config.full_buffer_tabs,
                self.config.max_cache_size_bytes,
                self.current_cache_size_bytes,
                Message::CookieInputChanged,
                Message::SaveRawCookie,
                Message::ImportFirefoxCookies,
                Message::ImportChromeCookies,
                Message::Logout,
                Message::OpenSubscriptions,
                Message::OpenHistory,
                Message::OpenPlaylists,
                Message::SetPreferredResolution,
                Message::SetPreferredCodec,
                Message::SetPreferredAudioLanguage,
                Message::ToggleAutoPipOnUnfocus,
                Message::ToggleFullBufferTabs,
                Message::SetCacheLimitIndex,
                Message::ClearCache,
                Message::ToggleLoginModal,
            ));
        }

        stack(layers)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let is_pip = self.pip_window_id.is_some();
        let is_watching = matches!(self.current_view, ViewMode::Watch(_) | ViewMode::VideoFullscreen) || is_pip;
        let is_transitioning = self.view_transition_start.is_some();

        let mut subs = Vec::new();
        // Filter Winit/Wayland events at the subscription boundary so irrelevant events
        // (WheelScrolled, KeyReleased, ModifiersChanged) never trigger view() rebuilds!
        subs.push(iced::event::listen_with(|event, status, window_id| {
            match event {
                iced::Event::Window(
                    iced::window::Event::Resized(_)
                    | iced::window::Event::Moved(_)
                    | iced::window::Event::Closed
                    | iced::window::Event::CloseRequested
                    | iced::window::Event::Focused
                    | iced::window::Event::Unfocused,
                ) => Some(Message::EventOccurred(event, window_id)),
                iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { ref key, modifiers, .. }) => {
                    let is_playback_shortcut = match key {
                        iced::keyboard::Key::Named(
                            iced::keyboard::key::Named::Space
                            | iced::keyboard::key::Named::ArrowLeft
                            | iced::keyboard::key::Named::ArrowRight
                            | iced::keyboard::key::Named::ArrowUp
                            | iced::keyboard::key::Named::ArrowDown
                            | iced::keyboard::key::Named::Escape,
                        ) => true,
                        iced::keyboard::Key::Character(c) => matches!(
                            c.as_str(),
                            "k" | "K" | "j" | "J" | "l" | "L" | "m" | "M" | "t" | "T" | "c" | "C" | "f" | "F" | "i" | "I" | "p" | "P"
                        ),
                        _ => false,
                    };

                    if status == iced::event::Status::Ignored
                        || modifiers.control()
                        || modifiers.command()
                        || is_playback_shortcut
                    {
                        Some(Message::EventOccurred(event, window_id))
                    } else {
                        None
                    }
                }
                iced::Event::Mouse(
                    iced::mouse::Event::ButtonPressed(_)
                    | iced::mouse::Event::ButtonReleased(_)
                    | iced::mouse::Event::CursorEntered
                    | iced::mouse::Event::CursorLeft,
                ) => Some(Message::EventOccurred(event, window_id)),
                iced::Event::Mouse(iced::mouse::Event::WheelScrolled { .. }) => {
                    Some(Message::EventOccurred(event, window_id))
                }
                iced::Event::Mouse(iced::mouse::Event::CursorMoved { .. }) => {
                    static LAST_MOUSE_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
                    let now_ms = START.get_or_init(std::time::Instant::now).elapsed().as_millis() as u64;
                    let prev_ms = LAST_MOUSE_MS.load(std::sync::atomic::Ordering::Relaxed);
                    if now_ms.saturating_sub(prev_ms) >= 16 {
                        LAST_MOUSE_MS.store(now_ms, std::sync::atomic::Ordering::Relaxed);
                        Some(Message::EventOccurred(event, window_id))
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }));

        let has_ripple = self.osd_state.play_pause_anim
            .map(|(s, _)| s.elapsed().as_secs_f32() < 0.50)
            .unwrap_or(false);
        let has_seek_anim = self.osd_state.seek_anim
            .map(|a| a.last_tap_time.elapsed().as_secs_f32() < 0.65)
            .unwrap_or(false);
        let is_fs_anim = self.fullscreen_anim.is_some();
        let is_loading_buffer = is_watching
            && (!self.osd_state.first_frame_rendered || self.osd_state.is_buffering)
            && !self.osd_state.has_playback_error;
        let has_toast = self.toast_message
            .as_ref()
            .map(|(_, t)| t.elapsed().as_secs_f32() < 3.0)
            .unwrap_or(false);
        let has_tab_shake = self.tab_shake_anim
            .map(|(_, t)| t.elapsed().as_secs_f32() < 0.6)
            .unwrap_or(false);
        let has_tab_open = self.tab_opened_anim
            .map(|(_, t)| t.elapsed().as_secs_f32() < 0.25)
            .unwrap_or(false);

        let is_video_active = is_watching && self.osd_state.is_playing && (self.is_window_focused || is_pip);

        if is_video_active {
            // Frame-driven presentation: stay continuously subscribed while video is playing on-screen
            // or in PiP so transient UI animations never drop or recreate the FRAME_NOTIFY stream.
            subs.push(Subscription::run(frame_ready_stream));
        }

        let has_active_animation = is_transitioning
            || has_ripple
            || has_seek_anim
            || is_fs_anim
            || is_loading_buffer
            || has_toast
            || has_tab_shake
            || has_tab_open;

        let has_playing_tab_glow = self.osd_state.is_playing
            && self.playing_video_id.is_some()
            && !self.is_window_minimized
            && !self.is_fullscreen;

        if has_active_animation && (self.is_window_focused || is_pip) {
            subs.push(iced::time::every(Duration::from_millis(16)).map(|_| Message::Tick));
        } else if has_playing_tab_glow {
            // Smooth ~31 FPS cadence for perimeter glow on playing tab, even when idle or unfocused
            subs.push(iced::time::every(Duration::from_millis(32)).map(|_| Message::Tick));
        } else if is_watching || self.osd_state.is_playing {
            let tick_rate = if !self.is_window_focused && !is_pip {
                Duration::from_secs(2)
            } else {
                match self.power_profile {
                    PowerProfile::Battery => Duration::from_millis(400),
                    PowerProfile::Ac => Duration::from_millis(250),
                }
            };
            subs.push(iced::time::every(tick_rate).map(|_| Message::Tick));
        }

        subs.push(Subscription::run(ipc_url_stream));

        Subscription::batch(subs)
    }

    pub fn theme(&self, _id: Id) -> iced::Theme {
        iced::Theme::Dark
    }

    pub fn style(&self, _theme: &iced::Theme) -> iced::daemon::Appearance {
        iced::daemon::Appearance {
            background_color: iced::Color::TRANSPARENT,
            text_color: COLOR_TEXT_PRIMARY,
        }
    }
}

fn window_state_path() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    Some(home.join(".config/mitub/window_state.json"))
}

fn load_window_state() -> ((f32, f32), bool) {
    if let Some(path) = window_state_path() {
        if let Ok(data) = fs::read_to_string(path) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&data) {
                let w = val.get("width").and_then(|v| v.as_f64()).unwrap_or(1280.0) as f32;
                let h = val.get("height").and_then(|v| v.as_f64()).unwrap_or(720.0) as f32;
                let max = val.get("maximized").and_then(|v| v.as_bool()).unwrap_or_else(|| {
                    w >= 1900.0 && h >= 1000.0
                });
                return ((w.clamp(600.0, 7680.0), h.clamp(400.0, 4320.0)), max);
            }
        }
    }
    ((1280.0, 720.0), false)
}

fn save_window_state(width: f32, height: f32, maximized: bool) {
    if width < 600.0 || height < 400.0 {
        return;
    }
    if let Some(path) = window_state_path() {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let data = serde_json::json!({
            "width": width,
            "height": height,
            "maximized": maximized
        });
        let _ = fs::write(path, data.to_string());
    }
}

fn load_app_icon() -> Option<window::Icon> {
    let icon_bytes = include_bytes!("../assets/icon.png");
    if let Ok(img) = image_crate::load_from_memory(icon_bytes) {
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        window::icon::from_rgba(rgba.into_raw(), w, h).ok()
    } else {
        None
    }
}

// Renamed alias to avoid confusion with iced::widget::image
use ::image as image_crate;

fn main() -> iced::Result {
    // Check if an argument (e.g. YouTube URL) was passed and an existing instance is already running
    if let Some(arg) = std::env::args().nth(1) {
        if try_send_to_existing_instance(&arg) {
            println!("[Mitub IPC] Sent URL to existing instance successfully: {arg}");
            return Ok(());
        }
    }

    // Set up cross-platform local TCP listener for single-instance IPC
    let port = get_ipc_port();
    if let Ok(listener) = std::net::TcpListener::bind(("127.0.0.1", port)) {
        let _ = listener.set_nonblocking(true);
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let _ = IPC_RX.set(tokio::sync::Mutex::new(Some(rx)));

        tokio::task::spawn(async move {
            if let Ok(tokio_listener) = tokio::net::TcpListener::from_std(listener) {
                use tokio::io::AsyncReadExt;
                while let Ok((mut socket, _)) = tokio_listener.accept().await {
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        let mut buf = vec![0u8; 2048];
                        if let Ok(n) = socket.read(&mut buf).await {
                            if n > 0 {
                                let content = String::from_utf8_lossy(&buf[..n]).trim().to_string();
                                if !content.is_empty() {
                                    let _ = tx.send(content);
                                }
                            }
                        }
                    });
                }
            }
        });
    }

    // 1. Hardware video acceleration environment setup
    std::env::set_var("GST_VAAPI_ALL_DRIVERS", "1");

    // 2. Bound Tokio runtime worker threads to 4 to eliminate excess thread-local mimalloc arenas
    if std::env::var_os("TOKIO_WORKER_THREADS").is_none() {
        std::env::set_var("TOKIO_WORKER_THREADS", "4");
    }

    // 3. Configure mimalloc to immediately return free pages to Linux kernel via MADV_DONTNEED
    power_manager::configure_allocator_low_memory();

    // 4. Default to OpenGL/GLES backend for wgpu if not explicitly overridden.
    if std::env::var_os("WGPU_BACKEND").is_none() {
        std::env::set_var("WGPU_BACKEND", "gl");
    }

    println!("[Mitub] Starting Native YouTube Client (100% Rust + libmpv + iced)");

    iced::daemon(MitubApp::title, MitubApp::update, MitubApp::view)
        .subscription(MitubApp::subscription)
        .theme(MitubApp::theme)
        .style(MitubApp::style)
        .run_with(MitubApp::new)
}
