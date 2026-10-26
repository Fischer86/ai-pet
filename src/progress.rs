//! Progression du voyageur : XP, niveaux, rangs, titres d'honneur (Shōgō) et les trois Voies (Zazen, Éveil, Bushidō).
//! Persistée de manière atomique et fiable dans `progress.json` (dossier de données applicatives).

use crate::kde;
use crate::zen_ui::ZenPalette;
use eframe::egui::Color32;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Les trois Voies de progression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum XpPath {
    /// Méditation guidée & maîtrise du souffle
    Zazen,
    /// Apprentissage & résolution d'énigmes du Sensei
    Eveil,
    /// Discipline, Pomodoros, minuteurs, actions & assiduité
    Bushido,
}

impl XpPath {
    pub const ALL: [XpPath; 3] = [XpPath::Zazen, XpPath::Eveil, XpPath::Bushido];

    pub fn label(&self) -> &'static str {
        match self {
            Self::Zazen => "Zazen",
            Self::Eveil => "Éveil",
            Self::Bushido => "Bushidō",
        }
    }

    pub fn subtitle(&self) -> &'static str {
        match self {
            Self::Zazen => "Voie du Souffle & de la Sérénité",
            Self::Eveil => "Voie de la Connaissance & de l'Esprit",
            Self::Bushido => "Voie de l'Action & de la Discipline",
        }
    }

    pub fn color(&self) -> Color32 {
        match self {
            Self::Zazen => ZenPalette::JADE_LIGHT,
            Self::Eveil => ZenPalette::GOLD_LEAF,
            Self::Bushido => ZenPalette::SAKURA,
        }
    }

    pub fn how_to(&self) -> &'static str {
        match self {
            Self::Zazen => "+2 XP par cycle de souffle complet, bonus de session à 2, 5, 10 et 20 min",
            Self::Eveil => "+15 à +25 XP par énigme résolue au Dojo (bonus de série), +3 XP par essai",
            Self::Bushido => "+30 XP par Pomodoro, +6 XP par sablier, +4 XP par action, bonus quotidien",
        }
    }

    /// Titre de maîtrise propre au niveau atteint dans cette Voie.
    pub fn mastery_title(&self, path_level: u32) -> &'static str {
        match self {
            Self::Zazen => match path_level {
                0..=1 => "Souffle Naissant",
                2..=3 => "Respire-Nuages",
                4..=6 => "Cœur d'Eau Calme",
                7..=10 => "Gardien du Silence",
                _ => "Maître de l'Ensō",
            },
            Self::Eveil => match path_level {
                0..=1 => "Lecteur d'Estampes",
                2..=3 => "Chercheur de Vérité",
                4..=6 => "Érudit du Rouleau",
                7..=10 => "Oracle du Tokonoma",
                _ => "Esprit Omniscient",
            },
            Self::Bushido => match path_level {
                0..=1 => "Porteur de Bokken",
                2..=3 => "Lame Disciplinée",
                4..=6 => "Gardien du Sablier",
                7..=10 => "Samouraï Implacable",
                _ => "Kensei Invaincu",
            },
        }
    }
}

/// Position détaillée dans un niveau.
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

    pub fn percent(&self) -> u32 {
        (self.ratio() * 100.0).round() as u32
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

/// Paliers de Rangs Globaux (niveau requis, titre japonais/français, description poétique).
pub const GLOBAL_RANKS: &[(u32, &str, &str)] = &[
    (1, "Shoshinsha • Novice du Seuil", "Celui qui franchit la porte du Dojo avec l'esprit du débutant"),
    (2, "Deshi • Disciple du Vent", "Apprenti attentif au murmure du souffle et à l'étude"),
    (3, "Kōhai • Initié du Tatami", "Cadet appliqué qui s'exerce sur les trois Voies"),
    (5, "Senpai • Marcheur des Bambous", "Aîné guidé par la constance et la clarté"),
    (7, "Bushi • Lame Sereine", "Guerrier dont le sabre reflète la paix intérieure"),
    (10, "Samouraï • Cœur Immobile", "Maître de soi face au tumulte du monde"),
    (13, "Sensei • Maître du Pavillon", "Érudit qui éclaire la Voie et transmet la sagesse"),
    (16, "Rōshi • Sage des Nuages", "Vieux maître à l'esprit limpide comme un lac d'automne"),
    (20, "Kensei • Saint du Sabre", "Harmonie souveraine entre la pensée, le souffle et l'acte"),
    (25, "Satori • Éveillé Céleste", "Éveil accompli, vent libre parmi les dix mille choses"),
];

/// Rang (nom, description) associé au niveau global.
pub fn rank_for(level: u32) -> (&'static str, &'static str) {
    let mut current = (GLOBAL_RANKS[0].1, GLOBAL_RANKS[0].2);
    for &(req_lvl, title, desc) in GLOBAL_RANKS {
        if level >= req_lvl {
            current = (title, desc);
        } else {
            break;
        }
    }
    current
}

/// Nom court du rang (pour les petits badges).
pub fn short_rank_for(level: u32) -> &'static str {
    let (full, _) = rank_for(level);
    full.split(" • ").next().unwrap_or(full)
}

/// Prochain rang : (niveau requis, nom complet) ou `None` si le rang ultime est atteint.
pub fn next_rank(level: u32) -> Option<(u32, &'static str)> {
    GLOBAL_RANKS
        .iter()
        .find(|&&(req_lvl, _, _)| req_lvl > level)
        .map(|&(req_lvl, title, _)| (req_lvl, title))
}

/// Définition d'un Titre d'Honneur (Shōgō) déblocable et équipable par le joueur.
#[derive(Debug, Clone, Copy)]
pub struct HonorTitle {
    pub id: &'static str,
    pub name: &'static str,
    pub condition_desc: &'static str,
    pub category: XpPath,
}

pub const HONOR_TITLES: &[HonorTitle] = &[
    HonorTitle {
        id: "shoshin",
        name: "Esprit du Débutant",
        condition_desc: "Entrer dans le Dojo pour la première fois (Niveau 1)",
        category: XpPath::Eveil,
    },
    HonorTitle {
        id: "jade_breath",
        name: "Souffle de Jade",
        condition_desc: "Atteindre le Niveau 2 en Zazen ou méditer 5 minutes",
        category: XpPath::Zazen,
    },
    HonorTitle {
        id: "truth_seeker",
        name: "Chercheur de Vérité",
        condition_desc: "Résoudre 5 énigmes du Sensei dans la Voie de l'Éveil",
        category: XpPath::Eveil,
    },
    HonorTitle {
        id: "disciplined_blade",
        name: "Lame Disciplinée",
        condition_desc: "Accomplir 5 actions, sabliers ou Pomodoros (Bushidō)",
        category: XpPath::Bushido,
    },
    HonorTitle {
        id: "still_water",
        name: "Cœur d'Eau Calme",
        condition_desc: "Atteindre le Niveau 4 en Zazen ou méditer 20 minutes",
        category: XpPath::Zazen,
    },
    HonorTitle {
        id: "scroll_scholar",
        name: "Érudit du Rouleau",
        condition_desc: "Obtenir une série de 5 bonnes réponses consécutives au Dojo",
        category: XpPath::Eveil,
    },
    HonorTitle {
        id: "hourglass_keeper",
        name: "Gardien du Sablier",
        condition_desc: "Terminer 3 sessions Pomodoro Zen complètes",
        category: XpPath::Bushido,
    },
    HonorTitle {
        id: "three_paths",
        name: "Harmonie des Trois Voies",
        condition_desc: "Atteindre au moins le Niveau 3 dans chacune des 3 Voies",
        category: XpPath::Eveil,
    },
    HonorTitle {
        id: "tokonoma_oracle",
        name: "Oracle du Tokonoma",
        condition_desc: "Résoudre 20 énigmes justes auprès du Sensei",
        category: XpPath::Eveil,
    },
    HonorTitle {
        id: "silence_master",
        name: "Maître du Silence",
        condition_desc: "Atteindre le Niveau 7 en Zazen ou méditer 60 minutes",
        category: XpPath::Zazen,
    },
    HonorTitle {
        id: "unwavering_samurai",
        name: "Samouraï Inébranlable",
        condition_desc: "Maintenir une assiduité de 5 jours ou Bushidō Niveau 6",
        category: XpPath::Bushido,
    },
    HonorTitle {
        id: "celestial_satori",
        name: "Éveillé du Vent Céleste",
        condition_desc: "Atteindre le Niveau Global 12",
        category: XpPath::Eveil,
    },
];

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
            "system_info" | "top_processes" | "network_info" | "battery_info" => {
                self.auto_allow_system_info
            }
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

/// Entrée du journal d'expérience persisté.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XpLogEntry {
    pub path: XpPath,
    pub amount: u64,
    pub reason: String,
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
    pub total_breaths: u32,

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

    /// Identifiant du titre d'honneur choisi par l'utilisateur (`None` = utilise le rang global).
    pub equipped_title_id: Option<String>,
    /// Liste des identifiants de titres déjà débloqués (conservés même si les stats évoluent).
    pub unlocked_title_ids: Vec<String>,
    /// Journal des derniers gains d'XP (persisté pour garder la trace de sa progression).
    pub recent_xp_log: Vec<XpLogEntry>,

    /// Indique si le Sensei prend des initiatives autonomes périodiques (activé par défaut).
    #[serde(default = "default_true")]
    pub sensei_autonomy: bool,

    pub permissions: PermissionsConfig,
}

fn default_true() -> bool {
    true
}

pub fn progress_file_path() -> PathBuf {
    kde::app_data_dir().join("progress.json")
}

fn progress_backup_path() -> PathBuf {
    kde::app_data_dir().join("progress.backup.json")
}

impl Progress {
    /// Charge la progression depuis `progress.json` (ou `progress.backup.json` en secours).
    pub fn load() -> Self {
        let primary = progress_file_path();
        let backup = progress_backup_path();

        let mut prog: Self = fs::read_to_string(&primary)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .or_else(|| {
                fs::read_to_string(&backup)
                    .ok()
                    .and_then(|s| serde_json::from_str(&s).ok())
            })
            .unwrap_or_default();

        prog.refresh_unlocked_titles();
        prog
    }

    /// Sauvegarde atomique dans `progress.json` + copie de sécurité `progress.backup.json`.
    pub fn save(&mut self) {
        self.refresh_unlocked_titles();
        let dir = kde::app_data_dir();
        let _ = fs::create_dir_all(&dir);

        if let Ok(json) = serde_json::to_string_pretty(self) {
            let tmp_path = dir.join("progress.json.tmp");
            let final_path = progress_file_path();
            let backup_path = progress_backup_path();

            if fs::write(&tmp_path, &json).is_ok() {
                let _ = fs::rename(&tmp_path, &final_path);
            } else {
                let _ = fs::write(&final_path, &json);
            }
            let _ = fs::write(&backup_path, &json);
        }
    }

    pub fn path_xp(&self, path: XpPath) -> u64 {
        match path {
            XpPath::Zazen => self.zazen_xp,
            XpPath::Eveil => self.eveil_xp,
            XpPath::Bushido => self.bushido_xp,
        }
    }

    pub fn add_xp_with_log(&mut self, path: XpPath, amount: u64, reason: &str) {
        match path {
            XpPath::Zazen => self.zazen_xp += amount,
            XpPath::Eveil => self.eveil_xp += amount,
            XpPath::Bushido => self.bushido_xp += amount,
        }
        if !reason.trim().is_empty() {
            self.recent_xp_log.insert(
                0,
                XpLogEntry {
                    path,
                    amount,
                    reason: reason.to_string(),
                },
            );
            if self.recent_xp_log.len() > 15 {
                self.recent_xp_log.truncate(15);
            }
        }
        self.refresh_unlocked_titles();
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

    /// Vérifie si un titre d'honneur est débloqué selon l'état actuel ou l'historique.
    pub fn is_title_unlocked(&self, id: &str) -> bool {
        if self.unlocked_title_ids.iter().any(|u| u == id) {
            return true;
        }
        let z_lvl = self.path_level(XpPath::Zazen).level;
        let e_lvl = self.path_level(XpPath::Eveil).level;
        let b_lvl = self.path_level(XpPath::Bushido).level;
        let g_lvl = self.global_level().level;

        match id {
            "shoshin" => true,
            "jade_breath" => z_lvl >= 2 || self.meditation_secs >= 300 || self.total_breaths >= 15,
            "truth_seeker" => e_lvl >= 2 || self.quiz_correct >= 5,
            "disciplined_blade" => {
                b_lvl >= 2 || (self.actions_done + self.timers_done + self.pomodoros_done) >= 5
            }
            "still_water" => z_lvl >= 4 || self.meditation_secs >= 1200,
            "scroll_scholar" => e_lvl >= 4 || self.best_streak >= 5,
            "hourglass_keeper" => self.pomodoros_done >= 3 || b_lvl >= 4,
            "three_paths" => z_lvl >= 3 && e_lvl >= 3 && b_lvl >= 3,
            "tokonoma_oracle" => self.quiz_correct >= 20 || e_lvl >= 7,
            "silence_master" => z_lvl >= 7 || self.meditation_secs >= 3600,
            "unwavering_samurai" => self.visit_streak >= 5 || b_lvl >= 6,
            "celestial_satori" => g_lvl >= 12,
            _ => false,
        }
    }

    /// Met à jour la liste persistée des titres débloqués et retourne les nouveaux titres acquis.
    pub fn refresh_unlocked_titles(&mut self) -> Vec<&'static HonorTitle> {
        let mut newly_unlocked = Vec::new();
        for t in HONOR_TITLES {
            if self.is_title_unlocked(t.id) && !self.unlocked_title_ids.iter().any(|u| u == t.id) {
                self.unlocked_title_ids.push(t.id.to_string());
                newly_unlocked.push(t);
            }
        }
        newly_unlocked
    }

    /// Retourne le titre affiché pour le joueur (soit son Titre d'Honneur équipé, soit son Rang Global).
    pub fn display_title(&self) -> &'static str {
        if let Some(ref eq_id) = self.equipped_title_id {
            if let Some(t) = HONOR_TITLES.iter().find(|t| t.id == eq_id) {
                if self.is_title_unlocked(t.id) {
                    return t.name;
                }
            }
        }
        let (rank_name, _) = rank_for(self.global_level().level);
        rank_name
    }

    /// Retourne une version courte du titre actif pour le petit talisman du bureau.
    pub fn short_display_title(&self) -> &'static str {
        if let Some(ref eq_id) = self.equipped_title_id {
            if let Some(t) = HONOR_TITLES.iter().find(|t| t.id == eq_id) {
                if self.is_title_unlocked(t.id) {
                    return t.name;
                }
            }
        }
        short_rank_for(self.global_level().level)
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

    /// Détermine la directive volontaire actuelle du Sensei selon l'Harmonie du Jour et l'équilibre des Voies.
    pub fn current_sensei_directive(&self) -> (&'static str, &'static str, XpPath) {
        if self.daily_quizzes == 0 {
            (
                "Ordre du Sensei : Affûte ton esprit avec une énigme d'Éveil pour ouvrir ta journée.",
                "Relever l'Énigme >",
                XpPath::Eveil,
            )
        } else if self.daily_breaths < 3 {
            (
                "Ordre du Sensei : Accomplis 3 cycles de souffle Zazen pour apaiser ton esprit.",
                "Respirer en Zazen >",
                XpPath::Zazen,
            )
        } else if self.daily_actions == 0 {
            (
                "Ordre du Sensei : Lance un sablier Pomodoro ou un bilan système sur la Voie du Bushidō.",
                "Passer à l'Action >",
                XpPath::Bushido,
            )
        } else {
            let z = self.zazen_xp;
            let e = self.eveil_xp;
            let b = self.bushido_xp;
            if e <= z && e <= b {
                (
                    "Harmonie du Jour accomplie ! Poursuis ton élévation sur la Voie de l'Éveil.",
                    "Continuer l'Éveil >",
                    XpPath::Eveil,
                )
            } else if z <= e && z <= b {
                (
                    "Harmonie du Jour accomplie ! Approfondis ton souffle dans la Voie du Zazen.",
                    "Méditer en Zazen >",
                    XpPath::Zazen,
                )
            } else {
                (
                    "Harmonie du Jour accomplie ! Forge ta discipline avec un Pomodoro Bushidō.",
                    "Lancer un Rituel >",
                    XpPath::Bushido,
                )
            }
        }
    }

    /// Résumé injecté dans la conscience temps réel de Kaze.
    pub fn summary_for_ai(&self) -> String {
        let g = self.global_level();
        let (rank, _) = rank_for(g.level);
        format!(
            "Niveau {} (rang {}, titre « {} », {} XP total) | Zazen Nv {} ({} min médités) | Éveil Nv {} ({}/{} bonnes réponses) | Bushidō Nv {} ({} Pomodoros, assiduité {} j) | Harmonie du Jour: Souffles {}/3, Éveil {}/1, Action {}/1",
            g.level,
            rank,
            self.display_title(),
            self.total_xp(),
            self.path_level(XpPath::Zazen).level,
            self.meditation_secs / 60,
            self.path_level(XpPath::Eveil).level,
            self.quiz_correct,
            self.quiz_answered,
            self.path_level(XpPath::Bushido).level,
            self.pomodoros_done,
            self.visit_streak,
            self.daily_breaths.min(3),
            self.daily_quizzes.min(1),
            self.daily_actions.min(1)
        )
    }
}
