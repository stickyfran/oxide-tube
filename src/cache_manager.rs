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
    "5 GB (Estándar)",
    "10 GB",
    "20 GB",
    "Ilimitado",
];

pub fn default_cache_limit() -> u64 {
    5 * 1024 * 1024 * 1024 // 5 GB default
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

pub fn get_cache_size_bytes() -> u64 {
    let mut total: u64 = dir_size_recursive(&mitub_cache_root_dir());
    // Also include legacy /tmp/mitub_cache if it still exists from earlier versions
    let legacy = PathBuf::from("/tmp/mitub_cache");
    if legacy.exists() {
        total += dir_size_recursive(&legacy);
    }
    total
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

pub fn prune_cache_if_needed(max_bytes: u64, active_video_id: Option<&str>) -> u64 {
    if max_bytes == u64::MAX {
        return 0;
    }
    let root = mitub_cache_root_dir();
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

        // Never prune cached video/audio/subtitles belonging to the actively playing video
        if let Some(active_id) = active_video_id {
            if !active_id.is_empty() {
                if let Some(file_name) = f.path.file_name().and_then(|n| n.to_str()) {
                    if file_name.contains(active_id) {
                        continue;
                    }
                }
            }
        }

        if fs::remove_file(&f.path).is_ok() {
            total_size = total_size.saturating_sub(f.size);
            freed += f.size;
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

    freed
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
        assert_eq!(prune_cache_if_needed(u64::MAX, None), 0);
    }
}

