use crate::kde;
use crate::sprites::PetMood;
use crate::tasks;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct ActiveTimer {
    pub id: u32,
    pub label: String,
    pub total_duration: Duration,
    pub started_at: Instant,
    pub is_pomodoro: bool,
}

impl ActiveTimer {
    pub fn remaining(&self) -> Duration {
        self.total_duration.saturating_sub(self.started_at.elapsed())
    }

    pub fn is_expired(&self) -> bool {
        self.started_at.elapsed() >= self.total_duration
    }

    pub fn progress_ratio(&self) -> f32 {
        let total = self.total_duration.as_secs_f32().max(1.0);
        (self.started_at.elapsed().as_secs_f32() / total).clamp(0.0, 1.0)
    }

    pub fn format_remaining(&self) -> String {
        let rem = self.remaining().as_secs();
        let mins = rem / 60;
        let secs = rem % 60;
        let kind = if self.is_pomodoro { "Pomodoro" } else { "Minuteur" };
        format!(
            "#{} [{}] « {} » : reste {:02}m {:02}s ({:.0}%)",
            self.id,
            kind,
            self.label,
            mins,
            secs,
            self.progress_ratio() * 100.0
        )
    }
}

#[derive(Default)]
pub struct TimerRegistry {
    next_id: u32,
    pub active: Vec<ActiveTimer>,
    pub completed_history: Vec<(String, Instant)>,
}

impl TimerRegistry {
    pub fn add_timer(&mut self, secs: u64, label: String, is_pomodoro: bool) -> ActiveTimer {
        self.next_id += 1;
        let timer = ActiveTimer {
            id: self.next_id,
            label,
            total_duration: Duration::from_secs(secs),
            started_at: Instant::now(),
            is_pomodoro,
        };
        self.active.push(timer.clone());
        timer
    }

    pub fn cancel(&mut self, filter: &str) -> usize {
        let f = filter.trim().to_lowercase();
        let before = self.active.len();
        if f.is_empty() || f == "all" || f == "tout" || f == "tous" {
            self.active.clear();
        } else {
            self.active.retain(|t| {
                let id_match = format!("#{}", t.id) == f || t.id.to_string() == f;
                let name_match = t.label.to_lowercase().contains(&f)
                    || (t.is_pomodoro && f.contains("pomo"));
                !(id_match || name_match)
            });
        }
        before.saturating_sub(self.active.len())
    }

    pub fn poll_expired(&mut self) -> Vec<ActiveTimer> {
        let mut expired = Vec::new();
        let mut remaining = Vec::new();
        for t in self.active.drain(..) {
            if t.is_expired() {
                self.completed_history.push((t.label.clone(), Instant::now()));
                if self.completed_history.len() > 5 {
                    self.completed_history.remove(0);
                }
                expired.push(t);
            } else {
                remaining.push(t);
            }
        }
        self.active = remaining;
        expired
    }

    pub fn summary_for_ai(&self) -> String {
        let mut parts = Vec::new();
        if self.active.is_empty() {
            parts.push("Aucun minuteur ni Pomodoro en cours.".to_string());
        } else {
            let list: Vec<String> = self.active.iter().map(|t| t.format_remaining()).collect();
            parts.push(format!("Minuteurs actifs ({}) : {}", self.active.len(), list.join(" | ")));
        }

        if !self.completed_history.is_empty() {
            let recent: Vec<String> = self
                .completed_history
                .iter()
                .rev()
                .take(3)
                .map(|(lbl, when)| {
                    let ago = when.elapsed().as_secs();
                    if ago < 60 {
                        format!("« {} » (terminé il y a {}s)", lbl, ago)
                    } else {
                        format!("« {} » (terminé il y a {}m)", lbl, ago / 60)
                    }
                })
                .collect();
            parts.push(format!("Récemment écoulés : {}", recent.join(", ")));
        }
        parts.join(" — ")
    }
}

pub fn global_timers() -> &'static Arc<Mutex<TimerRegistry>> {
    static REGISTRY: OnceLock<Arc<Mutex<TimerRegistry>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Arc::new(Mutex::new(TimerRegistry::default())))
}

/// État interne du compagnon (Self-Awareness)
#[derive(Debug, Clone)]
pub struct PetSelfState {
    pub mood: PetMood,
    pub roaming: bool,
    pub meditating: bool,
    pub pos_x: f32,
    pub pos_y: f32,
    pub session_start: Instant,
    pub interactions: u32,
    pub active_model: String,
}

impl Default for PetSelfState {
    fn default() -> Self {
        Self {
            mood: PetMood::Idle,
            roaming: false,
            meditating: false,
            pos_x: 220.0,
            pos_y: 220.0,
            session_start: Instant::now(),
            interactions: 0,
            active_model: "qwen2.5:3b".into(),
        }
    }
}

pub fn global_self_state() -> &'static Arc<Mutex<PetSelfState>> {
    static STATE: OnceLock<Arc<Mutex<PetSelfState>>> = OnceLock::new();
    STATE.get_or_init(|| Arc::new(Mutex::new(PetSelfState::default())))
}

/// Mémoire persistante long-terme de Kaze (`memory.json`)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryBank {
    pub facts: Vec<String>,
}

fn memory_file_path() -> PathBuf {
    kde::app_data_dir().join("memory.json")
}

pub fn load_memory() -> MemoryBank {
    let path = memory_file_path();
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_memory(bank: &MemoryBank) {
    let path = memory_file_path();
    if let Ok(json) = serde_json::to_string_pretty(bank) {
        let _ = fs::write(path, json);
    }
}

pub fn remember_fact(fact: &str) -> String {
    let clean = fact.trim();
    if clean.is_empty() {
        return "⚠️ Aucun souvenir précisé.".into();
    }
    let mut bank = load_memory();
    if !bank.facts.iter().any(|f| f.eq_ignore_ascii_case(clean)) {
        bank.facts.push(clean.to_string());
    }
    save_memory(&bank);
    format!("🧠 Souvenir gravé dans la mémoire de Kaze : « {} »", clean)
}

pub fn recall_memory() -> String {
    let bank = load_memory();
    if bank.facts.is_empty() {
        "🧠 La mémoire long-terme de Kaze est encore vierge.".into()
    } else {
        let list: Vec<String> = bank
            .facts
            .iter()
            .enumerate()
            .map(|(i, f)| format!("{}. {}", i + 1, f))
            .collect();
        format!("🧠 Souvenirs de Kaze ({}) :\n{}", bank.facts.len(), list.join("\n"))
    }
}

pub fn forget_memory(query: &str) -> String {
    let q = query.trim().to_lowercase();
    let mut bank = load_memory();
    if q.is_empty() || q == "all" || q == "tout" {
        bank.facts.clear();
        save_memory(&bank);
        return "🍃 Tous les souvenirs long-terme ont été effacés.".into();
    }
    let before = bank.facts.len();
    bank.facts.retain(|f| !f.to_lowercase().contains(&q));
    let removed = before.saturating_sub(bank.facts.len());
    save_memory(&bank);
    format!("🍃 {} souvenir(s) oublié(s).", removed)
}

/// Construit le rapport complet de Conscience Temps Réel (Deep Awareness) injecté dans le modèle.
pub fn build_deep_awareness_context() -> String {
    let timer_summary = global_timers()
        .lock()
        .map(|reg| reg.summary_for_ai())
        .unwrap_or_else(|_| "Inconnu".into());

    let self_summary = global_self_state()
        .lock()
        .map(|st| {
            let mins = st.session_start.elapsed().as_secs() / 60;
            format!(
                "Posture: {} | Mode Méditation (bas-droite): {} | Balade: {} | Position écran: ({:.0}, {:.0}) | Ensemble depuis: {} min | Échanges: {}",
                st.mood.label(),
                if st.meditating { "actif" } else { "inactif" },
                if st.roaming { "active" } else { "immobile" },
                st.pos_x,
                st.pos_y,
                mins,
                st.interactions
            )
        })
        .unwrap_or_default();

    let sys_ctx = kde::get_compact_context();
    let top_procs = kde::get_top3_processes_compact();
    let git_ctx = get_workspace_and_git_context();
    let clip_preview = kde::peek_clipboard_compact(90);
    let notes_preview = tasks::peek_recent_notes(4);
    let memory = load_memory();
    let mem_str = if memory.facts.is_empty() {
        "Aucun fait mémorisé".to_string()
    } else {
        memory.facts.join(" ; ")
    };

    let progress_summary = crate::progress::Progress::load().summary_for_ai();

    format!(
        "=== CONSCIENCE TEMPS RÉEL DE KAZE ===\n\
        • [SYSTÈME & OS] {}\n\
        • [APPLICATIONS ACTIVES (TOP CPU/RAM)] {}\n\
        • [MINUTEURS & POMODORO] {}\n\
        • [PROGRESSION & RANG] {}\n\
        • [ÉTAT INTERNE DU SAMOURAÏ] {}\n\
        • [ESPACE DE TRAVAIL & GIT] {}\n\
        • [PRESSE-PAPIERS ACTUEL] {}\n\
        • [NOTES RÉCENTES DU PARCHEMIN] {}\n\
        • [MÉMOIRE LONG-TERME UTILISATEUR] {}",
        sys_ctx,
        top_procs,
        timer_summary,
        progress_summary,
        self_summary,
        git_ctx,
        clip_preview,
        notes_preview,
        mem_str
    )
}

fn get_workspace_and_git_context() -> String {
    let cwd = std::env::current_dir()
        .ok()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| ".".into());

    let branch = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string());

    if let Some(b) = branch {
        let dirty_count = Command::new("git")
            .args(["status", "--porcelain"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.lines().count())
            .unwrap_or(0);
        format!("Dossier: {} | Git branche: {} ({} fichiers modifiés)", cwd, b, dirty_count)
    } else {
        format!("Dossier: {}", cwd)
    }
}
