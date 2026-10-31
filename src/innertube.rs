use crate::auth::AuthSession;
use crate::ui::watch_view::{extract_timestamps, extract_unique_urls};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, COOKIE, ORIGIN, USER_AGENT};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

const INNERTUBE_CLIENT_NAME: &str = "1";
const INNERTUBE_CLIENT_VERSION: &str = "2.20261015.01.00";
const YOUTUBE_ORIGIN: &str = "https://www.youtube.com";
const USER_AGENT_STR: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:132.0) Gecko/20100101 Firefox/132.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoCard {
    pub video_id: String,
    pub title: String,
    pub channel_name: String,
    pub channel_id: String,
    pub channel_avatar: String,
    pub views_str: String,
    pub published_str: String,
    pub duration_str: String,
    pub thumbnail_url: String,
    pub is_live: bool,
    pub meta_suffix: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamQuality {
    pub itag: u32,
    pub quality_label: String,
    pub mime_type: String,
    pub codecs: String,
    pub bitrate: u64,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub url: String,
    pub is_audio: bool,
    pub audio_track_id: String,
    pub audio_track_name: String,
    pub is_default_audio: bool,
    pub init_range: Option<(u64, u64)>,
    pub index_range: Option<(u64, u64)>,
    pub approx_duration_ms: u64,
}

impl StreamQuality {
    /// Returns the direct stream URL for playback.
    pub fn playback_uri(&self, _role: &str) -> String {
        self.url.clone()
    }

    /// On-demand single-stream probe for ISO MP4 boxes (ftyp, moov, sidx).
    /// Fetches only the first 8 KB to populate `init_range` and `index_range` for DASH MPD generation.
    pub async fn ensure_ranges(&mut self, client: &reqwest::Client) {
        if self.init_range.is_some() && self.index_range.is_some() {
            return;
        }
        if self.mime_type != "video/mp4" && self.mime_type != "audio/mp4" {
            return;
        }
        if let Ok(resp) = client.get(&self.url).header("Range", "bytes=0-8191").send().await {
            if let Ok(bytes) = resp.bytes().await {
                let (init, idx) = InnerTubeClient::parse_mp4_boxes(&bytes);
                self.init_range = init;
                self.index_range = idx;
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptionTrack {
    pub language_code: String,
    pub name: String,
    pub base_url: String,
    pub vss_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleCue {
    pub start_ms: u64,
    pub duration_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentItem {
    pub author: String,
    pub author_avatar: String,
    pub text: String,
    pub published_time: String,
    pub likes_count: String,
    #[serde(default)]
    pub parsed_urls: Vec<String>,
    #[serde(default)]
    pub parsed_timestamps: Vec<(String, u64)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoChapter {
    pub title: String,
    pub start_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchDetails {
    pub video_id: String,
    pub title: String,
    pub channel_name: String,
    pub channel_id: String,
    pub channel_avatar: String,
    pub subscriber_count: String,
    pub likes_str: String,
    #[serde(default)]
    pub dislikes_str: String,
    pub description: String,
    #[serde(default)]
    pub published_date: String,
    #[serde(default)]
    pub views_str: String,
    pub is_live: bool,
    pub is_subscribed: bool,
    pub like_status: String, // "LIKE", "DISLIKE", or "INDIFFERENT"
    pub live_chat_continuation: Option<String>,
    pub recommended_videos: Vec<VideoCard>,
    pub comments: Vec<CommentItem>,
    pub video_streams: Vec<StreamQuality>,
    pub audio_streams: Vec<StreamQuality>,
    pub caption_tracks: Vec<CaptionTrack>,
    #[serde(default)]
    pub chapters: Vec<VideoChapter>,
    #[serde(default)]
    pub parsed_desc_urls: Vec<String>,
    #[serde(default)]
    pub parsed_desc_timestamps: Vec<(String, u64)>,
    #[serde(default)]
    pub videostats_playback_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelDetails {
    pub channel_id: String,
    pub title: String,
    pub handle: String,
    pub subscriber_count: String,
    pub banner_url: Option<String>,
    pub avatar_url: String,
    pub is_subscribed: bool,
    pub tabs: Vec<ChannelTab>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelTab {
    pub title: String,
    pub tab_id: String,
    pub videos: Vec<VideoCard>,
}

pub struct InnerTubeClient {
    client: reqwest::Client,
    session: Arc<RwLock<AuthSession>>,
    visitor_data: Arc<RwLock<String>>,
    bot_detected: Arc<AtomicBool>,
    watch_cache: Arc<RwLock<HashMap<String, (Instant, Arc<WatchDetails>)>>>,
    feed_continuation: Arc<RwLock<Option<(String, String)>>>,
}

pub fn json_contains_str(val: &Value, needle: &str) -> bool {
    match val {
        Value::String(s) => s.contains(needle),
        Value::Array(arr) => arr.iter().any(|v| json_contains_str(v, needle)),
        Value::Object(map) => map.iter().any(|(k, v)| k.contains(needle) || json_contains_str(v, needle)),
        _ => false,
    }
}

pub fn json_has_subscribed_true(val: &Value) -> bool {
    match val {
        Value::Object(map) => {
            if let Some(v) = map.get("subscribed") {
                if v.as_bool() == Some(true) {
                    return true;
                }
            }
            map.values().any(json_has_subscribed_true)
        }
        Value::Array(arr) => arr.iter().any(json_has_subscribed_true),
        _ => false,
    }
}

impl InnerTubeClient {
    pub fn new(session: Arc<RwLock<AuthSession>>) -> Self {
        let client = reqwest::Client::builder()
            .gzip(true)
            .brotli(true)
            .tcp_nodelay(true)
            .pool_max_idle_per_host(16)
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            .connect_timeout(std::time::Duration::from_secs(8))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        let initial_vd = dirs::home_dir()
            .and_then(|h| std::fs::read_to_string(h.join(".config/mitub/visitor_data.txt")).ok())
            .unwrap_or_default()
            .trim()
            .to_string();

        Self {
            client,
            session,
            visitor_data: Arc::new(RwLock::new(initial_vd)),
            bot_detected: Arc::new(AtomicBool::new(false)),
            watch_cache: Arc::new(RwLock::new(HashMap::new())),
            feed_continuation: Arc::new(RwLock::new(None)),
        }
    }

    async fn capture_set_cookies(&self, headers: &HeaderMap) {
        let mut updated = Vec::new();
        for val in headers.get_all(reqwest::header::SET_COOKIE) {
            if let Ok(s) = val.to_str() {
                if let Some(first_part) = s.split(';').next() {
                    if let Some((k, v)) = first_part.split_once('=') {
                        let key = k.trim();
                        let value = v.trim();
                        if !value.is_empty() && value != "deleted" {
                            updated.push((key.to_string(), value.to_string()));
                        }
                    }
                }
            }
        }
        if !updated.is_empty() {
            let mut session = self.session.write().await;
            let mut changed = false;
            for (k, v) in updated {
                if session.cookies.get(&k) != Some(&v) {
                    session.set_cookie(&k, &v);
                    changed = true;
                }
            }
            if changed {
                let session_clone = session.clone();
                tokio::task::spawn_blocking(move || {
                    let _ = session_clone.save_to_disk();
                });
            }
        }
    }

    async fn resp_simd_json(resp: reqwest::Response) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let bytes = resp.bytes().await?;
        let mut buf = Vec::from(bytes);
        match simd_json::serde::from_slice::<Value>(&mut buf) {
            Ok(v) => Ok(v),
            Err(_) => Ok(serde_json::from_slice::<Value>(&buf)?),
        }
    }

    async fn capture_visitor_data(&self, json_val: &Value) {
        if let Some(vd) = json_val
            .get("responseContext")
            .and_then(|rc| rc.get("visitorData"))
            .and_then(|v| v.as_str())
        {
            if !vd.is_empty() {
                let mut lock = self.visitor_data.write().await;
                if *lock != vd {
                    *lock = vd.to_string();
                    let vd_clone = vd.to_string();
                    tokio::task::spawn_blocking(move || {
                        if let Some(home) = dirs::home_dir() {
                            let path = home.join(".config/mitub/visitor_data.txt");
                            let _ = std::fs::write(path, vd_clone);
                        }
                    });
                }
            }
        }
    }

    pub fn set_session(&self, new_session: AuthSession) {
        if let Ok(mut guard) = self.session.try_write() {
            *guard = new_session;
            return;
        }
        let session_lock = self.session.clone();
        tokio::spawn(async move {
            let mut s = session_lock.write().await;
            *s = new_session;
        });
    }

    async fn try_refresh_firefox_session(&self) -> bool {
        let fresh_res = tokio::task::spawn_blocking(AuthSession::from_firefox).await;
        if let Ok(Ok(fresh)) = fresh_res {
            if fresh.is_authenticated() {
                let mut s = self.session.write().await;
                *s = fresh;
                return true;
            }
        }
        false
    }

    async fn build_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_STR));
        headers.insert(ORIGIN, HeaderValue::from_static(YOUTUBE_ORIGIN));
        headers.insert("X-Origin", HeaderValue::from_static(YOUTUBE_ORIGIN));
        headers.insert("X-Goog-AuthUser", HeaderValue::from_static("0"));
        headers.insert("X-Youtube-Client-Name", HeaderValue::from_static(INNERTUBE_CLIENT_NAME));
        headers.insert("X-Youtube-Client-Version", HeaderValue::from_static(INNERTUBE_CLIENT_VERSION));

        let session = self.session.read().await;
        let cookie_str = session.cookie_header();
        if !cookie_str.is_empty() {
            if let Ok(cookie_val) = HeaderValue::from_str(&cookie_str) {
                headers.insert(COOKIE, cookie_val);
            }
        }

        if let Some(auth_val) = session.get_authorization_header() {
            if let Ok(auth_header) = HeaderValue::from_str(&auth_val) {
                headers.insert(AUTHORIZATION, auth_header);
            }
        }

        headers
    }

    fn build_public_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_STR));
        headers.insert(ORIGIN, HeaderValue::from_static(YOUTUBE_ORIGIN));
        headers.insert("X-Origin", HeaderValue::from_static(YOUTUBE_ORIGIN));
        headers.insert("X-Goog-AuthUser", HeaderValue::from_static("0"));
        headers.insert("X-Youtube-Client-Name", HeaderValue::from_static(INNERTUBE_CLIENT_NAME));
        headers.insert("X-Youtube-Client-Version", HeaderValue::from_static(INNERTUBE_CLIENT_VERSION));
        headers
    }

    fn innertube_context() -> Value {
        serde_json::json!({
            "client": {
                "clientName": "WEB",
                "clientVersion": INNERTUBE_CLIENT_VERSION,
                "hl": "es",
                "gl": "ES",
                "utcOffsetMinutes": 0
            }
        })
    }

    // =========================================================================
    // ANTI-SHORTS FIREWALL INSPECTION
    // =========================================================================

    /// Checks if a JSON node is a Shorts container or item that must be rejected
    pub fn is_shorts_object(val: &Value) -> bool {
        if let Some(obj) = val.as_object() {
            // Discard specific renderer keys
            if obj.contains_key("reelShelfRenderer")
                || obj.contains_key("shortsLockupViewModel")
                || obj.contains_key("reelItemRenderer")
            {
                return true;
            }

            // richShelfRenderer with isShorts == true
            if let Some(shelf) = obj.get("richShelfRenderer") {
                if shelf.get("isShorts").and_then(|v| v.as_bool()).unwrap_or(false) {
                    return true;
                }
            }

            // lockupViewModel with contentType == "LOCKUP_CONTENT_TYPE_SHORTS" or Members-Only badge
            if let Some(vm) = obj.get("lockupViewModel").or_else(|| obj.get("content").and_then(|c| c.get("lockupViewModel"))) {
                if let Some(ctype) = vm.get("contentType").and_then(|s| s.as_str()) {
                    if ctype == "LOCKUP_CONTENT_TYPE_SHORTS" {
                        return true;
                    }
                }
                if let Some(cid) = vm.get("contentId").and_then(|s| s.as_str()) {
                    if cid.contains("shorts") {
                        return true;
                    }
                }
                if json_contains_str(vm, "BADGE_STYLE_TYPE_MEMBERS_ONLY")
                    || json_contains_str(vm, "Solo para miembros")
                    || json_contains_str(vm, "Members only")
                    || json_contains_str(vm, "SPONSORSHIP_STAR")
                {
                    return true;
                }
            }

            // Inspect videoRenderer / compactVideoRenderer / gridVideoRenderer
            for renderer_key in &["videoRenderer", "compactVideoRenderer", "gridVideoRenderer"] {
                if let Some(renderer) = obj.get(*renderer_key) {
                    if let Some(nav) = renderer.get("navigationEndpoint") {
                        if nav.get("reelWatchEndpoint").is_some() {
                            return true;
                        }
                    }
                    if let Some(badges) = renderer.get("badges").and_then(|v| v.as_array()) {
                        for badge in badges {
                            if let Some(mb) = badge.get("metadataBadgeRenderer") {
                                let style = mb.get("style").and_then(|s| s.as_str()).unwrap_or("");
                                let label = mb.get("label").and_then(|s| s.as_str()).unwrap_or("");
                                if style == "BADGE_STYLE_TYPE_MEMBERS_ONLY"
                                    || label.contains("miembros")
                                    || label.contains("Members")
                                {
                                    return true;
                                }
                            }
                        }
                    }
                    if let Some(overlays) = renderer.get("thumbnailOverlays").and_then(|v| v.as_array()) {
                        for overlay in overlays {
                            if overlay.get("thumbnailOverlayInlineUnplayableRenderer").is_some() {
                                return true;
                            }
                            if let Some(time_status) = overlay.get("thumbnailOverlayTimeStatusRenderer") {
                                if let Some(style) = time_status.get("style").and_then(|s| s.as_str()) {
                                    if style == "SHORTS" {
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// Extracts VideoCard from standard renderer object or modern lockupViewModel
    pub fn parse_video_card(val: &Value) -> Option<VideoCard> {
        if Self::is_shorts_object(val) {
            return None;
        }

        // Support modern YouTube lockupViewModel
        let lockup = val.get("lockupViewModel")
            .or_else(|| val.get("content").and_then(|c| c.get("lockupViewModel")));

        if let Some(vm) = lockup {
            let ctype = vm.get("contentType").and_then(|s| s.as_str()).unwrap_or("");
            if ctype == "LOCKUP_CONTENT_TYPE_SHORTS"
                || ctype == "LOCKUP_CONTENT_TYPE_PLAYLIST"
                || ctype == "LOCKUP_CONTENT_TYPE_ALBUM"
            {
                return None;
            }

            let video_id_opt = vm.get("contentId")
                .and_then(|s| s.as_str())
                .filter(|s| !s.is_empty() && !s.starts_with("PL") && !s.starts_with("RD"))
                .or_else(|| {
                    vm.get("rendererContext")
                        .and_then(|r| r.get("commandContext"))
                        .and_then(|c| c.get("onTap"))
                        .and_then(|t| t.get("innertubeCommand").or(Some(t)))
                        .and_then(|i| i.get("watchEndpoint"))
                        .and_then(|w| w.get("videoId"))
                        .and_then(|s| s.as_str())
                })
                .or_else(|| {
                    vm.get("onTap")
                        .and_then(|t| t.get("innertubeCommand").or(Some(t)))
                        .and_then(|i| i.get("watchEndpoint"))
                        .and_then(|w| w.get("videoId"))
                        .and_then(|s| s.as_str())
                });

            let Some(raw_vid) = video_id_opt else {
                return None;
            };

            let video_id = if raw_vid.len() == 11 {
                raw_vid.to_string()
            } else if let Some(idx) = raw_vid.find("v=") {
                raw_vid[idx + 2..].chars().take(11).collect()
            } else if let Some(stripped) = raw_vid.strip_prefix("shorts/") {
                stripped.chars().take(11).collect()
            } else if raw_vid.len() > 11 {
                raw_vid.chars().take(11).collect()
            } else {
                return None;
            };

            let meta = vm.get("metadata").and_then(|m| m.get("lockupMetadataViewModel"));

            let title = meta
                .and_then(|m| m.get("title"))
                .and_then(|t| t.get("content"))
                .and_then(|s| s.as_str())
                .unwrap_or("Sin título")
                .to_string();

            let rows = meta
                .and_then(|m| m.get("metadata"))
                .and_then(|c| c.get("contentMetadataViewModel"))
                .and_then(|cm| cm.get("metadataRows"))
                .and_then(|arr| arr.as_array());

            let channel_name = rows
                .and_then(|r| r.first())
                .and_then(|row| row.get("metadataParts"))
                .and_then(|parts| parts.as_array())
                .and_then(|p| p.first())
                .and_then(|p| p.get("text"))
                .and_then(|t| t.get("content"))
                .and_then(|s| s.as_str())
                .unwrap_or("Canal")
                .to_string();

            let channel_id = meta
                .and_then(|m| m.get("image"))
                .and_then(|i| i.get("decoratedAvatarViewModel"))
                .and_then(|d| d.get("rendererContext"))
                .and_then(|r| r.get("commandContext"))
                .and_then(|c| c.get("onTap"))
                .and_then(|t| t.get("innertubeCommand"))
                .and_then(|ic| ic.get("browseEndpoint"))
                .and_then(|b| b.get("browseId"))
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();

            let channel_avatar = meta
                .and_then(|m| m.get("image"))
                .and_then(|i| i.get("decoratedAvatarViewModel"))
                .and_then(|d| d.get("avatar"))
                .and_then(|a| a.get("avatarViewModel"))
                .and_then(|av| av.get("image"))
                .and_then(|im| im.get("sources"))
                .and_then(|arr| arr.as_array())
                .and_then(|arr| arr.first())
                .and_then(|src| src.get("url"))
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();

            // Support both 2-row metadata (Home/Subscriptions) and 1-row metadata (FEhistory)
            let views_str = rows
                .and_then(|r| r.get(1))
                .and_then(|row| row.get("metadataParts"))
                .and_then(|parts| parts.as_array())
                .and_then(|p| p.first())
                .or_else(|| {
                    rows.and_then(|r| r.first())
                        .and_then(|row| row.get("metadataParts"))
                        .and_then(|parts| parts.as_array())
                        .and_then(|p| p.get(1))
                })
                .and_then(|p| p.get("text"))
                .and_then(|t| t.get("content"))
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();

            let published_str = rows
                .and_then(|r| r.get(1))
                .and_then(|row| row.get("metadataParts"))
                .and_then(|parts| parts.as_array())
                .and_then(|p| p.get(1))
                .or_else(|| {
                    rows.and_then(|r| r.first())
                        .and_then(|row| row.get("metadataParts"))
                        .and_then(|parts| parts.as_array())
                        .and_then(|p| p.get(2))
                })
                .and_then(|p| p.get("text"))
                .and_then(|t| t.get("content"))
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();

            let duration_str = vm.get("contentImage")
                .and_then(|ci| ci.get("thumbnailViewModel"))
                .and_then(|tv| tv.get("overlays"))
                .and_then(|arr| arr.as_array())
                .and_then(|arr| {
                    arr.iter().find_map(|o| {
                        o.get("thumbnailBottomOverlayViewModel")
                            .or_else(|| o.get("thumbnailOverlayBadgeViewModel"))
                            .and_then(|bo| bo.get("badges"))
                            .and_then(|b_arr| b_arr.as_array())
                            .and_then(|b_arr| b_arr.first())
                            .and_then(|b| b.get("thumbnailBadgeViewModel"))
                            .and_then(|tb| tb.get("text"))
                            .and_then(|s| s.as_str())
                    })
                })
                .unwrap_or("00:00")
                .to_string();

            let thumbnail_url = vm.get("contentImage")
                .and_then(|ci| ci.get("thumbnailViewModel"))
                .and_then(|tv| tv.get("image"))
                .and_then(|img| img.get("sources"))
                .and_then(|arr| arr.as_array())
                .and_then(|arr| {
                    arr.iter()
                        .filter(|item| item.get("width").and_then(|w| w.as_u64()).map(|w| w <= 480).unwrap_or(false))
                        .last()
                        .or_else(|| arr.last())
                })
                .and_then(|src| src.get("url"))
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();

            let thumbnail_url = if thumbnail_url.starts_with("//") {
                format!("https:{thumbnail_url}")
            } else if thumbnail_url.is_empty() {
                format!("https://i.ytimg.com/vi/{video_id}/mqdefault.jpg")
            } else {
                thumbnail_url
            };

            let is_live = duration_str.to_lowercase().contains("en vivo") || duration_str.to_lowercase().contains("live");

            let mut parts: Vec<&str> = Vec::with_capacity(2);
            if !views_str.is_empty() {
                parts.push(&views_str);
            }
            if !published_str.is_empty() {
                parts.push(&published_str);
            } else if !duration_str.is_empty() {
                parts.push(&duration_str);
            }
            let meta_suffix = if parts.is_empty() {
                String::new()
            } else {
                format!(" • {}", parts.join(" • "))
            };

            return Some(VideoCard {
                video_id,
                title,
                channel_name,
                channel_id,
                channel_avatar,
                views_str,
                published_str,
                duration_str,
                thumbnail_url,
                is_live,
                meta_suffix,
            });
        }

        let renderer = val.get("videoRenderer")
            .or_else(|| val.get("compactVideoRenderer"))
            .or_else(|| val.get("gridVideoRenderer"))?;

        let video_id = renderer.get("videoId")
            .and_then(|s| s.as_str())
            .or_else(|| {
                renderer.get("navigationEndpoint")
                    .and_then(|n| n.get("watchEndpoint"))
                    .and_then(|w| w.get("videoId"))
                    .and_then(|s| s.as_str())
            })?
            .to_string();

        let title = renderer.get("title")
            .and_then(|t| {
                t.get("simpleText")
                    .and_then(|s| s.as_str())
                    .or_else(|| {
                        t.get("runs")
                            .and_then(|r| r.as_array())
                            .and_then(|arr| arr.first())
                            .and_then(|f| f.get("text"))
                            .and_then(|txt| txt.as_str())
                    })
            })
            .unwrap_or("Sin título")
            .to_string();

        let channel_name = renderer.get("ownerText")
            .or_else(|| renderer.get("shortBylineText"))
            .and_then(|o| {
                o.get("runs")
                    .and_then(|r| r.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|f| f.get("text"))
                    .and_then(|txt| txt.as_str())
            })
            .unwrap_or("")
            .to_string();

        let channel_id = renderer.get("ownerText")
            .or_else(|| renderer.get("shortBylineText"))
            .and_then(|o| {
                o.get("runs")
                    .and_then(|r| r.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|f| f.get("navigationEndpoint"))
                    .and_then(|nav| nav.get("browseEndpoint"))
                    .and_then(|b| b.get("browseId"))
                    .and_then(|id| id.as_str())
            })
            .unwrap_or("")
            .to_string();

        let channel_avatar = renderer.get("channelThumbnailSupportedRenderers")
            .and_then(|c| c.get("channelThumbnailWithLinkRenderer"))
            .and_then(|t| t.get("thumbnail"))
            .and_then(|th| th.get("thumbnails"))
            .and_then(|arr| arr.as_array())
            .and_then(|arr| {
                // Prefer avatar size around 68-88px over 176+px to save download bandwidth & RAM
                arr.iter()
                    .filter(|item| item.get("width").and_then(|w| w.as_u64()).map(|w| w <= 100).unwrap_or(false))
                    .last()
                    .or_else(|| arr.first())
            })
            .and_then(|t| t.get("url"))
            .and_then(|u| u.as_str())
            .unwrap_or("")
            .to_string();

        let views_str = renderer.get("viewCountText")
            .or_else(|| renderer.get("shortViewCountText"))
            .and_then(|v| {
                v.get("simpleText")
                    .and_then(|s| s.as_str())
                    .map(|s| s.to_string())
                    .or_else(|| {
                        v.get("runs")
                            .and_then(|r| r.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|x| x.get("text").and_then(|t| t.as_str()))
                                    .collect::<Vec<_>>()
                                    .join("")
                            })
                    })
            })
            .unwrap_or_else(|| "0 vistas".to_string());

        let published_str = renderer.get("publishedTimeText")
            .and_then(|p| p.get("simpleText"))
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string();

        let duration_str = renderer.get("lengthText")
            .and_then(|l| l.get("simpleText"))
            .and_then(|s| s.as_str())
            .unwrap_or("00:00")
            .to_string();

        let thumbnail_url = renderer.get("thumbnail")
            .and_then(|th| th.get("thumbnails"))
            .and_then(|arr| arr.as_array())
            .and_then(|arr| {
                // Select a thumbnail <= 480px width (e.g. hqdefault/mqdefault) instead of maxresdefault (1280x720)
                arr.iter()
                    .filter(|item| item.get("width").and_then(|w| w.as_u64()).map(|w| w <= 480).unwrap_or(false))
                    .last()
                    .or_else(|| arr.last())
            })
            .and_then(|t| t.get("url"))
            .and_then(|u| u.as_str())
            .unwrap_or("")
            .to_string();

        let is_live = renderer.get("badges")
            .and_then(|b| b.as_array())
            .map(|arr| {
                arr.iter().any(|badge| {
                    badge.get("metadataBadgeRenderer")
                        .and_then(|m| m.get("style"))
                        .and_then(|s| s.as_str())
                        .map(|s| s == "BADGE_STYLE_TYPE_LIVE_NOW")
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);

        let mut parts: Vec<&str> = Vec::with_capacity(2);
        if !views_str.is_empty() {
            parts.push(&views_str);
        }
        if !published_str.is_empty() {
            parts.push(&published_str);
        } else if !duration_str.is_empty() {
            parts.push(&duration_str);
        }
        let meta_suffix = if parts.is_empty() {
            String::new()
        } else {
            format!(" • {}", parts.join(" • "))
        };

        Some(VideoCard {
            video_id,
            title,
            channel_name,
            channel_id,
            channel_avatar,
            views_str,
            published_str,
            duration_str,
            thumbnail_url,
            is_live,
            meta_suffix,
        })
    }

    /// Recursively filters and gathers VideoCards from any InnerTube JSON response
    pub fn collect_video_cards(val: &Value, list: &mut Vec<VideoCard>) {
        if Self::is_shorts_object(val) {
            return;
        }

        if let Some(card) = Self::parse_video_card(val) {
            list.push(card);
            return;
        }

        if let Some(arr) = val.as_array() {
            for item in arr {
                Self::collect_video_cards(item, list);
            }
        } else if let Some(obj) = val.as_object() {
            for (_, item) in obj {
                Self::collect_video_cards(item, list);
            }
        }
    }

    // =========================================================================
    // API METHODS
    // =========================================================================

    /// Loads Home feed: uses FEwhat_to_watch when authenticated, or Trending as fallback
    pub async fn get_home_feed(&self) -> Result<Vec<VideoCard>, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{YOUTUBE_ORIGIN}/youtubei/v1/browse?prettyPrint=false");
        let has_auth = self.session.read().await.is_authenticated();

        let mut videos = Vec::new();
        let mut next_token: Option<String> = None;

        if has_auth {
            let body = serde_json::json!({
                "context": Self::innertube_context(),
                "browseId": "FEwhat_to_watch"
            });

            for attempt in 0..2 {
                let headers = self.build_headers().await;
                if let Ok(resp) = self.client.post(&url).headers(headers).json(&body).send().await {
                    self.capture_set_cookies(resp.headers()).await;
                    if resp.status().is_success() {
                        if let Ok(json_val) = Self::resp_simd_json(resp).await {
                            self.capture_visitor_data(&json_val).await;
                            Self::collect_video_cards(&json_val, &mut videos);
                            next_token = Self::extract_feed_continuation_token(&json_val);
                        }
                    }
                }
                if !videos.is_empty() || attempt == 1 || !self.try_refresh_firefox_session().await {
                    break;
                }
            }
        }

        // Fallback if unauthenticated or if FEwhat_to_watch returned 0 items (e.g. history off or stale cookies)
        if videos.is_empty() {
            let public_headers = Self::build_public_headers();
            // YouTube's official Trending/Popular feed ID (FEtrending returns HTTP 400 in modern InnerTube)
            let fallback_body = serde_json::json!({
                "context": Self::innertube_context(),
                "browseId": "UC4R8DWoMoI7CAwX8_LjQHig"
            });
            if let Ok(r) = self.client.post(&url).headers(public_headers.clone()).json(&fallback_body).send().await {
                if r.status().is_success() {
                    if let Ok(val) = Self::resp_simd_json(r).await {
                        self.capture_visitor_data(&val).await;
                        Self::collect_video_cards(&val, &mut videos);
                        next_token = Self::extract_feed_continuation_token(&val);
                    }
                }
            }
        }

        // Secondary fallback to popular search if trending also returned nothing
        if videos.is_empty() {
            if let Ok(fallback_search) = self.search("tendencias").await {
                return Ok(fallback_search);
            }
        }

        *self.feed_continuation.write().await = next_token.map(|t| ("browse".to_string(), t));

        Ok(videos)
    }

    pub async fn get_history(&self) -> Result<Vec<VideoCard>, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{YOUTUBE_ORIGIN}/youtubei/v1/browse?prettyPrint=false");
        let body = serde_json::json!({
            "context": Self::innertube_context(),
            "browseId": "FEhistory"
        });

        let mut videos = Vec::new();
        let mut next_token: Option<String> = None;
        for attempt in 0..2 {
            let headers = self.build_headers().await;
            let resp = self.client.post(&url).headers(headers).json(&body).send().await?;
            self.capture_set_cookies(resp.headers()).await;
            let json_val: Value = Self::resp_simd_json(resp).await?;
            self.capture_visitor_data(&json_val).await;
            Self::collect_video_cards(&json_val, &mut videos);
            next_token = Self::extract_feed_continuation_token(&json_val);

            if !videos.is_empty() || attempt == 1 || !self.try_refresh_firefox_session().await {
                break;
            }
        }

        *self.feed_continuation.write().await = next_token.map(|t| ("browse".to_string(), t));

        Ok(videos)
    }

    /// Reports a watched video back to the user's YouTube Account History (`FEhistory`)
    /// using YouTube's official `playbackTracking.videostatsPlaybackUrl` endpoint.
    /// If `videostats_url` is already known from `WatchDetails`, avoids redundant `/player` fetch.
    pub async fn record_watch_history(&self, video_id: &str, videostats_url: Option<&str>) {
        if !self.session.read().await.is_authenticated() || video_id.is_empty() {
            return;
        }
        let headers = self.build_headers().await;

        let base_url_opt = if let Some(u) = videostats_url {
            Some(u.to_string())
        } else {
            let player_url = format!("{YOUTUBE_ORIGIN}/youtubei/v1/player?prettyPrint=false");
            let body = serde_json::json!({
                "context": Self::innertube_context(),
                "videoId": video_id
            });

            if let Ok(resp) = self.client.post(&player_url).headers(headers.clone()).json(&body).send().await {
                self.capture_set_cookies(resp.headers()).await;
                if let Ok(json_val) = Self::resp_simd_json(resp).await {
                    json_val
                        .get("playbackTracking")
                        .and_then(|pt| pt.get("videostatsPlaybackUrl"))
                        .and_then(|vp| vp.get("baseUrl"))
                        .and_then(|u| u.as_str())
                        .map(|s| s.to_string())
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(base_url) = base_url_opt {
            let stats_url = base_url.replace("https://s.youtube.com", YOUTUBE_ORIGIN);
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0);
            let cpn = format!("mitub{:011x}", ts & 0xfff_ffff_ffff);
            let full_url = format!("{stats_url}&ver=2&c=WEB&cpn={cpn}&cmt=0");
            if let Ok(stats_resp) = self.client.get(&full_url).headers(headers).send().await {
                self.capture_set_cookies(stats_resp.headers()).await;
            }
        }
    }

    /// Loads Subscriptions feed (FEsubscriptions) for the authenticated user
    pub async fn get_subscriptions_feed(&self) -> Result<Vec<VideoCard>, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{YOUTUBE_ORIGIN}/youtubei/v1/browse?prettyPrint=false");
        let body = serde_json::json!({
            "context": Self::innertube_context(),
            "browseId": "FEsubscriptions"
        });

        let mut videos = Vec::new();
        let mut next_token: Option<String> = None;
        for attempt in 0..2 {
            let headers = self.build_headers().await;
            let resp = self.client.post(&url).headers(headers).json(&body).send().await?;
            self.capture_set_cookies(resp.headers()).await;
            let json_val: Value = Self::resp_simd_json(resp).await?;
            self.capture_visitor_data(&json_val).await;
            Self::collect_video_cards(&json_val, &mut videos);
            next_token = Self::extract_feed_continuation_token(&json_val);

            if !videos.is_empty() || attempt == 1 || !self.try_refresh_firefox_session().await {
                break;
            }
        }

        *self.feed_continuation.write().await = next_token.map(|t| ("browse".to_string(), t));

        Ok(videos)
    }

    /// Loads user playlists / library feed (FEplaylist_aggregation / FElibrary)
    pub async fn get_playlists_feed(&self) -> Result<Vec<VideoCard>, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{YOUTUBE_ORIGIN}/youtubei/v1/browse?prettyPrint=false");
        let body = serde_json::json!({
            "context": Self::innertube_context(),
            "browseId": "FEplaylist_aggregation"
        });

        let mut videos = Vec::new();
        for attempt in 0..2 {
            let headers = self.build_headers().await;
            let resp = self.client.post(&url).headers(headers).json(&body).send().await?;
            self.capture_set_cookies(resp.headers()).await;
            let json_val: Value = Self::resp_simd_json(resp).await?;
            self.capture_visitor_data(&json_val).await;
            Self::collect_video_cards(&json_val, &mut videos);

            if !videos.is_empty() || attempt == 1 || !self.try_refresh_firefox_session().await {
                break;
            }
        }

        if videos.is_empty() {
            // Fallback to FElibrary
            let body = serde_json::json!({
                "context": Self::innertube_context(),
                "browseId": "FElibrary"
            });
            let headers = self.build_headers().await;
            if let Ok(resp) = self.client.post(&url).headers(headers).json(&body).send().await {
                self.capture_set_cookies(resp.headers()).await;
                if let Ok(json_val) = Self::resp_simd_json(resp).await {
                    Self::collect_video_cards(&json_val, &mut videos);
                }
            }
        }

        Ok(videos)
    }

    pub fn extract_user_profile(json_val: &Value) -> (Option<String>, Option<String>) {
        let mut avatar_url = None;
        let mut user_name = None;

        let sanitize_url = |u: &str| -> String {
            if u.starts_with("//") {
                format!("https:{u}")
            } else {
                u.to_string()
            }
        };

        // 1. Direct modern userProfile object in InnerTube guide response
        if let Some(up) = json_val.get("userProfile") {
            if let Some(name) = up.get("accountName").and_then(|n| n.as_str()) {
                if !name.trim().is_empty() {
                    user_name = Some(name.to_string());
                }
            }
            if let Some(url) = up.get("accountPhoto")
                .and_then(|p| p.get("thumbnails"))
                .and_then(|t| t.as_array())
                .and_then(|arr| arr.last())
                .and_then(|th| th.get("url"))
                .and_then(|u| u.as_str())
            {
                avatar_url = Some(sanitize_url(url));
            }
        }

        // 2. Fallbacks for topbar and guide items
        if user_name.is_none() || avatar_url.is_none() {
            if let Some(buttons) = json_val.get("topbar")
                .and_then(|t| t.get("desktopTopbarRenderer"))
                .and_then(|d| d.get("topbarButtons"))
                .and_then(|b| b.as_array())
            {
                for btn in buttons {
                    let menu_btn = btn.get("topbarMenuButtonRenderer");
                    if let Some(m) = menu_btn {
                        if avatar_url.is_none() {
                            if let Some(url) = m.get("avatar")
                                .and_then(|a| a.get("thumbnails"))
                                .and_then(|t| t.as_array())
                                .and_then(|arr| arr.last())
                                .and_then(|th| th.get("url"))
                                .and_then(|u| u.as_str())
                            {
                                avatar_url = Some(url.to_string());
                            }
                        }
                        if avatar_url.is_none() {
                            if let Some(url) = m.get("avatarViewModel")
                                .and_then(|a| a.get("image"))
                                .and_then(|i| i.get("sources"))
                                .and_then(|s| s.as_array())
                                .and_then(|arr| arr.last())
                                .and_then(|th| th.get("url"))
                                .and_then(|u| u.as_str())
                            {
                                avatar_url = Some(url.to_string());
                            }
                        }
                    }
                }
            }
        }

        if user_name.is_none() || avatar_url.is_none() {
            if let Some(items) = json_val.get("items").and_then(|i| i.as_array()) {
                for item in items {
                    if let Some(acct) = item.get("guideAccountProviderRenderer") {
                        if user_name.is_none() {
                            if let Some(name) = acct.get("accountName").and_then(|n| n.get("simpleText")).and_then(|s| s.as_str()) {
                                user_name = Some(name.to_string());
                            }
                        }
                        if avatar_url.is_none() {
                            if let Some(url) = acct.get("accountPhoto").and_then(|p| p.get("thumbnails")).and_then(|t| t.as_array()).and_then(|arr| arr.last()).and_then(|th| th.get("url")).and_then(|u| u.as_str()) {
                                avatar_url = Some(url.to_string());
                            }
                        }
                    }
                }
            }
        }

        (avatar_url, user_name)
    }

    /// Fetches the authenticated user's profile info (avatar URL and display name)
    pub async fn get_user_profile(&self) -> (Option<String>, Option<String>) {
        let has_auth = self.session.read().await.is_authenticated();
        if !has_auth {
            return (None, None);
        }
        let url = format!("{YOUTUBE_ORIGIN}/youtubei/v1/guide?prettyPrint=false");
        let headers = self.build_headers().await;
        let body = serde_json::json!({
            "context": Self::innertube_context()
        });
        if let Ok(resp) = self.client.post(&url).headers(headers).json(&body).send().await {
            if resp.status().is_success() {
                if let Ok(val) = Self::resp_simd_json(resp).await {
                    let (a, n) = Self::extract_user_profile(&val);
                    if a.is_some() || n.is_some() {
                        return (a, n);
                    }
                }
            }
        }
        (None, None)
    }

    pub async fn fetch_thumbnail_bytes(&self, url: &str) -> Option<Vec<u8>> {
        static THUMB_NET_SEM: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(12);
        if url.is_empty() {
            return None;
        }
        let full_url = if url.starts_with("//") {
            format!("https:{url}")
        } else {
            url.to_string()
        };

        use sha1::{Digest, Sha1};
        let mut hasher = Sha1::new();
        hasher.update(full_url.as_bytes());
        let filename = format!("{}.thumb", hex::encode(hasher.finalize()));

        let cache_dir = crate::cache_manager::thumbnail_cache_dir();
        let cache_path = cache_dir.join(&filename);

        if cache_path.is_file() {
            if let Ok(bytes) = tokio::fs::read(&cache_path).await {
                if !bytes.is_empty() {
                    return Some(bytes);
                }
            }
        }

        let _permit = THUMB_NET_SEM.acquire().await.ok()?;
        let mut fetched_bytes: Option<Vec<u8>> = None;

        for attempt in 0..3 {
            if attempt > 0 {
                tokio::time::sleep(Duration::from_millis(180 * attempt as u64)).await;
            }
            let req = self.client.get(&full_url).header(USER_AGENT, USER_AGENT_STR);
            if let Ok(Ok(resp)) = tokio::time::timeout(Duration::from_secs(6), req.send()).await {
                if resp.status().is_success() {
                    if let Ok(Ok(b)) = tokio::time::timeout(Duration::from_secs(6), resp.bytes()).await {
                        if !b.is_empty() {
                            fetched_bytes = Some(b.to_vec());
                            break;
                        }
                    }
                } else if resp.status() == reqwest::StatusCode::NOT_FOUND && full_url.contains("maxresdefault") {
                    let fallback_url = full_url.replace("maxresdefault", "hqdefault");
                    let fb_req = self.client.get(&fallback_url).header(USER_AGENT, USER_AGENT_STR);
                    if let Ok(Ok(fb_resp)) = tokio::time::timeout(Duration::from_secs(6), fb_req.send()).await {
                        if fb_resp.status().is_success() {
                            if let Ok(Ok(b)) = tokio::time::timeout(Duration::from_secs(6), fb_resp.bytes()).await {
                                if !b.is_empty() {
                                    fetched_bytes = Some(b.to_vec());
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        let bytes = fetched_bytes?;

        let bytes_to_save = bytes.clone();
        let path_to_save = cache_path.clone();
        tokio::spawn(async move {
            if let Some(parent) = path_to_save.parent() {
                let _ = tokio::fs::create_dir_all(parent).await;
            }
            let _ = tokio::fs::write(&path_to_save, &bytes_to_save).await;
        });

        Some(bytes)
    }

    /// Autocomplete suggestions while typing in search bar
    pub async fn get_search_suggestions(&self, query: &str) -> Vec<String> {
        if query.trim().is_empty() {
            return Vec::new();
        }

        let encoded_query = url::form_urlencoded::byte_serialize(query.as_bytes()).collect::<String>();
        let url = format!("https://suggestqueries-clients6.youtube.com/complete/search?client=youtube&ds=yt&q={encoded_query}");

        if let Ok(resp) = self.client.get(&url).header(USER_AGENT, USER_AGENT_STR).send().await {
            if let Ok(text) = resp.text().await {
                // Returns JSONP format: window.google.ac.h(["query",[["sugg1",0],["sugg2",0]...]])
                if let Some(start) = text.find('(') {
                    if let Some(end) = text.rfind(')') {
                        let json_slice = &text[start + 1..end];
                        if let Ok(Value::Array(items)) = serde_json::from_str(json_slice) {
                            if let Some(Value::Array(suggestions)) = items.get(1) {
                                return suggestions.iter().filter_map(|item| {
                                    item.as_array()?.first()?.as_str().map(|s| s.to_string())
                                }).collect();
                            }
                        }
                    }
                }
            }
        }

        Vec::new()
    }

    /// Searches YouTube videos
    pub async fn search(&self, query: &str) -> Result<Vec<VideoCard>, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{YOUTUBE_ORIGIN}/youtubei/v1/search?prettyPrint=false");
        let headers = self.build_headers().await;

        let body = serde_json::json!({
            "context": Self::innertube_context(),
            "query": query
        });

        let resp = self.client.post(&url)
            .headers(headers)
            .json(&body)
            .send()
            .await?;

        let json_val: Value = Self::resp_simd_json(resp).await?;
        self.capture_visitor_data(&json_val).await;
        let mut results = Vec::new();
        Self::collect_video_cards(&json_val, &mut results);
        let next_token = Self::extract_feed_continuation_token(&json_val);
        *self.feed_continuation.write().await = next_token.map(|t| ("search".to_string(), t));

        Ok(results)
    }

    /// Extracts the last continuation token from a feed/search response
    fn extract_feed_continuation_token(val: &Value) -> Option<String> {
        let mut last_token = None;
        Self::collect_continuation_tokens_recursive(val, &mut last_token);
        last_token
    }

    fn collect_continuation_tokens_recursive(val: &Value, out: &mut Option<String>) {
        if Self::is_shorts_object(val) {
            return;
        }
        if let Some(obj) = val.as_object() {
            if let Some(cir) = obj.get("continuationItemRenderer") {
                if let Some(tok) = cir
                    .get("continuationEndpoint")
                    .and_then(|ep| ep.get("continuationCommand"))
                    .and_then(|cmd| cmd.get("token"))
                    .and_then(|t| t.as_str())
                {
                    if !tok.is_empty() {
                        *out = Some(tok.to_string());
                    }
                }
            }
            if let Some(ncd) = obj.get("nextContinuationData") {
                if let Some(tok) = ncd.get("continuation").and_then(|t| t.as_str()) {
                    if !tok.is_empty() {
                        *out = Some(tok.to_string());
                    }
                }
            }
            for (_, v) in obj {
                Self::collect_continuation_tokens_recursive(v, out);
            }
        } else if let Some(arr) = val.as_array() {
            for item in arr {
                Self::collect_continuation_tokens_recursive(item, out);
            }
        }
    }

    pub async fn has_feed_continuation(&self) -> bool {
        self.feed_continuation.read().await.is_some()
    }

    /// Fetches the next batch of videos for infinite scrolling in Home/Subscriptions/History/Search
    pub async fn fetch_more_feed(&self) -> Result<Vec<VideoCard>, Box<dyn std::error::Error + Send + Sync>> {
        let current = self.feed_continuation.read().await.clone();
        let Some((endpoint, token)) = current else {
            return Ok(Vec::new());
        };

        let url = format!("{YOUTUBE_ORIGIN}/youtubei/v1/{endpoint}?prettyPrint=false");
        let headers = self.build_headers().await;
        let body = serde_json::json!({
            "context": Self::innertube_context(),
            "continuation": token
        });

        let resp = self.client.post(&url).headers(headers).json(&body).send().await?;
        self.capture_set_cookies(resp.headers()).await;
        let json_val: Value = Self::resp_simd_json(resp).await?;
        self.capture_visitor_data(&json_val).await;

        let mut videos = Vec::new();
        Self::collect_video_cards(&json_val, &mut videos);
        let next_token = Self::extract_feed_continuation_token(&json_val);

        *self.feed_continuation.write().await = next_token.map(|t| (endpoint, t));

        Ok(videos)
    }

    pub async fn invalidate_watch_cache(&self, video_id: &str) {
        self.watch_cache.write().await.remove(video_id);
    }

    /// Fetches player streams, adaptive formats, captions, and details for a video
    pub async fn get_watch_details(&self, video_id: &str) -> Result<Arc<WatchDetails>, Box<dyn std::error::Error + Send + Sync>> {
        {
            let cache = self.watch_cache.read().await;
            if let Some((inserted, details)) = cache.get(video_id) {
                if !details.is_live && inserted.elapsed() < Duration::from_secs(3600) && !details.video_streams.is_empty() {
                    return Ok(Arc::clone(details));
                }
            }
        }

        let headers = self.build_headers().await;
        let visitor_data = self.visitor_data.read().await.clone();
        let is_bot = self.bot_detected.load(Ordering::Relaxed);

        // Fetch dislikes (and fallback likes count) concurrently in background with a tight timeout
        let ryd_client = self.client.clone();
        let ryd_vid = video_id.to_string();
        let ryd_handle = tokio::spawn(async move {
            let ryd_url = format!("https://returnyoutubedislikeapi.com/votes?videoId={ryd_vid}");
            if let Ok(Ok(ryd_resp)) = tokio::time::timeout(
                Duration::from_millis(1000),
                ryd_client.get(&ryd_url).header(USER_AGENT, USER_AGENT_STR).send(),
            )
            .await
            {
                if ryd_resp.status().is_success() {
                    if let Ok(ryd_json) = Self::resp_simd_json(ryd_resp).await {
                        let d_str = ryd_json
                            .get("dislikes")
                            .and_then(|v| v.as_u64())
                            .map(Self::format_compact_count)
                            .unwrap_or_default();
                        let l_str = ryd_json
                            .get("likes")
                            .and_then(|v| v.as_u64())
                            .filter(|&l| l > 0)
                            .map(Self::format_compact_count)
                            .unwrap_or_default();
                        return (d_str, l_str);
                    }
                }
            }
            (String::new(), String::new())
        });

        let next_url = format!("{YOUTUBE_ORIGIN}/youtubei/v1/next?prettyPrint=false");
        let next_body = serde_json::json!({
            "context": Self::innertube_context(),
            "videoId": video_id
        });

        let player_url = format!("{YOUTUBE_ORIGIN}/youtubei/v1/player?prettyPrint=false");
        let visionos_ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 15_7_3) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15";

        let mut client_obj = serde_json::json!({
            "clientName": "VISIONOS",
            "clientVersion": "1.02",
            "deviceMake": "Apple",
            "deviceModel": "RealityDevice17,1",
            "userAgent": visionos_ua,
            "osName": "visionOS",
            "osVersion": "26.5.23O471",
            "hl": "es",
            "gl": "ES",
            "utcOffsetMinutes": 0
        });
        if !visitor_data.is_empty() {
            client_obj["visitorData"] = Value::String(visitor_data.clone());
        }

        let player_body = serde_json::json!({
            "context": {
                "client": client_obj
            },
            "videoId": video_id
        });

        let mut player_headers = HeaderMap::new();
        player_headers.insert(USER_AGENT, HeaderValue::from_static(visionos_ua));
        player_headers.insert(ORIGIN, HeaderValue::from_static(YOUTUBE_ORIGIN));
        player_headers.insert("X-Youtube-Client-Name", HeaderValue::from_static("101"));
        player_headers.insert("X-Youtube-Client-Version", HeaderValue::from_static("1.02"));
        if !visitor_data.is_empty() {
            if let Ok(hv) = HeaderValue::from_str(&visitor_data) {
                player_headers.insert("X-Goog-Visitor-Id", hv);
            }
        }
        if let Some(cookie) = headers.get(reqwest::header::COOKIE) {
            player_headers.insert(reqwest::header::COOKIE, cookie.clone());
        }
        // Note: Do NOT add Authorization header to VISIONOS player requests.
        // Google's VisionOS client rejects web SAPISIDHASH signatures with HTTP 400 INVALID_ARGUMENT.

        let (next_json, mut player_json): (Value, Value) = if is_bot {
            let next_resp = self.client.post(&next_url).headers(headers).json(&next_body).send().await?;
            let n_json: Value = Self::resp_simd_json(next_resp).await?;
            self.capture_visitor_data(&n_json).await;
            (n_json, Value::Null)
        } else if visitor_data.is_empty() {
            // Must fetch /next first to obtain visitorData required by VISIONOS
            let next_resp = self.client.post(&next_url).headers(headers).json(&next_body).send().await?;
            let n_json: Value = Self::resp_simd_json(next_resp).await?;
            self.capture_visitor_data(&n_json).await;
            let fresh_vd = self.visitor_data.read().await.clone();
            if !fresh_vd.is_empty() {
                client_obj["visitorData"] = Value::String(fresh_vd.clone());
                if let Ok(hv) = HeaderValue::from_str(&fresh_vd) {
                    player_headers.insert("X-Goog-Visitor-Id", hv);
                }
            }
            let player_body_fresh = serde_json::json!({
                "context": { "client": client_obj },
                "videoId": video_id
            });
            let p_res = self.client.post(&player_url).headers(player_headers).json(&player_body_fresh).send().await;
            let p_json = match p_res {
                Ok(r) => Self::resp_simd_json(r).await.unwrap_or(Value::Null),
                Err(_) => Value::Null,
            };
            (n_json, p_json)
        } else {
            let (next_res, player_res) = tokio::join!(
                self.client.post(&next_url).headers(headers).json(&next_body).send(),
                self.client.post(&player_url).headers(player_headers.clone()).json(&player_body).send(),
            );
            let next_resp = next_res?;
            let n_json: Value = Self::resp_simd_json(next_resp).await?;
            self.capture_visitor_data(&n_json).await;
            let p_json = match player_res {
                Ok(r) => Self::resp_simd_json(r).await.unwrap_or(Value::Null),
                Err(_) => Value::Null,
            };
            (n_json, p_json)
        };

        if let Some(status) = player_json.get("playabilityStatus").and_then(|p| p.get("status")).and_then(|s| s.as_str()) {
            if status == "LOGIN_REQUIRED" || status == "UNPLAYABLE" || status == "ERROR" {
                player_json = Value::Null;
            }
        }

        let mut video_streams = Vec::new();
        let mut audio_streams = Vec::new();

        // Extract adaptiveFormats
        if let Some(streaming_data) = player_json.get("streamingData") {
            if let Some(adaptive_formats) = streaming_data.get("adaptiveFormats").and_then(|v| v.as_array()) {
                for fmt in adaptive_formats {
                    let itag = fmt.get("itag").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    let mime_type_full = fmt.get("mimeType").and_then(|v| v.as_str()).unwrap_or("");
                    let quality_label = fmt.get("qualityLabel").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let bitrate = fmt.get("bitrate").and_then(|v| v.as_u64()).unwrap_or(0);
                    let width = fmt.get("width").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    let height = fmt.get("height").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    let fps = fmt.get("fps").and_then(|v| v.as_u64()).unwrap_or(30) as u32;
                    let url = fmt.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();

                    if url.is_empty() {
                        continue;
                    }

                    let is_audio = mime_type_full.starts_with("audio/");
                    let mut audio_track_id = "default".to_string();
                    let mut audio_track_name = String::new();
                    let mut is_default_audio = true;

                    if is_audio {
                        if let Some(at) = fmt.get("audioTrack") {
                            is_default_audio = at.get("audioIsDefault").and_then(|v| v.as_bool()).unwrap_or(true);
                            if let Some(id_str) = at.get("id").and_then(|v| v.as_str()) {
                                audio_track_id = id_str.to_string();
                            }
                            if let Some(dn) = at.get("displayName").and_then(|v| v.as_str()) {
                                audio_track_name = dn.to_string();
                            }
                        }
                    }

                    // Skip DRC duplicate tracks when standard track is present
                    if is_audio {
                        if let Some(xt) = fmt.get("xtags").and_then(|v| v.as_str()) {
                            if !xt.is_empty() && audio_track_id == "default" {
                                continue;
                            }
                        }
                    }

                    let parse_range = |key: &str| -> Option<(u64, u64)> {
                        let r = fmt.get(key)?;
                        let s = r.get("start")?.as_str()?.parse::<u64>().ok()?;
                        let e = r.get("end")?.as_str()?.parse::<u64>().ok()?;
                        Some((s, e))
                    };
                    let init_range = parse_range("initRange");
                    let index_range = parse_range("indexRange");
                    let approx_duration_ms = fmt
                        .get("approxDurationMs")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(0);

                    let codecs = if let Some(c_start) = mime_type_full.find("codecs=\"") {
                        let rem = &mime_type_full[c_start + 8..];
                        rem.split('"').next().unwrap_or("").to_string()
                    } else {
                        String::new()
                    };

                    let codec_tag = if codecs.starts_with("av01") {
                        "AV1"
                    } else if codecs.starts_with("vp9") || codecs.starts_with("vp09") {
                        "VP9"
                    } else if codecs.starts_with("avc1") {
                        "H264"
                    } else if codecs.contains("opus") {
                        "Opus"
                    } else if codecs.starts_with("mp4a") {
                        "AAC"
                    } else {
                        "Stream"
                    };

                    let kbps = (bitrate / 1000).max(1);
                    let display_label = if is_audio {
                        if !audio_track_name.is_empty() {
                            format!("{audio_track_name} ({codec_tag} {kbps}k)")
                        } else {
                            format!("Original ({codec_tag} {kbps}k)")
                        }
                    } else {
                        quality_label.clone()
                    };

                    let stream = StreamQuality {
                        itag,
                        quality_label: display_label,
                        mime_type: mime_type_full.split(';').next().unwrap_or("").to_string(),
                        codecs,
                        bitrate,
                        width,
                        height,
                        fps,
                        url,
                        is_audio,
                        audio_track_id,
                        audio_track_name: if audio_track_name.is_empty() {
                            format!("Original ({codec_tag} {kbps}k)")
                        } else {
                            format!("{audio_track_name} ({codec_tag} {kbps}k)")
                        },
                        is_default_audio,
                        init_range,
                        index_range,
                        approx_duration_ms,
                    };

                    if is_audio {
                        audio_streams.push(stream);
                    } else {
                        video_streams.push(stream);
                    }
                }
            }

            // Also parse non-adaptive formats as fallback
            if video_streams.is_empty() {
                if let Some(formats) = streaming_data.get("formats").and_then(|v| v.as_array()) {
                    for fmt in formats {
                        let itag = fmt.get("itag").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        let quality_label = fmt.get("qualityLabel").and_then(|v| v.as_str()).unwrap_or("360p").to_string();
                        let url = fmt.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        if !url.is_empty() {
                            video_streams.push(StreamQuality {
                                itag,
                                quality_label,
                                mime_type: "video/mp4".to_string(),
                                codecs: "avc1.42001E".to_string(),
                                bitrate: 500_000,
                                width: 640,
                                height: 360,
                                fps: 30,
                                url,
                                is_audio: false,
                                audio_track_id: String::new(),
                                audio_track_name: String::new(),
                                is_default_audio: false,
                                init_range: None,
                                index_range: None,
                                approx_duration_ms: 0,
                            });
                        }
                    }
                }
            }

            // If still empty, check for HLS manifest URL (live streams or live replays)
            if video_streams.is_empty() {
                if let Some(hls_url) = streaming_data.get("hlsManifestUrl").and_then(|v| v.as_str()) {
                    if !hls_url.is_empty() {
                        video_streams.push(StreamQuality {
                            itag: 9999,
                            quality_label: "Live / HLS".to_string(),
                            mime_type: "application/x-mpegURL".to_string(),
                            codecs: "avc1".to_string(),
                            bitrate: 1_500_000,
                            width: 1920,
                            height: 1080,
                            fps: 30,
                            url: hls_url.to_string(),
                            is_audio: false,
                            audio_track_id: String::new(),
                            audio_track_name: String::new(),
                            is_default_audio: false,
                            init_range: None,
                            index_range: None,
                            approx_duration_ms: 0,
                        });
                    }
                }
            }
        }

        // If InnerTube player API returned no playable video streams (e.g. YouTube bot detection LOGIN_REQUIRED or SABR streams),
        // fallback to extracting streams via yt-dlp with user cookies + node runtime!
        if video_streams.is_empty() {
            Self::extract_streams_via_ytdlp(&self.client, video_id, &mut video_streams, &mut audio_streams).await;
        }

        // Sort video_streams:
        // Highest resolution first (2160p (4K) -> 1440p -> 1080p -> 720p -> 480p -> 360p -> 240p -> 144p).
        // Within each resolution: WebM container first (for matroskademux HTTP byte-range seeking), then AV1 -> VP9 -> AVC1, then fps, bitrate.
        video_streams.sort_by(|a, b| {
            let codec_score = |c: &str| -> u32 {
                if c.starts_with("av01") {
                    10
                } else if c.starts_with("vp9") || c.starts_with("vp09") {
                    8
                } else {
                    5
                }
            };
            let is_webm_a = a.mime_type == "video/webm";
            let is_webm_b = b.mime_type == "video/webm";
            let sa = (a.height, is_webm_a, codec_score(&a.codecs), a.fps, a.bitrate);
            let sb = (b.height, is_webm_b, codec_score(&b.codecs), b.fps, b.bitrate);
            sb.cmp(&sa)
        });

        // Deduplicate video streams by (height, codec_family) so both VP9 and AV1 are available in the quality selector!
        let mut seen_height_codec = std::collections::HashSet::new();
        video_streams.retain(|s| {
            if s.height == 0 {
                return false;
            }
            let family = if s.codecs.starts_with("av01") {
                "AV1"
            } else if s.codecs.starts_with("vp9") || s.codecs.starts_with("vp09") {
                "VP9"
            } else {
                "AVC1"
            };
            // Keep VP9 and AV1 (and AVC1 only if neither VP9 nor AV1 exists at that height)
            if family == "AVC1" && seen_height_codec.iter().any(|&(h, _)| h == s.height) {
                return false;
            }
            seen_height_codec.insert((s.height, family))
        });

        // Sort audio_streams: original/default audio first, then WebM Opus, then bitrate
        audio_streams.sort_by(|a, b| {
            let is_orig_a = a.is_default_audio || a.quality_label.to_lowercase().contains("original") || a.quality_label.to_lowercase().contains("default");
            let is_orig_b = b.is_default_audio || b.quality_label.to_lowercase().contains("original") || b.quality_label.to_lowercase().contains("default");
            let is_webm_opus_a = a.mime_type == "audio/webm" && (a.itag == 251 || a.codecs.contains("opus"));
            let is_webm_opus_b = b.mime_type == "audio/webm" && (b.itag == 251 || b.codecs.contains("opus"));

            (is_orig_b, is_webm_opus_b, b.bitrate)
                .cmp(&(is_orig_a, is_webm_opus_a, a.bitrate))
        });

        // Deduplicate audio_streams by (audio_track_id, itag)
        let mut seen_audio = std::collections::HashSet::new();
        audio_streams.retain(|s| seen_audio.insert((s.audio_track_id.clone(), s.itag)));

        // Extract captions
        let mut caption_tracks = Vec::new();
        if let Some(captions) = player_json.get("captions") {
            if let Some(tracks) = captions
                .get("playerCaptionsTracklistRenderer")
                .and_then(|r| r.get("captionTracks"))
                .and_then(|arr| arr.as_array())
            {
                for track in tracks {
                    let language_code = track.get("languageCode").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let name = track.get("name")
                        .and_then(|n| n.get("simpleText"))
                        .and_then(|s| s.as_str())
                        .unwrap_or(&language_code)
                        .to_string();
                    let base_url = track.get("baseUrl").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let vss_id = track.get("vssId").and_then(|v| v.as_str()).unwrap_or("").to_string();

                    if !base_url.is_empty() {
                        caption_tracks.push(CaptionTrack {
                            language_code,
                            name,
                            base_url,
                            vss_id,
                        });
                    }
                }
            }
        }

        // Extract Title, Channel, Likes, Description from next_json & player_json
        let (title, channel_name, channel_id, channel_avatar, subscriber_count, mut likes_str, description, is_live, is_subscribed, like_status, published_date, views_str) =
            Self::parse_video_metadata(&next_json, &player_json);

        // Await the concurrent dislikes/likes lookup started earlier
        let (ryd_dislikes, ryd_likes) = ryd_handle.await.unwrap_or_default();
        let dislikes_str = ryd_dislikes;
        if (likes_str.is_empty() || likes_str == "Me gusta") && !ryd_likes.is_empty() {
            likes_str = ryd_likes;
        }

        // Extract Live Chat continuation token
        let live_chat_continuation = next_json
            .get("contents")
            .and_then(|c| c.get("twoColumnWatchNextResults"))
            .and_then(|t| t.get("conversationBar"))
            .and_then(|cb| cb.get("liveChatRenderer"))
            .and_then(|lcr| lcr.get("continuations"))
            .and_then(|arr| arr.as_array())
            .and_then(|arr| arr.first())
            .and_then(|c| c.get("reloadContinuationData").or_else(|| c.get("timedContinuationData")))
            .and_then(|rcd| rcd.get("continuation"))
            .and_then(|s| s.as_str())
            .map(|s| s.to_string());

        // Extract recommended videos from secondaryResults (Anti-Shorts filtered)
        let mut recommended_videos = Vec::new();
        if let Some(secondary) = next_json
            .get("contents")
            .and_then(|c| c.get("twoColumnWatchNextResults"))
            .and_then(|t| t.get("secondaryResults"))
        {
            Self::collect_video_cards(secondary, &mut recommended_videos);
        }

        // Extract top comments (try direct first, fallback to continuation token with timeout)
        let mut comments = Self::parse_comments(&next_json);
        if comments.is_empty() {
            if let Some(token) = Self::extract_comment_continuation(&next_json) {
                if let Ok(Ok(fetched)) = tokio::time::timeout(
                    Duration::from_millis(750),
                    self.get_comments_by_continuation(&token),
                )
                .await
                {
                    comments = fetched;
                }
            }
        }

        let mut chapters = Self::extract_chapters(&next_json, &description);
        if chapters.is_empty() {
            let mut rec_chapters = Vec::new();
            Self::collect_chapters_recursive(&player_json, &mut rec_chapters);
            rec_chapters.sort_by_key(|c| c.start_secs);
            rec_chapters.dedup_by_key(|c| c.start_secs);
            if rec_chapters.len() >= 2 {
                chapters = rec_chapters;
            }
        }

        let parsed_desc_urls = extract_unique_urls(&description);
        let parsed_desc_timestamps = extract_timestamps(&description);

        let videostats_playback_url = player_json
            .get("playbackTracking")
            .and_then(|pt| pt.get("videostatsPlaybackUrl"))
            .and_then(|vp| vp.get("baseUrl"))
            .and_then(|u| u.as_str())
            .map(|s| s.to_string());

        let details = Arc::new(WatchDetails {
            video_id: video_id.to_string(),
            title,
            channel_name,
            channel_id,
            channel_avatar,
            subscriber_count,
            likes_str,
            dislikes_str,
            description,
            published_date,
            views_str,
            is_live,
            is_subscribed,
            like_status,
            live_chat_continuation,
            recommended_videos,
            comments,
            video_streams,
            audio_streams,
            caption_tracks,
            chapters,
            parsed_desc_urls,
            parsed_desc_timestamps,
            videostats_playback_url,
        });

        if !details.is_live {
            self.watch_cache.write().await.insert(video_id.to_string(), (Instant::now(), Arc::clone(&details)));
        }
        Ok(details)
    }

    pub fn find_ytdlp_binary_pub() -> std::path::PathBuf {
        Self::find_ytdlp_binary()
    }

    fn find_ytdlp_binary() -> std::path::PathBuf {
        if let Some(home) = dirs::home_dir() {
            let user_local = home.join(".local/bin/yt-dlp");
            if user_local.exists() {
                return user_local;
            }
        }
        if std::path::Path::new("/usr/bin/yt-dlp").exists() {
            return std::path::PathBuf::from("/usr/bin/yt-dlp");
        }
        std::path::PathBuf::from("yt-dlp")
    }

    async fn extract_streams_via_ytdlp(
        _client: &reqwest::Client,
        video_id: &str,
        video_streams: &mut Vec<StreamQuality>,
        audio_streams: &mut Vec<StreamQuality>,
    ) {
        let binary = Self::find_ytdlp_binary();
        let target_url = format!("https://www.youtube.com/watch?v={video_id}");

        let path_env = std::env::var("PATH").unwrap_or_default();
        let mut combined_path = path_env;
        if let Some(home) = dirs::home_dir() {
            let p1 = home.join(".local/bin");
            let p2 = home.join(".bun/bin");
            for p in [p1, p2] {
                let s = p.to_string_lossy();
                if !combined_path.contains(&*s) {
                    combined_path = format!("{s}:{combined_path}");
                }
            }
        }
        for sys_dir in ["/usr/local/bin", "/usr/bin", "/bin"] {
            if !combined_path.contains(sys_dir) {
                combined_path.push(':');
                combined_path.push_str(sys_dir);
            }
        }

        // 1st attempt: direct fast extraction (avoids stale cookie errors and loads in ~2.5s)
        let mut cmd = tokio::process::Command::new(&binary);
        cmd.env("PATH", &combined_path)
            .arg("--js-runtimes")
            .arg("node")
            .arg("--js-runtimes")
            .arg("bun")
            .arg("-J")
            .arg("--no-playlist")
            .arg("--no-warnings")
            .arg(&target_url);

        let output = match cmd.output().await {
            Ok(out) if out.status.success() => Some(out.stdout),
            Ok(out) => {
                eprintln!("[yt-dlp] 1st attempt exit {:?}, trying fallback with firefox cookies", out.status.code());
                // 2nd fallback attempt: with firefox cookies (for age-restricted or members-only videos)
                let mut fallback = tokio::process::Command::new(&binary);
                fallback.env("PATH", &combined_path)
                    .arg("--js-runtimes")
                    .arg("node")
                    .arg("--js-runtimes")
                    .arg("bun")
                    .arg("--cookies-from-browser")
                    .arg("firefox")
                    .arg("-J")
                    .arg("--no-playlist")
                    .arg("--no-warnings")
                    .arg(&target_url);
                match fallback.output().await {
                    Ok(f_out) if f_out.status.success() => Some(f_out.stdout),
                    _ => None,
                }
            }
            Err(e) => {
                eprintln!("[yt-dlp] spawn error: {e}");
                None
            }
        };

        let Some(stdout) = output else {
            return;
        };

        let Ok(json) = serde_json::from_slice::<Value>(&stdout) else {
            return;
        };

        let duration = json.get("duration").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let approx_duration_ms = (duration * 1000.0) as u64;

        if let Some(formats) = json.get("formats").and_then(|v| v.as_array()) {
            for fmt in formats {
                let url = fmt.get("url").and_then(|v| v.as_str()).unwrap_or("");
                if url.is_empty() {
                    continue;
                }

                let itag = fmt
                    .get("format_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse::<u32>().ok())
                    .unwrap_or(0);

                let ext = fmt.get("ext").and_then(|v| v.as_str()).unwrap_or("");
                let vcodec = fmt.get("vcodec").and_then(|v| v.as_str()).unwrap_or("none");
                let acodec = fmt.get("acodec").and_then(|v| v.as_str()).unwrap_or("none");

                if vcodec == "none" && acodec == "none" {
                    continue; // Skip storyboards/thumbnails
                }

                let is_audio = vcodec == "none" && acodec != "none";
                let mime_type = if is_audio {
                    if ext == "webm" { "audio/webm" } else { "audio/mp4" }
                } else {
                    if ext == "webm" { "video/webm" } else { "video/mp4" }
                }
                .to_string();

                let codecs = if is_audio {
                    acodec.to_string()
                } else {
                    vcodec.to_string()
                };

                let width = fmt.get("width").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                let height = fmt.get("height").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                let fps = fmt.get("fps").and_then(|v| v.as_u64()).unwrap_or(30) as u32;
                let bitrate = fmt
                    .get("tbr")
                    .and_then(|v| v.as_f64())
                    .map(|k| (k * 1000.0) as u64)
                    .unwrap_or(0);

                let codec_tag = if codecs.starts_with("av01") {
                    "AV1"
                } else if codecs.starts_with("vp9") || codecs.starts_with("vp09") {
                    "VP9"
                } else if codecs.starts_with("avc1") {
                    "H264"
                } else if codecs.contains("opus") {
                    "Opus"
                } else if codecs.starts_with("mp4a") {
                    "AAC"
                } else {
                    "Stream"
                };

                let kbps = (bitrate / 1000).max(1);
                let format_note = fmt.get("format_note").and_then(|v| v.as_str()).unwrap_or("");
                let lang = fmt.get("language").and_then(|v| v.as_str()).unwrap_or("");
                let note_lower = format_note.to_lowercase();

                let is_default_audio = if is_audio {
                    note_lower.contains("default")
                        || note_lower.contains("original")
                        || lang.starts_with("es")
                        || fmt.get("language_preference").and_then(|v| v.as_i64()).map(|p| p >= 0).unwrap_or(false)
                } else {
                    false
                };

                let audio_display_name = if is_audio {
                    if !format_note.is_empty() {
                        let clean_note = format_note.split(',').next().unwrap_or(format_note).trim();
                        format!("{clean_note} ({codec_tag} {kbps}k)")
                    } else if !lang.is_empty() {
                        format!("{lang} ({codec_tag} {kbps}k)")
                    } else {
                        format!("Audio ({codec_tag} {kbps}k)")
                    }
                } else {
                    String::new()
                };

                let quality_label = if is_audio {
                    audio_display_name.clone()
                } else {
                    if !format_note.is_empty() {
                        format_note.to_string()
                    } else {
                        format!("{height}p")
                    }
                };

                let parse_rng = |key: &str| -> Option<(u64, u64)> {
                    let v = fmt.get(key)?;
                    if let Some(arr) = v.as_array() {
                        let s = arr.first()?.as_u64()?;
                        let e = arr.get(1)?.as_u64()?;
                        Some((s, e))
                    } else if let Some(obj) = v.as_object() {
                        let s = obj.get("start")?.as_str()?.parse::<u64>().ok().or_else(|| obj.get("start")?.as_u64())?;
                        let e = obj.get("end")?.as_str()?.parse::<u64>().ok().or_else(|| obj.get("end")?.as_u64())?;
                        Some((s, e))
                    } else {
                        None
                    }
                };
                let init_range = parse_rng("init_range").or_else(|| parse_rng("initRange"));
                let index_range = parse_rng("index_range").or_else(|| parse_rng("indexRange"));

                let stream = StreamQuality {
                    itag,
                    quality_label,
                    mime_type,
                    codecs,
                    bitrate,
                    width,
                    height,
                    fps,
                    url: url.to_string(),
                    is_audio,
                    audio_track_id: if !lang.is_empty() { lang.to_string() } else { "default".to_string() },
                    audio_track_name: audio_display_name,
                    is_default_audio,
                    init_range,
                    index_range,
                    approx_duration_ms,
                };

                if is_audio {
                    audio_streams.push(stream);
                } else {
                    video_streams.push(stream);
                }
            }
        }
    }

    pub fn parse_mp4_boxes(data: &[u8]) -> (Option<(u64, u64)>, Option<(u64, u64)>) {
        let mut offset = 0;
        let mut moov_end = None;
        let mut sidx_range = None;

        while offset + 8 <= data.len() {
            let size = u32::from_be_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]]) as usize;
            if size < 8 || offset + size > data.len() + 65536 {
                break;
            }
            let box_type = &data[offset + 4..offset + 8];
            let start = offset as u64;
            let end = (offset + size - 1) as u64;

            if box_type == b"moov" {
                moov_end = Some(end);
            } else if box_type == b"sidx" {
                sidx_range = Some((start, end));
            }

            offset += size;
        }

        let init_range = moov_end.map(|end| (0, end));
        (init_range, sidx_range)
    }

    pub fn parse_timestamp_secs(raw: &str) -> Option<u64> {
        let trimmed = raw.trim_matches(|c: char| !c.is_ascii_digit() && c != ':');
        if trimmed.is_empty() || !trimmed.contains(':') {
            return None;
        }
        let parts: Vec<&str> = trimmed.split(':').collect();
        match parts.as_slice() {
            [m_str, s_str] => {
                if m_str.len() > 3 || s_str.len() != 2 {
                    return None;
                }
                let m = m_str.parse::<u64>().ok()?;
                let s = s_str.parse::<u64>().ok()?;
                if s >= 60 {
                    return None;
                }
                Some(m * 60 + s)
            }
            [h_str, m_str, s_str] => {
                if h_str.len() > 2 || m_str.len() != 2 || s_str.len() != 2 {
                    return None;
                }
                let h = h_str.parse::<u64>().ok()?;
                let m = m_str.parse::<u64>().ok()?;
                let s = s_str.parse::<u64>().ok()?;
                if m >= 60 || s >= 60 {
                    return None;
                }
                Some(h * 3600 + m * 60 + s)
            }
            _ => None,
        }
    }

    fn extract_chapter_renderer(cr: &Value) -> Option<VideoChapter> {
        let title = cr
            .get("title")
            .and_then(|t| {
                t.get("simpleText")
                    .and_then(|s| s.as_str())
                    .map(|s| s.to_string())
                    .or_else(|| {
                        t.get("runs")
                            .and_then(|r| r.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|x| x.get("text").and_then(|s| s.as_str()))
                                    .collect::<Vec<_>>()
                                    .join("")
                            })
                    })
            })?
            .trim()
            .to_string();
        if title.is_empty() {
            return None;
        }
        let start_ms = cr
            .get("timeRangeStartMillis")
            .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok())))?;
        Some(VideoChapter {
            title,
            start_secs: start_ms / 1000,
        })
    }

    fn extract_macro_marker_renderer(mr: &Value) -> Option<VideoChapter> {
        let title = mr
            .get("title")
            .and_then(|t| {
                t.get("simpleText")
                    .and_then(|s| s.as_str())
                    .map(|s| s.to_string())
                    .or_else(|| {
                        t.get("runs")
                            .and_then(|r| r.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|x| x.get("text").and_then(|s| s.as_str()))
                                    .collect::<Vec<_>>()
                                    .join("")
                            })
                    })
            })?
            .trim()
            .to_string();
        if title.is_empty() {
            return None;
        }

        let start_secs = mr
            .get("timeRangeStartMillis")
            .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok())))
            .map(|ms| ms / 1000)
            .or_else(|| {
                mr.get("onTap")
                    .and_then(|ot| ot.get("watchEndpoint"))
                    .and_then(|we| we.get("startTimeSeconds"))
                    .and_then(|v| v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok())))
            })
            .or_else(|| {
                mr.get("timeDescription")
                    .and_then(|td| td.get("simpleText").and_then(|s| s.as_str()))
                    .and_then(Self::parse_timestamp_secs)
            })?;

        Some(VideoChapter {
            title,
            start_secs,
        })
    }

    fn collect_chapters_recursive(val: &Value, out: &mut Vec<VideoChapter>) {
        if let Some(obj) = val.as_object() {
            if let Some(cr) = obj.get("chapterRenderer") {
                if let Some(ch) = Self::extract_chapter_renderer(cr) {
                    out.push(ch);
                }
                return;
            }
            if let Some(mr) = obj.get("macroMarkersListItemRenderer") {
                if let Some(ch) = Self::extract_macro_marker_renderer(mr) {
                    out.push(ch);
                }
                return;
            }
            for (_, v) in obj {
                Self::collect_chapters_recursive(v, out);
            }
        } else if let Some(arr) = val.as_array() {
            for item in arr {
                Self::collect_chapters_recursive(item, out);
            }
        }
    }

    pub fn parse_chapters_from_description(description: &str) -> Vec<VideoChapter> {
        let mut chapters = Vec::new();
        for line in description.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.len() > 220 {
                continue;
            }
            let tokens: Vec<&str> = trimmed.split_whitespace().collect();
            let mut found_idx = None;
            let mut found_secs = 0u64;
            for (idx, tok) in tokens.iter().enumerate() {
                if let Some(secs) = Self::parse_timestamp_secs(tok) {
                    found_idx = Some(idx);
                    found_secs = secs;
                    break;
                }
            }
            if let Some(idx) = found_idx {
                let mut rem_tokens = Vec::with_capacity(tokens.len());
                for (i, &t) in tokens.iter().enumerate() {
                    if i != idx {
                        rem_tokens.push(t);
                    }
                }
                let raw_title = rem_tokens.join(" ");
                let clean_title = raw_title
                    .trim_matches(|c: char| {
                        c.is_whitespace()
                            || matches!(c, '-' | '–' | '—' | ':' | '|' | '•' | '·' | '(' | ')' | '[' | ']')
                    })
                    .trim()
                    .to_string();
                if clean_title.chars().count() >= 2
                    && !clean_title.starts_with("http://")
                    && !clean_title.starts_with("https://")
                {
                    chapters.push(VideoChapter {
                        title: clean_title,
                        start_secs: found_secs,
                    });
                }
            }
        }
        chapters.sort_by_key(|c| c.start_secs);
        chapters.dedup_by_key(|c| c.start_secs);
        if chapters.len() >= 2 {
            chapters
        } else {
            Vec::new()
        }
    }

    pub fn extract_chapters(next_json: &Value, description: &str) -> Vec<VideoChapter> {
        let mut chapters = Vec::new();

        // 1. Check multiMarkersPlayerBarRenderer.markersMap (DESCRIPTION_CHAPTERS or AUTO_CHAPTERS)
        if let Some(markers_map) = next_json
            .get("playerOverlays")
            .and_then(|p| p.get("playerOverlayRenderer"))
            .and_then(|p| p.get("decoratedPlayerBarRenderer"))
            .and_then(|d| d.get("decoratedPlayerBarRenderer"))
            .and_then(|d| d.get("playerBar"))
            .and_then(|pb| pb.get("multiMarkersPlayerBarRenderer"))
            .and_then(|mm| mm.get("markersMap"))
            .and_then(|m| m.as_array())
        {
            for entry in markers_map {
                if let Some(ch_arr) = entry
                    .get("value")
                    .and_then(|v| v.get("chapters"))
                    .and_then(|c| c.as_array())
                {
                    let mut candidate = Vec::new();
                    for item in ch_arr {
                        if let Some(cr) = item.get("chapterRenderer") {
                            if let Some(ch) = Self::extract_chapter_renderer(cr) {
                                candidate.push(ch);
                            }
                        }
                    }
                    if candidate.len() >= 2 {
                        chapters = candidate;
                        break;
                    }
                }
            }
        }

        // 2. Fallback: recursive search for chapterRenderer in next_json
        if chapters.len() < 2 {
            let mut rec_chapters = Vec::new();
            Self::collect_chapters_recursive(next_json, &mut rec_chapters);
            rec_chapters.sort_by_key(|c| c.start_secs);
            rec_chapters.dedup_by_key(|c| c.start_secs);
            if rec_chapters.len() >= 2 {
                chapters = rec_chapters;
            }
        }

        // 3. Fallback: parse timestamps from video description text
        if chapters.len() < 2 && !description.is_empty() {
            chapters = Self::parse_chapters_from_description(description);
        }

        chapters.sort_by_key(|c| c.start_secs);
        chapters.dedup_by_key(|c| c.start_secs);
        chapters
    }

    pub async fn fetch_captions(&self, base_url: &str) -> Result<Vec<SubtitleCue>, Box<dyn std::error::Error + Send + Sync>> {
        let url = if base_url.contains("fmt=") {
            base_url.to_string()
        } else {
            format!("{base_url}&fmt=json3")
        };
        let resp = self.client.get(&url).send().await?;
        let json: Value = Self::resp_simd_json(resp).await?;
        let mut cues = Vec::new();
        if let Some(events) = json.get("events").and_then(|e| e.as_array()) {
            for ev in events {
                let start_ms = ev.get("tStartMs").and_then(|v| v.as_u64()).unwrap_or(0);
                let duration_ms = ev.get("dDurationMs").and_then(|v| v.as_u64()).unwrap_or(0);
                let mut text = String::new();
                if let Some(segs) = ev.get("segs").and_then(|s| s.as_array()) {
                    for s in segs {
                        if let Some(utf8) = s.get("utf8").and_then(|u| u.as_str()) {
                            text.push_str(utf8);
                        }
                    }
                }
                let clean_text = text.trim().to_string();
                if !clean_text.is_empty() {
                    cues.push(SubtitleCue {
                        start_ms,
                        duration_ms,
                        text: clean_text,
                    });
                }
            }
        }
        Ok(cues)
    }

    fn parse_video_metadata(next_json: &Value, player_json: &Value) -> (String, String, String, String, String, String, String, bool, bool, String, String, String) {
        let video_details = player_json.get("videoDetails");

        let mut title = video_details
            .and_then(|v| v.get("title"))
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();

        let mut channel_name = video_details
            .and_then(|v| v.get("author"))
            .and_then(|a| a.as_str())
            .unwrap_or("")
            .to_string();

        let mut channel_id = video_details
            .and_then(|v| v.get("channelId"))
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();

        let is_live = video_details
            .and_then(|v| v.get("isLiveContent"))
            .and_then(|l| l.as_bool())
            .unwrap_or(false);

        let mut description = video_details
            .and_then(|v| v.get("shortDescription"))
            .and_then(|d| d.as_str())
            .unwrap_or("")
            .to_string();

        let mut avatar = String::new();
        let mut subs = String::new();
        let mut likes = "Me gusta".to_string();
        let mut published_date = String::new();
        let mut views_str = String::new();

        let mut is_subscribed = false;
        let mut like_status = "INDIFFERENT".to_string();

        // Extract rich metadata from next_json (videoPrimaryInfoRenderer & videoSecondaryInfoRenderer)
        if let Some(results) = next_json
            .get("contents")
            .and_then(|c| c.get("twoColumnWatchNextResults"))
            .and_then(|t| t.get("results"))
            .and_then(|r| r.get("results"))
            .and_then(|r| r.get("contents"))
            .and_then(|arr| arr.as_array())
        {
            for item in results {
                if let Some(primary) = item.get("videoPrimaryInfoRenderer") {
                    if let Some(dt) = primary.get("dateText").and_then(|d| d.get("simpleText")).and_then(|s| s.as_str()) {
                        published_date = dt.to_string();
                    }
                    if let Some(vc) = primary.get("viewCount").and_then(|v| v.get("videoViewCountRenderer")).and_then(|r| r.get("viewCount")).and_then(|v| v.get("simpleText")).and_then(|s| s.as_str()) {
                        views_str = vc.to_string();
                    } else if let Some(vc) = primary.get("viewCount").and_then(|v| v.get("videoViewCountRenderer")).and_then(|r| r.get("shortViewCount")).and_then(|v| v.get("simpleText")).and_then(|s| s.as_str()) {
                        views_str = vc.to_string();
                    }

                    if title.is_empty() {
                        if let Some(runs) = primary.get("title").and_then(|t| t.get("runs")).and_then(|r| r.as_array()) {
                            let joined: String = runs
                                .iter()
                                .filter_map(|x| x.get("text").and_then(|s| s.as_str()))
                                .collect();
                            if !joined.is_empty() {
                                title = joined;
                            }
                        }
                    }

                    // Try to find like button text & likeStatus
                    Self::extract_likes_and_status_recursive(primary, &mut likes, &mut like_status);
                    if likes.is_empty() || likes == "dis" || likes == "dislike" {
                        likes = "Me gusta".to_string();
                    }
                }

                if let Some(item_renderer) = item.get("videoSecondaryInfoRenderer") {
                    if let Some(sub_btn) = item_renderer.get("subscribeButton") {
                        if json_has_subscribed_true(sub_btn) {
                            is_subscribed = true;
                        }
                    }

                    if let Some(owner) = item_renderer.get("owner").and_then(|o| o.get("videoOwnerRenderer")) {
                        if channel_name.is_empty() {
                            if let Some(name_str) = owner
                                .get("title")
                                .and_then(|t| t.get("runs"))
                                .and_then(|r| r.as_array())
                                .and_then(|arr| arr.first())
                                .and_then(|f| f.get("text"))
                                .and_then(|s| s.as_str())
                            {
                                channel_name = name_str.to_string();
                            }
                        }

                        if channel_id.is_empty() {
                            if let Some(cid_str) = owner
                                .get("navigationEndpoint")
                                .and_then(|n| n.get("browseEndpoint"))
                                .and_then(|b| b.get("browseId"))
                                .and_then(|s| s.as_str())
                                .or_else(|| {
                                    owner
                                        .get("title")
                                        .and_then(|t| t.get("runs"))
                                        .and_then(|r| r.as_array())
                                        .and_then(|arr| arr.first())
                                        .and_then(|f| f.get("navigationEndpoint"))
                                        .and_then(|n| n.get("browseEndpoint"))
                                        .and_then(|b| b.get("browseId"))
                                        .and_then(|s| s.as_str())
                                })
                            {
                                channel_id = cid_str.to_string();
                            }
                        }

                        if let Some(thumb) = owner
                            .get("thumbnail")
                            .and_then(|t| t.get("thumbnails"))
                            .and_then(|arr| arr.as_array())
                            .and_then(|arr| arr.last())
                            .and_then(|t| t.get("url"))
                            .and_then(|u| u.as_str())
                        {
                            avatar = thumb.to_string();
                        }
                        if let Some(sub_text) = owner.get("subscriberCountText").and_then(|s| s.get("simpleText")).and_then(|s| s.as_str()) {
                            subs = sub_text.to_string();
                        }
                    }

                    if let Some(desc_str) = item_renderer
                        .get("attributedDescription")
                        .and_then(|ad| ad.get("content"))
                        .and_then(|s| s.as_str())
                    {
                        if desc_str.len() > description.len() {
                            description = desc_str.to_string();
                        }
                    }
                }
            }
        }

        if title.is_empty() {
            title = "Sin título".to_string();
        }
        if channel_name.is_empty() {
            channel_name = "Canal de YouTube".to_string();
        }

        // Also check modern frameworkUpdates for likeStatusEntity if not found in primary
        if like_status == "INDIFFERENT" || likes == "Me gusta" {
            if let Some(fw) = next_json.get("frameworkUpdates") {
                Self::extract_likes_and_status_recursive(fw, &mut likes, &mut like_status);
            }
        }

        (title, channel_name, channel_id, avatar, subs, likes, description, is_live, is_subscribed, like_status, published_date, views_str)
    }

    pub fn format_compact_count(count: u64) -> String {
        if count >= 1_000_000 {
            let m = count as f64 / 1_000_000.0;
            if m >= 10.0 {
                format!("{:.0} M", m)
            } else {
                format!("{:.1} M", m).replace(".0 ", " ")
            }
        } else if count >= 1_000 {
            let k = count as f64 / 1_000.0;
            if k >= 10.0 {
                format!("{:.0} K", k)
            } else {
                format!("{:.1} K", k).replace(".0 ", " ")
            }
        } else {
            count.to_string()
        }
    }

    fn extract_likes_and_status_recursive(val: &Value, likes: &mut String, like_status: &mut String) {
        if let Some(obj) = val.as_object() {
            if let Some(st) = obj.get("likeStatus").and_then(|s| s.as_str()) {
                if st == "LIKE" || st == "DISLIKE" {
                    *like_status = st.to_string();
                }
            }
            if let Some(lvm) = obj.get("likeButtonViewModel") {
                if let Some(st) = lvm
                    .get("likeStatusEntity")
                    .and_then(|e| e.get("likeStatus"))
                    .and_then(|s| s.as_str())
                {
                    if st == "LIKE" || st == "DISLIKE" {
                        *like_status = st.to_string();
                    }
                }
                let title_opt = lvm
                    .get("toggleButtonViewModel")
                    .and_then(|t| t.get("toggleButtonViewModel").or(Some(t)))
                    .and_then(|t| t.get("defaultButtonViewModel"))
                    .and_then(|d| d.get("buttonViewModel"))
                    .and_then(|b| b.get("title"))
                    .and_then(|s| s.as_str());
                if let Some(t_str) = title_opt {
                    let clean = t_str.trim();
                    if !clean.is_empty() && clean.chars().any(|c| c.is_ascii_digit()) {
                        *likes = clean.to_string();
                    }
                }
                if likes.is_empty() || *likes == "Me gusta" {
                    let acc_opt = lvm
                        .get("toggleButtonViewModel")
                        .and_then(|t| t.get("toggleButtonViewModel").or(Some(t)))
                        .and_then(|t| t.get("defaultButtonViewModel"))
                        .and_then(|d| d.get("buttonViewModel"))
                        .and_then(|b| b.get("accessibilityText"))
                        .and_then(|s| s.as_str());
                    if let Some(acc) = acc_opt {
                        for tok in acc.split_whitespace() {
                            if tok.chars().any(|c| c.is_ascii_digit()) {
                                *likes = tok.trim_matches(&[',', '.'][..]).to_string();
                                break;
                            }
                        }
                    }
                }
            }
            if (likes.is_empty() || *likes == "Me gusta") && obj.contains_key("defaultText") {
                if let Some(dt) = obj
                    .get("defaultText")
                    .and_then(|d| d.get("simpleText"))
                    .and_then(|s| s.as_str())
                {
                    let clean = dt.trim();
                    if !clean.is_empty() && clean.chars().any(|c| c.is_ascii_digit()) {
                        *likes = clean.to_string();
                    }
                }
            }
            for (_, v) in obj {
                Self::extract_likes_and_status_recursive(v, likes, like_status);
            }
        } else if let Some(arr) = val.as_array() {
            for item in arr {
                Self::extract_likes_and_status_recursive(item, likes, like_status);
            }
        }
    }

    fn parse_comments(next_json: &Value) -> Vec<CommentItem> {
        let mut comments = Vec::new();
        if let Some(results) = next_json
            .get("contents")
            .and_then(|c| c.get("twoColumnWatchNextResults"))
            .and_then(|t| t.get("results"))
            .and_then(|r| r.get("results"))
            .and_then(|r| r.get("contents"))
            .and_then(|arr| arr.as_array())
        {
            for item in results {
                if let Some(section) = item.get("itemSectionRenderer") {
                    if let Some(contents) = section.get("contents").and_then(|c| c.as_array()) {
                        for c in contents {
                            if let Some(thread) = c.get("commentThreadRenderer") {
                                if let Some(renderer) = thread.get("comment").and_then(|r| r.get("commentRenderer")) {
                                    let author = renderer.get("authorText")
                                        .and_then(|a| a.get("simpleText"))
                                        .and_then(|s| s.as_str())
                                        .unwrap_or("Usuario")
                                        .to_string();

                                    let author_avatar = renderer.get("authorThumbnail")
                                        .and_then(|t| t.get("thumbnails"))
                                        .and_then(|arr| arr.as_array())
                                        .and_then(|arr| arr.last())
                                        .and_then(|u| u.get("url"))
                                        .and_then(|u| u.as_str())
                                        .unwrap_or("")
                                        .to_string();

                                    let text = renderer.get("contentText")
                                        .and_then(|ct| {
                                            ct.get("runs")
                                                .and_then(|arr| arr.as_array())
                                                .map(|runs| {
                                                    runs.iter()
                                                        .filter_map(|r| r.get("text").and_then(|t| t.as_str()))
                                                        .collect::<Vec<_>>()
                                                        .join("")
                                                })
                                        })
                                        .unwrap_or_default();

                                    let published_time = renderer.get("publishedTimeText")
                                        .and_then(|p| p.get("runs"))
                                        .and_then(|arr| arr.as_array())
                                        .and_then(|arr| arr.first())
                                        .and_then(|f| f.get("text"))
                                        .and_then(|t| t.as_str())
                                        .unwrap_or("")
                                        .to_string();

                                    let likes_count = renderer.get("voteCount")
                                        .and_then(|v| v.get("simpleText"))
                                        .and_then(|s| s.as_str())
                                        .unwrap_or("0")
                                        .to_string();

                                    let parsed_urls = extract_unique_urls(&text);
                                    let parsed_timestamps = extract_timestamps(&text);
                                    comments.push(CommentItem {
                                        author,
                                        author_avatar,
                                        text,
                                        published_time,
                                        likes_count,
                                        parsed_urls,
                                        parsed_timestamps,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        comments
    }

    fn find_continuation_token_recursive(val: &Value) -> Option<String> {
        if let Some(obj) = val.as_object() {
            if let Some(token) = obj
                .get("continuationCommand")
                .and_then(|cmd| cmd.get("token"))
                .and_then(|t| t.as_str())
            {
                if !token.is_empty() {
                    return Some(token.to_string());
                }
            }
            for (_, v) in obj {
                if let Some(found) = Self::find_continuation_token_recursive(v) {
                    return Some(found);
                }
            }
        } else if let Some(arr) = val.as_array() {
            for item in arr {
                if let Some(found) = Self::find_continuation_token_recursive(item) {
                    return Some(found);
                }
            }
        }
        None
    }

    /// Extracts continuation token for comments from watch next response
    pub fn extract_comment_continuation(next_json: &Value) -> Option<String> {
        if let Some(results) = next_json
            .get("contents")
            .and_then(|c| c.get("twoColumnWatchNextResults"))
            .and_then(|t| t.get("results"))
            .and_then(|r| r.get("results"))
            .and_then(|r| r.get("contents"))
            .and_then(|arr| arr.as_array())
        {
            for item in results {
                if let Some(section) = item.get("itemSectionRenderer") {
                    let is_comment_section = section.get("sectionIdentifier").and_then(|s| s.as_str()) == Some("comment-item-section")
                        || section.get("targetId").and_then(|s| s.as_str()) == Some("comments-section");
                    if is_comment_section {
                        if let Some(token) = Self::find_continuation_token_recursive(section) {
                            return Some(token);
                        }
                    }
                }
            }
        }

        // Also check engagementPanels for engagement-panel-comments-section
        if let Some(panels) = next_json.get("engagementPanels").and_then(|p| p.as_array()) {
            for panel in panels {
                if let Some(list_renderer) = panel.get("engagementPanelSectionListRenderer") {
                    let is_comments_panel = list_renderer.get("panelIdentifier").and_then(|s| s.as_str()) == Some("engagement-panel-comments-section")
                        || list_renderer.get("targetId").and_then(|s| s.as_str()) == Some("engagement-panel-comments-section");
                    if is_comments_panel {
                        if let Some(token) = Self::find_continuation_token_recursive(list_renderer) {
                            return Some(token);
                        }
                    }
                }
            }
        }

        None
    }

    /// Fetches comments using continuation token supporting modern entityBatchUpdate payloads
    pub async fn get_comments_by_continuation(&self, token: &str) -> Result<Vec<CommentItem>, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{YOUTUBE_ORIGIN}/youtubei/v1/next?prettyPrint=false");
        let headers = self.build_headers().await;
        let body = serde_json::json!({
            "context": Self::innertube_context(),
            "continuation": token
        });

        let resp = self.client.post(&url)
            .headers(headers)
            .json(&body)
            .send()
            .await?;

        let json: Value = Self::resp_simd_json(resp).await?;
        let mut comments = Vec::new();

        // 1. Check modern frameworkUpdates entityBatchUpdate
        if let Some(mutations) = json.get("frameworkUpdates")
            .and_then(|f| f.get("entityBatchUpdate"))
            .and_then(|e| e.get("mutations"))
            .and_then(|m| m.as_array())
        {
            for m in mutations {
                if let Some(payload) = m.get("payload") {
                    if let Some(comment_payload) = payload.get("commentEntityPayload") {
                        let author = comment_payload.get("author")
                            .and_then(|a| a.get("displayName"))
                            .and_then(|s| s.as_str())
                            .unwrap_or("Usuario")
                            .to_string();

                        let author_avatar = comment_payload.get("author")
                            .and_then(|a| a.get("avatarThumbnailUrl"))
                            .and_then(|s| s.as_str())
                            .unwrap_or("")
                            .to_string();

                        let text = comment_payload.get("properties")
                            .and_then(|p| p.get("content"))
                            .and_then(|c| c.get("content"))
                            .and_then(|s| s.as_str())
                            .unwrap_or("")
                            .to_string();

                        let published_time = comment_payload.get("properties")
                            .and_then(|p| p.get("publishedTime"))
                            .and_then(|s| s.as_str())
                            .unwrap_or("")
                            .to_string();

                        if !text.is_empty() {
                            let parsed_urls = extract_unique_urls(&text);
                            let parsed_timestamps = extract_timestamps(&text);
                            comments.push(CommentItem {
                                author,
                                author_avatar,
                                text,
                                published_time,
                                likes_count: String::new(),
                                parsed_urls,
                                parsed_timestamps,
                            });
                        }
                    }
                }
            }
        }

        // 2. Also check classic endpoints
        if comments.is_empty() {
            if let Some(eps) = json.get("onResponseReceivedEndpoints").and_then(|a| a.as_array()) {
                for ep in eps {
                    let items = ep.get("reloadContinuationItemsCommand")
                        .and_then(|r| r.get("continuationItems"))
                        .or_else(|| ep.get("appendContinuationItemsAction").and_then(|a| a.get("continuationItems")));
                    if let Some(items) = items.and_then(|i| i.as_array()) {
                        for it in items {
                            if let Some(ct) = it.get("commentThreadRenderer") {
                                if let Some(renderer) = ct.get("comment").and_then(|c| c.get("commentRenderer")) {
                                    let author = renderer.get("authorText").and_then(|a| a.get("simpleText")).and_then(|s| s.as_str()).unwrap_or("Usuario").to_string();
                                    let author_avatar = renderer.get("authorThumbnail").and_then(|t| t.get("thumbnails")).and_then(|a| a.as_array()).and_then(|a| a.last()).and_then(|u| u.get("url")).and_then(|s| s.as_str()).unwrap_or("").to_string();
                                    let text = renderer.get("contentText").and_then(|ct| ct.get("runs")).and_then(|a| a.as_array()).map(|runs| {
                                        runs.iter().filter_map(|r| r.get("text").and_then(|t| t.as_str())).collect::<Vec<_>>().join("")
                                    }).unwrap_or_default();
                                    let published_time = renderer.get("publishedTimeText").and_then(|p| p.get("runs")).and_then(|a| a.as_array()).and_then(|a| a.first()).and_then(|f| f.get("text")).and_then(|s| s.as_str()).unwrap_or("").to_string();
                                    let likes_count = renderer.get("voteCount").and_then(|v| v.get("simpleText")).and_then(|s| s.as_str()).unwrap_or("0").to_string();
                                    let parsed_urls = extract_unique_urls(&text);
                                    let parsed_timestamps = extract_timestamps(&text);
                                    comments.push(CommentItem { author, author_avatar, text, published_time, likes_count, parsed_urls, parsed_timestamps });
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(comments)
    }

    /// Fetches Channel profile with tabs (Videos, En Directo, Listas) with anti-shorts filter
    pub async fn get_channel(&self, channel_id: &str) -> Result<ChannelDetails, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{YOUTUBE_ORIGIN}/youtubei/v1/browse?prettyPrint=false");
        let headers = self.build_headers().await;

        let body = serde_json::json!({
            "context": Self::innertube_context(),
            "browseId": channel_id
        });

        let resp = self.client.post(&url)
            .headers(headers)
            .json(&body)
            .send()
            .await?;

        let json_val: Value = Self::resp_simd_json(resp).await?;

        let header = json_val.get("header")
            .and_then(|h| h.get("c4TabbedHeaderRenderer").or_else(|| h.get("pageHeaderRenderer")));

        let title = header
            .and_then(|h| h.get("title").or_else(|| h.get("pageTitle")))
            .and_then(|t| t.as_str().or_else(|| t.get("simpleText").and_then(|s| s.as_str())))
            .unwrap_or("Canal")
            .to_string();

        let handle = header
            .and_then(|h| h.get("channelHandleText").or_else(|| h.get("metadata")))
            .and_then(|m| m.get("runs").and_then(|arr| arr.as_array()))
            .and_then(|arr| arr.first())
            .and_then(|f| f.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("")
            .to_string();

        let subscriber_count = header
            .and_then(|h| h.get("subscriberCountText"))
            .and_then(|s| s.get("simpleText"))
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string();

        let avatar_url = header
            .and_then(|h| h.get("avatar").or_else(|| h.get("content").and_then(|c| c.get("image"))))
            .and_then(|a| a.get("thumbnails"))
            .and_then(|arr| arr.as_array())
            .and_then(|arr| arr.last())
            .and_then(|t| t.get("url"))
            .and_then(|u| u.as_str())
            .unwrap_or("")
            .to_string();

        let banner_url = header
            .and_then(|h| h.get("banner"))
            .and_then(|b| b.get("thumbnails"))
            .and_then(|arr| arr.as_array())
            .and_then(|arr| arr.last())
            .and_then(|t| t.get("url"))
            .and_then(|u| u.as_str())
            .map(|s| s.to_string());

        let mut tabs = Vec::new();
        if let Some(tabs_arr) = json_val
            .get("contents")
            .and_then(|c| c.get("twoColumnBrowseResultsRenderer"))
            .and_then(|t| t.get("tabs"))
            .and_then(|arr| arr.as_array())
        {
            for tab in tabs_arr {
                if let Some(tab_renderer) = tab.get("tabRenderer") {
                    let tab_title = tab_renderer.get("title").and_then(|t| t.as_str()).unwrap_or("");

                    // Strict anti-shorts filter: Discard any tab whose title or endpoint points to Shorts
                    if tab_title.eq_ignore_ascii_case("shorts") {
                        continue;
                    }
                    if let Some(endpoint) = tab_renderer.get("endpoint") {
                        if json_contains_str(endpoint, "reelWatchEndpoint") || json_contains_str(endpoint, "shorts") {
                            continue;
                        }
                    }

                    let tab_id = tab_renderer.get("tabIdentifier").and_then(|id| id.as_str()).unwrap_or(tab_title).to_string();
                    let mut tab_videos = Vec::new();
                    Self::collect_video_cards(tab_renderer, &mut tab_videos);

                    tabs.push(ChannelTab {
                        title: tab_title.to_string(),
                        tab_id,
                        videos: tab_videos,
                    });
                }
            }
        }

        let is_subscribed = header
            .map(json_has_subscribed_true)
            .unwrap_or(false);

        Ok(ChannelDetails {
            channel_id: channel_id.to_string(),
            title,
            handle,
            subscriber_count,
            banner_url,
            avatar_url,
            is_subscribed,
            tabs,
        })
    }

    pub async fn subscribe_channel(&self, channel_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{YOUTUBE_ORIGIN}/youtubei/v1/subscription/subscribe?prettyPrint=false");
        let headers = self.build_headers().await;
        let body = serde_json::json!({
            "context": Self::innertube_context(),
            "channelIds": [channel_id]
        });
        let resp = self.client.post(&url).headers(headers).json(&body).send().await?;
        if !resp.status().is_success() {
            return Err(format!("Subscription failed with HTTP {}", resp.status()).into());
        }
        Ok(())
    }

    pub async fn unsubscribe_channel(&self, channel_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{YOUTUBE_ORIGIN}/youtubei/v1/subscription/unsubscribe?prettyPrint=false");
        let headers = self.build_headers().await;
        let body = serde_json::json!({
            "context": Self::innertube_context(),
            "channelIds": [channel_id]
        });
        let resp = self.client.post(&url).headers(headers).json(&body).send().await?;
        if !resp.status().is_success() {
            return Err(format!("Unsubscription failed with HTTP {}", resp.status()).into());
        }
        Ok(())
    }

    pub async fn like_video(&self, video_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{YOUTUBE_ORIGIN}/youtubei/v1/like/like?prettyPrint=false");
        let headers = self.build_headers().await;
        let body = serde_json::json!({
            "context": Self::innertube_context(),
            "target": {
                "videoId": video_id
            }
        });
        let resp = self.client.post(&url).headers(headers).json(&body).send().await?;
        if !resp.status().is_success() {
            return Err(format!("Like failed with HTTP {}", resp.status()).into());
        }
        Ok(())
    }

    pub async fn dislike_video(&self, video_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{YOUTUBE_ORIGIN}/youtubei/v1/like/dislike?prettyPrint=false");
        let headers = self.build_headers().await;
        let body = serde_json::json!({
            "context": Self::innertube_context(),
            "target": {
                "videoId": video_id
            }
        });
        let resp = self.client.post(&url).headers(headers).json(&body).send().await?;
        if !resp.status().is_success() {
            return Err(format!("Dislike failed with HTTP {}", resp.status()).into());
        }
        Ok(())
    }

    pub async fn remove_like_video(&self, video_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{YOUTUBE_ORIGIN}/youtubei/v1/like/removelike?prettyPrint=false");
        let headers = self.build_headers().await;
        let body = serde_json::json!({
            "context": Self::innertube_context(),
            "target": {
                "videoId": video_id
            }
        });
        let resp = self.client.post(&url).headers(headers).json(&body).send().await?;
        if !resp.status().is_success() {
            return Err(format!("Remove like failed with HTTP {}", resp.status()).into());
        }
        Ok(())
    }
}
