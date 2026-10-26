//! Progression du voyageur : XP, niveaux, rangs et les trois Voies (Zazen, Éveil, Bushidō).
//! Persistée dans `progress.json` (dossier de données applicatives).

use crate::kde;
use crate::zen_ui::ZenPalette;
use eframe::egui::Color32;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Les trois Voies de progression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XpPath {
    /// Méditation guidée
    Zazen,
    /// Apprentissage (Dojo de questions)
    Eveil,
    /// Discipline (Pomodoros, minuteurs, actions, assiduité)
    Bushido,
}

impl XpPath {
    #[allow(dead_code)]
    pub const ALL: [XpPath; 3] = [XpPath::Zazen, XpPath::Eveil, XpPath::Bushido];

    pub fn label(&self) -> &'static str {
        match self {
            Self::Zazen => "Zazen",
            Self::Eveil => "Éveil",
            Self::Bushido => "Bushidō",
        }
    }

    #[allow(dead_code)]
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Zazen => "[Zazen]",
            Self::Eveil => "[Éveil]",
            Self::Bushido => "[Bushidō]",
        }
    }

    pub fn color(&self) -> Color32 {
        match self {
            Self::Zazen => ZenPalette::JADE_LIGHT,
            Self::Eveil => ZenPalette::GOLD_LEAF,
            Self::Bushido => ZenPalette::SAKURA,
        }
    }

    #[allow(dead_code)]
    pub fn how_to(&self) -> &'static str {
        match self {
            Self::Zazen => "+2 XP par souffle complet, bonus à 3, 5, 10 et 20 min",
            Self::Eveil => "Réponds juste aux questions du Dojo (+10 à +25 XP, bonus de série)",
            Self::Bushido => "Pomodoro (+30), minuteur (+5), action validée (+3), visite quotidienne",
        }
    }
}

/// Position dans un niveau.
#[derive(Debug, Clone, Copy)]
pub struct LevelInfo {
    pub level: u32,
    pub into: u64,
    pub need: u64,
}

impl LevelInfo {
    pub fn ratio(&self) -> f32 {
        self.into as f32 / self.need.max(1) as f32
    }
}

/// Courbe de niveau : le niveau L→L+1 coûte `base + step × (L-1)` XP.
fn level_from_xp(xp: u64, base: u64, step: u64) -> LevelInfo {
    let mut level: u32 = 1;
    let mut rem = xp;
    loop {
        let need = base + step * (level as u64 - 1);
        if rem < need || level >= 999 {
            return LevelInfo {
                level,
                into: rem,
                need,
            };
        }
        rem -= need;
        level += 1;
    }
}

/// Rang (nom, description) associé au niveau global.
pub fn rank_for(level: u32) -> (&'static str, &'static str) {
    match level {
        0..=2 => ("Deshi", "Disciple qui franchit la porte du dojo"),
        3..=4 => ("Kōhai", "Cadet appliqué sur la voie"),
        5..=7 => ("Senpai", "Aîné qui guide les plus jeunes"),
        8..=11 => ("Samouraï", "Guerrier au cœur calme"),
        12..=15 => ("Sensei", "Maître qui transmet la voie"),
        16..=20 => ("Rōshi", "Vieux maître à l'esprit limpide"),
        _ => ("Satori", "Éveil accompli, vent parmi les bambous"),
    }
}

/// Prochain rang : (niveau requis, nom) ou `None` si le rang ultime est atteint.
#[allow(dead_code)]
pub fn next_rank(level: u32) -> Option<(u32, &'static str)> {
    [3u32, 5, 8, 12, 16, 21]
        .iter()
        .find(|&&l| l > level)
        .map(|&l| (l, rank_for(l).0))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionsConfig {
    /// Autoriser automatiquement les lectures d'infos système (sys, top, net, bat)
    pub auto_allow_system_info: bool,
    /// Autoriser automatiquement les commandes multimédia et audio
    pub auto_allow_media_audio: bool,
    /// Autoriser automatiquement la gestion des minuteurs et Pomodoros
    pub auto_allow_timers: bool,
    /// Autoriser automatiquement la consultation des notes et mémoire
    pub auto_allow_read_notes_mem: bool,
    /// Autoriser automatiquement la consultation/liste de fichiers
    pub auto_allow_read_files: bool,
    /// Autoriser automatiquement le lancement d'applications / URLs
    pub auto_allow_apps_url: bool,
    /// Autoriser automatiquement les commandes shell libres (DÉCONSEILLÉ, par défaut false)
    pub auto_allow_shell: bool,
}

impl Default for PermissionsConfig {
    fn default() -> Self {
        Self {
            auto_allow_system_info: true,
            auto_allow_media_audio: true,
            auto_allow_timers: true,
            auto_allow_read_notes_mem: true,
            auto_allow_read_files: false,
            auto_allow_apps_url: false,
            auto_allow_shell: false,
        }
    }
}

impl PermissionsConfig {
    pub fn is_task_auto_approved(&self, task_action: &str) -> bool {
        let act = task_action.trim().to_lowercase();
        match act.as_str() {
            "system_info" | "top_processes" | "network_info" | "battery_info" => self.auto_allow_system_info,
            "audio_control" | "media_control" | "screenshot" => self.auto_allow_media_audio,
            "timer_status" | "set_timer" | "pomodoro" | "cancel_timer" => self.auto_allow_timers,
            "read_notes" | "recall_memory" => self.auto_allow_read_notes_mem,
            "list_files" | "find_files" | "read_file" => self.auto_allow_read_files,
            "open_app" | "open_url" | "kde_action" => self.auto_allow_apps_url,
            "run_shell" => self.auto_allow_shell,
            _ => false,
        }
    }

    pub fn set_zen_balanced(&mut self) {
        *self = Self::default();
    }

    pub fn set_strict_all(&mut self) {
        self.auto_allow_system_info = false;
        self.auto_allow_media_audio = false;
        self.auto_allow_timers = false;
        self.auto_allow_read_notes_mem = false;
        self.auto_allow_read_files = false;
        self.auto_allow_apps_url = false;
        self.auto_allow_shell = false;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Progress {
    pub zazen_xp: u64,
    pub eveil_xp: u64,
    pub bushido_xp: u64,

    pub meditation_secs: u64,
    pub meditation_sessions: u32,
    pub best_session_secs: u64,

    pub quiz_answered: u32,
    pub quiz_correct: u32,
    pub best_streak: u32,

    pub pomodoros_done: u32,
    pub timers_done: u32,
    pub actions_done: u32,

    pub last_visit_day: i64,
    pub visit_streak: u32,

    // Harmonie du Jour (3 petits gestes quotidiens sans pression)
    pub daily_breaths: u32,
    pub daily_quizzes: u32,
    pub daily_actions: u32,
    pub daily_harmony_claimed: bool,

    pub permissions: PermissionsConfig,
}

fn progress_file_path() -> PathBuf {
    kde::app_data_dir().join("progress.json")
}

impl Progress {
    pub fn load() -> Self {
        fs::read_to_string(progress_file_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(progress_file_path(), json);
        }
    }

    pub fn path_xp(&self, path: XpPath) -> u64 {
        match path {
            XpPath::Zazen => self.zazen_xp,
            XpPath::Eveil => self.eveil_xp,
            XpPath::Bushido => self.bushido_xp,
        }
    }

    pub fn add_xp(&mut self, path: XpPath, amount: u64) {
        match path {
            XpPath::Zazen => self.zazen_xp += amount,
            XpPath::Eveil => self.eveil_xp += amount,
            XpPath::Bushido => self.bushido_xp += amount,
        }
    }

    pub fn total_xp(&self) -> u64 {
        self.zazen_xp + self.eveil_xp + self.bushido_xp
    }

    /// Niveau global (somme des trois Voies).
    pub fn global_level(&self) -> LevelInfo {
        level_from_xp(self.total_xp(), 100, 60)
    }

    /// Niveau propre à une Voie.
    pub fn path_level(&self, path: XpPath) -> LevelInfo {
        level_from_xp(self.path_xp(path), 50, 30)
    }

    /// Enregistre la visite du jour. Retourne `(xp, série)` si c'est la première visite du jour.
    pub fn register_daily_visit(&mut self) -> Option<(u64, u32)> {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let today = (secs + 3600) / 86400;
        if self.last_visit_day == today {
            return None;
        }
        if self.last_visit_day == today - 1 {
            self.visit_streak += 1;
        } else {
            self.visit_streak = 1;
        }
        self.last_visit_day = today;
        // Réinitialisation douce du rituel quotidien
        self.daily_breaths = 0;
        self.daily_quizzes = 0;
        self.daily_actions = 0;
        self.daily_harmony_claimed = false;

        let xp = 10 + (self.visit_streak.min(10) as u64 - 1) * 2;
        Some((xp, self.visit_streak))
    }

    /// Indique si les 3 petits gestes de l'Harmonie du Jour sont accomplis et prêts à être célébrés.
    pub fn check_and_claim_daily_harmony(&mut self) -> bool {
        if !self.daily_harmony_claimed
            && self.daily_breaths >= 3
            && self.daily_quizzes >= 1
            && self.daily_actions >= 1
        {
            self.daily_harmony_claimed = true;
            true
        } else {
            false
        }
    }

    /// Résumé injecté dans la conscience temps réel de Kaze.
    pub fn summary_for_ai(&self) -> String {
        let g = self.global_level();
        let (rank, _) = rank_for(g.level);
        format!(
            "Niveau {} (rang {}, {} XP) | Zazen Nv {} ({} min médités) | Éveil Nv {} ({}/{} bonnes réponses) | Bushidō Nv {} ({} Pomodoros, assiduité {} j)",
            g.level,
            rank,
            self.total_xp(),
            self.path_level(XpPath::Zazen).level,
            self.meditation_secs / 60,
            self.path_level(XpPath::Eveil).level,
            self.quiz_correct,
            self.quiz_answered,
            self.path_level(XpPath::Bushido).level,
            self.pomodoros_done,
            self.visit_streak
        )
    }
}
