//! Multi-account storage.
//!
//! Each account keeps its own `config.json` (tabs, history, playback choices)
//! and `cookies.json` (YouTube session) under
//! `~/.config/mitub/accounts/<id>/`. A small manifest remembers the account
//! names/avatars and which one is active.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountEntry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountsManifest {
    #[serde(default)]
    pub active: String,
    #[serde(default)]
    pub accounts: Vec<AccountEntry>,
}

impl Default for AccountsManifest {
    fn default() -> Self {
        Self {
            active: String::new(),
            accounts: Vec::new(),
        }
    }
}

pub fn config_root() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config/mitub")
}

pub fn accounts_root() -> PathBuf {
    config_root().join("accounts")
}

fn manifest_path() -> PathBuf {
    config_root().join("accounts.json")
}

pub fn manifest() -> AccountsManifest {
    ensure_initialized();
    fs::read_to_string(manifest_path())
        .ok()
        .and_then(|data| serde_json::from_str::<AccountsManifest>(&data).ok())
        .filter(|manifest| !manifest.accounts.is_empty() && !manifest.active.is_empty())
        .unwrap_or_else(|| {
            let entry = AccountEntry {
                id: "default".to_string(),
                name: "Cuenta 1".to_string(),
                avatar_url: None,
            };
            AccountsManifest {
                active: entry.id.clone(),
                accounts: vec![entry],
            }
        })
}

fn save_manifest(manifest: &AccountsManifest) {
    let root = config_root();
    let _ = fs::create_dir_all(&root);
    if let Ok(data) = serde_json::to_string_pretty(manifest) {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let tmp = root.join(format!("accounts.tmp.{}.{}", std::process::id(), nonce));
        if fs::write(&tmp, data).is_ok() {
            let _ = fs::rename(&tmp, manifest_path());
        }
    }
}

/// Creates the account layout on first run and migrates the legacy single
/// `config.json` / `cookies.json` into the default account.
pub fn ensure_initialized() {
    if manifest_path().exists() {
        return;
    }
    let root = config_root();
    let accounts = accounts_root();
    let default_dir = accounts.join("default");
    let _ = fs::create_dir_all(&default_dir);

    // Migrate the old single-account files, keeping the originals as a backup
    // on the first move only when the target does not exist yet.
    for (legacy, target) in [
        (root.join("config.json"), default_dir.join("config.json")),
        (root.join("cookies.json"), default_dir.join("cookies.json")),
    ] {
        if !target.exists() && legacy.exists() {
            if fs::rename(&legacy, &target).is_err() {
                let _ = fs::copy(&legacy, &target);
            }
        }
    }

    let (name, avatar_url) = fs::read_to_string(default_dir.join("config.json"))
        .ok()
        .and_then(|data| serde_json::from_str::<serde_json::Value>(&data).ok())
        .map(|value| {
            (
                value
                    .get("cached_user_name")
                    .and_then(|v| v.as_str())
                    .filter(|name| !name.is_empty())
                    .unwrap_or("Cuenta 1")
                    .to_string(),
                value
                    .get("cached_avatar_url")
                    .and_then(|v| v.as_str())
                    .map(ToOwned::to_owned),
            )
        })
        .unwrap_or_else(|| ("Cuenta 1".to_string(), None));

    save_manifest(&AccountsManifest {
        active: "default".to_string(),
        accounts: vec![AccountEntry {
            id: "default".to_string(),
            name,
            avatar_url,
        }],
    });
}

pub fn active_id() -> String {
    let manifest = manifest();
    if manifest.active.is_empty() {
        manifest
            .accounts
            .first()
            .map(|account| account.id.clone())
            .unwrap_or_else(|| "default".to_string())
    } else {
        manifest.active
    }
}

pub fn active_dir() -> PathBuf {
    let dir = accounts_root().join(active_id());
    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn active_config_path() -> PathBuf {
    active_dir().join("config.json")
}

pub fn active_cookies_path() -> PathBuf {
    active_dir().join("cookies.json")
}

/// Whether the account has a saved YouTube session on disk.
pub fn has_saved_session(id: &str) -> bool {
    accounts_root().join(id).join("cookies.json").is_file()
}

pub fn set_active(id: &str) {
    let mut manifest = manifest();
    if manifest.accounts.iter().any(|account| account.id == id) {
        manifest.active = id.to_string();
        save_manifest(&manifest);
    }
}

/// Creates a fresh account slot and makes it active.
pub fn create_account(name: &str) -> AccountEntry {
    ensure_initialized();
    let mut manifest = manifest();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let mut id = format!("account-{stamp}");
    let mut suffix = 1;
    while manifest.accounts.iter().any(|account| account.id == id) {
        id = format!("account-{stamp}-{suffix}");
        suffix += 1;
    }
    let entry = AccountEntry {
        id: id.clone(),
        name: name.to_string(),
        avatar_url: None,
    };
    manifest.accounts.push(entry.clone());
    manifest.active = id;
    save_manifest(&manifest);
    let _ = fs::create_dir_all(accounts_root().join(&entry.id));
    entry
}

pub fn update_active_profile(name: &str, avatar_url: Option<&str>) {
    let active = active_id();
    let mut manifest = manifest();
    let mut changed = false;
    for account in &mut manifest.accounts {
        if account.id == active {
            if !name.is_empty() && account.name != name {
                account.name = name.to_string();
                changed = true;
            }
            if let Some(url) = avatar_url {
                if account.avatar_url.as_deref() != Some(url) {
                    account.avatar_url = Some(url.to_string());
                    changed = true;
                }
            }
        }
    }
    if changed {
        save_manifest(&manifest);
    }
}

pub fn clear_active_session() {
    let path = active_cookies_path();
    let _ = fs::remove_file(path);
}

/// Stable identity of the YouTube account stored in an account folder.
///
/// Uses a hash of `SAPISID` (or the equivalent secure cookie) so two profiles
/// that logged into the same YouTube account can be detected. Returns `None`
/// when the account has no saved session.
pub fn session_fingerprint(id: &str) -> Option<String> {
    use sha1::{Digest, Sha1};
    let data = fs::read_to_string(accounts_root().join(id).join("cookies.json")).ok()?;
    let cookies: std::collections::HashMap<String, String> = serde_json::from_str(&data).ok()?;
    let raw = cookies
        .get("SAPISID")
        .or_else(|| cookies.get("__Secure-3PAPISID"))
        .or_else(|| cookies.get("SID"))?;
    let mut hasher = Sha1::new();
    hasher.update(raw.as_bytes());
    Some(hex::encode(hasher.finalize()))
}

fn same_account(a: &AccountEntry, b: &AccountEntry) -> bool {
    let fp_a = session_fingerprint(&a.id);
    let fp_b = session_fingerprint(&b.id);
    if let (Some(fp_a), Some(fp_b)) = (&fp_a, &fp_b) {
        return fp_a == fp_b;
    }
    // One of them is logged out: fall back to the profile identity.
    let avatar_a = a.avatar_url.as_deref().filter(|url| !url.is_empty());
    let avatar_b = b.avatar_url.as_deref().filter(|url| !url.is_empty());
    avatar_a.is_some() && avatar_a == avatar_b && a.name == b.name
}

/// Returns `(keep, remove)` when two profiles point at the same YouTube
/// account. The first entry in the manifest is kept so the profile with the
/// oldest tabs/history survives.
pub fn find_duplicate_accounts() -> Option<(AccountEntry, AccountEntry)> {
    let accounts = manifest().accounts;
    for (i, keep) in accounts.iter().enumerate() {
        for remove in accounts.iter().skip(i + 1) {
            if same_account(keep, remove) {
                return Some((keep.clone(), remove.clone()));
            }
        }
    }
    None
}

/// Merges `remove` into `keep`: takes the freshest cookies and profile data,
/// deletes the duplicate folder and removes it from the manifest.
pub fn merge_accounts(keep: &str, remove: &str) -> bool {
    if keep == remove {
        return false;
    }
    let keep_dir = accounts_root().join(keep);
    let remove_dir = accounts_root().join(remove);
    let mut manifest = manifest();
    let removed_entry = manifest.accounts.iter().find(|a| a.id == remove).cloned();

    if remove_dir.join("cookies.json").is_file() {
        let _ = fs::create_dir_all(&keep_dir);
        let _ = fs::copy(
            remove_dir.join("cookies.json"),
            keep_dir.join("cookies.json"),
        );
    }
    if let Some(entry) = &removed_entry {
        for account in &mut manifest.accounts {
            if account.id == keep {
                if !entry.name.is_empty() {
                    account.name = entry.name.clone();
                }
                if entry.avatar_url.is_some() {
                    account.avatar_url = entry.avatar_url.clone();
                }
            }
        }
    }
    let _ = fs::remove_dir_all(&remove_dir);
    manifest.accounts.retain(|account| account.id != remove);
    if manifest.active == remove {
        manifest.active = keep.to_string();
    }
    save_manifest(&manifest);
    true
}

/// Logs out of `id` and deletes its tabs, history and saved session.
///
/// Returns `Some(new_active_id)` when the active profile changed (or when the
/// last remaining account was reset in place), so the caller can restart.
pub fn delete_account(id: &str) -> Option<String> {
    let mut manifest = manifest();
    let dir = accounts_root().join(id);

    if manifest.accounts.len() <= 1 {
        // Keep one slot so the app always has somewhere to log in.
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::create_dir_all(&dir);
        for account in &mut manifest.accounts {
            if account.id == id {
                account.name = "Cuenta 1".to_string();
                account.avatar_url = None;
            }
        }
        manifest.active = id.to_string();
        save_manifest(&manifest);
        return Some(id.to_string());
    }

    let was_active = manifest.active == id;
    manifest.accounts.retain(|account| account.id != id);
    let _ = fs::remove_dir_all(&dir);
    if was_active {
        let next = manifest
            .accounts
            .first()
            .map(|account| account.id.clone())
            .unwrap_or_else(|| "default".to_string());
        manifest.active = next.clone();
        save_manifest(&manifest);
        Some(next)
    } else {
        save_manifest(&manifest);
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_legacy_single_account_and_supports_switching() {
        let tmp = std::env::temp_dir().join(format!("mitub-accounts-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join(".config/mitub")).unwrap();
        fs::write(
            tmp.join(".config/mitub/config.json"),
            r#"{"cached_user_name":"Alice","cached_avatar_url":"https://example.com/a.png"}"#,
        )
        .unwrap();
        fs::write(tmp.join(".config/mitub/cookies.json"), "cookies").unwrap();

        let previous_home = std::env::var_os("HOME");
        std::env::set_var("HOME", &tmp);

        ensure_initialized();
        assert!(tmp.join(".config/mitub/accounts/default/config.json").exists());
        assert!(tmp.join(".config/mitub/accounts/default/cookies.json").exists());
        assert!(!tmp.join(".config/mitub/config.json").exists());
        let first = manifest();
        assert_eq!(first.active, "default");
        assert_eq!(first.accounts.len(), 1);
        assert_eq!(first.accounts[0].name, "Alice");

        let second = create_account("Cuenta 2");
        assert_eq!(active_id(), second.id);
        assert!(accounts_root().join(&second.id).exists());
        set_active("default");
        assert_eq!(active_id(), "default");
        ensure_initialized();
        assert_eq!(manifest().accounts.len(), 2);

        clear_active_session();
        assert!(!tmp.join(".config/mitub/accounts/default/cookies.json").exists());

        // Duplicate profiles that logged into the same account are merged,
        // keeping the oldest profile and the freshest cookies.
        fs::write(
            accounts_root().join("default").join("cookies.json"),
            r#"{"SAPISID":"same-account"}"#,
        )
        .unwrap();
        fs::write(
            accounts_root().join(&second.id).join("cookies.json"),
            r#"{"SAPISID":"same-account"}"#,
        )
        .unwrap();
        let (keep, remove) = find_duplicate_accounts().expect("duplicate detected");
        assert_eq!(keep.id, "default");
        assert_eq!(remove.id, second.id);
        assert!(merge_accounts(&keep.id, &remove.id));
        assert_eq!(manifest().accounts.len(), 1);
        assert!(!accounts_root().join(&second.id).exists());
        assert!(accounts_root().join("default").join("cookies.json").is_file());

        // A different YouTube account is not a duplicate.
        let third = create_account("Cuenta 3");
        fs::write(
            accounts_root().join(&third.id).join("cookies.json"),
            r#"{"SAPISID":"another-account"}"#,
        )
        .unwrap();
        assert!(find_duplicate_accounts().is_none());

        // Deleting a non-active account does not change the active profile.
        assert!(delete_account("default").is_none());
        assert_eq!(active_id(), third.id);
        assert_eq!(manifest().accounts.len(), 1);

        // Deleting the last account resets it in place instead of leaving the
        // app without an account slot.
        let reset = delete_account(&third.id);
        assert_eq!(reset.as_deref(), Some(third.id.as_str()));
        assert_eq!(manifest().accounts.len(), 1);
        assert!(!accounts_root().join(&third.id).join("cookies.json").exists());

        match previous_home {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }
        let _ = fs::remove_dir_all(&tmp);
    }
}
