use tokio::sync::mpsc;
use zbus::{interface, proxy};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    Restore,
    Quit,
}

struct StatusNotifierItem {
    sender: mpsc::UnboundedSender<TrayAction>,
}

#[interface(name = "org.kde.StatusNotifierItem")]
impl StatusNotifierItem {
    #[zbus(property)]
    fn category(&self) -> &str {
        "ApplicationStatus"
    }

    #[zbus(property)]
    fn id(&self) -> &str {
        "mitub"
    }

    #[zbus(property)]
    fn title(&self) -> &str {
        "OxideTube"
    }

    #[zbus(property)]
    fn status(&self) -> &str {
        "Active"
    }

    #[zbus(property)]
    fn icon_name(&self) -> &str {
        "multimedia-video-player"
    }

    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        false
    }

    fn activate(&self, _x: i32, _y: i32) {
        let _ = self.sender.send(TrayAction::Restore);
    }

    fn secondary_activate(&self, _x: i32, _y: i32) {
        let _ = self.sender.send(TrayAction::Restore);
    }

    fn context_menu(&self, _x: i32, _y: i32) {
        let _ = self.sender.send(TrayAction::Restore);
    }

    fn scroll(&self, _delta: i32, _orientation: &str) {}
}

#[proxy(
    default_service = "org.kde.StatusNotifierWatcher",
    default_path = "/StatusNotifierWatcher",
    interface = "org.kde.StatusNotifierWatcher"
)]
trait StatusNotifierWatcher {
    fn register_status_notifier_item(&self, service: &str) -> zbus::Result<()>;
}

/// Spawns a background task providing FreeDesktop / KDE StatusNotifierItem tray icon.
/// Returns an unbounded receiver of TrayActions (e.g. user clicked the tray icon).
pub fn start_tray_service() -> mpsc::UnboundedReceiver<TrayAction> {
    let (tx, rx) = mpsc::unbounded_channel();
    let pid = std::process::id();
    let service_name = format!("org.kde.StatusNotifierItem-{}-1", pid);

    tokio::spawn(async move {
        let item = StatusNotifierItem { sender: tx };

        let conn_result = zbus::connection::Builder::session()
            .and_then(|b| b.name(service_name.as_str()))
            .and_then(|b| b.serve_at("/StatusNotifierItem", item))
            .and_then(|b| Ok(b.build()));

        let connection = match conn_result {
            Ok(future) => match future.await {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[Mitub Tray] Could not establish D-Bus session connection: {e}");
                    return;
                }
            },
            Err(e) => {
                eprintln!("[Mitub Tray] Failed to configure D-Bus connection builder: {e}");
                return;
            }
        };

        if let Ok(watcher) = StatusNotifierWatcherProxy::new(&connection).await {
            if let Err(e) = watcher.register_status_notifier_item(&service_name).await {
                eprintln!("[Mitub Tray] Notice: StatusNotifierWatcher registration returned: {e}");
            }
        }

        // Keep the connection alive indefinitely
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
        }
    });

    rx
}
