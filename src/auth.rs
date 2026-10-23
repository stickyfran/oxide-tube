use rusqlite::{Connection, OpenFlags};
use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const ESSENTIAL_AUTH_COOKIES: &[&str] = &[
    "SAPISID",
    "__Secure-3PAPISID",
    "__Secure-1PAPISID",
    "APISID",
    "SSID",
    "HSID",
    "SID",
    "SIDCC",
    "LOGIN_INFO",
    "__Secure-1PSID",
    "__Secure-3PSID",
    "__Secure-1PSIDTS",
    "__Secure-3PSIDTS",
    "__Secure-1PSIDCC",
    "__Secure-3PSIDCC",
    "PREF",
    "VISITOR_INFO1_LIVE",
    "VISITOR_PRIVACY_METADATA",
    "YSC",
];

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct AuthSession {
    pub cookies: HashMap<String, String>,
}

#[allow(dead_code)]
impl AuthSession {
    pub fn new() -> Self {
        Self {
            cookies: HashMap::new(),
        }
    }

    pub fn is_authenticated(&self) -> bool {
        self.cookies.contains_key("SAPISID") || self.cookies.contains_key("__Secure-3PAPISID")
    }

    pub fn set_cookie(&mut self, name: &str, value: &str) {
        if name.starts_with("ST-") || value.is_empty() {
            return;
        }
        // Only store essential cookies to keep header sizes within HTTP specs and prevent 413 errors
        if ESSENTIAL_AUTH_COOKIES.contains(&name) || name.starts_with("__Secure-") || name.starts_with("__Host-") {
            self.cookies.insert(name.to_string(), value.to_string());
        }
    }

    /// Parses raw Cookie string e.g. "SAPISID=xxx; HSID=yyy; SID=zzz"
    pub fn from_raw_cookie_header(raw: &str) -> Self {
        let mut session = Self::new();
        for item in raw.split(';') {
            let part = item.trim();
            if let Some((k, v)) = part.split_once('=') {
                session.set_cookie(k.trim(), v.trim());
            }
        }
        let _ = session.save_to_disk();
        session
    }

    /// Parses Netscape format cookies.txt (tab separated)
    pub fn from_netscape_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let content = fs::read_to_string(path)?;
        let mut session = Self::new();

        // First pass: google.com fallback cookies
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let fields: Vec<&str> = trimmed.split('\t').collect();
            if fields.len() >= 7 {
                let domain = fields[0];
                let name = fields[5];
                let value = fields[6];
                if domain.contains("google.com") {
                    session.set_cookie(name, value);
                }
            }
        }

        // Second pass: youtube.com cookies take strict priority
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let fields: Vec<&str> = trimmed.split('\t').collect();
            if fields.len() >= 7 {
                let domain = fields[0];
                let name = fields[5];
                let value = fields[6];
                if domain.contains("youtube.com") {
                    session.set_cookie(name, value);
                }
            }
        }

        let _ = session.save_to_disk();
        Ok(session)
    }

    /// Extracts cookies from Firefox/Librewolf default-release profile cookies.sqlite (including live WAL journal)
    pub fn from_firefox() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let home = dirs::home_dir().ok_or("Cannot resolve home directory")?;

        let mut candidate_dirs = Vec::new();

        #[cfg(target_os = "linux")]
        {
            let uid = unsafe { libc::getuid() };
            candidate_dirs.push(home.join(".config/mozilla/firefox"));
            candidate_dirs.push(home.join(".mozilla/firefox"));
            candidate_dirs.push(PathBuf::from(format!("/run/user/{uid}/psd")));
            candidate_dirs.push(home.join(".var/app/org.mozilla.firefox/.mozilla/firefox"));
            candidate_dirs.push(home.join(".var/app/io.gitlab.librewolf-community/.gitlab/librewolf"));
        }

        #[cfg(target_os = "windows")]
        {
            if let Some(app_data) = dirs::data_dir() {
                // %APPDATA%\Mozilla\Firefox
                candidate_dirs.push(app_data.join("Mozilla/Firefox"));
            }
            if let Some(local_app_data) = dirs::data_local_dir() {
                candidate_dirs.push(local_app_data.join("Mozilla/Firefox"));
            }
        }

        #[cfg(target_os = "macos")]
        {
            candidate_dirs.push(home.join("Library/Application Support/Firefox"));
        }

        let mut cookie_path: Option<PathBuf> = None;

        for base in &candidate_dirs {
            if !base.exists() {
                continue;
            }

            // 1. Try reading profiles.ini to get the exact active profile directory
            let profiles_ini = base.join("profiles.ini");
            if profiles_ini.exists() {
                if let Ok(content) = fs::read_to_string(&profiles_ini) {
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if let Some(def_val) = trimmed.strip_prefix("Default=") {
                            let cand = base.join(def_val).join("cookies.sqlite");
                            if cand.exists() {
                                cookie_path = Some(cand);
                                break;
                            }
                        } else if let Some(path_val) = trimmed.strip_prefix("Path=") {
                            let cand = base.join(path_val).join("cookies.sqlite");
                            if cand.exists() {
                                cookie_path = Some(cand);
                            }
                        }
                    }
                }
            }

            if cookie_path.is_some() {
                break;
            }

            // 2. Directory scan fallback: strictly ignore backup/recovery/hidden folders
            if let Ok(entries) = fs::read_dir(base) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                    // Skip hidden overlay lowerdirs, backup, crash recovery, and shadow mount copies
                    let is_backup = name.starts_with('.')
                        || name.contains("backup")
                        || name.contains("crashrecovery")
                        || name.contains("-back-")
                        || name.contains("-ovfs")
                        || name.ends_with("-rw");

                    if is_backup {
                        continue;
                    }

                    let cand = path.join("cookies.sqlite");
                    if cand.exists() {
                        if name.contains("default-release") || name.contains("default") || name.contains("firefox") {
                            cookie_path = Some(cand);
                            break;
                        } else if cookie_path.is_none() {
                            cookie_path = Some(cand);
                        }
                    }
                }
            }
            if cookie_path.is_some() {
                break;
            }
        }

        let cookie_file = match cookie_path {
            Some(p) => p,
            None => return Err("No se encontró el archivo cookies.sqlite de Firefox".into()),
        };

        // Copy cookies.sqlite + cookies.sqlite-wal + cookies.sqlite-shm to a temporary directory.
        // Opening with `?immutable=1` ignores the WAL file where running Firefox stores rotated
        // `__Secure-1PSIDTS` and `__Secure-3PSIDTS` tokens, which caused expired session reads!
        let pid = std::process::id();
        let tmp_dir = std::env::temp_dir().join(format!("mitub_ff_cookies_{pid}"));
        let _ = fs::create_dir_all(&tmp_dir);
        let tmp_db = tmp_dir.join("cookies.sqlite");

        fs::copy(&cookie_file, &tmp_db)?;
        if let Some(parent) = cookie_file.parent() {
            let wal_src = parent.join("cookies.sqlite-wal");
            if wal_src.exists() {
                let _ = fs::copy(&wal_src, tmp_dir.join("cookies.sqlite-wal"));
            }
            let shm_src = parent.join("cookies.sqlite-shm");
            if shm_src.exists() {
                let _ = fs::copy(&shm_src, tmp_dir.join("cookies.sqlite-shm"));
            }
        }

        let conn_res = Connection::open_with_flags(&tmp_db, OpenFlags::SQLITE_OPEN_READ_ONLY);
        let conn = match conn_res {
            Ok(c) => c,
            Err(e) => {
                let _ = fs::remove_dir_all(&tmp_dir);
                return Err(e.into());
            }
        };

        // Filter out third-party embedded iframe partitionKeys (`originAttributes = ''` or youtube partition),
        // and ensure `.youtube.com` cookies always override `.google.com` cookies!
        let mut stmt = conn.prepare(
            "SELECT name, value FROM moz_cookies \
             WHERE (host = '.youtube.com' OR host = 'www.youtube.com' OR host = 'youtube.com' OR host = '.google.com') \
               AND (originAttributes = '' OR originAttributes LIKE '%youtube.com%') \
             ORDER BY CASE WHEN host LIKE '%youtube.com' THEN 1 ELSE 0 END ASC, lastAccessed ASC",
        )?;

        let rows = stmt.query_map([], |row| {
            let name: String = row.get(0)?;
            let value: String = row.get(1)?;
            Ok((name, value))
        })?;

        let mut session = Self::new();
        for row in rows.flatten() {
            session.set_cookie(&row.0, &row.1);
        }

        drop(stmt);
        drop(conn);
        let _ = fs::remove_dir_all(&tmp_dir);

        if session.cookies.is_empty() || !session.is_authenticated() {
            return Err("No se encontraron cookies activas de YouTube en Firefox".into());
        }

        let _ = session.save_to_disk();
        Ok(session)
    }

    /// Automatically syncs live cookies from Firefox (including WAL-rotated PSIDTS tokens),
    /// falling back to saved cookies on disk (~/.config/mitub/cookies.json) if Firefox is unavailable.
    pub fn auto_load() -> Self {
        // Priority 1: Live Firefox extraction so rotated __Secure-1PSIDTS/__Secure-3PSIDTS are always fresh
        if let Ok(session) = Self::from_firefox() {
            if session.is_authenticated() {
                println!("[Auth] Sesión activa sincronizada desde Firefox (WAL actualizado)");
                return session;
            }
        }

        // Priority 2: Saved session on disk (~/.config/mitub/cookies.json)
        if let Ok(session) = Self::load_from_disk() {
            if session.is_authenticated() {
                println!("[Auth] Sesión activa cargada desde disco (~/.config/mitub/cookies.json)");
                return session;
            }
        }

        Self::new()
    }

    fn config_path() -> Option<PathBuf> {
        let home = dirs::home_dir()?;
        Some(home.join(".config/mitub/cookies.json"))
    }

    pub fn save_to_disk(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(path) = Self::config_path() {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let data = serde_json::to_string_pretty(&self.cookies)?;
            fs::write(path, data)?;
        }
        Ok(())
    }

    /// Cross-platform Chrome / Chromium cookie discovery & extraction
    pub fn from_chrome() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let home = dirs::home_dir().ok_or("Cannot resolve home directory")?;

        let mut candidate_dirs = Vec::new();

        #[cfg(target_os = "linux")]
        {
            candidate_dirs.push(home.join(".config/google-chrome"));
            candidate_dirs.push(home.join(".config/chromium"));
            candidate_dirs.push(home.join(".config/BraveSoftware/Brave-Browser"));
            candidate_dirs.push(home.join(".config/microsoft-edge"));
            candidate_dirs.push(home.join(".config/vivaldi"));
            candidate_dirs.push(home.join(".var/app/com.google.Chrome/config/google-chrome"));
        }

        #[cfg(target_os = "windows")]
        {
            if let Some(local_app_data) = dirs::data_local_dir() {
                // %LOCALAPPDATA%\Google\Chrome\User Data
                candidate_dirs.push(local_app_data.join("Google/Chrome/User Data"));
                candidate_dirs.push(local_app_data.join("Chromium/User Data"));
                candidate_dirs.push(local_app_data.join("BraveSoftware/Brave-Browser/User Data"));
                candidate_dirs.push(local_app_data.join("Microsoft/Edge/User Data"));
            }
        }

        #[cfg(target_os = "macos")]
        {
            candidate_dirs.push(home.join("Library/Application Support/Google/Chrome"));
            candidate_dirs.push(home.join("Library/Application Support/Chromium"));
            candidate_dirs.push(home.join("Library/Application Support/BraveSoftware/Brave-Browser"));
            candidate_dirs.push(home.join("Library/Application Support/Microsoft Edge"));
        }

        let mut cookie_path: Option<PathBuf> = None;
        for base in &candidate_dirs {
            if !base.exists() {
                continue;
            }
            // Check Default/Network/Cookies, Default/Cookies, Profile 1/Network/Cookies, etc.
            let subdirs = vec!["Default", "Profile 1", "Profile 2", "."];
            for sub in subdirs {
                let p1 = base.join(sub).join("Network/Cookies");
                if p1.exists() {
                    cookie_path = Some(p1);
                    break;
                }
                let p2 = base.join(sub).join("Cookies");
                if p2.exists() {
                    cookie_path = Some(p2);
                    break;
                }
            }
            if cookie_path.is_some() {
                break;
            }
        }

        let cookie_file = match cookie_path {
            Some(p) => p,
            None => return Err("No se encontró la base de datos de cookies de Chrome / Chromium".into()),
        };

        let pid = std::process::id();
        let tmp_dir = std::env::temp_dir().join(format!("mitub_chrome_cookies_{pid}"));
        let _ = fs::create_dir_all(&tmp_dir);
        let tmp_db = tmp_dir.join("Cookies");

        fs::copy(&cookie_file, &tmp_db)?;

        let conn_res = Connection::open_with_flags(&tmp_db, OpenFlags::SQLITE_OPEN_READ_ONLY);
        let conn = match conn_res {
            Ok(c) => c,
            Err(e) => {
                let _ = fs::remove_dir_all(&tmp_dir);
                return Err(e.into());
            }
        };

        let mut stmt = conn.prepare(
            "SELECT name, value FROM cookies \
             WHERE (host_key = '.youtube.com' OR host_key = 'www.youtube.com' OR host_key = 'youtube.com' OR host_key = '.google.com') \
               AND value != ''",
        )?;

        let rows = stmt.query_map([], |row| {
            let name: String = row.get(0)?;
            let value: String = row.get(1)?;
            Ok((name, value))
        })?;

        let mut session = Self::new();
        for row in rows.flatten() {
            session.set_cookie(&row.0, &row.1);
        }

        drop(stmt);
        drop(conn);
        let _ = fs::remove_dir_all(&tmp_dir);

        if session.cookies.is_empty() || !session.is_authenticated() {
            return Err("Chrome tiene las cookies cifradas por el llavero del sistema o no hay sesión de YouTube. Usa Firefox o pega tu Cookie header.".into());
        }

        let _ = session.save_to_disk();
        Ok(session)
    }

    pub fn clear_from_disk() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(path) = Self::config_path() {
            if path.exists() {
                let _ = fs::remove_file(path);
            }
        }
        Ok(())
    }

    pub fn load_from_disk() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let path = Self::config_path().ok_or("Cannot resolve config path")?;
        let data = fs::read_to_string(path)?;
        let cookies: HashMap<String, String> = serde_json::from_str(&data)?;
        Ok(Self { cookies })
    }

    /// Formats all stored cookies into a single Cookie: header string
    pub fn cookie_header(&self) -> String {
        self.cookies
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// Calculates SAPISIDHASH, SAPISID1PHASH, and SAPISID3PHASH authentication signatures required by YouTube InnerTube
    pub fn get_authorization_header(&self) -> Option<String> {
        let sapisid = self
            .cookies
            .get("SAPISID")
            .or_else(|| self.cookies.get("__Secure-3PAPISID"))
            .or_else(|| self.cookies.get("__Secure-1PAPISID"))?;

        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_secs();

        let hash_for = |token: &str| -> String {
            let mut hasher = Sha1::new();
            let payload = format!("{ts} {token} https://www.youtube.com");
            hasher.update(payload.as_bytes());
            hex::encode(hasher.finalize())
        };

        let mut parts = vec![format!("SAPISIDHASH {ts}_{}", hash_for(sapisid))];

        if let Some(p1) = self.cookies.get("__Secure-1PAPISID").or(Some(sapisid)) {
            parts.push(format!("SAPISID1PHASH {ts}_{}", hash_for(p1)));
        }
        if let Some(p3) = self.cookies.get("__Secure-3PAPISID").or(Some(sapisid)) {
            parts.push(format!("SAPISID3PHASH {ts}_{}", hash_for(p3)));
        }

        Some(parts.join(" "))
    }
}
