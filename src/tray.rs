use tokio::sync::mpsc;
use zbus::{interface, proxy};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    Restore,
    Quit,
}

fn load_tray_icon_pixmap() -> Vec<(i32, i32, Vec<u8>)> {
    let icon_bytes = include_bytes!("../assets/icon.png");
    if let Ok(img) = image::load_from_memory(icon_bytes) {
        let resized = img.resize(32, 32, image::imageops::FilterType::Lanczos3);
        let rgba = resized.to_rgba8();
        let (w, h) = rgba.dimensions();
        let mut argb = Vec::with_capacity((w * h * 4) as usize);
        for pixel in rgba.pixels() {
            argb.push(pixel[3]); // Alpha
            argb.push(pixel[0]); // Red
            argb.push(pixel[1]); // Green
            argb.push(pixel[2]); // Blue
        }
        vec![(w as i32, h as i32, argb)]
    } else {
        vec![]
    }
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
        "Tube-oxide"
    }

    #[zbus(property)]
    fn title(&self) -> &str {
        "Tube-oxide"
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
    fn icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
        load_tray_icon_pixmap()
    }

    #[zbus(property)]
    fn icon_theme_path(&self) -> &str {
        ""
    }

    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn menu(&self) -> zbus::zvariant::ObjectPath<'_> {
        zbus::zvariant::ObjectPath::from_static_str_unchecked("/NO_DBUSMENU")
    }

    #[zbus(property)]
    fn overlay_icon_name(&self) -> &str {
        ""
    }

    #[zbus(property)]
    fn overlay_icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
        vec![]
    }

    #[zbus(property)]
    fn attention_icon_name(&self) -> &str {
        ""
    }

    #[zbus(property)]
    fn attention_icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
        vec![]
    }

    #[zbus(property)]
    fn attention_movie_name(&self) -> &str {
        ""
    }

    #[zbus(property)]
    fn window_id(&self) -> i32 {
        0
    }

    #[zbus(property)]
    fn tool_tip(&self) -> (String, Vec<(i32, i32, Vec<u8>)>, String, String) {
        (
            "multimedia-video-player".to_string(),
            vec![],
            "Tube-oxide".to_string(),
            "Reproductor nativo de YouTube".to_string(),
        )
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

    fn provide_xdg_activation_token(&self, _token: &str) {
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
                    eprintln!("[Tube-oxide Tray] Could not establish D-Bus session connection: {e}");
                    return;
                }
            },
            Err(e) => {
                eprintln!("[Tube-oxide Tray] Failed to configure D-Bus connection builder: {e}");
                return;
            }
        };

        if let Ok(watcher) = StatusNotifierWatcherProxy::new(&connection).await {
            if let Err(e) = watcher.register_status_notifier_item(&service_name).await {
                eprintln!("[Tube-oxide Tray] Notice: StatusNotifierWatcher registration returned: {e}");
            }
        }

        // Keep the connection alive indefinitely
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
        }
    });

    rx
}
