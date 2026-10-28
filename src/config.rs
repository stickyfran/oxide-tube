use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreferredResolution {
    Res4K,    // 2160p
    Res1440p, // 1440p
    Res1080p, // 1080p
    Res720p,  // 720p
}

impl PreferredResolution {
    pub fn label(&self) -> &'static str {
        match self {
            PreferredResolution::Res4K => "4K (2160p)",
            PreferredResolution::Res1440p => "1440p (2K)",
            PreferredResolution::Res1080p => "1080p (FHD)",
            PreferredResolution::Res720p => "720p (HD)",
        }
    }

    pub fn max_height(&self) -> u32 {
        match self {
            PreferredResolution::Res4K => 2160,
            PreferredResolution::Res1440p => 1440,
            PreferredResolution::Res1080p => 1080,
            PreferredResolution::Res720p => 720,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreferredCodec {
    Av1,
    Vp9,
    H264,
}

impl PreferredCodec {
    pub fn label(&self) -> &'static str {
        match self {
            PreferredCodec::Av1 => "AV1 (Eficiencia Max)",
            PreferredCodec::Vp9 => "VP9",
            PreferredCodec::H264 => "H.264 (AVC)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PreferredAudioLanguage {
    #[default]
    Original,
    Spanish,
    English,
}

impl PreferredAudioLanguage {
    pub fn label(&self) -> &'static str {
        match self {
            PreferredAudioLanguage::Original => "Original (por defecto)",
            PreferredAudioLanguage::Spanish => "Español",
            PreferredAudioLanguage::English => "Inglés",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoTab {
    pub video_id: String,
    pub title: String,
    pub channel_name: String,
    #[serde(default)]
    pub saved_pos_ms: u64,
    #[serde(default)]
    pub thumbnail_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalHistoryItem {
    pub video_id: String,
    pub title: String,
    pub channel_name: String,
    pub channel_id: String,
    pub channel_avatar: String,
    pub duration_str: String,
    pub thumbnail_url: String,
    pub saved_pos_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CardStyle {
    #[default]
    Squircle,
    Rectangular,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GridColumnsConfig {
    #[default]
    Cols4,
    Auto,
    Cols2,
    Cols5,
    Cols6,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub preferred_resolution: PreferredResolution,
    pub preferred_codec: PreferredCodec,
    #[serde(default)]
    pub preferred_audio_lang: PreferredAudioLanguage,
    #[serde(default)]
    pub auto_pip_on_unfocus: bool,
    #[serde(default)]
    pub full_buffer_tabs: bool,
    #[serde(default)]
    pub autoplay_next: bool,
    #[serde(default)]
    pub saved_volume: Option<f32>,
    #[serde(default)]
    pub is_muted: Option<bool>,
    #[serde(default)]
    pub theater_mode: bool,
    #[serde(default)]
    pub subtitles_enabled: bool,
    #[serde(default)]
    pub preferred_caption_lang: Option<String>,
    #[serde(default)]
    pub pip_position: Option<(f32, f32)>,
    #[serde(default)]
    pub pip_size: Option<(f32, f32)>,
    #[serde(default)]
    pub saved_tabs: Vec<VideoTab>,
    #[serde(default)]
    pub active_tab_idx: Option<usize>,
    #[serde(default)]
    pub saved_positions: std::collections::HashMap<String, u64>,
    #[serde(default)]
    pub local_history: Vec<LocalHistoryItem>,
    #[serde(default)]
    pub cached_user_name: Option<String>,
    #[serde(default)]
    pub cached_avatar_url: Option<String>,
    #[serde(default = "crate::cache_manager::default_cache_limit")]
    pub max_cache_size_bytes: u64,
    #[serde(default = "default_true")]
    pub close_to_tray: bool,
    #[serde(default)]
    pub card_style: CardStyle,
    #[serde(default)]
    pub home_columns: GridColumnsConfig,
}

fn default_true() -> bool {
    true
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            preferred_resolution: PreferredResolution::Res4K,
            preferred_codec: PreferredCodec::Av1,
            preferred_audio_lang: PreferredAudioLanguage::Original,
            auto_pip_on_unfocus: false,
            full_buffer_tabs: false,
            autoplay_next: false,
            saved_volume: None,
            is_muted: None,
            theater_mode: false,
            subtitles_enabled: false,
            preferred_caption_lang: None,
            pip_position: None,
            pip_size: None,
            saved_tabs: Vec::new(),
            active_tab_idx: None,
            saved_positions: std::collections::HashMap::new(),
            local_history: Vec::new(),
            cached_user_name: None,
            cached_avatar_url: None,
            max_cache_size_bytes: crate::cache_manager::default_cache_limit(),
            close_to_tray: true,
            card_style: CardStyle::Squircle,
            home_columns: GridColumnsConfig::Cols4,
        }
    }
}

impl AppConfig {
    pub fn file_path() -> Option<PathBuf> {
        let home = dirs::home_dir()?;
        Some(home.join(".config/mitub/config.json"))
    }

    pub fn load() -> Self {
        if let Some(path) = Self::file_path() {
            if let Ok(data) = fs::read_to_string(path) {
                if let Ok(mut cfg) = serde_json::from_str::<AppConfig>(&data) {
                    // Self-heal any saved positions that are at or past the end of a video
                    let mut finished_ids = std::collections::HashSet::new();
                    for item in &mut cfg.local_history {
                        if let Some(dur_secs) = crate::innertube::InnerTubeClient::parse_timestamp_secs(&item.duration_str) {
                            let dur_ms = dur_secs * 1000;
                            if dur_ms > 5000 && item.saved_pos_ms + 5000 >= dur_ms {
                                item.saved_pos_ms = 0;
                                finished_ids.insert(item.video_id.clone());
                            }
                        }
                    }
                    for vid in &finished_ids {
                        cfg.saved_positions.remove(vid);
                    }
                    for tab in &mut cfg.saved_tabs {
                        if finished_ids.contains(&tab.video_id) {
                            tab.saved_pos_ms = 0;
                        }
                    }
                    return cfg;
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        if let Some(path) = Self::file_path() {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Ok(data) = serde_json::to_string(self) {
                let tmp_path = path.with_extension("tmp");
                if fs::write(&tmp_path, data).is_ok() {
                    let _ = fs::rename(&tmp_path, &path);
                }
            }
        }
    }

    pub fn save_async(&self) {
        let cfg = self.clone();
        tokio::task::spawn_blocking(move || {
            cfg.save();
        });
    }

    pub fn record_playback_progress(
        &mut self,
        video_id: &str,
        title: &str,
        channel_name: &str,
        channel_id: &str,
        channel_avatar: &str,
        duration_str: &str,
        pos_ms: u64,
    ) {
        if video_id.is_empty() {
            return;
        }
        let parsed_dur_ms = crate::innertube::InnerTubeClient::parse_timestamp_secs(duration_str)
            .map(|s| s * 1000)
            .unwrap_or(0);
        let is_near_end = parsed_dur_ms > 5000 && pos_ms + 5000 >= parsed_dur_ms;

        if is_near_end {
            self.saved_positions.remove(video_id);
        } else if pos_ms > 1000 {
            self.saved_positions.insert(video_id.to_string(), pos_ms);
            if self.saved_positions.len() > 200 {
                let valid_ids: std::collections::HashSet<_> = self.local_history.iter().map(|h| h.video_id.as_str()).collect();
                self.saved_positions.retain(|k, _| valid_ids.contains(k.as_str()));
            }
        }
        let thumb_url = format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", video_id);
        let existing_pos = if is_near_end {
            0
        } else {
            self.saved_positions.get(video_id).copied().unwrap_or(pos_ms)
        };
        if let Some(pos) = self.local_history.iter().position(|h| h.video_id == video_id) {
            let mut item = self.local_history.remove(pos);
            if !title.is_empty() && title != "Cargando..." && title != "Sin título" {
                item.title = title.to_string();
            }
            if !channel_name.is_empty() {
                item.channel_name = channel_name.to_string();
            }
            if !channel_id.is_empty() {
                item.channel_id = channel_id.to_string();
            }
            if !channel_avatar.is_empty() {
                item.channel_avatar = channel_avatar.to_string();
            }
            if !duration_str.is_empty() && duration_str != "00:00" {
                item.duration_str = duration_str.to_string();
            }
            if is_near_end {
                item.saved_pos_ms = 0;
            } else if pos_ms > 1000 {
                item.saved_pos_ms = pos_ms;
            }
            self.local_history.insert(0, item);
        } else {
            self.local_history.insert(
                0,
                LocalHistoryItem {
                    video_id: video_id.to_string(),
                    title: if title.is_empty() { "Video".to_string() } else { title.to_string() },
                    channel_name: channel_name.to_string(),
                    channel_id: channel_id.to_string(),
                    channel_avatar: channel_avatar.to_string(),
                    duration_str: if duration_str.is_empty() { "00:00".to_string() } else { duration_str.to_string() },
                    thumbnail_url: thumb_url,
                    saved_pos_ms: existing_pos,
                },
            );
            if self.local_history.len() > 150 {
                self.local_history.truncate(150);
            }
        }
    }
}
