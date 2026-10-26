use crate::awareness;
use crate::kde;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PetTask {
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub arg: String,
}

impl PetTask {
    /// Indique si cette tâche produit des données d'inspection que le modèle doit analyser/résumer en 2e passe.
    pub fn needs_synthesis(&self) -> bool {
        matches!(
            self.action.trim().to_lowercase().as_str(),
            "read_file"
                | "find_files"
                | "list_files"
                | "run_shell"
                | "top_processes"
                | "clipboard_read"
        )
    }
}

/// Décrit de façon claire et lisible en français l'action exacte qui sera exécutée après confirmation.
pub fn describe_task_human(action: &str, arg: &str) -> String {
    let a = action.trim().to_lowercase();
    let p = arg.trim();
    match a.as_str() {
        "system_info" => "Lire le bilan complet du système (CPU, RAM, Disque, Audio, Uptime)".into(),
        "top_processes" => "Inspecter les applications utilisant le plus de mémoire RAM".into(),
        "network_info" => "Lire l'état de la connexion réseau et l'adresse IP".into(),
        "battery_info" => "Vérifier l'état de la batterie et de l'alimentation".into(),
        "audio_control" => format!("Modifier ou consulter le volume audio ({})", if p.is_empty() { "statut" } else { p }),
        "media_control" => format!("Envoyer la commande multimédia « {} » au lecteur actif", if p.is_empty() { "PlayPause" } else { p }),
        "screenshot" => "Prendre une capture complète de votre écran".into(),
        "kde_action" => format!("Exécuter l'action de bureau « {} »", p),
        "timer_status" => "Consulter l'état détaillé des minuteurs et Pomodoros actifs".into(),
        "set_timer" => format!("Démarrer un nouveau minuteur ({})", p),
        "pomodoro" => format!("Démarrer une session Pomodoro Zen ({} min)", if p.is_empty() { "25" } else { p }),
        "cancel_timer" => format!("Arrêter le(s) minuteur(s) ({})", if p.is_empty() { "tous" } else { p }),
        "clipboard_read" => "Lire le contenu complet de votre presse-papiers".into(),
        "clipboard_write" => format!("Écrire dans votre presse-papiers : « {} »", p),
        "write_note" => format!("Enregistrer une nouvelle note sur le parchemin : « {} »", p),
        "read_notes" => "Lire toutes les notes enregistrées sur le parchemin".into(),
        "clear_notes" => "Effacer définitivement toutes les notes du parchemin".into(),
        "remember_fact" => format!("Mémoriser durablement dans memory.json : « {} »", p),
        "recall_memory" => "Lire tous les souvenirs de la mémoire long-terme".into(),
        "forget_memory" => format!("Supprimer des souvenirs de la mémoire long-terme ({})", if p.is_empty() { "tous" } else { p }),
        "open_app" => format!("Lancer l'application « {} » sur votre ordinateur", p),
        "open_url" => format!("Ouvrir dans votre navigateur web : « {} »", p),
        "list_files" => format!("Lister les fichiers du dossier « {} »", if p.is_empty() { "~" } else { p }),
        "find_files" => format!("Rechercher les fichiers correspondant à « {} » dans votre dossier personnel", p),
        "read_file" => format!("Lire le contenu du fichier « {} »", p),
        "run_shell" => format!("Exécuter la commande système shell : `{}`", p),
        "notify" => format!("Envoyer une notification bureau : « {} »", p),
        other => format!("Exécuter `{}` avec l'argument `{}`", other, p),
    }
}

pub fn execute_task(task: &PetTask) -> Option<String> {
    let action = task.action.trim().to_lowercase();
    let arg = task.arg.trim();

    match action.as_str() {
        "" | "none" => None,
        // 1. Monitoring & Diagnostic Système Multi-Plateforme
        "system_info" => Some(format!("📊 État du système :\n{}", kde::get_system_snapshot())),
        "top_processes" => Some(kde::get_top_processes_detailed()),
        "network_info" => Some(get_network_info()),
        "battery_info" => Some(get_battery_info()),

        // 2. Contrôle Bureau (KDE / macOS / Windows), Audio & Multimédia
        "audio_control" => Some(control_audio(arg)),
        "media_control" => Some(control_media(arg)),
        "screenshot" => Some(take_screenshot(arg)),
        "kde_action" => Some(run_desktop_action(arg)),

        // 3. Minuteurs Conscients, Pomodoro, Notes, Mémoire & Presse-papiers
        "timer_status" => Some(get_timer_status()),
        "set_timer" => Some(start_timer(arg)),
        "pomodoro" => Some(start_pomodoro(arg)),
        "cancel_timer" => Some(cancel_timer(arg)),
        "clipboard_read" => Some(read_clipboard()),
        "clipboard_write" => Some(write_clipboard(arg)),
        "write_note" => Some(write_note(arg)),
        "read_notes" => Some(read_notes()),
        "clear_notes" => Some(clear_notes()),
        "remember_fact" => Some(awareness::remember_fact(arg)),
        "recall_memory" => Some(awareness::recall_memory()),
        "forget_memory" => Some(awareness::forget_memory(arg)),

        // 4. Applications, Web, Fichiers & Commandes
        "open_app" => Some(open_application(arg)),
        "open_url" => Some(open_url(arg)),
        "list_files" => Some(list_directory(arg)),
        "find_files" => Some(find_files(arg)),
        "read_file" => Some(read_file_snippet(arg)),
        "run_shell" => Some(run_safe_shell(arg)),
        "notify" => {
            let msg = if arg.is_empty() {
                "Kaze veille sereinement sur votre espace de travail !"
            } else {
                arg
            };
            match kde::send_kde_notification("🎋 Kaze (AI Pet)", msg) {
                Ok(_) => Some(format!("🔔 Notification envoyée : « {} »", msg)),
                Err(e) => Some(format!("⚠️ Erreur notification : {}", e)),
            }
        }
        _ => None,
    }
}

/// Informations réseau multi-plateforme (Linux nmcli/ip, macOS ipconfig/ifconfig, Windows ipconfig).
fn get_network_info() -> String {
    let mut lines = Vec::new();
    if cfg!(target_os = "linux") {
        if let Ok(out) = Command::new("nmcli")
            .args(["-t", "-f", "NAME,TYPE,DEVICE", "connection", "show", "--active"])
            .output()
        {
            let s = String::from_utf8_lossy(&out.stdout);
            for l in s.lines().take(4) {
                let parts: Vec<&str> = l.split(':').collect();
                if parts.len() >= 3 && parts[0] != "lo" {
                    lines.push(format!(
                        "• Connexion : {} ({} sur {})",
                        parts[0], parts[1], parts[2]
                    ));
                }
            }
        }
        if let Ok(out) = Command::new("hostname").arg("-I").output() {
            let ips = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !ips.is_empty() {
                lines.push(format!("• Adresses IP : {}", ips));
            }
        }
    } else if cfg!(target_os = "macos") {
        if let Ok(out) = Command::new("ipconfig").args(["getifaddr", "en0"]).output() {
            let ip = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !ip.is_empty() {
                lines.push(format!("• IP (en0) : {}", ip));
            }
        }
    } else if cfg!(target_os = "windows") {
        if let Ok(out) = Command::new("ipconfig").output() {
            let s = String::from_utf8_lossy(&out.stdout);
            for l in s.lines().filter(|l| l.contains("IPv4")).take(3) {
                lines.push(format!("• {}", l.trim()));
            }
        }
    }

    if lines.is_empty() {
        "🌐 Aucune information réseau détaillée disponible.".into()
    } else {
        format!("🌐 État du réseau :\n{}", lines.join("\n"))
    }
}

/// État de la batterie multi-plateforme (Linux /sys/class/power_supply, macOS pmset, Windows WMIC).
fn get_battery_info() -> String {
    if cfg!(target_os = "linux") {
        if let Ok(entries) = fs::read_dir("/sys/class/power_supply") {
            let mut info = Vec::new();
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                let cap = fs::read_to_string(path.join("capacity"))
                    .ok()
                    .map(|s| s.trim().to_string());
                let status = fs::read_to_string(path.join("status"))
                    .ok()
                    .map(|s| s.trim().to_string());
                if let Some(c) = cap {
                    let st = status.unwrap_or_else(|| "Inconnu".into());
                    info.push(format!("• {} : {}% ({})", name, c, st));
                }
            }
            if !info.is_empty() {
                return format!("🔋 Batterie & Alimentation :\n{}", info.join("\n"));
            }
        }
    } else if cfg!(target_os = "macos") {
        if let Ok(out) = Command::new("pmset").args(["-g", "batt"]).output() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                return format!("🔋 Alimentation macOS :\n{}", s);
            }
        }
    } else if cfg!(target_os = "windows") {
        if let Ok(out) = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "(Get-CimInstance Win32_Battery).EstimatedChargeRemaining",
            ])
            .output()
        {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                return format!("🔋 Batterie Windows : {}%", s);
            }
        }
    }
    "🔌 Alimentation sur secteur (aucune batterie détectée).".into()
}

/// Contrôle audio multi-plateforme (Linux wpctl, macOS osascript, Windows PowerShell).
fn control_audio(arg: &str) -> String {
    let a = arg.trim().to_lowercase();
    if a.is_empty() || a == "status" {
        return format!("🔊 Volume actuel : {}", kde::get_audio_volume_summary());
    }

    if cfg!(target_os = "linux") {
        if a == "mute" || a == "muet" || a == "toggle" {
            let _ = Command::new("wpctl")
                .args(["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"])
                .status();
            return format!("🔊 Audio basculé : {}", kde::get_audio_volume_summary());
        }
        if a == "unmute" {
            let _ = Command::new("wpctl")
                .args(["set-mute", "@DEFAULT_AUDIO_SINK@", "0"])
                .status();
            return format!("🔊 Son réactivé : {}", kde::get_audio_volume_summary());
        }
        let cleaned = a.replace('%', "");
        let wp_arg = if let Some(val) = cleaned.strip_prefix('+') {
            let n: u32 = val.parse().unwrap_or(10).clamp(1, 50);
            format!("{}%+", n)
        } else if let Some(val) = cleaned.strip_prefix('-') {
            let n: u32 = val.parse().unwrap_or(10).clamp(1, 50);
            format!("{}%-", n)
        } else if let Ok(n) = cleaned.parse::<u32>() {
            format!("{}%", n.clamp(0, 120))
        } else {
            return format!("🔊 Volume actuel : {}", kde::get_audio_volume_summary());
        };

        let _ = Command::new("wpctl")
            .args(["set-mute", "@DEFAULT_AUDIO_SINK@", "0"])
            .status();
        let _ = Command::new("wpctl")
            .args(["set-volume", "-l", "1.2", "@DEFAULT_AUDIO_SINK@", &wp_arg])
            .status();
        return format!("🔊 Volume ajusté : {}", kde::get_audio_volume_summary());
    } else if cfg!(target_os = "macos") {
        let cleaned = a.replace('%', "");
        if a == "mute" || a == "muet" || a == "toggle" {
            let _ = Command::new("osascript")
                .args(["-e", "set volume output muted not (output muted of (get volume settings))"])
                .status();
            return format!("🔊 Audio macOS basculé : {}", kde::get_audio_volume_summary());
        } else if a == "unmute" {
            let _ = Command::new("osascript")
                .args(["-e", "set volume without output muted"])
                .status();
            return format!("🔊 Son macOS réactivé : {}", kde::get_audio_volume_summary());
        } else if let Some(val) = cleaned.strip_prefix('+') {
            let delta: i32 = val.parse().unwrap_or(10).clamp(1, 50);
            let script = format!(
                "set volume output volume ((output volume of (get volume settings)) + {})",
                delta
            );
            let _ = Command::new("osascript").args(["-e", &script]).status();
            return format!("🔊 Volume macOS : {}", kde::get_audio_volume_summary());
        } else if let Some(val) = cleaned.strip_prefix('-') {
            let delta: i32 = val.parse().unwrap_or(10).clamp(1, 50);
            let script = format!(
                "set volume output volume ((output volume of (get volume settings)) - {})",
                delta
            );
            let _ = Command::new("osascript").args(["-e", &script]).status();
            return format!("🔊 Volume macOS : {}", kde::get_audio_volume_summary());
        } else if let Ok(n) = cleaned.parse::<u32>() {
            let _ = Command::new("osascript")
                .args(["-e", &format!("set volume output volume {}", n.min(100))])
                .status();
            return format!("🔊 Volume macOS réglé à {}%.", n.min(100));
        }
    } else if cfg!(target_os = "windows") {
        let cleaned = a.replace('%', "");
        let ps_cmd = if a == "mute" || a == "muet" || a == "toggle" {
            "(New-Object -ComObject WScript.Shell).SendKeys([char]173)".to_string()
        } else if cleaned.starts_with('-') {
            "1..5 | ForEach-Object { (New-Object -ComObject WScript.Shell).SendKeys([char]174) }"
                .to_string()
        } else {
            "1..5 | ForEach-Object { (New-Object -ComObject WScript.Shell).SendKeys([char]175) }"
                .to_string()
        };
        let _ = Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps_cmd])
            .status();
        return "🔊 Volume Windows ajusté.".into();
    }
    format!("🔊 Volume : {}", kde::get_audio_volume_summary())
}

/// Contrôle multimédia multi-plateforme (Linux D-Bus MPRIS2, macOS Music/Spotify osascript, Windows Media Keys).
fn control_media(arg: &str) -> String {
    let a = arg.trim().to_lowercase();
    let method = match a.as_str() {
        "play" | "lecture" => "Play",
        "pause" | "stop" => "Pause",
        "next" | "suivant" => "Next",
        "prev" | "previous" | "precedent" | "précédent" => "Previous",
        _ => "PlayPause",
    };

    if cfg!(target_os = "linux") {
        if let Ok(out) = Command::new("dbus-send")
            .args([
                "--session",
                "--dest=org.freedesktop.DBus",
                "--type=method_call",
                "--print-reply",
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus.ListNames",
            ])
            .output()
        {
            let s = String::from_utf8_lossy(&out.stdout);
            if let Some(player) = s
                .lines()
                .filter_map(|l| l.split('"').nth(1))
                .find(|name| name.starts_with("org.mpris.MediaPlayer2."))
            {
                let status = Command::new("dbus-send")
                    .args([
                        "--session",
                        "--type=method_call",
                        &format!("--dest={}", player),
                        "/org/mpris/MediaPlayer2",
                        &format!("org.mpris.MediaPlayer2.Player.{}", method),
                    ])
                    .status();
                if status.map(|st| st.success()).unwrap_or(false) {
                    let short_player = player.trim_start_matches("org.mpris.MediaPlayer2.");
                    return format!("🎵 Commande « {} » envoyée à {} !", method, short_player);
                }
            }
        }
        return "🎵 Aucun lecteur multimédia actif (MPRIS2) détecté.".into();
    } else if cfg!(target_os = "macos") {
        let osa_cmd = match method {
            "Next" => "next track",
            "Previous" => "previous track",
            _ => "playpause",
        };
        let script = format!(
            "if application \"Spotify\" is running then\n\
                tell application \"Spotify\" to {}\n\
             else\n\
                tell application \"Music\" to {}\n\
             end if",
            osa_cmd, osa_cmd
        );
        let _ = Command::new("osascript").args(["-e", &script]).status();
        return format!("🎵 Commande média ({}) envoyée sur macOS.", method);
    } else if cfg!(target_os = "windows") {
        let vk = match method {
            "Next" => 176,
            "Previous" => 177,
            _ => 179,
        };
        let ps = format!(
            "(New-Object -ComObject WScript.Shell).SendKeys([char]{})",
            vk
        );
        let _ = Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps])
            .status();
        return format!("🎵 Commande média ({}) envoyée sur Windows.", method);
    }
    "🎵 Contrôle média exécuté.".into()
}

/// Capture d'écran multi-plateforme (Linux KDE Spectacle, macOS screencapture, Windows PowerShell).
fn take_screenshot(_arg: &str) -> String {
    let out_dir = kde::app_data_dir().join("captures");
    let _ = fs::create_dir_all(&out_dir);
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let file_path = out_dir.join(format!("kaze_{}.png", secs));
    let path_str = file_path.to_string_lossy().to_string();

    let res = if cfg!(target_os = "linux") {
        Command::new("spectacle")
            .args(["-b", "-n", "-o", &path_str])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    } else if cfg!(target_os = "macos") {
        Command::new("screencapture")
            .args(["-x", &path_str])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    } else {
        let ps = format!(
            "Add-Type -AssemblyName System.Windows.Forms,System.Drawing; \
             $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds; \
             $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height; \
             $g = [System.Drawing.Graphics]::FromImage($bmp); \
             $g.CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size); \
             $bmp.Save({:?})",
            path_str
        );
        Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    };

    match res {
        Ok(_) => {
            let _ = kde::send_kde_notification(
                "📸 Capture Kaze",
                &format!("Enregistrée dans {}", file_path.display()),
            );
            format!("📸 Capture d'écran effectuée :\n{}", file_path.display())
        }
        Err(e) => format!("⚠️ Impossible de capturer l'écran : {}", e),
    }
}

/// Actions de bureau (Linux KDE, macOS, Windows).
fn run_desktop_action(arg: &str) -> String {
    let a = arg.trim().to_lowercase();
    match a.as_str() {
        "lock" | "verrouiller" => {
            if cfg!(target_os = "linux") {
                let _ = Command::new("loginctl").arg("lock-session").spawn();
            } else if cfg!(target_os = "macos") {
                let _ = Command::new("pmset").arg("displaysleepnow").spawn();
            } else if cfg!(target_os = "windows") {
                let _ = Command::new("rundll32.exe")
                    .args(["user32.dll,LockWorkStation"])
                    .spawn();
            }
            "🔒 Session verrouillée.".into()
        }
        "krunner" | "recherche" | "spotlight" | "launcher" => {
            if cfg!(target_os = "linux") {
                let _ = Command::new("krunner")
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn();
                "🔍 KRunner ouvert sur Linux / KDE Plasma.".into()
            } else if cfg!(target_os = "macos") {
                let _ = Command::new("osascript")
                    .args([
                        "-e",
                        "tell application \"System Events\" to keystroke space using command down",
                    ])
                    .spawn();
                "🔍 Spotlight ouvert sur macOS.".into()
            } else if cfg!(target_os = "windows") {
                let _ = Command::new("explorer.exe")
                    .arg("shell:::{2559a1f8-21d7-11d4-bdaf-00c04f60b9f0}")
                    .spawn();
                "🔍 Recherche Windows ouverte.".into()
            } else {
                open_application("fichiers")
            }
        }
        "settings" | "parametres" | "paramètres" => open_application("parametres"),
        "monitor" | "moniteur" => open_application("moniteur"),
        _ => open_application(arg),
    }
}

fn read_clipboard() -> String {
    match kde::read_clipboard_text() {
        Some(text) => {
            let preview: String = text.chars().take(320).collect();
            format!("📋 Contenu du presse-papiers :\n« {} »", preview)
        }
        None => "📋 Le presse-papiers est actuellement vide.".into(),
    }
}

pub fn write_clipboard(text: &str) -> String {
    if text.is_empty() {
        return "⚠️ Aucun texte à copier.".into();
    }
    match kde::write_clipboard_text(text) {
        Ok(_) => format!(
            "📋 Copié dans le presse-papiers ({} caractères) !",
            text.chars().count()
        ),
        Err(e) => format!("⚠️ Erreur presse-papiers : {}", e),
    }
}

/// Lancement d'application multi-plateforme (Linux KDE, macOS `open -a`, Windows `cmd /C start`).
fn open_application(raw_app: &str) -> String {
    if raw_app.is_empty() {
        return "⚠️ Aucune application spécifiée.".into();
    }
    let lower = raw_app.to_lowercase();

    if cfg!(target_os = "macos") {
        let app = match lower.as_str() {
            "fichiers" | "explorateur" | "dolphin" | "finder" => "Finder",
            "terminal" | "console" | "konsole" => "Terminal",
            "editeur" | "éditeur" | "kate" | "notepad" => "TextEdit",
            "parametres" | "paramètres" | "systemsettings" => "System Settings",
            "calculatrice" | "kcalc" => "Calculator",
            "moniteur" | "plasma-systemmonitor" => "Activity Monitor",
            other => other,
        };
        return match Command::new("open").args(["-a", app]).spawn() {
            Ok(_) => format!("🚀 Application macOS lancée : {}", app),
            Err(e) => format!("⚠️ Impossible d'ouvrir {} : {}", app, e),
        };
    } else if cfg!(target_os = "windows") {
        let app = match lower.as_str() {
            "fichiers" | "explorateur" | "dolphin" => "explorer.exe",
            "terminal" | "console" | "konsole" => "powershell.exe",
            "editeur" | "éditeur" | "kate" => "notepad.exe",
            "parametres" | "paramètres" | "systemsettings" => "ms-settings:",
            "calculatrice" | "kcalc" => "calc.exe",
            "moniteur" | "plasma-systemmonitor" => "taskmgr.exe",
            other => other,
        };
        return match Command::new("cmd").args(["/C", "start", "", app]).spawn() {
            Ok(_) => format!("🚀 Application Windows lancée : {}", app),
            Err(e) => format!("⚠️ Impossible d'ouvrir {} : {}", app, e),
        };
    }

    // Linux / KDE Plasma
    let mapped = match lower.as_str() {
        "fichiers" | "explorateur" | "dossier" | "dossiers" | "file manager" => "dolphin",
        "terminal" | "console" | "shell" => "konsole",
        "editeur" | "éditeur" | "texte" | "bloc-notes" => "kate",
        "parametres" | "paramètres" | "settings" | "configuration" => "systemsettings",
        "calculatrice" | "calc" => "kcalc",
        "capture" | "screenshot" => "spectacle",
        "moniteur" | "taches" | "tâches" | "system monitor" => "plasma-systemmonitor",
        other => other,
    };

    let parts: Vec<&str> = mapped.split_whitespace().collect();
    let bin = parts[0];
    let args = &parts[1..];

    match Command::new(bin)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(_) => format!("🚀 Application lancée : {}", bin),
        Err(_) => {
            if Command::new("kstart")
                .arg(bin)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .is_ok()
            {
                format!("🚀 Application lancée via KDE kstart : {}", bin)
            } else {
                format!("⚠️ Impossible de trouver l'application « {} ».", bin)
            }
        }
    }
}

/// Ouvre une URL ou recherche web sur Linux (`xdg-open`), macOS (`open`) et Windows (`cmd /C start`).
fn open_url(raw_url: &str) -> String {
    if raw_url.is_empty() {
        return "⚠️ Aucune URL fournie.".into();
    }
    let url = if raw_url.starts_with("http://") || raw_url.starts_with("https://") {
        raw_url.to_string()
    } else if raw_url.contains('.') && !raw_url.contains(' ') {
        format!("https://{}", raw_url)
    } else {
        format!("https://duckduckgo.com/?q={}", raw_url.replace(' ', "+"))
    };

    let res = if cfg!(target_os = "macos") {
        Command::new("open")
            .arg(&url)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    } else if cfg!(target_os = "windows") {
        Command::new("cmd")
            .args(["/C", "start", "", &url])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    } else {
        Command::new("xdg-open")
            .arg(&url)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    };

    match res {
        Ok(_) => format!("🌐 Ouvert dans le navigateur : {}", url),
        Err(e) => format!("⚠️ Erreur d'ouverture navigateur : {}", e),
    }
}

fn run_safe_shell(cmd: &str) -> String {
    if cmd.is_empty() {
        return "⚠️ Commande vide.".into();
    }
    let forbidden = [
        "rm -rf",
        "del /f /s /q",
        "format ",
        "mkfs",
        "dd if=",
        ":(){",
        "shutdown",
        "reboot",
        "poweroff",
        "chmod -R 777 /",
        "> /dev/sd",
        "sudo ",
        "su -",
        "pkill -9",
    ];
    let lower = cmd.to_lowercase();
    for bad in forbidden {
        if lower.contains(bad) {
            return format!("🛑 Commande bloquée par sécurité : contient « {} ».", bad);
        }
    }

    let output = if cfg!(target_os = "windows") {
        Command::new("powershell")
            .args(["-NoProfile", "-Command", cmd])
            .output()
    } else {
        Command::new("sh").arg("-c").arg(cmd).output()
    };

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            let combined = if !stdout.is_empty() && !stderr.is_empty() {
                format!("{}\n{}", stdout, stderr)
            } else if !stdout.is_empty() {
                stdout
            } else if !stderr.is_empty() {
                stderr
            } else {
                "(Commande exécutée sans sortie)".into()
            };

            let lines: Vec<&str> = combined.lines().take(14).collect();
            let mut result = lines.join("\n");
            if combined.lines().count() > 14 {
                result.push_str("\n… (sortie tronquée)");
            }
            format!("💻 $ {}\n{}", cmd, result)
        }
        Err(e) => format!("⚠️ Erreur d'exécution : {}", e),
    }
}

fn expand_tilde(path: &str) -> PathBuf {
    let home = kde::user_home_dir();
    if path.is_empty() || path == "~" {
        home
    } else if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        home.join(rest)
    } else {
        PathBuf::from(path)
    }
}

fn list_directory(path_str: &str) -> String {
    let target = expand_tilde(path_str);
    match fs::read_dir(&target) {
        Ok(entries) => {
            let mut names: Vec<String> = entries
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    if name.starts_with('.') {
                        return None;
                    }
                    let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    Some(if is_dir {
                        format!("📁 {}/", name)
                    } else {
                        format!("📄 {}", name)
                    })
                })
                .collect();
            names.sort();
            let total = names.len();
            let preview: Vec<String> = names.into_iter().take(15).collect();
            if preview.is_empty() {
                format!("📂 {} est vide.", target.display())
            } else {
                let suffix = if total > 15 {
                    format!("\n… (+{} autres)", total - 15)
                } else {
                    String::new()
                };
                format!(
                    "📂 {} ({}) :\n{}{}",
                    target.display(),
                    total,
                    preview.join("\n"),
                    suffix
                )
            }
        }
        Err(e) => format!("⚠️ Impossible de lire {} : {}", target.display(), e),
    }
}

/// Recherche récursive multi-plateforme en pur Rust (fonctionne sur Linux, macOS et Windows sans `find`).
fn find_files(query: &str) -> String {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return "⚠️ Précisez un nom de fichier à chercher.".into();
    }
    let root = kde::user_home_dir();
    let mut results = Vec::new();
    walk_find(&root, &q, 0, 4, &mut results);

    if results.is_empty() {
        format!("🔎 Aucun fichier trouvé pour « {} ».", query)
    } else {
        format!(
            "🔎 Fichiers trouvés ({}) :\n{}",
            results.len(),
            results.join("\n")
        )
    }
}

fn walk_find(dir: &Path, query_lower: &str, depth: usize, max_depth: usize, out: &mut Vec<String>) {
    if depth > max_depth || out.len() >= 10 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if out.len() >= 10 {
            break;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.')
            || name == "node_modules"
            || name == "target"
            || name == "AppData"
            || name == "Library"
        {
            continue;
        }
        let path = entry.path();
        if name.to_lowercase().contains(query_lower) {
            out.push(path.display().to_string());
        }
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            walk_find(&path, query_lower, depth + 1, max_depth, out);
        }
    }
}

fn read_file_snippet(path_str: &str) -> String {
    if path_str.is_empty() {
        return "⚠️ Aucun chemin de fichier indiqué.".into();
    }
    let path = expand_tilde(path_str);
    match fs::read_to_string(&path) {
        Ok(content) => {
            let lines: Vec<&str> = content.lines().take(12).collect();
            let suffix = if content.lines().count() > 12 {
                "\n… (tronqué)"
            } else {
                ""
            };
            format!("📖 {} :\n{}{}", path.display(), lines.join("\n"), suffix)
        }
        Err(e) => format!("⚠️ Impossible de lire {} : {}", path.display(), e),
    }
}

fn notes_file_path() -> PathBuf {
    kde::app_data_dir().join("notes.txt")
}

fn write_note(note: &str) -> String {
    if note.is_empty() {
        return "⚠️ Note vide.".into();
    }
    let path = notes_file_path();
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mins = (secs / 60) % 60;
    let hours = ((secs / 3600) + 1) % 24;

    match OpenOptions::new().create(true).append(true).open(&path) {
        Ok(mut f) => {
            let _ = writeln!(f, "[{:02}:{:02}] {}", hours, mins, note);
            format!("📝 Note gravée sur le parchemin : « {} »", note)
        }
        Err(e) => format!("⚠️ Erreur écriture note : {}", e),
    }
}

pub fn read_notes() -> String {
    let path = notes_file_path();
    match fs::read_to_string(&path) {
        Ok(content) if !content.trim().is_empty() => {
            format!("📒 Parchemin de notes :\n{}", content.trim())
        }
        _ => "📒 Votre parchemin de notes est vierge pour l'instant.".into(),
    }
}

pub fn peek_recent_notes(count: usize) -> String {
    let path = notes_file_path();
    match fs::read_to_string(&path) {
        Ok(content) if !content.trim().is_empty() => {
            let lines: Vec<&str> = content.lines().rev().take(count).collect();
            lines.into_iter().rev().collect::<Vec<_>>().join(" | ")
        }
        _ => "Aucune note".into(),
    }
}

pub fn clear_notes() -> String {
    let path = notes_file_path();
    let _ = fs::write(path, "");
    "🧹 Parchemin de notes effacé avec sérénité.".into()
}

/// État en direct de tous les minuteurs et Pomodoros.
pub fn get_timer_status() -> String {
    if let Ok(reg) = awareness::global_timers().lock() {
        return format!("⏳ {}", reg.summary_for_ai());
    }
    "⚠️ Impossible de lire les minuteurs.".into()
}

/// Annule un ou tous les minuteurs / Pomodoros actifs.
pub fn cancel_timer(arg: &str) -> String {
    if let Ok(mut reg) = awareness::global_timers().lock() {
        let count = reg.cancel(arg);
        if count > 0 {
            return format!("🛑 {} minuteur(s) / Pomodoro(s) arrêté(s).", count);
        }
    }
    "ℹ️ Aucun minuteur actif correspondant à annuler.".into()
}

/// Démarre un minuteur conscient enregistré dans `global_timers()`.
fn start_timer(arg: &str) -> String {
    let mut parts = arg.splitn(2, char::is_whitespace);
    let first = parts.next().unwrap_or("30");
    let rest = parts.next().unwrap_or("").trim();

    // Support de "5m", "2min", "45s", "45"
    let lower_first = first.to_lowercase();
    let digits: u64 = lower_first
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse()
        .unwrap_or(30)
        .clamp(1, 7200);

    let secs = if lower_first.contains('m') && !lower_first.contains("ms") {
        digits * 60
    } else {
        digits
    };

    let label = if rest.is_empty() {
        format!("Sablier {}s", secs)
    } else {
        rest.to_string()
    };

    if let Ok(mut reg) = awareness::global_timers().lock() {
        let timer = reg.add_timer(secs, label.clone(), false);
        return format!(
            "⏱️ Minuteur #{} lancé pour {}m {:02}s (« {} »).\nKaze suit son décompte en temps réel et vous préviendra !",
            timer.id,
            secs / 60,
            secs % 60,
            label
        );
    }
    "⚠️ Erreur lors de la création du minuteur.".into()
}

/// Démarre un cycle Pomodoro Zen conscient enregistré dans `global_timers()`.
pub fn start_pomodoro(arg: &str) -> String {
    let mins: u64 = arg
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse()
        .unwrap_or(25)
        .clamp(1, 120);

    let secs = mins * 60;
    let label = format!("Pomodoro Zen ({} min)", mins);

    if let Ok(mut reg) = awareness::global_timers().lock() {
        let timer = reg.add_timer(secs, label, true);
        return format!(
            "🍵 Session Pomodoro Zen #{} démarrée pour {} minutes.\nRespirez calmement, Kaze affiche et surveille votre progression.",
            timer.id, mins
        );
    }
    "⚠️ Erreur lors du démarrage du Pomodoro.".into()
}

/// Convertit un timestamp UNIX (secondes) en `(année, mois, jour, heure, minute, seconde)` UTC+1.
fn civil_datetime_now() -> (i32, u32, u32, u32, u32, u32) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
        + 3600; // Fuseau local approximatif (UTC+1)

    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400) as u32;
    let hour = rem / 3600;
    let min = (rem % 3600) / 60;
    let sec = rem % 60;

    // Algorithme de Howard Hinnant (civil_from_days)
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64 + era * 400) as i32;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };

    (year, m, d, hour, min, sec)
}

/// Exporte la réponse actuelle de Kaze (et l'historique récent de la session) dans un fichier externe `.md`.
pub fn export_response_to_markdown(
    custom_arg: &str,
    response_text: &str,
    model: &str,
    mood_label: &str,
    history: &[crate::ollama::ChatMessage],
) -> Result<PathBuf, String> {
    let (year, month, day, hour, min, sec) = civil_datetime_now();
    let date_human = format!(
        "{:04}-{:02}-{:02} à {:02}:{:02}:{:02}",
        year, month, day, hour, min, sec
    );
    let date_slug = format!(
        "{:04}-{:02}-{:02}_{:02}h{:02}m{:02}s",
        year, month, day, hour, min, sec
    );

    let trimmed_arg = custom_arg.trim();
    let target_path = if !trimmed_arg.is_empty() {
        let mut p = expand_tilde(trimmed_arg);
        if p.extension().and_then(|e| e.to_str()) != Some("md") {
            let fname = p
                .file_name()
                .map(|f| format!("{}.md", f.to_string_lossy()))
                .unwrap_or_else(|| format!("kaze_{}.md", date_slug));
            p.set_file_name(fname);
        }
        if p.parent().map(|par| par.as_os_str().is_empty()).unwrap_or(true) {
            default_exports_dir().join(p)
        } else {
            if let Some(par) = p.parent() {
                let _ = fs::create_dir_all(par);
            }
            p
        }
    } else {
        default_exports_dir().join(format!("kaze_reponse_{}.md", date_slug))
    };

    let mut md = String::new();
    md.push_str("# 🎋 Parchemin de Kaze (風) — Export Zen\n\n");
    md.push_str(&format!("- **Date** : {}\n", date_human));
    md.push_str(&format!("- **Esprit IA (Ollama)** : `{}`\n", model));
    md.push_str(&format!("- **Posture du Samouraï** : {}\n\n", mood_label));
    md.push_str("---\n\n");
    md.push_str("## 💬 Réponse de Kaze\n\n");
    md.push_str(response_text.trim());
    md.push_str("\n\n");

    if !history.is_empty() {
        md.push_str("---\n\n");
        md.push_str("## 📜 Journal de la session\n\n");
        for msg in history {
            let speaker = if msg.role == "user" {
                "🧑‍💻 **Voyageur**"
            } else {
                "🎋 **Kaze (風)**"
            };
            md.push_str(&format!("### {}\n\n{}\n\n", speaker, msg.content.trim()));
        }
    }

    fs::write(&target_path, md)
        .map_err(|e| format!("Impossible d'écrire {} : {}", target_path.display(), e))?;

    let _ = kde::send_kde_notification(
        "📝 Parchemin Markdown exporté",
        &format!("Enregistré dans {}", target_path.display()),
    );

    Ok(target_path)
}

fn default_exports_dir() -> PathBuf {
    let home = kde::user_home_dir();
    let docs = home.join("Documents");
    let base = if docs.is_dir() {
        docs.join("Kaze_Exports")
    } else {
        home.join("Kaze_Exports")
    };
    let _ = fs::create_dir_all(&base);
    base
}

/// Ouvre un fichier externe (ex: le fichier `.md` exporté) avec l'application par défaut du système.
pub fn open_file_external(path: &Path) {
    let p_str = path.to_string_lossy().to_string();
    if cfg!(target_os = "macos") {
        let _ = Command::new("open")
            .arg(&p_str)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    } else if cfg!(target_os = "windows") {
        let _ = Command::new("cmd")
            .args(["/C", "start", "", &p_str])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    } else {
        let _ = Command::new("xdg-open")
            .arg(&p_str)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
}
