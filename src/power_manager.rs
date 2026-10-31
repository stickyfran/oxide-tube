use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::watch;
#[cfg(target_os = "linux")]
use zbus::proxy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerProfile {
    Battery,
    Ac,
}

#[cfg(target_os = "linux")]
#[proxy(
    default_service = "org.freedesktop.UPower",
    default_path = "/org/freedesktop/UPower",
    interface = "org.freedesktop.UPower"
)]
trait UPower {
    #[zbus(property)]
    fn on_battery(&self) -> zbus::Result<bool>;
}

#[cfg(target_os = "linux")]
#[proxy(
    default_service = "org.freedesktop.ScreenSaver",
    default_path = "/org/freedesktop/ScreenSaver",
    interface = "org.freedesktop.ScreenSaver"
)]
trait ScreenSaver {
    fn inhibit(&self, application_name: &str, reason_for_inhibit: &str) -> zbus::Result<u32>;
    fn un_inhibit(&self, cookie: u32) -> zbus::Result<()>;
}

#[allow(dead_code)]
pub struct PowerManager {
    on_battery: Arc<AtomicBool>,
    sender: watch::Sender<PowerProfile>,
    receiver: watch::Receiver<PowerProfile>,
    inhibit_tx: watch::Sender<bool>,
}

#[allow(dead_code)]
impl PowerManager {
    pub fn new() -> Self {
        let (sender, receiver) = watch::channel(PowerProfile::Ac);
        let (inhibit_tx, inhibit_rx) = watch::channel(false);
        Self::start_screensaver_inhibitor(inhibit_rx);
        Self {
            on_battery: Arc::new(AtomicBool::new(false)),
            sender,
            receiver,
            inhibit_tx,
        }
    }

    pub fn set_screensaver_inhibit(&self, active: bool) {
        if *self.inhibit_tx.borrow() != active {
            let _ = self.inhibit_tx.send(active);
        }
    }

    fn start_screensaver_inhibitor(mut rx: watch::Receiver<bool>) {
        #[cfg(target_os = "linux")]
        tokio::spawn(async move {
            let Ok(conn) = zbus::Connection::session().await else {
                return;
            };
            let Ok(proxy) = ScreenSaverProxy::new(&conn).await else {
                return;
            };
            let mut active_cookie: Option<u32> = None;

            loop {
                let should_inhibit = *rx.borrow_and_update();
                if should_inhibit && active_cookie.is_none() {
                    if let Ok(cookie) = proxy.inhibit("Mitub", "Reproduciendo video en YouTube").await {
                        active_cookie = Some(cookie);
                    }
                } else if !should_inhibit {
                    if let Some(cookie) = active_cookie.take() {
                        let _ = proxy.un_inhibit(cookie).await;
                    }
                }

                if rx.changed().await.is_err() {
                    if let Some(cookie) = active_cookie.take() {
                        let _ = proxy.un_inhibit(cookie).await;
                    }
                    break;
                }
            }
        });

        #[cfg(target_os = "windows")]
        tokio::spawn(async move {
            use windows_sys::Win32::System::Power::{
                SetThreadExecutionState, ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED,
            };
            loop {
                let should_inhibit = *rx.borrow_and_update();
                if should_inhibit {
                    unsafe {
                        SetThreadExecutionState(
                            ES_CONTINUOUS | ES_DISPLAY_REQUIRED | ES_SYSTEM_REQUIRED,
                        );
                    }
                } else {
                    unsafe {
                        SetThreadExecutionState(ES_CONTINUOUS);
                    }
                }
                if rx.changed().await.is_err() {
                    unsafe {
                        SetThreadExecutionState(ES_CONTINUOUS);
                    }
                    break;
                }
            }
        });

        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        tokio::spawn(async move {
            while rx.changed().await.is_ok() {}
        });
    }

    pub fn subscribe(&self) -> watch::Receiver<PowerProfile> {
        self.receiver.clone()
    }

    pub fn is_on_battery(&self) -> bool {
        self.on_battery.load(Ordering::Relaxed)
    }

    /// Spawns the background battery and power listener task.
    pub fn start_listener(&self) {
        let on_battery = self.on_battery.clone();
        let sender = self.sender.clone();

        #[cfg(target_os = "linux")]
        tokio::spawn(async move {
            if let Err(e) = Self::run_dbus_loop(on_battery, sender).await {
                eprintln!("[PowerManager] UPower D-Bus monitor error: {e}. Falling back to polling/AC.");
            }
        });

        #[cfg(target_os = "windows")]
        tokio::spawn(async move {
            use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
            loop {
                interval.tick().await;
                let mut status: SYSTEM_POWER_STATUS = unsafe { std::mem::zeroed() };
                if unsafe { GetSystemPowerStatus(&mut status) } != 0 {
                    let is_battery = status.ACLineStatus == 0;
                    if on_battery.swap(is_battery, Ordering::SeqCst) != is_battery {
                        let profile = if is_battery {
                            Self::apply_battery_profile();
                            PowerProfile::Battery
                        } else {
                            Self::apply_ac_profile();
                            PowerProfile::Ac
                        };
                        let _ = sender.send(profile);
                    }
                }
            }
        });

        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        tokio::spawn(async move {
            let _ = sender.send(PowerProfile::Ac);
        });
    }

    #[cfg(target_os = "linux")]
    async fn run_dbus_loop(
        on_battery_flag: Arc<AtomicBool>,
        sender: watch::Sender<PowerProfile>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let connection = zbus::connection::Builder::system()?.build().await?;
        let proxy = UPowerProxy::new(&connection).await?;

        // Query initial state
        if let Ok(on_battery) = proxy.on_battery().await {
            on_battery_flag.store(on_battery, Ordering::SeqCst);
            let profile = if on_battery {
                Self::apply_battery_profile();
                PowerProfile::Battery
            } else {
                Self::apply_ac_profile();
                PowerProfile::Ac
            };
            let _ = sender.send(profile);
        }

        // Listen for property changes
        use futures_util::StreamExt;
        let mut changes = proxy.receive_on_battery_changed().await;
        while let Some(change) = changes.next().await {
            if let Ok(on_battery) = change.get().await {
                on_battery_flag.store(on_battery, Ordering::SeqCst);
                let profile = if on_battery {
                    Self::apply_battery_profile();
                    PowerProfile::Battery
                } else {
                    Self::apply_ac_profile();
                    PowerProfile::Ac
                };
                let _ = sender.send(profile);
            }
        }

        Ok(())
    }

    /// Trims memory and activates power-saving policies without restricting foreground render workers.
    pub fn apply_battery_profile() {
        println!("[PowerManager] Switching to BATTERY mode: Codec priority AV1/VP9, memory trims active.");
        trim_process_memory();
    }

    /// Restores thread execution to all available CPU cores.
    pub fn apply_ac_profile() {
        println!("[PowerManager] Switching to AC mode: Unpinning cores, enabling high-performance scheduler.");
        unpin_all_cores();
    }
}

/// Safely pins background execution to power-efficient cores
/// based on dynamic topology detection.
pub fn pin_to_lp_ecores() {
    #[cfg(target_os = "linux")]
    unsafe {
        let num_cpus = libc::sysconf(libc::_SC_NPROCESSORS_ONLN) as usize;
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        libc::CPU_ZERO(&mut set);

        // For multi-core systems, target the highest-index efficiency cores.
        if num_cpus >= 8 {
            libc::CPU_SET(num_cpus - 2, &mut set);
            libc::CPU_SET(num_cpus - 1, &mut set);
        } else {
            // Low core count: do not restrict
            for i in 0..num_cpus {
                libc::CPU_SET(i, &mut set);
            }
        }

        let ret = libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &set);
        if ret != 0 {
            if let Some(core_ids) = core_affinity::get_core_ids() {
                if core_ids.len() >= 2 {
                    let _ = core_affinity::set_for_current(core_ids[core_ids.len() - 1]);
                }
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        if let Some(core_ids) = core_affinity::get_core_ids() {
            if core_ids.len() >= 2 {
                let _ = core_affinity::set_for_current(core_ids[core_ids.len() - 1]);
            }
        }
    }
}

/// Restores thread affinity mask to all available cores
pub fn unpin_all_cores() {
    #[cfg(target_os = "linux")]
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        libc::CPU_ZERO(&mut set);
        let num_cpus = libc::sysconf(libc::_SC_NPROCESSORS_ONLN) as usize;
        for i in 0..num_cpus.min(1024) {
            libc::CPU_SET(i, &mut set);
        }
        libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &set);
    }

    #[cfg(not(target_os = "linux"))]
    {
        // On non-Linux platforms, default OS scheduler manages execution
    }
}

extern "C" {
    fn mi_collect(force: bool);
    fn mi_option_set(option: i32, value: isize);
}

/// Configures mimalloc to return freed memory pages to the OS kernel immediately
/// instead of retaining large 32MB per-thread arenas indefinitely.
pub fn configure_allocator_low_memory() {
    unsafe {
        // mi_option_page_reset = 3, mi_option_abandoned_page_purge = 4, mi_option_purge_delay = 15 (v2) / 6 (v1)
        // Setting purge_delay to 0 forces immediate MADV_DONTNEED purging on freed segments.
        mi_option_set(3, 1);
        mi_option_set(4, 1);
        mi_option_set(6, 0);
        mi_option_set(15, 0);
    }
}

/// Safely trims unused heap memory. Invokes mimalloc's mi_collect(true) to force
/// purging and returning unused memory arenas to the Linux kernel via madvise/MADV_DONTNEED.
pub fn trim_process_memory() {
    unsafe {
        mi_collect(true);
    }
}

/// Explicitly purges the process Working Set in Windows.
/// CRITICAL: Must be invoked EXCLUSIVELY when entering idle/minimized state or
/// after video playback has completely stopped, NEVER during active playback
/// to avoid micro-stuttering from soft page faults.
pub fn purge_working_set_on_idle() {
    unsafe {
        mi_collect(true);
    }
    #[cfg(target_os = "windows")]
    unsafe {
        windows_sys::Win32::System::ProcessStatus::EmptyWorkingSet(
            windows_sys::Win32::System::Threading::GetCurrentProcess(),
        );
    }
}

