use crate::innertube::{
    INNERTUBE_CLIENT_NAME, INNERTUBE_CLIENT_VERSION, USER_AGENT_STR, YOUTUBE_ORIGIN,
};
use rusqlite::{Connection, OpenFlags};
use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
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
        // Never overwrite a working saved session with an empty/invalid paste.
        if session.is_authenticated() {
            let _ = session.save_to_disk();
        }
        session
    }

    /// Parses Netscape format cookies.txt (tab separated).
    ///
    /// Handles the `#HttpOnly_` domain prefix used by browsers/curl, and gives
    /// `.youtube.com` rows strict priority over `.google.com` rows.
    fn parse_netscape(content: &str) -> Self {
        let mut google = Vec::new();
        let mut youtube = Vec::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            // `#HttpOnly_.youtube.com` is data, not a comment.
            let data = trimmed.strip_prefix("#HttpOnly_").unwrap_or(trimmed);
            if data.starts_with('#') {
                continue;
            }

            let fields: Vec<&str> = data.split('\t').collect();
            if fields.len() < 7 {
                continue;
            }
            let domain = fields[0];
            let name = fields[5];
            let value = fields[6];
            if domain.contains("youtube.com") {
                youtube.push((name, value));
            } else if domain.contains("google.com") {
                google.push((name, value));
            }
        }

        let mut session = Self::new();
        for (name, value) in google {
            session.set_cookie(name, value);
        }
        for (name, value) in youtube {
            session.set_cookie(name, value);
        }
        session
    }

    /// Parses Netscape format cookies.txt (tab separated)
    pub fn from_netscape_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let content = fs::read_to_string(path)?;
        let session = Self::parse_netscape(&content);
        if session.is_authenticated() {
            let _ = session.save_to_disk();
        }
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

        // Open cookies.sqlite using SQLite URI with immutable=1, or fallback to temporary directory copy with WAL journal
        let conn_res = Connection::open_with_flags(
            format!("file:{}?immutable=1", cookie_file.to_string_lossy()),
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
        );

        let (conn, _tmp_cleanup) = match conn_res {
            Ok(c) => (c, None),
            Err(_) => {
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

                let fallback_conn = Connection::open_with_flags(&tmp_db, OpenFlags::SQLITE_OPEN_READ_ONLY)
                    .map_err(|e| {
                        let _ = fs::remove_dir_all(&tmp_dir);
                        e
                    })?;
                (fallback_conn, Some(tmp_dir))
            }
        };

        // Filter out third-party embedded iframe partitionKeys (`originAttributes = ''` or youtube partition),
        // and ensure `.youtube.com` cookies always override `.google.com` cookies!
        let mut stmt = conn.prepare(
            "SELECT name, value FROM moz_cookies \
             WHERE (host LIKE '%youtube.com' OR host LIKE '%google.com' OR host LIKE '%google.com%') \
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
        if let Some(tmp_dir) = _tmp_cleanup {
            let _ = fs::remove_dir_all(&tmp_dir);
        }

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
        #[cfg(target_os = "windows")]
        {
            if let Some(app_data) = dirs::data_dir() {
                return Some(app_data.join("mitub/cookies.json"));
            }
        }
        let home = dirs::home_dir()?;
        Some(home.join(".config/mitub/cookies.json"))
    }

    pub fn save_to_disk(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(path) = Self::config_path() {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let data = serde_json::to_string_pretty(&self.cookies)?;
            let tmp_path = path.with_extension(format!("tmp.{}", std::process::id()));

            let mut options = fs::OpenOptions::new();
            options.write(true).create(true).truncate(true);
            #[cfg(unix)]
            options.mode(0o600);

            let mut file = options.open(&tmp_path)?;
            file.write_all(data.as_bytes())?;
            file.flush()?;
            drop(file);

            fs::rename(&tmp_path, &path)?;
        }
        Ok(())
    }

    /// Cross-platform Chrome / Chromium cookie discovery & extraction
    pub fn from_chrome() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let home = dirs::home_dir().ok_or("Cannot resolve home directory")?;

        // (base directory, yt-dlp browser id)
        let mut candidate_dirs: Vec<(PathBuf, &'static str)> = Vec::new();

        #[cfg(target_os = "linux")]
        {
            candidate_dirs.push((home.join(".config/google-chrome"), "chrome"));
            candidate_dirs.push((home.join(".config/google-chrome-beta"), "chrome"));
            candidate_dirs.push((home.join(".config/chromium"), "chromium"));
            candidate_dirs.push((home.join(".config/BraveSoftware/Brave-Browser"), "brave"));
            candidate_dirs.push((home.join(".config/microsoft-edge"), "edge"));
            candidate_dirs.push((home.join(".config/vivaldi"), "vivaldi"));
            candidate_dirs.push((home.join(".config/opera"), "opera"));
            // Flatpak / Snap locations
            candidate_dirs.push((home.join(".var/app/com.google.Chrome/config/google-chrome"), "chrome"));
            candidate_dirs.push((home.join(".var/app/org.chromium.Chromium/config/chromium"), "chromium"));
            candidate_dirs.push((home.join(".var/app/com.brave.Browser/config/Brave-Browser"), "brave"));
            candidate_dirs.push((home.join(".var/app/com.microsoft.Edge/config/microsoft-edge"), "edge"));
            candidate_dirs.push((home.join(".var/app/com.vivaldi.Vivaldi/config/vivaldi"), "vivaldi"));
            candidate_dirs.push((home.join("snap/chromium/common/chromium"), "chromium"));
            candidate_dirs.push((home.join("snap/chromium/current/.config/chromium"), "chromium"));
        }

        #[cfg(target_os = "windows")]
        {
            if let Some(local_app_data) = dirs::data_local_dir() {
                // %LOCALAPPDATA%\Google\Chrome\User Data
                candidate_dirs.push((local_app_data.join("Google/Chrome/User Data"), "chrome"));
                candidate_dirs.push((local_app_data.join("Google/Chrome Beta/User Data"), "chrome"));
                candidate_dirs.push((local_app_data.join("Google/Chrome SxS/User Data"), "chrome"));
                candidate_dirs.push((local_app_data.join("Chromium/User Data"), "chromium"));
                candidate_dirs.push((local_app_data.join("BraveSoftware/Brave-Browser/User Data"), "brave"));
                candidate_dirs.push((local_app_data.join("Microsoft/Edge/User Data"), "edge"));
                candidate_dirs.push((local_app_data.join("Vivaldi/User Data"), "vivaldi"));
            }
        }

        #[cfg(target_os = "macos")]
        {
            candidate_dirs.push((home.join("Library/Application Support/Google/Chrome"), "chrome"));
            candidate_dirs.push((home.join("Library/Application Support/Chromium"), "chromium"));
            candidate_dirs.push((home.join("Library/Application Support/BraveSoftware/Brave-Browser"), "brave"));
            candidate_dirs.push((home.join("Library/Application Support/Microsoft Edge"), "edge"));
            candidate_dirs.push((home.join("Library/Application Support/Vivaldi"), "vivaldi"));
        }

        // Pick the most recently modified Cookies DB within the first installed
        // browser (candidate order is Chrome, Chromium, Brave, Edge, ...).
        // Hardcoding only "Default"/"Profile 1"/"Profile 2" missed custom profiles.
        let mut found: Option<(PathBuf, &'static str, PathBuf)> = None;
        for (base, browser) in &candidate_dirs {
            if !base.exists() {
                continue;
            }

            let mut profiles: Vec<PathBuf> = ["Default", "Profile 1", "Profile 2", "Profile 3", "Profile 4",
                "Profile 5", "Profile 6", "Profile 7", "Profile 8", "Profile 9", "Profile 10"]
                .iter()
                .map(|sub| base.join(sub))
                .collect();
            if let Ok(entries) = fs::read_dir(base) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() && !profiles.contains(&path) {
                        profiles.push(path);
                    }
                }
            }

            let mut best: Option<(SystemTime, PathBuf, PathBuf)> = None;
            for profile in profiles {
                for candidate in [profile.join("Network/Cookies"), profile.join("Cookies")] {
                    if !candidate.exists() {
                        continue;
                    }
                    let modified = fs::metadata(&candidate)
                        .and_then(|m| m.modified())
                        .unwrap_or(UNIX_EPOCH);
                    if best.as_ref().map_or(true, |(t, _, _)| modified > *t) {
                        best = Some((modified, candidate, profile.clone()));
                    }
                }
            }

            if let Some((_, cookie_path, profile)) = best {
                found = Some((cookie_path, *browser, profile));
                break;
            }
        }

        let (cookie_file, browser, profile) = match found {
            Some((path, browser, profile)) => (path, browser, Some(profile)),
            None => {
                // Browser not installed: let yt-dlp try its own discovery.
                return Self::from_ytdlp_browser("chrome", None, false);
            }
        };

        let uses_app_bound = Self::chrome_db_uses_app_bound(&cookie_file);
        match Self::extract_chrome_cookie_db(&cookie_file) {
            Ok(session) if session.is_authenticated() => {
                let _ = session.save_to_disk();
                Ok(session)
            }
            // Modern Chromium stores encrypted_value (DPAPI/App-Bound v20 on
            // Windows, Secret Service on Linux) and leaves `value` empty.
            // Delegate decryption to yt-dlp using the exact browser + profile.
            _ => Self::from_ytdlp_browser(browser, profile.as_deref(), uses_app_bound),
        }
    }

    /// Detects Chrome 127+ App-Bound Encryption (`v20`) values, which yt-dlp
    /// cannot decrypt yet. Used only to produce an actionable error message.
    fn chrome_db_uses_app_bound(cookie_file: &Path) -> bool {
        let conn = match Connection::open_with_flags(cookie_file, OpenFlags::SQLITE_OPEN_READ_ONLY) {
            Ok(c) => c,
            Err(_) => return false,
        };
        let mut stmt = match conn.prepare(
            "SELECT 1 FROM cookies WHERE hex(encrypted_value) LIKE '763230%' LIMIT 1",
        ) {
            Ok(s) => s,
            Err(_) => return false,
        };
        stmt.exists([]).unwrap_or(false)
    }

    /// Reads plaintext `value` rows from a Chrome/Chromium Cookies database.
    /// A copy is used (including WAL/SHM sidecars) so a running browser is safe.
    fn extract_chrome_cookie_db(cookie_file: &Path) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let pid = std::process::id();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let tmp_dir = std::env::temp_dir().join(format!("mitub_chrome_cookies_{pid}_{nonce}"));
        let _ = fs::create_dir_all(&tmp_dir);
        let tmp_db = tmp_dir.join("Cookies");

        let copy_result = (|| -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            fs::copy(cookie_file, &tmp_db)?;
            if let Some(parent) = cookie_file.parent() {
                for suffix in ["-wal", "-shm"] {
                    let src = parent.join(format!("Cookies{suffix}"));
                    if src.exists() {
                        let _ = fs::copy(&src, tmp_dir.join(format!("Cookies{suffix}")));
                    }
                }
            }
            Ok(())
        })();

        if let Err(e) = copy_result {
            let _ = fs::remove_dir_all(&tmp_dir);
            return Err(e);
        }

        let conn = match Connection::open_with_flags(
            &tmp_db,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        ) {
            Ok(c) => c,
            Err(e) => {
                let _ = fs::remove_dir_all(&tmp_dir);
                return Err(Box::new(e));
            }
        };

        let result = (|| -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
            let mut session = Self::new();
            // Google first, YouTube second: YouTube rows must override Google ones.
            for domain in ["%google.com", "%youtube.com"] {
                let mut stmt = conn.prepare(
                    "SELECT name, value FROM cookies WHERE host_key LIKE ?1 AND value != ''",
                )?;
                let rows = stmt.query_map([domain], |row| {
                    let name: String = row.get(0)?;
                    let value: String = row.get(1)?;
                    Ok((name, value))
                })?;
                for row in rows.flatten() {
                    session.set_cookie(&row.0, &row.1);
                }
            }
            Ok(session)
        })();

        drop(conn);
        let _ = fs::remove_dir_all(&tmp_dir);
        result
    }

    /// Extracts browser cookies by delegating decryption to yt-dlp when available
    fn from_ytdlp_browser(
        browser: &str,
        profile: Option<&Path>,
        uses_app_bound: bool,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let binary = crate::innertube::InnerTubeClient::find_ytdlp_binary_pub();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let tmp_cookie_file =
            std::env::temp_dir().join(format!("mitub_{browser}_export_{}_{nonce}.txt", std::process::id()));

        let mut browser_spec = browser.to_string();
        if let Some(profile) = profile {
            browser_spec.push(':');
            browser_spec.push_str(&profile.to_string_lossy());
        }

        let output = std::process::Command::new(&binary)
            .arg("--cookies-from-browser")
            .arg(&browser_spec)
            .arg("--cookies")
            .arg(&tmp_cookie_file)
            .arg("--skip-download")
            .arg("--no-warnings")
            .arg("https://www.youtube.com")
            .output();

        match output {
            Ok(out) => {
                // yt-dlp writes the exported jar even when the URL itself fails,
                // so do not require a successful exit status here.
                if tmp_cookie_file.exists() {
                    if let Ok(session) = Self::from_netscape_file(&tmp_cookie_file) {
                        if session.is_authenticated() {
                            let _ = fs::remove_file(&tmp_cookie_file);
                            return Ok(session);
                        }
                    }
                }

                let stderr = String::from_utf8_lossy(&out.stderr);
                let hint = if uses_app_bound
                    || stderr.contains("v20")
                    || stderr.to_lowercase().contains("app-bound")
                {
                    " Chrome 127+ usa cifrado App-Bound (v20), que yt-dlp todavía no puede descifrar."
                } else {
                    ""
                };
                let detail = stderr.lines().last().unwrap_or("").trim();
                let _ = fs::remove_file(&tmp_cookie_file);
                Err(format!(
                    "No se pudo descifrar la sesión de {browser}.{hint} \
                     Usa Firefox o pega el header Cookie manualmente. {detail}"
                )
                .into())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let _ = fs::remove_file(&tmp_cookie_file);
                Err("No se encontró yt-dlp. Instálalo (o colócalo junto al ejecutable) \
                     para poder descifrar las cookies de Chrome/Chromium."
                    .into())
            }
            Err(e) => {
                let _ = fs::remove_file(&tmp_cookie_file);
                Err(format!("No se pudo ejecutar yt-dlp: {e}").into())
            }
        }
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

    /// Verifies if the stored credentials represent an active authenticated YouTube session
    /// by calling YouTube's account menu endpoint with the same auth/client version
    /// the rest of the app uses.
    pub async fn verify_active_session(&self) -> bool {
        if !self.is_authenticated() {
            return false;
        }

        let auth_val = match self.get_authorization_header() {
            Some(v) => v,
            None => return false,
        };

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(8))
            .build()
            .unwrap_or_default();

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(reqwest::header::USER_AGENT, reqwest::header::HeaderValue::from_static(USER_AGENT_STR));
        headers.insert(reqwest::header::ORIGIN, reqwest::header::HeaderValue::from_static(YOUTUBE_ORIGIN));
        headers.insert("X-Origin", reqwest::header::HeaderValue::from_static(YOUTUBE_ORIGIN));
        headers.insert("X-Goog-AuthUser", reqwest::header::HeaderValue::from_static("0"));
        headers.insert("X-Youtube-Client-Name", reqwest::header::HeaderValue::from_static(INNERTUBE_CLIENT_NAME));
        headers.insert("X-Youtube-Client-Version", reqwest::header::HeaderValue::from_static(INNERTUBE_CLIENT_VERSION));
        if let Ok(v) = reqwest::header::HeaderValue::from_str(&auth_val) {
            headers.insert(reqwest::header::AUTHORIZATION, v);
        }
        let cookie_str = self.cookie_header();
        if let Ok(c) = reqwest::header::HeaderValue::from_str(&cookie_str) {
            headers.insert(reqwest::header::COOKIE, c);
        }

        let body = serde_json::json!({
            "context": {
                "client": {
                    "clientName": "WEB",
                    "clientVersion": INNERTUBE_CLIENT_VERSION,
                    "hl": "es",
                    "gl": "ES",
                    "utcOffsetMinutes": 0
                }
            }
        });

        let url = format!("{YOUTUBE_ORIGIN}/youtubei/v1/account/account_menu?prettyPrint=false");
        if let Ok(resp) = client.post(&url).headers(headers).json(&body).send().await {
            if resp.status().is_success() {
                if let Ok(val) = resp.json::<serde_json::Value>().await {
                    return crate::innertube::InnerTubeClient::is_response_logged_in(&val);
                }
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn netscape_parser_accepts_httponly_and_prefers_youtube() {
        let data = concat!(
            "# Netscape HTTP Cookie File\n",
            "#HttpOnly_.google.com\tTRUE\t/\tTRUE\t0\tSAPISID\tgoogle-value\n",
            ".google.com\tTRUE\t/\tTRUE\t0\tLOGIN_INFO\tlogin-info\n",
            "#HttpOnly_.youtube.com\tTRUE\t/\tTRUE\t0\tSAPISID\tyoutube-value\n",
            "# comment that must be ignored\n",
        );

        let session = AuthSession::parse_netscape(data);
        assert!(session.is_authenticated());
        assert_eq!(
            session.cookies.get("SAPISID").map(String::as_str),
            Some("youtube-value")
        );
        assert_eq!(
            session.cookies.get("LOGIN_INFO").map(String::as_str),
            Some("login-info")
        );
    }

    #[test]
    fn chrome_db_reads_plaintext_values_and_prefers_youtube() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "mitub_auth_test_{}_{}",
            std::process::id(),
            nonce
        ));
        fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("Cookies");

        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE cookies (
                    host_key TEXT NOT NULL,
                    name TEXT NOT NULL,
                    value TEXT NOT NULL,
                    encrypted_value BLOB
                );",
            )
            .unwrap();
            conn.execute_batch(
                "INSERT INTO cookies (host_key, name, value, encrypted_value) VALUES
                    ('.google.com', 'SAPISID', 'google-value', x''),
                    ('.google.com', 'HSID', 'google-hsid', x''),
                    ('.youtube.com', 'SAPISID', 'youtube-value', x''),
                    ('example.com', 'SAPISID', 'wrong-domain', x'');",
            )
            .unwrap();
        }

        let session = AuthSession::extract_chrome_cookie_db(&db_path).unwrap();
        assert!(session.is_authenticated());
        assert_eq!(
            session.cookies.get("SAPISID").map(String::as_str),
            Some("youtube-value")
        );
        assert_eq!(
            session.cookies.get("HSID").map(String::as_str),
            Some("google-hsid")
        );
        assert!(!session.cookies.values().any(|v| v == "wrong-domain"));

        let _ = fs::remove_dir_all(&dir);
    }
}
