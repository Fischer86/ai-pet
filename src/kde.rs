use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use sysinfo::{Disks, System};

/// Charge l'icône du Samouraï pour la barre des tâches (KDE Plasma, Windows, macOS).
pub fn load_window_icon() -> eframe::egui::IconData {
    let bytes = include_bytes!("../assets/Samurai/3x/idle_0.png");
    if let Ok(img) = image::load_from_memory(bytes) {
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        return eframe::egui::IconData {
            rgba: rgba.into_raw(),
            width: w,
            height: h,
        };
    }
    eframe::egui::IconData {
        rgba: vec![0; 32 * 32 * 4],
        width: 32,
        height: 32,
    }
}

/// Détecte la résolution de l'écran principal sur Linux (xrandr), macOS et Windows.
pub fn detect_primary_screen_size() -> (f32, f32) {
    if cfg!(target_os = "linux") {
        if let Ok(out) = Command::new("xrandr").arg("--current").output() {
            let s = String::from_utf8_lossy(&out.stdout);
            for line in s.lines() {
                if line.contains('*') {
                    if let Some(res_token) = line.split_whitespace().next() {
                        if let Some((w_str, h_str)) = res_token.split_once('x') {
                            if let (Ok(w), Ok(h)) = (w_str.parse::<f32>(), h_str.parse::<f32>()) {
                                if w >= 800.0 && h >= 600.0 {
                                    return (w, h);
                                }
                            }
                        }
                    }
                }
            }
        }
    } else if cfg!(target_os = "macos") {
        if let Ok(out) = Command::new("osascript")
            .args([
                "-e",
                "tell application \"Finder\" to get bounds of window of desktop",
            ])
            .output()
        {
            let s = String::from_utf8_lossy(&out.stdout);
            let nums: Vec<f32> = s
                .split(',')
                .filter_map(|p| p.trim().parse::<f32>().ok())
                .collect();
            if nums.len() == 4 && nums[2] >= 800.0 {
                return (nums[2], nums[3]);
            }
        }
    } else if cfg!(target_os = "windows") {
        if let Ok(out) = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "Add-Type -AssemblyName System.Windows.Forms; $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds; \"$($b.Width)x$($b.Height)\"",
            ])
            .output()
        {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if let Some((w_str, h_str)) = s.split_once('x') {
                if let (Ok(w), Ok(h)) = (w_str.parse::<f32>(), h_str.parse::<f32>()) {
                    if w >= 800.0 && h >= 600.0 {
                        return (w, h);
                    }
                }
            }
        }
    }
    (2560.0, 1440.0)
}

/// Retourne le dossier personnel de l'utilisateur sur Linux, macOS et Windows.
pub fn user_home_dir() -> PathBuf {
    if let Ok(h) = std::env::var("HOME") {
        return PathBuf::from(h);
    }
    if let Ok(up) = std::env::var("USERPROFILE") {
        return PathBuf::from(up);
    }
    PathBuf::from(".")
}

/// Retourne le dossier de données applicatives selon l'OS (Linux XDG, macOS Application Support, Windows APPDATA).
pub fn app_data_dir() -> PathBuf {
    let base = if cfg!(target_os = "windows") {
        std::env::var("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| user_home_dir().join("AppData").join("Roaming"))
    } else if cfg!(target_os = "macos") {
        user_home_dir()
            .join("Library")
            .join("Application Support")
    } else {
        std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| user_home_dir().join(".local").join("share"))
    };
    let dir = base.join("ai-pet");
    let _ = fs::create_dir_all(&dir);
    dir
}

/// Sauvegarde l'icône du Samouraï dans le dossier applicatif pour les notifications et le fichier `.desktop`.
pub fn ensure_saved_icon_path() -> PathBuf {
    let icon_path = app_data_dir().join("kaze_icon.png");
    if !icon_path.exists() {
        let bytes = include_bytes!("../assets/Samurai/3x/idle_0.png");
        let _ = fs::write(&icon_path, bytes);
    }
    icon_path
}

/// Nom lisible de l'OS et de l'environnement de bureau.
pub fn os_and_desktop_label() -> String {
    let os_name = System::name().unwrap_or_else(|| std::env::consts::OS.to_string());
    let os_ver = System::os_version().unwrap_or_default();
    if cfg!(target_os = "linux") {
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "Linux".into());
        let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "x11".into());
        format!("{} {} — {} ({})", os_name, os_ver, desktop, session)
    } else if cfg!(target_os = "macos") {
        format!("macOS {} ({})", os_ver, std::env::consts::ARCH)
    } else if cfg!(target_os = "windows") {
        format!("Windows {} ({})", os_ver, std::env::consts::ARCH)
    } else {
        format!("{} {}", os_name, os_ver)
    }
}

/// Bilan système multi-plateforme complet via `sysinfo`.
pub fn get_system_snapshot() -> String {
    let mut sys = System::new_all();
    sys.refresh_all();

    let total_mem_gb = sys.total_memory() as f64 / (1024.0 * 1024.0 * 1024.0);
    let used_mem_gb = sys.used_memory() as f64 / (1024.0 * 1024.0 * 1024.0);
    let mem_pct = if total_mem_gb > 0.0 {
        (used_mem_gb / total_mem_gb) * 100.0
    } else {
        0.0
    };

    let cpu_count = sys.cpus().len();
    let load = System::load_average();
    let load_str = if cfg!(target_os = "windows") {
        format!("{:.1}% ({} cœurs)", sys.global_cpu_usage(), cpu_count)
    } else {
        format!(
            "{:.2} / {:.2} / {:.2} ({} cœurs)",
            load.one, load.five, load.fifteen, cpu_count
        )
    };

    let uptime_secs = System::uptime();
    let uptime_str = format!("{}h {:02}m", uptime_secs / 3600, (uptime_secs % 3600) / 60);
    let disk_str = read_primary_disk();
    let vol_str = get_audio_volume_summary();

    format!(
        "• Système : {}\n• Charge CPU : {}\n• RAM : {:.1} Go / {:.1} Go ({:.0}%)\n• Disque : {}\n• Audio : {}\n• Uptime : {}",
        os_and_desktop_label(),
        load_str,
        used_mem_gb,
        total_mem_gb,
        mem_pct,
        disk_str,
        vol_str,
        uptime_str
    )
}

/// Résumé compact sur une ligne pour la conscience temps réel du modèle.
pub fn get_compact_context() -> String {
    let mut sys = System::new();
    sys.refresh_memory();

    let total_mem_gb = sys.total_memory() as f64 / (1024.0 * 1024.0 * 1024.0);
    let used_mem_gb = sys.used_memory() as f64 / (1024.0 * 1024.0 * 1024.0);
    let mem_pct = if total_mem_gb > 0.0 {
        (used_mem_gb / total_mem_gb) * 100.0
    } else {
        0.0
    };
    let load = System::load_average();
    let vol = get_audio_volume_summary();

    format!(
        "OS: {} | Charge: {:.2} | RAM: {:.1}/{:.1} Go ({:.0}%) | Audio: {}",
        os_and_desktop_label(),
        load.one,
        used_mem_gb,
        total_mem_gb,
        mem_pct,
        vol
    )
}

/// Retourne les 3 processus principaux les plus actifs sous forme compacte.
pub fn get_top3_processes_compact() -> String {
    let mut sys = System::new_all();
    sys.refresh_all();

    let mut procs: Vec<_> = sys
        .processes()
        .values()
        .filter(|p| p.thread_kind().is_none())
        .collect();
    procs.sort_by(|a, b| b.memory().cmp(&a.memory()));

    let mut seen = std::collections::HashSet::new();
    let mut top = Vec::new();
    for p in procs {
        let name = p.name().to_string_lossy().to_string();
        if seen.insert(name.clone()) {
            let mb = p.memory() as f64 / (1024.0 * 1024.0);
            top.push(format!("{} ({:.0} Mo)", name, mb));
            if top.len() >= 4 {
                break;
            }
        }
    }

    if top.is_empty() {
        "N/A".into()
    } else {
        top.join(", ")
    }
}

/// Liste détaillée des 6 processus distincts les plus gourmands.
pub fn get_top_processes_detailed() -> String {
    let mut sys = System::new_all();
    sys.refresh_all();

    let total_mem = sys.total_memory().max(1) as f64;
    let mut procs: Vec<_> = sys
        .processes()
        .values()
        .filter(|p| p.thread_kind().is_none())
        .collect();
    procs.sort_by(|a, b| b.memory().cmp(&a.memory()));

    let mut seen = std::collections::HashSet::new();
    let mut lines = Vec::new();
    for p in procs {
        let name = p.name().to_string_lossy().to_string();
        if !seen.insert(name.clone()) {
            continue;
        }
        let short: String = name.chars().take(18).collect();
        let mem_mb = p.memory() as f64 / (1024.0 * 1024.0);
        let mem_pct = (p.memory() as f64 / total_mem) * 100.0;
        lines.push(format!(
            "• {:<18} RAM: {:>6.0} Mo ({:>4.1}%)",
            short, mem_mb, mem_pct
        ));
        if lines.len() >= 6 {
            break;
        }
    }

    if lines.is_empty() {
        "⚠️ Aucun processus lisible.".into()
    } else {
        format!(
            "🔥 Applications utilisant le plus de mémoire :\n{}",
            lines.join("\n")
        )
    }
}

fn read_primary_disk() -> String {
    let disks = Disks::new_with_refreshed_list();
    if let Some(d) = disks
        .list()
        .iter()
        .find(|d| {
            let mp = d.mount_point().to_string_lossy();
            mp == "/" || mp == "C:\\" || mp == "C:"
        })
        .or_else(|| disks.list().first())
    {
        let total = d.total_space() as f64 / (1024.0 * 1024.0 * 1024.0);
        let avail = d.available_space() as f64 / (1024.0 * 1024.0 * 1024.0);
        let used = (total - avail).max(0.0);
        let pct = if total > 0.0 {
            (used / total) * 100.0
        } else {
            0.0
        };
        return format!("{:.0} Go / {:.0} Go ({:.0}%)", used, total, pct);
    }
    "N/A".into()
}

pub fn get_audio_volume_summary() -> String {
    if cfg!(target_os = "linux") {
        if let Ok(out) = Command::new("wpctl")
            .args(["get-volume", "@DEFAULT_AUDIO_SINK@"])
            .output()
        {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if let Some(rest) = s.strip_prefix("Volume:") {
                let muted = rest.contains("MUTED");
                let num_str = rest.split_whitespace().next().unwrap_or("0.0");
                if let Ok(val) = num_str.parse::<f64>() {
                    let pct = (val * 100.0).round() as i32;
                    return if muted {
                        format!("{}% (Muet 🔇)", pct)
                    } else {
                        format!("{}% 🔊", pct)
                    };
                }
            }
        }
    } else if cfg!(target_os = "macos") {
        if let Ok(out) = Command::new("osascript")
            .args(["-e", "output volume of (get volume settings)"])
            .output()
        {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                return format!("{}% 🔊", s);
            }
        }
    }
    "Actif 🔊".into()
}

pub fn read_clipboard_text() -> Option<String> {
    if let Ok(mut cb) = arboard::Clipboard::new() {
        if let Ok(txt) = cb.get_text() {
            let trimmed = txt.trim().to_string();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }
    if cfg!(target_os = "linux") {
        let mut cmd = Command::new("wl-paste");
        cmd.arg("--no-newline");
        if let Ok(wl) = std::env::var("KAZE_WAYLAND_DISPLAY") {
            cmd.env("WAYLAND_DISPLAY", wl);
        }
        if let Ok(out) = cmd.output() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }
    None
}

pub fn peek_clipboard_compact(max_chars: usize) -> String {
    match read_clipboard_text() {
        Some(txt) => {
            let one_line = txt.replace('\n', " ↵ ");
            let snippet: String = one_line.chars().take(max_chars).collect();
            if one_line.chars().count() > max_chars {
                format!("« {}… »", snippet)
            } else {
                format!("« {} »", snippet)
            }
        }
        None => "(vide)".into(),
    }
}

pub fn write_clipboard_text(text: &str) -> Result<(), String> {
    if cfg!(target_os = "linux") {
        let mut cmd = Command::new("wl-copy");
        if let Ok(wl) = std::env::var("KAZE_WAYLAND_DISPLAY") {
            cmd.env("WAYLAND_DISPLAY", wl);
        }
        if let Ok(mut child) = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                use std::io::Write;
                let _ = stdin.write_all(text.as_bytes());
                return Ok(());
            }
        }
    }
    let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    cb.set_text(text.to_string()).map_err(|e| e.to_string())
}

pub fn send_kde_notification(title: &str, body: &str) -> Result<(), String> {
    if cfg!(target_os = "linux") {
        let icon_path = ensure_saved_icon_path();
        let mut cmd = Command::new("notify-send");
        cmd.arg("--app-name=Kaze AI Pet")
            .arg(format!("--icon={}", icon_path.display()))
            .arg(title)
            .arg(body);
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
        cmd.spawn().map_err(|e| e.to_string())?;
        return Ok(());
    } else if cfg!(target_os = "macos") {
        let script = format!("display notification {:?} with title {:?}", body, title);
        Command::new("osascript")
            .args(["-e", &script])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        return Ok(());
    } else if cfg!(target_os = "windows") {
        let ps = format!(
            "[reflection.assembly]::loadwithpartialname('System.Windows.Forms'); \
             $n = new-object system.windows.forms.notifyicon; \
             $n.icon = [system.drawing.systemicons]::Information; \
             $n.visible = $true; \
             $n.showballoontip(4000, {:?}, {:?}, [system.windows.forms.tooltipicon]::None)",
            title, body
        );
        Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    Ok(())
}

/// Installe automatiquement :
/// 1) Le fichier `~/.local/share/applications/ai-pet.desktop` pour que le gestionnaire de tâches
///    KDE Plasma affiche toujours Kaze avec son icône de Samouraï dans la barre des tâches.
/// 2) Une règle KWin dédiée dans `~/.config/kwinrulesrc` (`above=true`, `skiptaskbar=false`)
///    et appelle `org.kde.KWin.reconfigure` via D-Bus pour que KWin force le maintien au-dessus
///    et la présence dans la barre des tâches au niveau du compositeur !
#[cfg(target_os = "linux")]
fn ensure_kde_desktop_and_kwin_rules() {
    let home = user_home_dir();
    let icon_path = ensure_saved_icon_path();
    let exe_path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("ai-pet"));

    // 1. Créer ~/.local/share/applications/ai-pet.desktop
    let apps_dir = home.join(".local/share/applications");
    let _ = fs::create_dir_all(&apps_dir);
    let desktop_file = apps_dir.join("ai-pet.desktop");
    let desktop_content = format!(
        "[Desktop Entry]\n\
        Type=Application\n\
        Name=Kaze AI Pet\n\
        Comment=Compagnon Samouraï Zen propulsé par Ollama\n\
        Exec={}\n\
        Icon={}\n\
        Terminal=false\n\
        StartupWMClass=ai-pet\n\
        Categories=Utility;\n",
        exe_path.display(),
        icon_path.display()
    );
    let _ = fs::write(desktop_file, desktop_content);

    // 2. Configurer ~/.config/kwinrulesrc si absent ou sans la règle Kaze
    let config_dir = home.join(".config");
    let _ = fs::create_dir_all(&config_dir);
    let kwinrules_path = config_dir.join("kwinrulesrc");
    let existing = fs::read_to_string(&kwinrules_path).unwrap_or_default();
    if !existing.contains("Description=Kaze AI Pet Always On Top") {
        let rule_content = "\
[1]\n\
Description=Kaze AI Pet Always On Top\n\
above=true\n\
aboverule=2\n\
desktops=\\\\0\n\
desktopsrule=2\n\
noborder=true\n\
noborderrule=2\n\
skippager=false\n\
skippagerrule=2\n\
skiptaskbar=false\n\
skiptaskbarrule=2\n\
wmclass=ai-pet\n\
wmclasscomplete=false\n\
wmclassmatch=1\n\
\n\
[General]\n\
count=1\n\
rules=1\n";
        if existing.trim().is_empty() {
            let _ = fs::write(&kwinrules_path, rule_content);
        }
        let _ = Command::new("gdbus")
            .args([
                "call",
                "--session",
                "--dest",
                "org.kde.KWin",
                "--object-path",
                "/KWin",
                "--method",
                "org.kde.KWin.reconfigure",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// Envoie de vrais événements EWMH `ClientMessage` à la fenêtre racine X11 via `x11rb` :
/// - Force `_NET_WM_STATE_ABOVE` (Always On Top) et `_NET_WM_STATE_STICKY` (Tous les bureaux)
/// - Retire `_NET_WM_STATE_SKIP_TASKBAR` et `_NET_WM_STATE_SKIP_PAGER` (Toujours dans la barre des tâches)
/// - Force `_NET_WM_WINDOW_TYPE_NORMAL` et retire l'ombre rectangulaire `_KDE_NET_WM_SHADOW`
#[cfg(target_os = "linux")]
pub fn enforce_x11_ewmh_top_and_taskbar(window_title: &str) {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{
        AtomEnum, ClientMessageEvent, ConnectionExt, EventMask, PropMode, Window,
    };

    let Ok((conn, screen_num)) = x11rb::connect(None) else {
        return;
    };
    let root = conn.setup().roots[screen_num].root;

    let intern = |name: &[u8]| -> Option<u32> {
        conn.intern_atom(false, name)
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| r.atom)
    };

    let Some(wm_state) = intern(b"_NET_WM_STATE") else { return; };
    let Some(wm_state_above) = intern(b"_NET_WM_STATE_ABOVE") else { return; };
    let Some(wm_state_sticky) = intern(b"_NET_WM_STATE_STICKY") else { return; };
    let Some(wm_state_skip_tb) = intern(b"_NET_WM_STATE_SKIP_TASKBAR") else { return; };
    let Some(wm_state_skip_pager) = intern(b"_NET_WM_STATE_SKIP_PAGER") else { return; };
    let Some(wm_window_type) = intern(b"_NET_WM_WINDOW_TYPE") else { return; };
    let Some(wm_window_type_normal) = intern(b"_NET_WM_WINDOW_TYPE_NORMAL") else { return; };
    let Some(net_wm_name) = intern(b"_NET_WM_NAME") else { return; };
    let Some(utf8_string) = intern(b"UTF8_STRING") else { return; };
    let Some(kde_shadow) = intern(b"_KDE_NET_WM_SHADOW") else { return; };

    fn find_window_recursive<C: Connection>(
        conn: &C,
        win: Window,
        target_title: &str,
        net_wm_name_atom: u32,
        utf8_atom: u32,
    ) -> Option<Window> {
        if let Some(prop) = conn
            .get_property(false, win, net_wm_name_atom, utf8_atom, 0, 256)
            .ok()
            .and_then(|c| c.reply().ok())
        {
            if let Ok(name) = std::str::from_utf8(&prop.value) {
                if name.trim_matches('\0') == target_title {
                    return Some(win);
                }
            }
        }
        if let Some(prop) = conn
            .get_property(false, win, AtomEnum::WM_NAME, AtomEnum::STRING, 0, 256)
            .ok()
            .and_then(|c| c.reply().ok())
        {
            if let Ok(name) = std::str::from_utf8(&prop.value) {
                if name.trim_matches('\0') == target_title {
                    return Some(win);
                }
            }
        }
        if let Some(tree) = conn.query_tree(win).ok().and_then(|c| c.reply().ok()) {
            for child in tree.children {
                if let Some(found) = find_window_recursive(
                    conn,
                    child,
                    target_title,
                    net_wm_name_atom,
                    utf8_atom,
                ) {
                    return Some(found);
                }
            }
        }
        None
    }

    let Some(target_win) =
        find_window_recursive(&conn, root, window_title, net_wm_name, utf8_string)
    else {
        return;
    };

    // 1. Définir _NET_WM_WINDOW_TYPE = _NET_WM_WINDOW_TYPE_NORMAL pour garantir l'entrée dans la barre des tâches
    let type_bytes = wm_window_type_normal.to_ne_bytes();
    let _ = conn.change_property(
        PropMode::REPLACE,
        target_win,
        wm_window_type,
        AtomEnum::ATOM,
        32,
        1,
        &type_bytes,
    );

    // 2. Retirer l'ombre rectangulaire KWin
    let _ = conn.delete_property(target_win, kde_shadow);

    // 3. Envoyer ClientMessage EWMH : _NET_WM_STATE_ADD (1) pour ABOVE et STICKY
    let add_event = ClientMessageEvent::new(
        32,
        target_win,
        wm_state,
        [1, wm_state_above, wm_state_sticky, 1, 0],
    );
    let _ = conn.send_event(
        false,
        root,
        EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
        add_event,
    );

    // 4. Envoyer ClientMessage EWMH : _NET_WM_STATE_REMOVE (0) pour SKIP_TASKBAR et SKIP_PAGER
    let remove_event = ClientMessageEvent::new(
        32,
        target_win,
        wm_state,
        [0, wm_state_skip_tb, wm_state_skip_pager, 1, 0],
    );
    let _ = conn.send_event(
        false,
        root,
        EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
        remove_event,
    );

    let _ = conn.flush();
}

#[cfg(not(target_os = "linux"))]
pub fn enforce_x11_ewmh_top_and_taskbar(_window_title: &str) {}

/// Initialise les règles KDE/KWin et lance le maintien EWMH.
pub fn apply_kwin_window_hints(window_title: &str) {
    #[cfg(target_os = "linux")]
    {
        ensure_kde_desktop_and_kwin_rules();
        let title = window_title.to_string();
        std::thread::spawn(move || {
            for _ in 0..8 {
                std::thread::sleep(std::time::Duration::from_millis(200));
                enforce_x11_ewmh_top_and_taskbar(&title);
            }
        });
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = window_title;
    }
}
