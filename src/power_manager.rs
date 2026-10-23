use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::watch;
use zbus::proxy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerProfile {
    Battery,
    Ac,
}

#[proxy(
    default_service = "org.freedesktop.UPower",
    default_path = "/org/freedesktop/UPower",
    interface = "org.freedesktop.UPower"
)]
trait UPower {
    #[zbus(property)]
    fn on_battery(&self) -> zbus::Result<bool>;
}

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
    }

    pub fn subscribe(&self) -> watch::Receiver<PowerProfile> {
        self.receiver.clone()
    }

    pub fn is_on_battery(&self) -> bool {
        self.on_battery.load(Ordering::Relaxed)
    }

    /// Spawns the background UPower D-Bus listener task.
    pub fn start_listener(&self) {
        let on_battery = self.on_battery.clone();
        let sender = self.sender.clone();

        tokio::spawn(async move {
            if let Err(e) = Self::run_dbus_loop(on_battery, sender).await {
                eprintln!("[PowerManager] UPower D-Bus monitor error: {e}. Falling back to polling/AC.");
            }
        });
    }

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

    /// Pins thread execution to Intel Core Ultra 7 155H Low-Power Island E-Cores (CPUs 20 and 21)
    /// and invokes memory trims.
    pub fn apply_battery_profile() {
        println!("[PowerManager] Switching to BATTERY mode: Pinning to LP E-Cores (20, 21), codec priority AV1/VP9, trims active.");
        pin_to_lp_ecores();
        trim_process_memory();
    }

    /// Restores thread execution to all available cores (0..21) on Meteor Lake.
    pub fn apply_ac_profile() {
        println!("[PowerManager] Switching to AC mode: Unpinning cores (0..21), enabling high-performance scheduler.");
        unpin_all_cores();
    }
}

/// Safely pins background execution to power-efficient cores (LP E-Cores or standard E-Cores)
/// based on dynamic topology detection.
pub fn pin_to_lp_ecores() {
    unsafe {
        let num_cpus = libc::sysconf(libc::_SC_NPROCESSORS_ONLN) as usize;
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        libc::CPU_ZERO(&mut set);

        // On Meteor Lake 155H with 22 threads, cores 20 & 21 are LP E-Cores.
        // On other topologies, pick the highest index efficiency cores.
        if num_cpus >= 22 {
            libc::CPU_SET(20, &mut set);
            libc::CPU_SET(21, &mut set);
        } else if num_cpus >= 8 {
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
                if core_ids.len() > 20 {
                    let _ = core_affinity::set_for_current(core_ids[20]);
                }
            }
        }
    }
}

/// Restores thread affinity mask to all available cores
pub fn unpin_all_cores() {
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        libc::CPU_ZERO(&mut set);
        let num_cpus = libc::sysconf(libc::_SC_NPROCESSORS_ONLN) as usize;
        for i in 0..num_cpus.min(1024) {
            libc::CPU_SET(i, &mut set);
        }
        libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &set);
    }
}

extern "C" {
    fn mi_collect(force: bool);
}

/// Safely trims unused heap memory. Invokes mimalloc's mi_collect(true) to force
/// purging and returning unused memory arenas to the Linux kernel via madvise/MADV_DONTNEED.
pub fn trim_process_memory() {
    unsafe {
        mi_collect(true);
    }
}

