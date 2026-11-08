use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

pub const CACHE_LIMITS_BYTES: [u64; 6] = [
    1 * 1024 * 1024 * 1024,
    2 * 1024 * 1024 * 1024,
    5 * 1024 * 1024 * 1024,
    10 * 1024 * 1024 * 1024,
    20 * 1024 * 1024 * 1024,
    u64::MAX,
];

pub const CACHE_LIMIT_LABELS: [&str; 6] = [
    "1 GB",
    "2 GB",
    "5 GB",
    "10 GB (Estándar)",
    "20 GB",
    "Ilimitado",
];

pub fn default_cache_limit() -> u64 {
    10 * 1024 * 1024 * 1024 // 10 GB default
}

pub fn limit_to_index(limit: u64) -> usize {
    match limit {
        l if l <= 1 * 1024 * 1024 * 1024 => 0,
        l if l <= 2 * 1024 * 1024 * 1024 => 1,
        l if l <= 5 * 1024 * 1024 * 1024 => 2,
        l if l <= 10 * 1024 * 1024 * 1024 => 3,
        l if l <= 20 * 1024 * 1024 * 1024 => 4,
        _ => 5,
    }
}

pub fn index_to_limit(idx: usize) -> u64 {
    CACHE_LIMITS_BYTES.get(idx).copied().unwrap_or(u64::MAX)
}

pub fn mitub_cache_root_dir() -> PathBuf {
    if let Some(mut dir) = dirs::cache_dir() {
        dir.push("mitub");
        dir
    } else {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join(".cache/mitub")
    }
}

pub fn save_video_title(video_id: &str, title: &str) {
    if video_id.is_empty() || title.is_empty() || title == video_id {
        return;
    }
    let dir = video_cache_dir();
    let _ = fs::create_dir_all(&dir);
    let path = dir.join(format!("{video_id}.title"));
    let _ = fs::write(path, title.as_bytes());
}

pub fn get_video_title(video_id: &str) -> Option<String> {
    let path = video_cache_dir().join(format!("{video_id}.title"));
    fs::read_to_string(path).ok().map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
}

pub fn video_cache_dir() -> PathBuf {
    mitub_cache_root_dir().join("video_cache")
}

pub fn thumbnail_cache_dir() -> PathBuf {
    mitub_cache_root_dir().join("thumbnails")
}

fn dir_size_recursive(path: &std::path::Path) -> u64 {
    let mut total: u64 = 0;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    total += meta.len();
                } else if meta.is_dir() {
                    total += dir_size_recursive(&entry.path());
                }
            }
        }
    }
    total
}

/// Size of the video cache (the part governed by the configured limit).
pub fn get_cache_size_bytes() -> u64 {
    let mut total: u64 = dir_size_recursive(&video_cache_dir());
    // Also include legacy /tmp/mitub_cache if it still exists from earlier versions
    let legacy = PathBuf::from("/tmp/mitub_cache");
    if legacy.exists() {
        total += dir_size_recursive(&legacy);
    }
    total
}

pub fn format_speed(bytes_per_sec: u64) -> String {
    if bytes_per_sec == 0 {
        String::new()
    } else if bytes_per_sec < 1024 * 1024 {
        format!("{:.1} KB/s", bytes_per_sec as f64 / 1024.0)
    } else {
        format!("{:.1} MB/s", bytes_per_sec as f64 / (1024.0 * 1024.0))
    }
}

pub fn format_size(bytes: u64) -> String {
    if bytes == 0 {
        "0 MB".to_string()
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// Lists fully cached videos as (video_id, total bytes of video + audio files).
pub fn list_cached_videos() -> Vec<(String, String, u64)> {
    let mut out = Vec::new();
    let dir = video_cache_dir();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(id) = name.strip_suffix("_video.mp4") {
                let mut size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                if let Ok(m) = fs::metadata(dir.join(format!("{id}_audio.mp4"))) {
                    size += m.len();
                }
                let title = get_video_title(id).unwrap_or_else(|| id.to_string());
                out.push((id.to_string(), title, size));
            }
        }
    }
    out.sort_by(|a, b| a.1.cmp(&b.1));
    out
}

/// Prunes the video cache (oldest first) down to 90% of `max_bytes`.
/// Files belonging to any id in `protected_ids` (playing video / open tabs) and in-flight `.part` files are kept.
pub fn delete_cached_video(video_id: &str) -> u64 {
    if video_id.is_empty() {
        return 0;
    }
    let dir = video_cache_dir();
    let mut freed: u64 = 0;
    let candidates = [
        dir.join(format!("{video_id}_video.mp4")),
        dir.join(format!("{video_id}_audio.mp4")),
        dir.join(format!("{video_id}_video.part")),
        dir.join(format!("{video_id}_audio.part")),
        dir.join(format!("{video_id}.title")),
    ];
    for path in &candidates {
        if let Ok(meta) = fs::metadata(path) {
            freed += meta.len();
        }
        let _ = fs::remove_file(path);
    }
    freed
}

pub fn prune_cache_if_needed(max_bytes: u64, protected_ids: &[String]) -> u64 {
    if max_bytes == u64::MAX {
        return 0;
    }
    let root = video_cache_dir();
    if !root.exists() {
        return 0;
    }

    struct CacheFile {
        path: PathBuf,
        size: u64,
        modified: SystemTime,
    }

    fn collect_files_recursive(path: &std::path::Path, files: &mut Vec<CacheFile>, total_size: &mut u64) {
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                if let Ok(meta) = entry.metadata() {
                    if meta.is_file() {
                        let size = meta.len();
                        let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                        *total_size += size;
                        files.push(CacheFile {
                            path: entry.path(),
                            size,
                            modified,
                        });
                    } else if meta.is_dir() {
                        collect_files_recursive(&entry.path(), files, total_size);
                    }
                }
            }
        }
    }

    let mut files = Vec::new();
    let mut total_size: u64 = 0;
    collect_files_recursive(&root, &mut files, &mut total_size);

    // Sort by modified time oldest first (FIFO / LRU eviction)
    files.sort_by_key(|f| f.modified);

    let mut freed: u64 = 0;
    let target_size = (max_bytes as f64 * 0.90) as u64; // 90% of limit target to prevent frequent thrashing

    for f in files {
        if total_size <= target_size {
            break;
        }

        // Never prune active in-flight partial downloads (.part)
        if let Some(ext) = f.path.extension().and_then(|e| e.to_str()) {
            if ext.eq_ignore_ascii_case("part") {
                continue;
            }
        }

        // Never prune cached files belonging to the playing video or open tabs
        if let Some(file_name) = f.path.file_name().and_then(|n| n.to_str()) {
            if protected_ids.iter().any(|id| !id.is_empty() && file_name.starts_with(id.as_str())) {
                continue;
            }
        }

        if fs::remove_file(&f.path).is_ok() {
            total_size = total_size.saturating_sub(f.size);
            freed += f.size;

            // Keep video/audio pairs consistent: never leave a video without its audio track
            let name = f.path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let sibling = if let Some(id) = name.strip_suffix("_video.mp4") {
                Some(format!("{id}_audio.mp4"))
            } else {
                name.strip_suffix("_audio.mp4").map(|id| format!("{id}_video.mp4"))
            };
            if let Some(sib) = sibling {
                let sib_path = f.path.with_file_name(sib);
                let sib_size = fs::metadata(&sib_path).map(|m| m.len()).unwrap_or(0);
                if sib_size > 0 && fs::remove_file(&sib_path).is_ok() {
                    total_size = total_size.saturating_sub(sib_size);
                    freed += sib_size;
                }
            }
            if let Some(vid_id) = name.strip_suffix("_video.mp4").or_else(|| name.strip_suffix("_audio.mp4")) {
                let title_path = f.path.with_file_name(format!("{vid_id}.title"));
                let _ = fs::remove_file(&title_path);
            }
        }
    }

    // Clean legacy /tmp if any
    let legacy = PathBuf::from("/tmp/mitub_cache");
    if legacy.exists() {
        if let Ok(entries) = fs::read_dir(&legacy) {
            for entry in entries.flatten() {
                let _ = fs::remove_file(entry.path());
            }
        }
        let _ = fs::remove_dir_all(&legacy);
    }

    // Retroactive cleanup: remove abandoned .part files and thumbnail garbage
    clean_orphaned_part_files(&protected_ids);
    prune_thumbnails_cache_if_needed();

    freed
}

/// Cleans abandoned .part files from previous interrupted sessions.
pub fn clean_orphaned_part_files(active_downloads: &[String]) {
    let dir = video_cache_dir();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if ext.eq_ignore_ascii_case("part") {
                    let fname = entry.file_name().to_string_lossy().to_string();
                    let is_active = active_downloads.iter().any(|id| !id.is_empty() && fname.starts_with(id));
                    if !is_active {
                        let _ = fs::remove_file(&path);
                    }
                }
            }
        }
    }
}

/// Prunes thumbnail cache: files older than 30 days or trims down if total thumbnails exceed 250 MB.
pub fn prune_thumbnails_cache_if_needed() {
    let dir = thumbnail_cache_dir();
    if !dir.exists() {
        return;
    }

    struct ThumbEntry {
        path: PathBuf,
        size: u64,
        modified: SystemTime,
    }

    let mut thumbs = Vec::new();
    let mut total_size: u64 = 0;
    let now = SystemTime::now();
    let thirty_days = std::time::Duration::from_secs(30 * 86400);

    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    let size = meta.len();
                    let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                    // Remove if older than 30 days
                    if let Ok(age) = now.duration_since(modified) {
                        if age > thirty_days {
                            let _ = fs::remove_file(entry.path());
                            continue;
                        }
                    }
                    total_size += size;
                    thumbs.push(ThumbEntry {
                        path: entry.path(),
                        size,
                        modified,
                    });
                }
            }
        }
    }

    // Limit thumbnails directory to 250 MB max
    const MAX_THUMB_BYTES: u64 = 250 * 1024 * 1024;
    if total_size > MAX_THUMB_BYTES {
        thumbs.sort_by_key(|t| t.modified); // oldest first
        for t in thumbs {
            if total_size <= MAX_THUMB_BYTES {
                break;
            }
            if fs::remove_file(&t.path).is_ok() {
                total_size = total_size.saturating_sub(t.size);
            }
        }
    }
}

fn clear_dir_contents(path: &std::path::Path) -> u64 {
    let mut freed = 0;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    freed += meta.len();
                    let _ = fs::remove_file(&p);
                } else if meta.is_dir() {
                    freed += clear_dir_contents(&p);
                    let _ = fs::remove_dir_all(&p);
                }
            }
        }
    }
    freed
}

pub fn clear_all_cache() -> u64 {
    let mut freed: u64 = 0;
    let root = mitub_cache_root_dir();
    freed += clear_dir_contents(&root);

    let legacy = PathBuf::from("/tmp/mitub_cache");
    if legacy.exists() {
        freed += clear_dir_contents(&legacy);
        let _ = fs::remove_dir_all(&legacy);
    }
    freed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_limit_index_conversion() {
        assert_eq!(limit_to_index(1 * 1024 * 1024 * 1024), 0);
        assert_eq!(limit_to_index(2 * 1024 * 1024 * 1024), 1);
        assert_eq!(limit_to_index(5 * 1024 * 1024 * 1024), 2);
        assert_eq!(limit_to_index(10 * 1024 * 1024 * 1024), 3);
        assert_eq!(limit_to_index(20 * 1024 * 1024 * 1024), 4);
        assert_eq!(limit_to_index(u64::MAX), 5);

        assert_eq!(index_to_limit(0), 1 * 1024 * 1024 * 1024);
        assert_eq!(index_to_limit(2), 5 * 1024 * 1024 * 1024);
        assert_eq!(index_to_limit(5), u64::MAX);
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(0), "0 MB");
        assert_eq!(format_size(500 * 1024), "500.0 KB");
        assert_eq!(format_size(150 * 1024 * 1024), "150.0 MB");
        assert_eq!(format_size(5 * 1024 * 1024 * 1024), "5.00 GB");
    }

    #[test]
    fn test_prune_unlimited_does_nothing() {
        assert_eq!(prune_cache_if_needed(u64::MAX, &[]), 0);
    }
}

