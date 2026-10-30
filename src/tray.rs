use std::sync::OnceLock;
use tokio::sync::broadcast;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    Restore,
    PlayPause,
    Stop,
    Quit,
}

static TRAY_CHANNEL: OnceLock<broadcast::Sender<TrayAction>> = OnceLock::new();

// ─── Tray visibility control ───────────────────────────────────────────────

/// Set to true to show the tray icon, false to hide it.
/// Called from main.rs when close_to_tray is toggled or the window is closed to tray.
pub fn set_tray_visible(visible: bool) {
    #[cfg(target_os = "linux")]
    linux::set_tray_visible(visible);
    #[cfg(target_os = "windows")]
    windows::set_tray_visible(visible);
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    let _ = visible;
}

// ─── Linux (KDE StatusNotifierItem via D-Bus) ─────────────────────────────

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use zbus::zvariant::{OwnedValue, Type, Value};
    use zbus::{interface, proxy};

    static TRAY_VISIBLE: AtomicBool = AtomicBool::new(false);

    pub fn set_tray_visible(visible: bool) {
        TRAY_VISIBLE.store(visible, Ordering::Relaxed);
        // Trigger a NewStatus signal so KDE picks up the change
        SNI_SIGNAL.get().map(|tx| { let _ = tx.send(visible); });
    }

    // Channel to notify the async SNI task of visibility changes
    static SNI_SIGNAL: OnceLock<std::sync::mpsc::SyncSender<bool>> = OnceLock::new();

    fn load_tray_icon_pixmap() -> Vec<(i32, i32, Vec<u8>)> {
        // Use the white symbolic icon (KDE style)
        let icon_bytes = include_bytes!("../assets/tray_icon_white.png");
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

    #[derive(Type, Serialize, Deserialize, Debug, Clone)]
    pub struct DbusMenuItemLayout {
        pub id: i32,
        pub fields: HashMap<String, OwnedValue>,
        pub children: Vec<OwnedValue>,
    }

    pub struct StatusNotifierItem {
        sender: broadcast::Sender<TrayAction>,
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

        /// "Active" = visible, "Passive" = hidden
        #[zbus(property)]
        fn status(&self) -> &str {
            if TRAY_VISIBLE.load(std::sync::atomic::Ordering::Relaxed) {
                "Active"
            } else {
                "Passive"
            }
        }

        #[zbus(property)]
        fn icon_name(&self) -> &str {
            // Empty name: use pixmap instead (white symbolic pixmap)
            ""
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
            zbus::zvariant::ObjectPath::from_static_str_unchecked("/MenuBar")
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
                "".to_string(),
                vec![],
                "Tube-oxide".to_string(),
                "Reproductor nativo de YouTube".to_string(),
            )
        }

        fn activate(&self, _x: i32, _y: i32) {
            let _ = self.sender.send(TrayAction::Restore);
        }

        fn secondary_activate(&self, _x: i32, _y: i32) {
            let _ = self.sender.send(TrayAction::PlayPause);
        }

        fn context_menu(&self, _x: i32, _y: i32) {
            // Handled via DBusMenu on /MenuBar
        }

        fn provide_xdg_activation_token(&self, token: &str) {
            if !token.is_empty() {
                std::env::set_var("XDG_ACTIVATION_TOKEN", token);
            }
        }

        fn scroll(&self, _delta: i32, _orientation: &str) {}
    }

    pub struct DBusMenu {
        sender: broadcast::Sender<TrayAction>,
    }

    #[interface(name = "com.canonical.dbusmenu")]
    impl DBusMenu {
        #[zbus(property)]
        fn version(&self) -> u32 {
            3
        }

        #[zbus(property)]
        fn text_direction(&self) -> &str {
            "ltr"
        }

        #[zbus(property)]
        fn status(&self) -> &str {
            "normal"
        }

        #[zbus(property)]
        fn icon_theme_path(&self) -> Vec<String> {
            vec![]
        }

        fn get_layout(
            &self,
            parent_id: i32,
            _recursion_depth: i32,
            _property_names: Vec<String>,
        ) -> zbus::fdo::Result<(u32, DbusMenuItemLayout)> {
            if parent_id != 0 {
                return Err(zbus::fdo::Error::InvalidArgs("Parent not found".into()));
            }

            let mut children = Vec::new();

            let make_item = |id: i32, label: &str, icon: &str| -> Option<OwnedValue> {
                let mut fields: HashMap<String, OwnedValue> = HashMap::new();
                fields.insert("label".to_string(), Value::from(label.to_string()).try_into().ok()?);
                fields.insert("enabled".to_string(), Value::from(true).try_into().ok()?);
                fields.insert("visible".to_string(), Value::from(true).try_into().ok()?);
                if !icon.is_empty() {
                    fields.insert("icon-name".to_string(), Value::from(icon.to_string()).try_into().ok()?);
                }
                let tuple = (id, fields, Vec::<OwnedValue>::new());
                let val = Value::from(tuple);
                OwnedValue::try_from(val).ok()
            };

            let make_separator = |id: i32| -> Option<OwnedValue> {
                let mut fields: HashMap<String, OwnedValue> = HashMap::new();
                fields.insert("type".to_string(), Value::from("separator".to_string()).try_into().ok()?);
                fields.insert("visible".to_string(), Value::from(true).try_into().ok()?);
                let tuple = (id, fields, Vec::<OwnedValue>::new());
                let val = Value::from(tuple);
                OwnedValue::try_from(val).ok()
            };

            // 1: Abrir / Restaurar Tube-oxide
            if let Some(v) = make_item(1, "Abrir Tube-oxide", "window-restore") {
                children.push(v);
            }
            // 2: Separador
            if let Some(v) = make_separator(2) {
                children.push(v);
            }
            // 3: Reproducir / Pausar
            if let Some(v) = make_item(3, "Reproducir / Pausar", "media-playback-start") {
                children.push(v);
            }
            // 4: Detener
            if let Some(v) = make_item(4, "Detener reproducción", "media-playback-stop") {
                children.push(v);
            }
            // 5: Separador
            if let Some(v) = make_separator(5) {
                children.push(v);
            }
            // 6: Salir
            if let Some(v) = make_item(6, "Salir de Tube-oxide", "application-exit") {
                children.push(v);
            }

            let mut root_fields: HashMap<String, OwnedValue> = HashMap::new();
            if let Ok(v) = Value::from("submenu".to_string()).try_into() {
                root_fields.insert("children-display".to_string(), v);
            }

            let root = DbusMenuItemLayout {
                id: 0,
                fields: root_fields,
                children,
            };

            Ok((1, root))
        }

        fn get_group_properties(
            &self,
            ids: Vec<i32>,
            _property_names: Vec<String>,
        ) -> Vec<(i32, HashMap<String, OwnedValue>)> {
            let mut result = Vec::new();
            for id in ids {
                let mut fields = HashMap::new();
                match id {
                    0 => {
                        let _ = Value::from("submenu".to_string()).try_into().map(|v| fields.insert("children-display".into(), v));
                    }
                    1 => {
                        let _ = Value::from("Abrir Tube-oxide".to_string()).try_into().map(|v| fields.insert("label".into(), v));
                        let _ = Value::from("window-restore".to_string()).try_into().map(|v| fields.insert("icon-name".into(), v));
                        let _ = Value::from(true).try_into().map(|v| fields.insert("enabled".into(), v));
                        let _ = Value::from(true).try_into().map(|v| fields.insert("visible".into(), v));
                    }
                    2 | 5 => {
                        let _ = Value::from("separator".to_string()).try_into().map(|v| fields.insert("type".into(), v));
                        let _ = Value::from(true).try_into().map(|v| fields.insert("visible".into(), v));
                    }
                    3 => {
                        let _ = Value::from("Reproducir / Pausar".to_string()).try_into().map(|v| fields.insert("label".into(), v));
                        let _ = Value::from("media-playback-start".to_string()).try_into().map(|v| fields.insert("icon-name".into(), v));
                        let _ = Value::from(true).try_into().map(|v| fields.insert("enabled".into(), v));
                        let _ = Value::from(true).try_into().map(|v| fields.insert("visible".into(), v));
                    }
                    4 => {
                        let _ = Value::from("Detener reproducción".to_string()).try_into().map(|v| fields.insert("label".into(), v));
                        let _ = Value::from("media-playback-stop".to_string()).try_into().map(|v| fields.insert("icon-name".into(), v));
                        let _ = Value::from(true).try_into().map(|v| fields.insert("enabled".into(), v));
                        let _ = Value::from(true).try_into().map(|v| fields.insert("visible".into(), v));
                    }
                    6 => {
                        let _ = Value::from("Salir de Tube-oxide".to_string()).try_into().map(|v| fields.insert("label".into(), v));
                        let _ = Value::from("application-exit".to_string()).try_into().map(|v| fields.insert("icon-name".into(), v));
                        let _ = Value::from(true).try_into().map(|v| fields.insert("enabled".into(), v));
                        let _ = Value::from(true).try_into().map(|v| fields.insert("visible".into(), v));
                    }
                    _ => {}
                }
                result.push((id, fields));
            }
            result
        }

        fn get_property(&self, id: i32, name: &str) -> zbus::fdo::Result<OwnedValue> {
            match (id, name) {
                (0, "children-display") => Value::from("submenu".to_string()).try_into().map_err(|e| zbus::fdo::Error::Failed(format!("{e}"))),
                (1, "label") => Value::from("Abrir Tube-oxide".to_string()).try_into().map_err(|e| zbus::fdo::Error::Failed(format!("{e}"))),
                (3, "label") => Value::from("Reproducir / Pausar".to_string()).try_into().map_err(|e| zbus::fdo::Error::Failed(format!("{e}"))),
                (4, "label") => Value::from("Detener reproducción".to_string()).try_into().map_err(|e| zbus::fdo::Error::Failed(format!("{e}"))),
                (6, "label") => Value::from("Salir de Tube-oxide".to_string()).try_into().map_err(|e| zbus::fdo::Error::Failed(format!("{e}"))),
                (2 | 5, "type") => Value::from("separator".to_string()).try_into().map_err(|e| zbus::fdo::Error::Failed(format!("{e}"))),
                (_, "enabled") => Value::from(true).try_into().map_err(|e| zbus::fdo::Error::Failed(format!("{e}"))),
                (_, "visible") => Value::from(true).try_into().map_err(|e| zbus::fdo::Error::Failed(format!("{e}"))),
                _ => Err(zbus::fdo::Error::UnknownProperty(name.into())),
            }
        }

        fn event(&self, id: i32, event_id: &str, _data: OwnedValue, _timestamp: u32) {
            self.handle_action(id, event_id);
        }

        fn event_group(&self, events: Vec<(i32, String, OwnedValue, u32)>) -> Vec<i32> {
            for (id, event_id, _data, _timestamp) in events {
                self.handle_action(id, &event_id);
            }
            Vec::new()
        }

        fn about_to_show(&self, _id: i32) -> bool {
            false
        }

        fn about_to_show_group(&self, _ids: Vec<i32>) -> (Vec<i32>, Vec<i32>) {
            (Vec::new(), Vec::new())
        }
    }

    impl DBusMenu {
        fn handle_action(&self, id: i32, event_id: &str) {
            if event_id == "clicked" || event_id == "activate" {
                match id {
                    1 => {
                        let _ = self.sender.send(TrayAction::Restore);
                    }
                    3 => {
                        let _ = self.sender.send(TrayAction::PlayPause);
                    }
                    4 => {
                        let _ = self.sender.send(TrayAction::Stop);
                    }
                    6 => {
                        let _ = self.sender.send(TrayAction::Quit);
                    }
                    _ => {}
                }
            }
        }
    }

    #[proxy(
        default_service = "org.kde.StatusNotifierWatcher",
        default_path = "/StatusNotifierWatcher",
        interface = "org.kde.StatusNotifierWatcher"
    )]
    trait StatusNotifierWatcher {
        fn register_status_notifier_item(&self, service: &str) -> zbus::Result<()>;
    }

    static STARTED: OnceLock<()> = OnceLock::new();

    pub fn init_tray(tx: broadcast::Sender<TrayAction>) {
        if STARTED.set(()).is_err() {
            return;
        }

        // Set up the visibility signal channel
        let (sig_tx, sig_rx) = std::sync::mpsc::sync_channel::<bool>(8);
        let _ = SNI_SIGNAL.set(sig_tx);

        let pid = std::process::id();
        let service_name = format!("org.kde.StatusNotifierItem-{}-1", pid);

        tokio::spawn(async move {
            let item = StatusNotifierItem { sender: tx.clone() };
            let menu = DBusMenu { sender: tx };

            let conn_result = zbus::connection::Builder::session()
                .and_then(|b| b.name(service_name.as_str()))
                .and_then(|b| b.serve_at("/StatusNotifierItem", item))
                .and_then(|b| b.serve_at("/MenuBar", menu))
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

            // Listen for visibility-change signals and emit NewStatus D-Bus signal
            let sni_iface_ref = connection
                .object_server()
                .interface::<_, StatusNotifierItem>("/StatusNotifierItem")
                .await;

            loop {
                // Poll for visibility changes (sync channel in async context via spawn_blocking)
                let sig_result = tokio::task::spawn_blocking({
                    let sig_rx_ptr = &sig_rx as *const _ as usize;
                    move || {
                        // SAFETY: sig_rx lives for the duration of this task
                        let rx = unsafe { &*(sig_rx_ptr as *const std::sync::mpsc::Receiver<bool>) };
                        rx.recv_timeout(std::time::Duration::from_secs(3600))
                    }
                }).await;

                if let Ok(Ok(_visible)) = sig_result {
                    // Emit NewStatus signal so the panel/taskbar updates
                    if let Ok(ref iface) = sni_iface_ref {
                        let ctxt = iface.signal_context();
                        let status = if TRAY_VISIBLE.load(std::sync::atomic::Ordering::Relaxed) {
                            "Active"
                        } else {
                            "Passive"
                        };
                        // Emit the NewStatus signal via zbus
                        let _ = zbus::object_server::Interface::properties_changed(
                            iface.get().await,
                            ctxt,
                            &std::collections::HashSet::from(["Status".to_string()]),
                        ).await;
                        let _ = ctxt
                            .connection()
                            .emit_signal(
                                None::<()>,
                                "/StatusNotifierItem",
                                "org.kde.StatusNotifierItem",
                                "NewStatus",
                                &(status,),
                            ).await;
                    }
                }
            }
        });
    }
}

// ─── Windows (Shell_NotifyIconW) ──────────────────────────────────────────

#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use std::ptr;
    use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{
        CreateBitmap, CreateCompatibleDC, DeleteDC, DeleteObject,
        GetDC, ReleaseDC, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    use windows_sys::Win32::UI::Shell::{
        Shell_NotifyIconW, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
        NIF_ICON, NIF_MESSAGE, NIF_TIP,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreateIconIndirect, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
        DestroyMenu, DestroyWindow, DispatchMessageW, GetCursorPos, GetMessageW,
        RegisterClassW, SetForegroundWindow, TrackPopupMenu, TranslateMessage,
        MF_SEPARATOR, MF_STRING, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RIGHTBUTTON,
        WM_COMMAND, WM_DESTROY, WM_LBUTTONUP, WM_RBUTTONUP, WM_USER, WNDCLASSW, ICONINFO,
    };

    const WM_TRAYICON: u32 = WM_USER + 101;
    const ID_RESTORE: usize = 1001;
    const ID_PLAY_PAUSE: usize = 1002;
    const ID_STOP: usize = 1003;
    const ID_QUIT: usize = 1004;

    static SENDER: std::sync::OnceLock<broadcast::Sender<TrayAction>> = std::sync::OnceLock::new();
    /// Handle to the hidden message-only window; 0 = not yet created
    static HWND_TRAY: AtomicIsize = AtomicIsize::new(0);
    /// Whether the icon is currently added to the tray
    static ICON_ADDED: AtomicBool = AtomicBool::new(false);

    /// Build an HICON from the white PNG bytes embedded at compile time.
    /// Returns 0 (null) on failure.
    unsafe fn create_white_hicon() -> isize {
        let icon_bytes = include_bytes!("../assets/tray_icon_white.png");
        let img = match image::load_from_memory(icon_bytes) {
            Ok(i) => i,
            Err(_) => return 0,
        };
        let rgba = img.resize(32, 32, image::imageops::FilterType::Lanczos3).to_rgba8();
        let (w, h) = rgba.dimensions();
        let w = w as i32;
        let h = h as i32;

        // Build a top-down DIB with pre-multiplied alpha for the color mask
        // Windows BITMAPINFOHEADER for 32-bpp top-down
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // negative = top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB as u32,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [windows_sys::Win32::Graphics::Gdi::RGBQUAD {
                rgbBlue: 0, rgbGreen: 0, rgbRed: 0, rgbReserved: 0,
            }],
        };

        // Convert RGBA → BGRA (Windows DIB order)
        let mut bgra: Vec<u8> = Vec::with_capacity((w * h * 4) as usize);
        for pixel in rgba.pixels() {
            bgra.push(pixel[2]); // B
            bgra.push(pixel[1]); // G
            bgra.push(pixel[0]); // R
            bgra.push(pixel[3]); // A
        }

        let screen_dc = GetDC(0);
        let mem_dc = CreateCompatibleDC(screen_dc);

        let mut color_bits_ptr: *mut std::ffi::c_void = ptr::null_mut();
        let color_bmp = windows_sys::Win32::Graphics::Gdi::CreateDIBSection(
            mem_dc,
            &bmi,
            DIB_RGB_COLORS,
            &mut color_bits_ptr,
            0,
            0,
        );
        if color_bmp == 0 {
            DeleteDC(mem_dc);
            ReleaseDC(0, screen_dc);
            return 0;
        }
        // Copy pixel data into the DIB
        std::ptr::copy_nonoverlapping(bgra.as_ptr(), color_bits_ptr as *mut u8, bgra.len());

        // Create a 1-bpp mask bitmap (all zeros = fully use color bitmap alpha)
        let mask_bmp = CreateBitmap(w, h, 1, 1, ptr::null());

        DeleteDC(mem_dc);
        ReleaseDC(0, screen_dc);

        if mask_bmp == 0 {
            DeleteObject(color_bmp);
            return 0;
        }

        let icon_info = ICONINFO {
            fIcon: 1,    // 1 = icon (not cursor)
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask_bmp,
            hbmColor: color_bmp,
        };
        let hicon = CreateIconIndirect(&icon_info);

        DeleteObject(mask_bmp);
        DeleteObject(color_bmp);

        hicon
    }

    unsafe fn build_nid(hwnd: HWND, hicon: isize) -> NOTIFYICONDATAW {
        let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_MESSAGE | NIF_TIP | if hicon != 0 { NIF_ICON } else { 0 };
        nid.uCallbackMessage = WM_TRAYICON;
        nid.hIcon = hicon;
        let tip: Vec<u16> = "Tube-oxide\0".encode_utf16().collect();
        let copy_len = tip.len().min(nid.szTip.len());
        ptr::copy_nonoverlapping(tip.as_ptr(), nid.szTip.as_mut_ptr(), copy_len);
        nid
    }

    pub fn set_tray_visible(visible: bool) {
        let hwnd = HWND_TRAY.load(Ordering::Relaxed) as HWND;
        if hwnd == 0 {
            return; // Window not created yet; init_tray will handle initial state
        }
        unsafe {
            if visible && !ICON_ADDED.load(Ordering::Relaxed) {
                let hicon = create_white_hicon();
                let nid = build_nid(hwnd, hicon);
                Shell_NotifyIconW(NIM_ADD, &nid);
                ICON_ADDED.store(true, Ordering::Relaxed);
            } else if !visible && ICON_ADDED.load(Ordering::Relaxed) {
                let nid = build_nid(hwnd, 0);
                Shell_NotifyIconW(NIM_DELETE, &nid);
                ICON_ADDED.store(false, Ordering::Relaxed);
            }
        }
    }

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_TRAYICON => {
                let event = lparam as u32;
                if event == WM_LBUTTONUP {
                    if let Some(tx) = SENDER.get() {
                        let _ = tx.send(TrayAction::Restore);
                    }
                } else if event == WM_RBUTTONUP {
                    let mut pt: POINT = std::mem::zeroed();
                    GetCursorPos(&mut pt);
                    SetForegroundWindow(hwnd);

                    let hmenu = CreatePopupMenu();
                    let to_wide = |s: &str| -> Vec<u16> {
                        s.encode_utf16().chain(std::iter::once(0)).collect()
                    };

                    let str_restore = to_wide("Abrir Tube-oxide");
                    let str_play_pause = to_wide("Reproducir / Pausar");
                    let str_stop = to_wide("Detener");
                    let str_quit = to_wide("Salir de Tube-oxide");

                    AppendMenuW(hmenu, MF_STRING, ID_RESTORE, str_restore.as_ptr());
                    AppendMenuW(hmenu, MF_SEPARATOR, 0, ptr::null());
                    AppendMenuW(hmenu, MF_STRING, ID_PLAY_PAUSE, str_play_pause.as_ptr());
                    AppendMenuW(hmenu, MF_STRING, ID_STOP, str_stop.as_ptr());
                    AppendMenuW(hmenu, MF_SEPARATOR, 0, ptr::null());
                    AppendMenuW(hmenu, MF_STRING, ID_QUIT, str_quit.as_ptr());

                    TrackPopupMenu(
                        hmenu,
                        TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | TPM_LEFTALIGN,
                        pt.x,
                        pt.y,
                        0,
                        hwnd,
                        ptr::null(),
                    );
                    DestroyMenu(hmenu);
                }
                0
            }
            WM_COMMAND => {
                let cmd_id = wparam as usize;
                if let Some(tx) = SENDER.get() {
                    match cmd_id {
                        ID_RESTORE => { let _ = tx.send(TrayAction::Restore); }
                        ID_PLAY_PAUSE => { let _ = tx.send(TrayAction::PlayPause); }
                        ID_STOP => { let _ = tx.send(TrayAction::Stop); }
                        ID_QUIT => { let _ = tx.send(TrayAction::Quit); }
                        _ => {}
                    }
                }
                0
            }
            WM_DESTROY => 0,
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    pub fn init_tray(tx: broadcast::Sender<TrayAction>) {
        let _ = SENDER.set(tx);
        std::thread::spawn(|| unsafe {
            let class_name: Vec<u16> = "TubeOxideTrayClass\0".encode_utf16().collect();
            let wc = WNDCLASSW {
                style: 0,
                lpfnWndProc: Some(window_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: 0,
                hIcon: 0,
                hCursor: 0,
                hbrBackground: 0,
                lpszMenuName: ptr::null(),
                lpszClassName: class_name.as_ptr(),
            };
            RegisterClassW(&wc);

            let hwnd = CreateWindowExW(
                0,
                class_name.as_ptr(),
                class_name.as_ptr(),
                0,
                0, 0, 0, 0,
                0, 0, 0, ptr::null(),
            );

            // Store hwnd so set_tray_visible can use it from any thread
            HWND_TRAY.store(hwnd as isize, Ordering::Relaxed);

            // Do NOT add the icon here — it will be added when set_tray_visible(true) is called

            let mut msg = std::mem::zeroed();
            while GetMessageW(&mut msg, 0, 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            // Clean up if still added
            if ICON_ADDED.load(Ordering::Relaxed) {
                let nid = build_nid(hwnd, 0);
                Shell_NotifyIconW(NIM_DELETE, &nid);
            }
            DestroyWindow(hwnd);
        });
    }
}

// ─── Fallback (other OS) ──────────────────────────────────────────────────

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
mod fallback {
    use super::*;
    pub fn init_tray(_tx: broadcast::Sender<TrayAction>) {}
}

// ─── Public API ───────────────────────────────────────────────────────────

pub fn get_tray_receiver() -> broadcast::Receiver<TrayAction> {
    let tx = TRAY_CHANNEL.get_or_init(|| {
        let (tx, _) = broadcast::channel(64);
        // Start the background tray service (icon is hidden until set_tray_visible(true))
        #[cfg(target_os = "linux")]
        linux::init_tray(tx.clone());
        #[cfg(target_os = "windows")]
        windows::init_tray(tx.clone());
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        fallback::init_tray(tx.clone());
        tx
    });
    tx.subscribe()
}

pub fn start_tray_service() -> broadcast::Receiver<TrayAction> {
    get_tray_receiver()
}
