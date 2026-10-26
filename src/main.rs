mod awareness;
mod kde;
mod ollama;
mod progress;
mod sprites;
mod tasks;
mod zen_ui;

use eframe::egui::{
    self, Align, Align2, Color32, CursorIcon, FontFamily, FontId, Frame, Key, Layout, Pos2, Rect,
    ResizeDirection, RichText, ScrollArea, Sense, TextureHandle, Vec2, ViewportBuilder,
    ViewportCommand, WindowLevel,
};
use ollama::{
    AgentFinalResult, AgentTurnOutcome, ChatMessage, PendingAgentAction, QuizQuestion,
    DEFAULT_MODEL,
};
use progress::{next_rank, rank_for, Progress, XpPath, HONOR_TITLES};
use sprites::{load_dojo_background, PetMood, SpriteAnimator};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};
use zen_ui::{
    draw_dojo_background_cover, draw_outlined_text, draw_pixel_bar, draw_pixel_floating_island,
    draw_pixel_plate, draw_resize_grip, draw_tatami_shadow, draw_washi_input_box,
    draw_washi_speech_bubble, draw_zazen_meditation_halo, fill_px, install_pixel_fonts, zen_button,
    zen_quiz_option_card, BreathPhase, BtnKind, ZenPalette, ZenParticles,
};

const WINDOW_TITLE: &str = "Kaze AI Pet";
const PET_WIN_WIDTH: f32 = 450.0;
const PET_WIN_HEIGHT: f32 = 490.0;
const DEFAULT_DOJO_WIDTH: f32 = 980.0;
const DEFAULT_DOJO_HEIGHT: f32 = 620.0;
const MIN_DOJO_WIDTH: f32 = 760.0;
const MIN_DOJO_HEIGHT: f32 = 500.0;

const ZEN_HAIKUS: &[&str] = &[
    "« L'eau calme reflète la lune sans effort. »",
    "« Un souffle après l'autre, la montagne demeure. »",
    "« Entre deux pensées, le vent murmure dans les bambous. »",
    "« Le sabre au fourreau protège la paix de l'esprit. »",
    "« Pétale de sakura tombant : rien ne presse l'instant. »",
];

/// Mode d'affichage de la bulle unique de dialogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BubbleViewMode {
    /// Mode Zen : affiche uniquement la demande actuelle et la réponse actuelle
    Zen,
    /// Mode Échange : affiche le fil complet des échanges
    Echange,
}

/// Les 5 Piliers épurés du Dojo (/menu ou /dojo).
/// Par défaut, le Dojo s'ouvre sur `Sanctuaire` (vue d'accueil calme et dégagée).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuTab {
    /// 1. Sanctuaire (Accueil calme : Conseil du Sensei, Niveau, 3 Voies & Titres d'Honneur)
    Sanctuaire,
    /// 2. Voie du Zazen : Préparation calme puis bouton « Démarrer la méditation »
    Zazen,
    /// 3. Voie de l'Éveil : Choix du sujet puis bouton « Démarrer l'énigme »
    Eveil,
    /// 4. Voie du Bushidō : Sabliers Pomodoro & Actions Système
    Bushido,
    /// 5. Esprit du Sensei : ROLE.md, Forge kaze-sensei, Autonomie, Mémoire & Sécurité
    Sensei,
}

impl MenuTab {
    const ALL: [MenuTab; 5] = [
        Self::Sanctuaire,
        Self::Zazen,
        Self::Eveil,
        Self::Bushido,
        Self::Sensei,
    ];

    fn label(&self) -> &'static str {
        match self {
            Self::Sanctuaire => "Sanctuaire",
            Self::Zazen => "Zazen",
            Self::Eveil => "Éveil",
            Self::Bushido => "Bushidō",
            Self::Sensei => "Sensei & IA",
        }
    }

    fn accent(&self) -> Color32 {
        match self {
            Self::Sanctuaire => ZenPalette::GOLD_LEAF,
            Self::Zazen => ZenPalette::JADE_LIGHT,
            Self::Eveil => ZenPalette::GOLD_LEAF,
            Self::Bushido => ZenPalette::SAKURA,
            Self::Sensei => ZenPalette::CYAN_SPIRIT,
        }
    }

    fn purpose(&self) -> &'static str {
        match self {
            Self::Sanctuaire => "Accueil du Dojo : Niveau, 3 Voies, Titres d'Honneur & Conseil du Sensei",
            Self::Zazen => "Voie du Zazen : Préparer et démarrer une séance de respiration 4-3-5",
            Self::Eveil => "Voie de l'Éveil : Choisir un sujet et démarrer une énigme du Sensei",
            Self::Bushido => "Voie du Bushidō : Sabliers Pomodoro & Actions Système",
            Self::Sensei => "Charte ROLE.md, Forge kaze-sensei, Autonomie, Mémoire & Sécurité",
        }
    }
}

/// Événements renvoyés par les workers asynchrones vers le thread d'interface.
enum WorkerEvent {
    PlanReady(Result<AgentTurnOutcome, String>),
    DecisionDone(Result<AgentFinalResult, String>),
    CommandOutput {
        text: String,
        mood: PetMood,
        quit_after: bool,
    },
    ModelsLoaded(Vec<String>),
    QuizReady(Result<QuizQuestion, String>),
    SenseiForged(Result<String, String>),
}

/// Notification visuelle flottante de gain d'XP (+15 XP Éveil).
struct XpPopup {
    text: String,
    path: XpPath,
    spawn_time: Instant,
    pos: Pos2,
}

/// État du Dojo d'apprentissage (Voie de l'Éveil).
struct QuizState {
    topic_input: String,
    loading: bool,
    question: Option<QuizQuestion>,
    selected_option: Option<usize>,
    was_correct: Option<bool>,
    streak: u32,
    error: Option<String>,
}

impl Default for QuizState {
    fn default() -> Self {
        Self {
            topic_input: "Philosophie Zen & Samouraï".into(),
            loading: false,
            question: None,
            selected_option: None,
            was_correct: None,
            streak: 0,
            error: None,
        }
    }
}

/// État global de l'application Kaze.
struct KazePetApp {
    animator: SpriteAnimator,
    dojo_bg_tex: TextureHandle,
    particles: ZenParticles,
    start_time: Instant,

    // Progression persistante (progress.json) & XP
    progress: Progress,
    xp_popups: Vec<XpPopup>,
    last_pat_xp: Instant,

    // Bulle unique tout-en-un (animée de façon fluide et ancrée juste au-dessus de Kaze)
    bubble_open: bool,
    bubble_anim: f32,
    bubble_h_anim: f32,
    bubble_view_mode: BubbleViewMode,
    full_text: String,
    revealed_chars: usize,
    last_typewriter_tick: Instant,
    bubble_feedback: Option<(String, Instant)>,
    last_exported_md: Option<PathBuf>,
    last_user_prompt: Option<String>,
    last_kaze_reply: Option<String>,

    // Grande Fenêtre Redimensionnable du Dojo (/menu & /eveil) avec background_dojo.jpeg
    show_control_panel: bool,
    menu_tab: MenuTab,
    dojo_window_size: Vec2,
    dojo_large_toggle: bool,
    saved_pet_pos: Pos2,
    dojo_pos: Option<Pos2>,
    new_memory_fact_input: String,

    // Confirmation d'action d'agent (Human-in-the-Loop)
    pending_confirmation: Option<PendingAgentAction>,

    // Méditation Zazen (Bureau & Dojo avec démarrage explicite)
    in_zazen: bool,
    meditation_started_at: Instant,
    meditation_in_corner: bool,
    saved_pos_before_meditation: Pos2,
    screen_size: (f32, f32),
    last_breath_count: u32,
    dojo_zazen_active: bool,
    dojo_zazen_started_at: Instant,
    dojo_last_breath_count: u32,

    // Dojo d'apprentissage (Éveil)
    quiz: QuizState,

    // Entrée texte, Modèle & Autonomie du Sensei
    input_text: String,
    focus_input_next: bool,
    is_loading: bool,
    forging_model: bool,
    active_model: String,
    available_models: Vec<String>,
    history: Vec<ChatMessage>,
    last_autonomy_pulse: Instant,

    // Communication asynchrone
    tx: Sender<WorkerEvent>,
    rx: Receiver<WorkerEvent>,

    // Mode balade autonome
    roaming_enabled: bool,
    window_pos: Pos2,
    roam_velocity_x: f32,
    next_roam_decision: Instant,

    // Système & KWin
    quit_at: Option<Instant>,
    kwin_hints_applied: bool,
    last_topmost_refresh: Instant,
}

impl KazePetApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install_pixel_fonts(&cc.egui_ctx);

        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = Color32::TRANSPARENT;
        visuals.window_fill = Color32::TRANSPARENT;
        visuals.extreme_bg_color = Color32::from_rgba_premultiplied(18, 22, 32, 220);
        visuals.override_text_color = Some(ZenPalette::TEXT_LIGHT);
        cc.egui_ctx.set_visuals(visuals);

        let animator = SpriteAnimator::load(&cc.egui_ctx);
        let dojo_bg_tex = load_dojo_background(&cc.egui_ctx);
        let (tx, rx) = mpsc::channel();
        let screen_size = kde::detect_primary_screen_size();

        // S'assurer que ROLE.md est prêt dans le dossier applicatif
        let _ = ollama::role_file_path();

        let tx_init = tx.clone();
        let ctx_init = cc.egui_ctx.clone();
        std::thread::spawn(move || {
            if let Ok(models) = ollama::list_models() {
                let names: Vec<String> = models.into_iter().map(|(n, _)| n).collect();
                let _ = tx_init.send(WorkerEvent::ModelsLoaded(names));
                ctx_init.request_repaint();
            }
        });

        let mut progress = Progress::load();
        let mut xp_popups = Vec::new();
        if let Some((daily_xp, streak)) = progress.register_daily_visit() {
            progress.add_xp_with_log(
                XpPath::Bushido,
                daily_xp,
                &format!("Assiduité quotidienne ({} j)", streak),
            );
            progress.save();
            xp_popups.push(XpPopup {
                text: format!("+{} XP Assiduité ({} j)", daily_xp, streak),
                path: XpPath::Bushido,
                spawn_time: Instant::now(),
                pos: Pos2::new(PET_WIN_WIDTH / 2.0, 290.0),
            });
        } else {
            progress.save();
        }

        let (directive_txt, _, _) = progress.current_sensei_directive();
        let welcome = format!(
            "Salutation, {} (Nv.{}).\n{}\nDouble-clique sur moi ou ouvre le Dojo pour commencer.",
            progress.display_title(),
            progress.global_level().level,
            directive_txt
        );
        let welcome_len = welcome.chars().count();

        Self {
            animator,
            dojo_bg_tex,
            particles: ZenParticles::new(),
            start_time: Instant::now(),
            progress,
            xp_popups,
            last_pat_xp: Instant::now() - Duration::from_secs(60),
            bubble_open: true,
            bubble_anim: 1.0,
            bubble_h_anim: 185.0,
            bubble_view_mode: BubbleViewMode::Zen,
            full_text: welcome.clone(),
            revealed_chars: welcome_len,
            last_typewriter_tick: Instant::now(),
            bubble_feedback: None,
            last_exported_md: None,
            last_user_prompt: None,
            last_kaze_reply: Some(welcome),
            show_control_panel: false,
            menu_tab: MenuTab::Sanctuaire,
            dojo_window_size: Vec2::new(DEFAULT_DOJO_WIDTH, DEFAULT_DOJO_HEIGHT),
            dojo_large_toggle: false,
            saved_pet_pos: Pos2::new(220.0, 180.0),
            dojo_pos: None,
            new_memory_fact_input: String::new(),
            pending_confirmation: None,
            in_zazen: false,
            meditation_started_at: Instant::now(),
            meditation_in_corner: false,
            saved_pos_before_meditation: Pos2::new(220.0, 180.0),
            screen_size,
            last_breath_count: 0,
            dojo_zazen_active: false,
            dojo_zazen_started_at: Instant::now(),
            dojo_last_breath_count: 0,
            quiz: QuizState::default(),
            input_text: String::new(),
            focus_input_next: true,
            is_loading: false,
            forging_model: false,
            active_model: DEFAULT_MODEL.to_string(),
            available_models: vec![DEFAULT_MODEL.to_string()],
            history: Vec::new(),
            last_autonomy_pulse: Instant::now(),
            tx,
            rx,
            roaming_enabled: false,
            window_pos: Pos2::new(220.0, 180.0),
            roam_velocity_x: 0.0,
            next_roam_decision: Instant::now() + Duration::from_secs(4),
            quit_at: None,
            kwin_hints_applied: false,
            last_topmost_refresh: Instant::now(),
        }
    }

    /// Ouvre la grande fenêtre redimensionnable du Dojo (`background_dojo.jpeg`) sur l'onglet souhaité,
    /// sans jamais lancer d'épreuve ou de méditation sans l'action explicite de l'utilisateur.
    fn open_dojo_window(&mut self, ctx: &egui::Context, tab: MenuTab) {
        if !self.show_control_panel {
            self.saved_pet_pos = self.window_pos;
            self.show_control_panel = true;
            self.roaming_enabled = false;

            if let Some(mon) = ctx.input(|i| i.viewport().monitor_size) {
                if mon.x >= 800.0 && mon.y >= 600.0 {
                    self.screen_size = (mon.x, mon.y);
                }
            }

            let (sw, sh) = self.screen_size;
            let target_pos = self.dojo_pos.unwrap_or_else(|| {
                Pos2::new(
                    ((sw - self.dojo_window_size.x) * 0.5).max(30.0),
                    ((sh - self.dojo_window_size.y) * 0.5).max(30.0),
                )
            });
            self.window_pos = target_pos;

            ctx.send_viewport_cmd(ViewportCommand::Resizable(true));
            ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(Vec2::new(
                MIN_DOJO_WIDTH,
                MIN_DOJO_HEIGHT,
            )));
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(self.dojo_window_size));
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(target_pos));
        }

        self.menu_tab = tab;
    }

    /// Referme la grande fenêtre du Dojo et ramène Kaze en mode Compagnon transparent sur le bureau.
    fn close_dojo_window(&mut self, ctx: &egui::Context) {
        if self.show_control_panel {
            if self.dojo_zazen_active {
                self.stop_dojo_zazen();
            }
            self.progress.save();
            self.dojo_pos = Some(self.window_pos);
            self.show_control_panel = false;

            ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(Vec2::new(
                PET_WIN_WIDTH,
                PET_WIN_HEIGHT,
            )));
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(Vec2::new(
                PET_WIN_WIDTH,
                PET_WIN_HEIGHT,
            )));
            ctx.send_viewport_cmd(ViewportCommand::Resizable(false));
            self.window_pos = self.saved_pet_pos;
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(self.saved_pet_pos));
            kde::enforce_x11_ewmh_top_and_taskbar(WINDOW_TITLE);
        }
    }

    /// Démarre explicitement une séance de méditation Zazen à l'intérieur du Dojo.
    fn start_dojo_zazen(&mut self) {
        self.dojo_zazen_active = true;
        self.dojo_zazen_started_at = Instant::now();
        self.dojo_last_breath_count = 0;
        self.animator.set_mood(PetMood::Swim, None);
        self.set_bubble_message(
            "Séance Zazen ouverte.\nSuis le cercle Ensō : 4s inspiration, 3s rétention, 5s expiration.",
            true,
        );
    }

    /// Termine la séance de méditation Zazen du Dojo et enregistre le temps médité.
    fn stop_dojo_zazen(&mut self) {
        if !self.dojo_zazen_active {
            return;
        }
        let elapsed_secs = self.dojo_zazen_started_at.elapsed().as_secs();
        self.dojo_zazen_active = false;
        self.progress.meditation_secs += elapsed_secs;
        self.progress.meditation_sessions += 1;
        if elapsed_secs > self.progress.best_session_secs {
            self.progress.best_session_secs = elapsed_secs;
        }
        let bonus_xp = if elapsed_secs >= 600 {
            35
        } else if elapsed_secs >= 300 {
            20
        } else if elapsed_secs >= 120 {
            10
        } else {
            0
        };
        if bonus_xp > 0 {
            self.grant_xp(
                XpPath::Zazen,
                bonus_xp,
                &format!("Session Dojo de {} min", elapsed_secs / 60),
            );
        } else {
            self.progress.save();
        }
        self.animator
            .set_mood(PetMood::Jump, Some(Duration::from_secs(2)));
        self.set_bubble_message(
            format!(
                "Séance Zazen terminée ({}m {:02}s • {} souffles). Ton esprit est apaisé.",
                elapsed_secs / 60,
                elapsed_secs % 60,
                self.dojo_last_breath_count
            ),
            true,
        );
    }

    /// Bascule entre la taille standard du Dojo (`980x620`) et une grande taille (`1320x800`).
    fn toggle_dojo_large_size(&mut self, ctx: &egui::Context) {
        self.dojo_large_toggle = !self.dojo_large_toggle;
        let (sw, sh) = self.screen_size;
        self.dojo_window_size = if self.dojo_large_toggle {
            Vec2::new((sw * 0.78).clamp(1100.0, 1480.0), (sh * 0.78).clamp(680.0, 900.0))
        } else {
            Vec2::new(DEFAULT_DOJO_WIDTH, DEFAULT_DOJO_HEIGHT)
        };
        let pos = Pos2::new(
            ((sw - self.dojo_window_size.x) * 0.5).max(20.0),
            ((sh - self.dojo_window_size.y) * 0.5).max(20.0),
        );
        self.window_pos = pos;
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(self.dojo_window_size));
        ctx.send_viewport_cmd(ViewportCommand::OuterPosition(pos));
    }

    /// Octroie de l'XP sur une voie, vérifie l'Harmonie du Jour et les Titres d'Honneur, puis sauvegarde.
    fn grant_xp(&mut self, path: XpPath, amount: u64, reason: &str) {
        let old_level = self.progress.global_level().level;
        let old_path_level = self.progress.path_level(path).level;
        self.progress.add_xp_with_log(path, amount, reason);

        let harmony_bonus = if self.progress.check_and_claim_daily_harmony() {
            self.progress
                .add_xp_with_log(XpPath::Zazen, 10, "Harmonie du Jour");
            self.progress
                .add_xp_with_log(XpPath::Eveil, 10, "Harmonie du Jour");
            self.progress
                .add_xp_with_log(XpPath::Bushido, 10, "Harmonie du Jour");
            true
        } else {
            false
        };

        let new_titles = self.progress.refresh_unlocked_titles();
        if let Some(first_new) = new_titles.first() {
            if self.progress.equipped_title_id.is_none() {
                self.progress.equipped_title_id = Some(first_new.id.to_string());
            }
        }
        let unlocked_names: Vec<&'static str> = new_titles.iter().map(|t| t.name).collect();

        self.progress.save();

        let new_level = self.progress.global_level().level;
        let new_path_level = self.progress.path_level(path).level;

        let popup_text = if !reason.is_empty() {
            format!("+{} XP {} • {}", amount, path.label(), reason)
        } else {
            format!("+{} XP {}", amount, path.label())
        };

        let popup_pos = if self.show_control_panel {
            Pos2::new(
                self.dojo_window_size.x * 0.18,
                self.dojo_window_size.y * 0.52,
            )
        } else {
            Pos2::new(PET_WIN_WIDTH / 2.0, 295.0)
        };

        self.xp_popups.push(XpPopup {
            text: popup_text,
            path,
            spawn_time: Instant::now(),
            pos: popup_pos,
        });

        if harmony_bonus {
            self.xp_popups.push(XpPopup {
                text: "+30 XP • Harmonie du Jour accomplie !".into(),
                path: XpPath::Eveil,
                spawn_time: Instant::now(),
                pos: popup_pos - Vec2::new(0.0, 24.0),
            });
        }

        for (idx, t_name) in unlocked_names.iter().enumerate() {
            self.xp_popups.push(XpPopup {
                text: format!("Titre débloqué : « {} » !", t_name),
                path: XpPath::Eveil,
                spawn_time: Instant::now(),
                pos: popup_pos - Vec2::new(0.0, 48.0 + idx as f32 * 22.0),
            });
        }

        if new_level > old_level {
            let (rank, desc) = rank_for(new_level);
            self.animator.set_mood(PetMood::Special, Some(Duration::from_secs(4)));
            self.set_bubble_message(
                format!(
                    "ÉLÉVATION ! Tu atteins le Niveau {} — Rang {} ({}).\nTon sanctuaire s'épanouit.",
                    new_level, rank, desc
                ),
                true,
            );
            let _ = kde::send_kde_notification(
                "Élévation Kaze !",
                &format!("Niveau {} atteint ({}) !", new_level, rank),
            );
        } else if new_path_level > old_path_level {
            self.animator.set_mood(PetMood::Jump, Some(Duration::from_secs(3)));
        }
    }

    fn set_bubble_message(&mut self, text: impl Into<String>, typewriter: bool) {
        let s = text.into();
        if typewriter {
            self.revealed_chars = 0;
            self.last_typewriter_tick = Instant::now();
        } else {
            self.revealed_chars = s.chars().count();
        }
        self.full_text = s.clone();
        self.last_kaze_reply = Some(s);
        self.bubble_open = true;
    }

    fn set_feedback(&mut self, msg: impl Into<String>) {
        self.bubble_feedback = Some((msg.into(), Instant::now()));
    }

    fn copy_current_response(&mut self) {
        match kde::write_clipboard_text(&self.full_text) {
            Ok(_) => {
                self.animator
                    .set_mood(PetMood::Jump, Some(Duration::from_millis(1200)));
                self.set_feedback("Copié dans le presse-papiers");
            }
            Err(e) => {
                self.set_feedback(format!("Erreur : {}", e));
            }
        }
    }

    fn export_current_response_md(&mut self, custom_arg: &str) {
        let mood_lbl = self.animator.current_mood.label();
        match tasks::export_response_to_markdown(
            custom_arg,
            &self.full_text,
            &self.active_model,
            mood_lbl,
            &self.history,
        ) {
            Ok(path) => {
                self.animator
                    .set_mood(PetMood::Special, Some(Duration::from_secs(2)));
                let file_name = path
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.display().to_string());
                self.last_exported_md = Some(path);
                self.set_feedback(format!("Exporté : {}", file_name));
            }
            Err(e) => {
                self.set_feedback(format!("Erreur : {}", e));
            }
        }
    }

    // --- Mode Zazen ---
    fn enter_zazen(&mut self, ctx: &egui::Context, move_to_corner: bool) {
        if self.show_control_panel {
            self.close_dojo_window(ctx);
        }
        self.in_zazen = true;
        self.meditation_started_at = Instant::now();
        self.meditation_in_corner = move_to_corner;
        self.last_breath_count = 0;
        self.roaming_enabled = false;
        self.roam_velocity_x = 0.0;
        self.animator.set_mood(PetMood::Swim, None);

        if let Some(mon) = ctx.input(|i| i.viewport().monitor_size) {
            if mon.x >= 800.0 && mon.y >= 600.0 {
                self.screen_size = (mon.x, mon.y);
            }
        }

        if move_to_corner {
            self.saved_pos_before_meditation = self.window_pos;
            let (sw, sh) = self.screen_size;
            let target_x = (sw - PET_WIN_WIDTH + 60.0).max(20.0);
            let target_y = (sh - PET_WIN_HEIGHT - 36.0).max(20.0);
            self.window_pos = Pos2::new(target_x, target_y);
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(self.window_pos));
        }

        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::WindowLevel(WindowLevel::AlwaysOnTop));
        kde::apply_kwin_window_hints(WINDOW_TITLE);
    }

    fn leave_zazen(&mut self, ctx: &egui::Context) {
        let elapsed_secs = self.meditation_started_at.elapsed().as_secs();
        self.progress.meditation_secs += elapsed_secs;
        self.progress.meditation_sessions += 1;
        if elapsed_secs > self.progress.best_session_secs {
            self.progress.best_session_secs = elapsed_secs;
        }

        let bonus_xp = if elapsed_secs >= 1200 {
            60
        } else if elapsed_secs >= 600 {
            35
        } else if elapsed_secs >= 300 {
            20
        } else if elapsed_secs >= 120 {
            10
        } else {
            0
        };

        if bonus_xp > 0 {
            self.grant_xp(
                XpPath::Zazen,
                bonus_xp,
                &format!("Session de {} min", elapsed_secs / 60),
            );
        } else {
            self.progress.save();
        }

        self.in_zazen = false;
        self.bubble_open = true;
        self.animator
            .set_mood(PetMood::Jump, Some(Duration::from_secs(3)));

        if self.meditation_in_corner {
            let (sw, sh) = self.screen_size;
            let wake_x = self
                .saved_pos_before_meditation
                .x
                .clamp(20.0, (sw - PET_WIN_WIDTH - 20.0).max(20.0));
            let wake_y = self
                .saved_pos_before_meditation
                .y
                .clamp(20.0, (sh - PET_WIN_HEIGHT - 60.0).max(20.0));
            self.window_pos = Pos2::new(wake_x, wake_y);
            self.meditation_in_corner = false;
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(self.window_pos));
        }

        self.set_bubble_message(
            format!(
                "Fin de la méditation Zazen ({}m {:02}s, {} souffles).\nTon esprit est apaisé.",
                elapsed_secs / 60,
                elapsed_secs % 60,
                self.last_breath_count
            ),
            true,
        );
    }

    fn toggle_meditation_corner(&mut self, ctx: &egui::Context) {
        let (sw, sh) = self.screen_size;
        if self.meditation_in_corner {
            let wake_x = self
                .saved_pos_before_meditation
                .x
                .clamp(20.0, (sw - PET_WIN_WIDTH - 20.0).max(20.0));
            let wake_y = self
                .saved_pos_before_meditation
                .y
                .clamp(20.0, (sh - PET_WIN_HEIGHT - 60.0).max(20.0));
            self.window_pos = Pos2::new(wake_x, wake_y);
            self.meditation_in_corner = false;
        } else {
            self.saved_pos_before_meditation = self.window_pos;
            let target_x = (sw - PET_WIN_WIDTH + 60.0).max(20.0);
            let target_y = (sh - PET_WIN_HEIGHT - 36.0).max(20.0);
            self.window_pos = Pos2::new(target_x, target_y);
            self.meditation_in_corner = true;
        }
        ctx.send_viewport_cmd(ViewportCommand::OuterPosition(self.window_pos));
    }

    // --- Mode Éveil (Quiz) ---
    fn request_new_quiz_question(&mut self, ctx: &egui::Context) {
        self.quiz.loading = true;
        self.quiz.error = None;
        self.quiz.selected_option = None;
        self.quiz.was_correct = None;
        self.animator.set_mood(PetMood::Swim, None);

        let tx = self.tx.clone();
        let model = self.active_model.clone();
        let topic = self.quiz.topic_input.clone();
        let ctx_clone = ctx.clone();

        std::thread::spawn(move || {
            let res = ollama::generate_quiz_question(&model, &topic);
            let _ = tx.send(WorkerEvent::QuizReady(res));
            ctx_clone.request_repaint();
        });
    }

    fn answer_quiz(&mut self, chosen_idx: usize) {
        let Some(q) = self.quiz.question.clone() else { return; };
        self.quiz.selected_option = Some(chosen_idx);
        self.progress.quiz_answered += 1;
        self.progress.daily_quizzes += 1;

        if chosen_idx == q.correct_idx {
            self.quiz.was_correct = Some(true);
            self.quiz.streak += 1;
            self.progress.quiz_correct += 1;
            if self.quiz.streak > self.progress.best_streak {
                self.progress.best_streak = self.quiz.streak;
            }

            let base_xp = 15u64;
            let streak_bonus = (self.quiz.streak as u64 - 1).min(5) * 3;
            let total = base_xp + streak_bonus;

            self.animator
                .set_mood(PetMood::Jump, Some(Duration::from_secs(3)));
            self.grant_xp(
                XpPath::Eveil,
                total,
                &format!("Série x{}", self.quiz.streak),
            );
            self.set_bubble_message(
                format!(
                    "Excellente réponse, Maître ! (+{} XP Éveil, Série x{})\n{}",
                    total, self.quiz.streak, q.explanation
                ),
                true,
            );
        } else {
            self.quiz.was_correct = Some(false);
            self.quiz.streak = 0;
            self.animator
                .set_mood(PetMood::Attack, Some(Duration::from_millis(1500)));
            self.grant_xp(XpPath::Eveil, 3, "Persévérance");
            self.set_bubble_message(
                format!(
                    "L'erreur est le premier pas vers la maîtrise (+3 XP).\n{}",
                    q.explanation
                ),
                true,
            );
        }
    }

    fn submit_input(&mut self, ctx: &egui::Context, raw: String) {
        let trimmed = raw.trim().to_string();
        if trimmed.is_empty() || self.is_loading {
            return;
        }

        if self.in_zazen {
            self.leave_zazen(ctx);
        } else {
            self.bubble_open = true;
        }

        if let Ok(mut st) = awareness::global_self_state().lock() {
            st.interactions += 1;
        }

        if self.pending_confirmation.is_some() {
            let lower = trimmed.to_lowercase();
            if matches!(
                lower.as_str(),
                "o" | "oui" | "y" | "yes" | "ok" | "accepte" | "confirme" | "vas-y"
            ) {
                self.confirm_pending_action(ctx);
                return;
            } else if matches!(
                lower.as_str(),
                "n" | "non" | "no" | "refuse" | "annule" | "stop" | "cancel"
            ) {
                self.reject_pending_action(ctx);
                return;
            }
            self.pending_confirmation = None;
        }

        if trimmed.starts_with('/') {
            self.handle_slash_command(ctx, &trimmed);
            return;
        }

        self.last_user_prompt = Some(trimmed.clone());
        self.progress.daily_actions += 1;
        self.is_loading = true;
        self.animator.set_mood(PetMood::Swim, None);
        self.set_bubble_message(
            format!(
                "Kaze médite sur ta demande ({})…",
                self.active_model
                    .split(':')
                    .next()
                    .unwrap_or(&self.active_model)
            ),
            false,
        );

        let tx = self.tx.clone();
        let model = self.active_model.clone();
        let history = self.history.clone();
        let user_msg = trimmed.clone();
        let ctx_clone = ctx.clone();

        self.history.push(ChatMessage {
            role: "user".into(),
            content: trimmed,
        });

        std::thread::spawn(move || {
            let res = ollama::plan_pet_action(&model, &history, &user_msg);
            let _ = tx.send(WorkerEvent::PlanReady(res));
            ctx_clone.request_repaint();
        });
    }

    fn request_user_confirmation(&mut self, pending: PendingAgentAction) {
        self.bubble_open = true;
        self.animator.set_mood(PetMood::Swim, None);
        let prompt_txt = pending.format_confirmation_prompt();
        self.pending_confirmation = Some(pending);
        self.set_bubble_message(prompt_txt, true);
    }

    fn confirm_pending_action(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.pending_confirmation.take() else { return; };
        self.is_loading = true;
        self.animator.set_mood(PetMood::Attack, None);
        self.set_bubble_message(format!("Exécution : {}…", pending.planned_action), false);

        self.progress.actions_done += 1;
        self.progress.daily_actions += 1;
        self.grant_xp(XpPath::Bushido, 4, "Action accomplie");

        let tx = self.tx.clone();
        let model = self.active_model.clone();
        let ctx_clone = ctx.clone();

        std::thread::spawn(move || {
            let res = ollama::execute_confirmed_action(&model, &pending);
            let _ = tx.send(WorkerEvent::DecisionDone(res));
            ctx_clone.request_repaint();
        });
    }

    fn confirm_pending_action_silently(
        &mut self,
        ctx: &egui::Context,
        pending: PendingAgentAction,
    ) {
        self.bubble_open = true;
        self.is_loading = true;
        self.animator.set_mood(pending.mood, None);
        self.set_bubble_message(format!("{}…", pending.planned_action), false);

        self.progress.actions_done += 1;
        self.progress.daily_actions += 1;
        self.grant_xp(XpPath::Bushido, 4, "Action fluide");

        let tx = self.tx.clone();
        let model = self.active_model.clone();
        let ctx_clone = ctx.clone();

        std::thread::spawn(move || {
            let res = ollama::execute_confirmed_action(&model, &pending);
            let _ = tx.send(WorkerEvent::DecisionDone(res));
            ctx_clone.request_repaint();
        });
    }

    fn reject_pending_action(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.pending_confirmation.take() else { return; };
        self.is_loading = true;
        self.animator.set_mood(PetMood::Idle, None);
        self.set_bubble_message(
            "Refus pris en compte (aucune action exécutée). Kaze adapte sa réponse…",
            false,
        );

        let tx = self.tx.clone();
        let model = self.active_model.clone();
        let ctx_clone = ctx.clone();

        std::thread::spawn(move || {
            let res = ollama::handle_rejected_action(&model, &pending);
            let _ = tx.send(WorkerEvent::DecisionDone(res));
            ctx_clone.request_repaint();
        });
    }

    fn execute_or_confirm_command(
        &mut self,
        ctx: &egui::Context,
        cmd_line: &str,
        action: &str,
        arg: &str,
        mood: PetMood,
    ) {
        let pending = PendingAgentAction::from_direct_command(cmd_line, action, arg, mood);
        if self.progress.permissions.is_task_auto_approved(action) {
            self.confirm_pending_action_silently(ctx, pending);
        } else {
            self.request_user_confirmation(pending);
        }
    }

    /// Déclenche une initiative volontaire et autonome du Sensei Kaze (conseil, observation ou proposition d'action).
    fn trigger_sensei_initiative(&mut self, ctx: &egui::Context) {
        if self.is_loading || self.pending_confirmation.is_some() {
            return;
        }
        self.last_autonomy_pulse = Instant::now();
        self.is_loading = true;
        self.animator.set_mood(PetMood::Special, None);
        self.set_bubble_message("Le Sensei observe l'état du sanctuaire…", true);

        let global_lvl = self.progress.global_level();
        let (dir_title, dir_desc, _) = self.progress.current_sensei_directive();
        let summary = format!(
            "Niveau {} ({}) • Zazen Nv.{} ({} souffles aujourd'hui) • Éveil Nv.{} ({} énigmes aujourd'hui) • Bushidō Nv.{} ({} actions aujourd'hui) • Directive prioritaire : {} ({})",
            global_lvl.level,
            self.progress.display_title(),
            self.progress.path_level(XpPath::Zazen).level,
            self.progress.daily_breaths,
            self.progress.path_level(XpPath::Eveil).level,
            self.progress.daily_quizzes,
            self.progress.path_level(XpPath::Bushido).level,
            self.progress.daily_actions,
            dir_title,
            dir_desc
        );

        let tx = self.tx.clone();
        let model = self.active_model.clone();
        let ctx_clone = ctx.clone();

        std::thread::spawn(move || {
            let res = ollama::generate_sensei_initiative(&model, &summary);
            let _ = tx.send(WorkerEvent::PlanReady(res));
            ctx_clone.request_repaint();
        });
    }

    /// Forge (fine-tune via `Modelfile.sensei`) le modèle spécialisé `kaze-sensei:latest` dans Ollama.
    fn forge_sensei_model_async(&mut self, ctx: &egui::Context) {
        if self.forging_model {
            return;
        }
        self.forging_model = true;
        self.animator.set_mood(PetMood::Swim, None);
        self.set_bubble_message(
            "Forge spirituelle du modèle spécialisé `kaze-sensei` (Modelfile.sensei) en cours…",
            true,
        );

        let base_model = if self.active_model.starts_with("kaze-sensei") {
            DEFAULT_MODEL.to_string()
        } else {
            self.active_model.clone()
        };
        let tx = self.tx.clone();
        let ctx_clone = ctx.clone();

        std::thread::spawn(move || {
            let res = ollama::forge_sensei_model(&base_model);
            if let Ok(models) = ollama::list_models() {
                let names: Vec<String> = models.into_iter().map(|(n, _)| n).collect();
                let _ = tx.send(WorkerEvent::ModelsLoaded(names));
            }
            let _ = tx.send(WorkerEvent::SenseiForged(res));
            ctx_clone.request_repaint();
        });
    }

    fn handle_slash_command(&mut self, ctx: &egui::Context, cmd_line: &str) {
        let mut parts = cmd_line.splitn(2, char::is_whitespace);
        let cmd = parts.next().unwrap_or("").to_lowercase();
        let arg = parts.next().unwrap_or("").trim().to_string();

        match cmd.as_str() {
            "/menu" | "/panel" | "/control" | "/dojo" => {
                if self.show_control_panel {
                    self.close_dojo_window(ctx);
                } else {
                    self.open_dojo_window(ctx, MenuTab::Sanctuaire);
                }
            }
            "/zen" => {
                self.bubble_view_mode = BubbleViewMode::Zen;
                self.set_bubble_message("Mode Zen activé (dernière demande & réponse).", true);
            }
            "/echange" | "/chat" => {
                self.bubble_view_mode = BubbleViewMode::Echange;
                self.set_bubble_message("Mode Échange activé (fil complet).", true);
            }
            "/close" | "/fermer" => {
                if self.show_control_panel {
                    self.close_dojo_window(ctx);
                } else {
                    self.bubble_open = false;
                }
            }
            "/balade" | "/roam" => {
                self.roaming_enabled = !self.roaming_enabled;
                let st = if self.roaming_enabled {
                    "activée"
                } else {
                    "arrêtée"
                };
                self.set_bubble_message(format!("Balade autonome {}.", st), true);
            }
            "/zazen" | "/meditate" | "/mediter" => {
                if arg.contains("bureau") || arg.contains("coin") || arg.contains("corner") {
                    let corner = arg.contains("coin") || arg.contains("corner");
                    self.enter_zazen(ctx, corner);
                } else {
                    self.open_dojo_window(ctx, MenuTab::Zazen);
                }
            }
            "/eveil" | "/learn" | "/quiz" => {
                if !arg.is_empty() {
                    self.quiz.topic_input = arg;
                }
                self.open_dojo_window(ctx, MenuTab::Eveil);
            }
            "/xp" | "/niveau" | "/rank" | "/voies" | "/titres" => {
                self.open_dojo_window(ctx, MenuTab::Sanctuaire);
            }
            "/sensei" | "/auto" => {
                self.trigger_sensei_initiative(ctx);
            }
            "/forge" | "/finetune" => {
                self.forge_sensei_model_async(ctx);
            }
            "/role" => {
                self.open_dojo_window(ctx, MenuTab::Sensei);
            }
            "/export" | "/md" => {
                self.export_current_response_md(&arg);
            }
            "/serve" | "/models" => {
                self.is_loading = true;
                self.animator.set_mood(PetMood::Swim, None);
                self.set_bubble_message("Éveil du sanctuaire Ollama…", false);
                let tx = self.tx.clone();
                let ctx_clone = ctx.clone();
                std::thread::spawn(move || {
                    let res = ollama::ensure_ollama_started();
                    if let Ok(models) = ollama::list_models() {
                        let names: Vec<String> = models.into_iter().map(|(n, _)| n).collect();
                        let _ = tx.send(WorkerEvent::ModelsLoaded(names));
                    }
                    let (text, mood) = match res {
                        Ok(msg) => (msg, PetMood::Jump),
                        Err(err) => (format!("Erreur : {}", err), PetMood::Idle),
                    };
                    let _ = tx.send(WorkerEvent::CommandOutput {
                        text,
                        mood,
                        quit_after: false,
                    });
                    ctx_clone.request_repaint();
                });
            }
            "/model" => {
                if arg.is_empty() {
                    let list = self.available_models.join(", ");
                    self.set_bubble_message(
                        format!("Modèle actif : {}\nDisponibles : {}", self.active_model, list),
                        true,
                    );
                } else {
                    let matched = self
                        .available_models
                        .iter()
                        .find(|m| m.starts_with(&arg))
                        .cloned()
                        .unwrap_or(arg);
                    self.active_model = matched.clone();
                    self.animator
                        .set_mood(PetMood::Jump, Some(Duration::from_secs(2)));
                    self.set_bubble_message(
                        format!("Esprit d'IA changé pour : {}", matched),
                        true,
                    );
                }
            }
            "/pomo" | "/pomodoro" => {
                self.execute_or_confirm_command(ctx, cmd_line, "pomodoro", &arg, PetMood::Special);
            }
            "/timer" => {
                self.execute_or_confirm_command(ctx, cmd_line, "set_timer", &arg, PetMood::Jump);
            }
            "/timers" => {
                self.execute_or_confirm_command(ctx, cmd_line, "timer_status", "", PetMood::Idle);
            }
            "/cancel" => {
                self.execute_or_confirm_command(
                    ctx,
                    cmd_line,
                    "cancel_timer",
                    &arg,
                    PetMood::Attack,
                );
            }
            "/sys" | "/status" => {
                self.execute_or_confirm_command(ctx, cmd_line, "system_info", "", PetMood::Special);
            }
            "/top" | "/ps" => {
                self.execute_or_confirm_command(
                    ctx,
                    cmd_line,
                    "top_processes",
                    "",
                    PetMood::Attack,
                );
            }
            "/net" | "/ip" => {
                self.execute_or_confirm_command(ctx, cmd_line, "network_info", "", PetMood::Run);
            }
            "/bat" | "/power" => {
                self.execute_or_confirm_command(ctx, cmd_line, "battery_info", "", PetMood::Idle);
            }
            "/vol" => {
                self.execute_or_confirm_command(
                    ctx,
                    cmd_line,
                    "audio_control",
                    &arg,
                    PetMood::Jump,
                );
            }
            "/media" => {
                self.execute_or_confirm_command(
                    ctx,
                    cmd_line,
                    "media_control",
                    &arg,
                    PetMood::Jump,
                );
            }
            "/shot" => {
                self.execute_or_confirm_command(ctx, cmd_line, "screenshot", &arg, PetMood::Attack);
            }
            "/clip" => {
                self.execute_or_confirm_command(ctx, cmd_line, "clipboard_read", "", PetMood::Idle);
            }
            "/copy" => {
                self.execute_or_confirm_command(
                    ctx,
                    cmd_line,
                    "clipboard_write",
                    &arg,
                    PetMood::Jump,
                );
            }
            "/open" => {
                self.execute_or_confirm_command(ctx, cmd_line, "open_app", &arg, PetMood::Run);
            }
            "/ls" => {
                self.execute_or_confirm_command(ctx, cmd_line, "list_files", &arg, PetMood::Run);
            }
            "/aware" => {
                self.animator
                    .set_mood(PetMood::Special, Some(Duration::from_secs(3)));
                self.set_bubble_message(awareness::build_deep_awareness_context(), true);
            }
            "/clear" => {
                self.history.clear();
                self.set_bubble_message("Historique effacé.", true);
            }
            "/quit" => {
                self.is_loading = true;
                self.animator.set_mood(PetMood::Special, None);
                self.set_bubble_message("Au revoir, voyageur !", false);
                let tx = self.tx.clone();
                let ctx_clone = ctx.clone();
                std::thread::spawn(move || {
                    ollama::stop_ollama();
                    let _ = tx.send(WorkerEvent::CommandOutput {
                        text: "À bientôt !".into(),
                        mood: PetMood::Special,
                        quit_after: true,
                    });
                    ctx_clone.request_repaint();
                });
            }
            _ => {
                self.set_bubble_message(
                    "Commandes du Sanctuaire :\n\
                    • /menu ou /dojo : ouvre la fenêtre du Dojo (Éveil, Zazen, Bushidō, Voies & Titres, Sensei & IA)\n\
                    • /sensei : demande une initiative volontaire du Sensei\n\
                    • /forge : forge le modèle spécialisé `kaze-sensei` (Modelfile.sensei)\n\
                    • /zazen [coin], /eveil [sujet], /pomo 25, /timer 5m, /sys, /top, /vol, /balade, /quit",
                    true,
                );
            }
        }
    }

    fn cycle_model(&mut self) {
        if self.available_models.is_empty() {
            return;
        }
        let idx = self
            .available_models
            .iter()
            .position(|m| m == &self.active_model)
            .unwrap_or(0);
        let next = (idx + 1) % self.available_models.len();
        self.active_model = self.available_models[next].clone();
        self.set_feedback(format!("Modèle : {}", self.active_model));
    }

    fn update_roaming(&mut self, ctx: &egui::Context) {
        if !self.roaming_enabled || self.is_loading || self.in_zazen || self.show_control_panel {
            return;
        }
        let now = Instant::now();
        if now >= self.next_roam_decision {
            let phase = (now.elapsed().subsec_nanos() / 100) % 4;
            match phase {
                0 => {
                    self.roam_velocity_x = 1.4;
                    self.animator.facing_left = false;
                    self.animator.set_mood(PetMood::Run, None);
                }
                1 => {
                    self.roam_velocity_x = -1.4;
                    self.animator.facing_left = true;
                    self.animator.set_mood(PetMood::Run, None);
                }
                2 => {
                    self.roam_velocity_x = 0.0;
                    self.animator.set_mood(PetMood::Idle, None);
                }
                _ => {
                    self.roam_velocity_x = 0.0;
                    self.animator
                        .set_mood(PetMood::Special, Some(Duration::from_secs(2)));
                }
            }
            let secs = 2 + (now.elapsed().subsec_nanos() % 4);
            self.next_roam_decision = now + Duration::from_secs(secs as u64);
        }

        if self.roam_velocity_x.abs() > 0.01 {
            let (sw, _) = self.screen_size;
            let min_x = 20.0;
            let max_x = (sw - PET_WIN_WIDTH - 20.0).max(min_x);
            let mut next_x = self.window_pos.x + self.roam_velocity_x;
            if next_x <= min_x {
                next_x = min_x;
                self.roam_velocity_x = self.roam_velocity_x.abs();
                self.animator.facing_left = false;
            } else if next_x >= max_x {
                next_x = max_x;
                self.roam_velocity_x = -self.roam_velocity_x.abs();
                self.animator.facing_left = true;
            }
            self.window_pos.x = next_x;
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(self.window_pos));
        }
    }

    /// Dessine la Grande Fenêtre Redimensionnable du Dojo (`background_dojo.jpeg`) où Kaze
    /// se tient debout sur le tatami face au jardin de bambous et à l'alcôve Tokonoma.
    /// Toutes les zones sont 100 % responsives, clippées et dotées de défilement automatique.
    fn draw_dojo_sanctuary_window(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let win_rect = ui.max_rect();
        let w = win_rect.width().max(360.0);
        let h = win_rect.height().max(360.0);

        if w >= MIN_DOJO_WIDTH && h >= MIN_DOJO_HEIGHT {
            self.dojo_window_size = win_rect.size();
        }

        let elapsed_secs = self.start_time.elapsed().as_secs_f32();
        let mood_accent = ZenPalette::mood_accent(self.animator.current_mood);
        let global_lvl = self.progress.global_level();
        let (rank_title, rank_desc) = rank_for(global_lvl.level);
        let active_title = self.progress.display_title();

        // 1. Arrière-plan Pixel-Art du Dojo en plein écran (aspect-fill cover)
        let bg_id = self.dojo_bg_tex.id();
        draw_dojo_background_cover(ui.painter(), win_rect, bg_id);

        // Pétales de sakura & lucioles flottant dans le Dojo
        self.particles.update_and_draw(
            ui.painter(),
            Pos2::new(w * 0.22, h * 0.54),
            h * 0.40,
            elapsed_secs,
        );

        // =============================================================
        // 2. BANNIÈRE SUPÉRIEURE D'EXPÉRIENCE, TITRE & FENÊTRE (DRAGGABLE)
        // =============================================================
        let top_bar_rect = Rect::from_min_size(Pos2::new(10.0, 8.0), Vec2::new(w - 20.0, 42.0));
        draw_pixel_plate(
            ui.painter(),
            top_bar_rect,
            Color32::from_rgba_unmultiplied(16, 20, 30, 238),
            ZenPalette::GOLD_LEAF,
            Some(self.menu_tab.accent()),
        );

        let top_inner = top_bar_rect.shrink2(Vec2::new(12.0, 6.0));
        let mut top_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(top_inner)
                .layout(Layout::left_to_right(Align::Center)),
        );
        top_ui.set_clip_rect(top_inner);

        // Badge Niveau + Titre équipé (cliquable pour revenir à l'accueil Sanctuaire, glissable pour déplacer la fenêtre)
        let lvl_badge_txt = format!("Nv.{} • {}", global_lvl.level, active_title);
        let drag_title = top_ui.add(
            egui::Label::new(
                RichText::new(&lvl_badge_txt)
                    .size(13.0)
                    .color(ZenPalette::GOLD_LEAF),
            )
            .sense(Sense::click_and_drag()),
        );
        if drag_title.drag_started() {
            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
        } else if drag_title.clicked() {
            self.menu_tab = MenuTab::Sanctuaire;
        }

        top_ui.add_space(10.0);

        // Grande jauge d'XP globale dans la bannière avec progression exacte
        let xp_bar_w = (w * 0.18).clamp(90.0, 210.0);
        let (xp_bar_rect, xp_bar_resp) =
            top_ui.allocate_exact_size(Vec2::new(xp_bar_w, 12.0), Sense::click());
        draw_pixel_bar(
            ui.painter(),
            xp_bar_rect,
            global_lvl.ratio(),
            ZenPalette::GOLD_LEAF,
        );
        if xp_bar_resp.clicked() {
            self.menu_tab = MenuTab::Sanctuaire;
        }

        top_ui.add_space(8.0);
        if w >= 680.0 {
            top_ui.label(
                RichText::new(format!(
                    "{} / {} XP ({}%) • Total {} XP",
                    global_lvl.into,
                    global_lvl.need,
                    global_lvl.percent(),
                    self.progress.total_xp()
                ))
                .size(11.5)
                .color(ZenPalette::TEXT_LIGHT),
            );
        }

        let mut should_close_dojo = false;
        let mut should_toggle_size = false;
        top_ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            if zen_button(ui, "Bureau x", BtnKind::Danger, Vec2::new(86.0, 24.0)).clicked() {
                should_close_dojo = true;
            }
            let size_lbl = if self.dojo_large_toggle {
                "Standard"
            } else {
                "Agrandir [ ]"
            };
            if zen_button(ui, size_lbl, BtnKind::Paper, Vec2::new(88.0, 24.0)).clicked() {
                should_toggle_size = true;
            }
            if w >= 860.0 {
                ui.label(
                    RichText::new(format!("Assiduité {} j", self.progress.visit_streak))
                        .size(11.0)
                        .color(ZenPalette::JADE_LIGHT),
                );
            }
        });

        // =============================================================
        // 3. SCÈNE GAUCHE DÉGAGÉE : KAZE SUR LE TATAMI + BULLE + DIALOGUE
        // =============================================================
        let left_col_w = (w * 0.31).clamp(235.0, 340.0);
        let left_x = 14.0;
        let kaze_cx = left_x + left_col_w * 0.5;

        // Barre de dialogue épurée tout en bas à gauche (sans boîte supplémentaire sous les pieds de Kaze)
        let chat_bar_rect =
            Rect::from_min_size(Pos2::new(left_x, h - 44.0), Vec2::new(left_col_w, 30.0));

        let kaze_feet_y = (chat_bar_rect.min.y - 18.0).max(240.0);
        let sprite_h = ((kaze_feet_y - 170.0) * 0.56).clamp(112.0, 196.0);
        let sprite_w = sprite_h * 1.38;

        // Ombre portée sur le tatami du Dojo sous les sandales de Kaze
        draw_tatami_shadow(
            ui.painter(),
            Pos2::new(kaze_cx - 3.0, kaze_feet_y - 5.0),
            sprite_w * 0.56,
        );

        // Ensō lumineux + gain d'XP UNIQUEMENT si l'utilisateur a cliqué sur « Démarrer Zazen » !
        if self.dojo_zazen_active {
            let med_secs = self.dojo_zazen_started_at.elapsed().as_secs_f32();
            let (phase, breath_ratio, cycles) = BreathPhase::compute(med_secs);
            draw_zazen_meditation_halo(
                ui.painter(),
                Pos2::new(kaze_cx, kaze_feet_y - sprite_h * 0.46),
                elapsed_secs,
                phase,
                breath_ratio,
            );
            if cycles > self.dojo_last_breath_count {
                self.dojo_last_breath_count = cycles;
                self.progress.total_breaths += 1;
                self.progress.daily_breaths += 1;
                self.grant_xp(XpPath::Zazen, 2, "Souffle Zazen au Dojo");
            }
        }

        // Sprite de Kaze posé librement sur le tatami
        let sprite_rect = Rect::from_center_size(
            Pos2::new(kaze_cx, kaze_feet_y - sprite_h * 0.48),
            Vec2::new(sprite_w, sprite_h),
        );
        let kaze_resp = ui.allocate_rect(sprite_rect, Sense::click());
        let tex_id = self.animator.current_texture().id();
        ui.painter().image(
            tex_id,
            sprite_rect,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );

        if kaze_resp.clicked() {
            let next_mood = match self.animator.current_mood {
                PetMood::Idle => PetMood::Attack,
                PetMood::Attack => PetMood::Jump,
                _ => PetMood::Special,
            };
            self.animator
                .set_mood(next_mood, Some(Duration::from_millis(1100)));
        }

        // Bulle Washi compacte au-dessus de Kaze
        let bubble_top = 58.0;
        let bubble_bottom = (sprite_rect.min.y - 4.0).clamp(bubble_top + 86.0, bubble_top + 170.0);
        let dojo_bubble_rect = Rect::from_min_max(
            Pos2::new(left_x, bubble_top),
            Pos2::new(left_x + left_col_w, bubble_bottom),
        );
        draw_washi_speech_bubble(ui.painter(), dojo_bubble_rect, mood_accent, 0.95);

        let b_box = Rect::from_min_size(
            dojo_bubble_rect.min,
            Vec2::new(
                dojo_bubble_rect.width(),
                (dojo_bubble_rect.height() - 15.0).max(40.0),
            ),
        )
        .shrink2(Vec2::new(10.0, 7.0));
        let mut b_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(b_box)
                .layout(Layout::top_down(Align::Min)),
        );
        b_ui.set_clip_rect(b_box);

        b_ui.horizontal(|ui| {
            ui.add_space(16.0);
            ui.label(
                RichText::new(format!("KAZE • {}", self.animator.current_mood.label()))
                    .size(10.5)
                    .color(ZenPalette::VERMILION),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if zen_button(ui, ".md", BtnKind::Paper, Vec2::new(30.0, 16.0)).clicked() {
                    self.export_current_response_md("");
                }
                if zen_button(ui, "Copier", BtnKind::Paper, Vec2::new(44.0, 16.0)).clicked() {
                    self.copy_current_response();
                }
            });
        });
        b_ui.add_space(2.0);

        let total_chars = self.full_text.chars().count();
        let displayed: String = self.full_text.chars().take(self.revealed_chars).collect();
        let _ = ScrollArea::vertical()
            .id_salt("dojo_left_bubble_scroll")
            .max_height((b_box.height() - 22.0).max(28.0))
            .auto_shrink([false, false])
            .show(&mut b_ui, |ui| {
                let lbl = ui.add(
                    egui::Label::new(
                        RichText::new(&displayed)
                            .size(11.5)
                            .color(ZenPalette::SUMI_INK),
                    )
                    .wrap()
                    .sense(Sense::click()),
                );
                if lbl.clicked() && self.revealed_chars < total_chars {
                    self.revealed_chars = total_chars;
                }
            });

        // Barre de dialogue rapide tout en bas à gauche
        draw_washi_input_box(ui.painter(), chat_bar_rect, false);
        let chat_inner = chat_bar_rect.shrink2(Vec2::new(6.0, 3.0));
        let mut chat_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(chat_inner)
                .layout(Layout::left_to_right(Align::Center)),
        );
        chat_ui.set_clip_rect(chat_inner);
        chat_ui.label(RichText::new("›").size(14.0).color(ZenPalette::VERMILION));
        let edit_w = (chat_inner.width() - 46.0).max(60.0);
        let chat_edit = egui::TextEdit::singleline(&mut self.input_text)
            .frame(false)
            .hint_text(
                RichText::new("Parler au Sensei Kaze…")
                    .size(11.0)
                    .color(ZenPalette::SUMI_MUTED),
            )
            .font(FontId::new(11.5, FontFamily::Proportional))
            .desired_width(edit_w)
            .text_color(ZenPalette::SUMI_INK);
        let chat_resp = chat_ui.add_enabled(!self.is_loading, chat_edit);
        let chat_enter = chat_resp.lost_focus() && chat_ui.input(|i| i.key_pressed(Key::Enter));
        let chat_send = zen_button(
            &mut chat_ui,
            if self.is_loading { "…" } else { ">" },
            BtnKind::Primary,
            Vec2::new(24.0, 20.0),
        )
        .clicked();
        if (chat_enter || chat_send) && !self.is_loading {
            let msg = std::mem::take(&mut self.input_text);
            self.submit_input(ctx, msg);
        }

        // =============================================================
        // 4. COLONNE DROITE ÉPURÉE : 5 ONGLETS CLAIRS + PANNEAU AÉRÉ
        // =============================================================
        let right_x = left_x + left_col_w + 16.0;
        let right_w = (w - right_x - 12.0).max(240.0);

        // 4A. Barre des 5 onglets (Sanctuaire par défaut, aucun déclenchement forcé au clic !)
        let tabs_rect = Rect::from_min_size(Pos2::new(right_x, 58.0), Vec2::new(right_w, 28.0));
        let mut tabs_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(tabs_rect)
                .layout(Layout::left_to_right(Align::Center)),
        );
        tabs_ui.set_clip_rect(tabs_rect);
        let tab_spacing = 6.0;
        tabs_ui.spacing_mut().item_spacing.x = tab_spacing;
        let tab_count = MenuTab::ALL.len() as f32;
        let tab_btn_w = ((right_w - tab_spacing * (tab_count - 1.0)) / tab_count)
            .floor()
            .max(44.0);

        for tab in MenuTab::ALL {
            let sel = self.menu_tab == tab;
            let kind = if sel {
                BtnKind::Selected(tab.accent())
            } else {
                BtnKind::Secondary
            };
            let resp = zen_button(&mut tabs_ui, tab.label(), kind, Vec2::new(tab_btn_w, 26.0));
            if resp.clicked() {
                self.menu_tab = tab;
            }
            resp.on_hover_text(tab.purpose());
        }

        // 4B. Panneau de contenu aéré, légèrement translucide pour laisser respirer le Dojo
        let panel_top = 92.0;
        let panel_h = (h - panel_top - 14.0).max(180.0);
        let panel_rect =
            Rect::from_min_size(Pos2::new(right_x, panel_top), Vec2::new(right_w, panel_h));

        draw_pixel_plate(
            ui.painter(),
            panel_rect,
            Color32::from_rgba_unmultiplied(18, 22, 32, 212),
            self.menu_tab.accent(),
            Some(self.menu_tab.accent()),
        );

        let inner = panel_rect.shrink2(Vec2::new(16.0, 14.0));
        let mut p_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(inner)
                .layout(Layout::top_down(Align::Min)),
        );
        p_ui.set_clip_rect(inner);

        let _ = ScrollArea::vertical()
            .id_salt(("dojo_tab_scroll", self.menu_tab.label()))
            .max_height(inner.height())
            .auto_shrink([false, false])
            .show(&mut p_ui, |ui| {
                let avail_w = ui.available_width().max(180.0);

                match self.menu_tab {
                    MenuTab::Sanctuaire => {
                        // =========================================================
                        // 1. ACCUEIL DU SANCTUAIRE (Vue par défaut calme et claire)
                        // =========================================================
                        let next_str = next_rank(global_lvl.level)
                            .map(|(nl, nr)| format!("Prochain rang : {} au Nv.{}", nr, nl))
                            .unwrap_or_else(|| "Rang ultime Satori atteint".into());

                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new(format!(
                                    "SANCTUAIRE DU DOJO — NIVEAU {} • {}",
                                    global_lvl.level, active_title
                                ))
                                .size(14.5)
                                .color(ZenPalette::GOLD_LEAF),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if zen_button(
                                    ui,
                                    "Conseil du Sensei",
                                    BtnKind::Paper,
                                    Vec2::new(126.0, 22.0),
                                )
                                .clicked()
                                {
                                    self.trigger_sensei_initiative(ctx);
                                }
                            });
                        });
                        ui.label(
                            RichText::new(format!(
                                "Rang : {} (« {} ») • {} • Harmonie du jour : Souffles {}/3, Éveil {}/1, Action {}/1",
                                rank_title,
                                rank_desc,
                                next_str,
                                self.progress.daily_breaths.min(3),
                                self.progress.daily_quizzes.min(1),
                                self.progress.daily_actions.min(1),
                            ))
                            .size(11.0)
                            .color(ZenPalette::TEXT_DIM),
                        );
                        ui.add_space(12.0);

                        // Les 3 Voies avec bouton « Ouvrir > » pour que l'utilisateur choisisse son activité
                        ui.label(
                            RichText::new("CHOISISSEZ VOTRE VOIE D'ENTRAÎNEMENT")
                                .size(12.5)
                                .color(ZenPalette::GOLD_LEAF),
                        );
                        ui.add_space(6.0);

                        let mut goto_tab: Option<MenuTab> = None;
                        for (path, target_tab, btn_lbl) in [
                            (XpPath::Zazen, MenuTab::Zazen, "Aller à Zazen >"),
                            (XpPath::Eveil, MenuTab::Eveil, "Aller à l'Éveil >"),
                            (XpPath::Bushido, MenuTab::Bushido, "Aller au Bushidō >"),
                        ] {
                            let pl = self.progress.path_level(path);
                            let m_title = path.mastery_title(pl.level);
                            let p_xp = self.progress.path_xp(path);
                            let card_h = 64.0;
                            let (p_rect, _) =
                                ui.allocate_exact_size(Vec2::new(avail_w, card_h), Sense::hover());
                            draw_pixel_plate(
                                ui.painter(),
                                p_rect,
                                Color32::from_rgba_unmultiplied(22, 26, 38, 220),
                                ZenPalette::SUMI_INK,
                                Some(path.color()),
                            );
                            let p_in = p_rect.shrink2(Vec2::new(12.0, 7.0));
                            let mut path_ui = ui.new_child(
                                egui::UiBuilder::new()
                                    .max_rect(p_in)
                                    .layout(Layout::top_down(Align::Min)),
                            );
                            path_ui.set_clip_rect(p_in);
                            path_ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "{} ({}) • Nv.{} « {} »",
                                        path.label(),
                                        path.subtitle(),
                                        pl.level,
                                        m_title
                                    ))
                                    .size(12.5)
                                    .color(path.color()),
                                );
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if zen_button(
                                        ui,
                                        btn_lbl,
                                        BtnKind::Secondary,
                                        Vec2::new(126.0, 20.0),
                                    )
                                    .clicked()
                                    {
                                        goto_tab = Some(target_tab);
                                    }
                                    ui.add_space(6.0);
                                    ui.label(
                                        RichText::new(format!(
                                            "{} / {} XP ({}% • Total {} XP)",
                                            pl.into,
                                            pl.need,
                                            pl.percent(),
                                            p_xp
                                        ))
                                        .size(11.0)
                                        .color(ZenPalette::TEXT_LIGHT),
                                    );
                                });
                            });
                            path_ui.add_space(4.0);
                            let (pb_rect, _) = path_ui
                                .allocate_exact_size(Vec2::new(p_in.width(), 7.0), Sense::hover());
                            draw_pixel_bar(ui.painter(), pb_rect, pl.ratio(), path.color());
                            path_ui.add_space(3.0);
                            path_ui.label(
                                RichText::new(path.how_to())
                                    .size(10.5)
                                    .color(ZenPalette::TEXT_DIM),
                            );
                            ui.add_space(8.0);
                        }

                        if let Some(t) = goto_tab {
                            self.menu_tab = t;
                        }

                        ui.add_space(4.0);

                        // Galerie des Titres d'Honneur (Shōgō) déblocables et équipables
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new("TITRES D'HONNEUR (SHŌGŌ) — CLIQUEZ POUR ÉQUIPER")
                                    .size(12.5)
                                    .color(ZenPalette::GOLD_LEAF),
                            );
                            if self.progress.equipped_title_id.is_some() {
                                if zen_button(
                                    ui,
                                    "Rang par défaut",
                                    BtnKind::Secondary,
                                    Vec2::new(118.0, 20.0),
                                )
                                .clicked()
                                {
                                    self.progress.equipped_title_id = None;
                                    self.progress.save();
                                }
                            }
                        });
                        ui.add_space(6.0);

                        let title_btn_w = if avail_w >= 500.0 {
                            ((avail_w - 12.0) / 3.0).floor()
                        } else {
                            ((avail_w - 6.0) / 2.0).floor()
                        };
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
                            for t in HONOR_TITLES {
                                let unlocked = self.progress.is_title_unlocked(t.id);
                                let equipped =
                                    self.progress.equipped_title_id.as_deref() == Some(t.id);
                                let label = if equipped {
                                    format!("★ {}", t.name)
                                } else if unlocked {
                                    t.name.to_string()
                                } else {
                                    format!("🔒 {}", t.name)
                                };
                                let kind = if equipped {
                                    BtnKind::Selected(t.category.color())
                                } else if unlocked {
                                    BtnKind::Paper
                                } else {
                                    BtnKind::Secondary
                                };
                                let resp =
                                    zen_button(ui, &label, kind, Vec2::new(title_btn_w, 26.0));
                                if resp.clicked() && unlocked {
                                    self.progress.equipped_title_id = Some(t.id.to_string());
                                    self.progress.save();
                                }
                                resp.on_hover_text(format!(
                                    "{}\nCondition : {}\nStatut : {}",
                                    t.name,
                                    t.condition_desc,
                                    if equipped {
                                        "Équipé actuellement"
                                    } else if unlocked {
                                        "Débloqué (cliquer pour équiper)"
                                    } else {
                                        "Verrouillé"
                                    }
                                ));
                            }
                        });

                        if !self.progress.recent_xp_log.is_empty() {
                            ui.add_space(10.0);
                            ui.label(
                                RichText::new("Derniers gains d'expérience :")
                                    .size(11.0)
                                    .color(ZenPalette::TEXT_DIM),
                            );
                            for entry in self.progress.recent_xp_log.iter().take(3) {
                                ui.label(
                                    RichText::new(format!(
                                        "  • +{} XP {} — {}",
                                        entry.amount,
                                        entry.path.label(),
                                        entry.reason
                                    ))
                                    .size(11.0)
                                    .color(entry.path.color()),
                                );
                            }
                        }
                    }

                    MenuTab::Zazen => {
                        // =========================================================
                        // 2. SANCTUAIRE ZAZEN : PRÉPARATION CALME PUIS BOUTON « DÉMARRER »
                        // =========================================================
                        let p_zazen = self.progress.path_level(XpPath::Zazen);
                        let mastery = XpPath::Zazen.mastery_title(p_zazen.level);
                        ui.label(
                            RichText::new("VOIE DU ZAZEN — MÉDITATION & COHÉRENCE DU SOUFFLE")
                                .size(14.5)
                                .color(ZenPalette::JADE_LIGHT),
                        );
                        ui.add(
                            egui::Label::new(
                                RichText::new(format!(
                                    "Niveau {} « {} » ({} / {} XP) • Temps médité : {} min ({} sessions • {} souffles)",
                                    p_zazen.level,
                                    mastery,
                                    p_zazen.into,
                                    p_zazen.need,
                                    self.progress.meditation_secs / 60,
                                    self.progress.meditation_sessions,
                                    self.progress.total_breaths
                                ))
                                .size(11.5)
                                .color(ZenPalette::TEXT_DIM),
                            )
                            .wrap(),
                        );
                        ui.add_space(6.0);
                        let (z_bar, _) =
                            ui.allocate_exact_size(Vec2::new(avail_w, 8.0), Sense::hover());
                        draw_pixel_bar(ui.painter(), z_bar, p_zazen.ratio(), ZenPalette::JADE_LIGHT);
                        ui.add_space(16.0);

                        if !self.dojo_zazen_active {
                            // ÉTAT 1 : PRÉPARATION CALME (aucune méditation forcée sans clic !)
                            let prep_h = 118.0;
                            let (prep_rect, _) =
                                ui.allocate_exact_size(Vec2::new(avail_w, prep_h), Sense::hover());
                            draw_pixel_plate(
                                ui.painter(),
                                prep_rect,
                                Color32::from_rgba_unmultiplied(22, 28, 40, 230),
                                ZenPalette::JADE_LIGHT,
                                Some(ZenPalette::JADE_LIGHT),
                            );
                            let prep_in = prep_rect.shrink2(Vec2::new(14.0, 12.0));
                            let mut prep_ui = ui.new_child(
                                egui::UiBuilder::new()
                                    .max_rect(prep_in)
                                    .layout(Layout::top_down(Align::Min)),
                            );
                            prep_ui.set_clip_rect(prep_in);
                            prep_ui.label(
                                RichText::new("PRÊT POUR UNE SÉANCE DE RESPIRATION ZAZEN ?")
                                    .size(13.0)
                                    .color(ZenPalette::JADE_LIGHT),
                            );
                            prep_ui.add_space(6.0);
                            prep_ui.add(
                                egui::Label::new(
                                    RichText::new(
                                        "Installez-vous confortablement. Lorsque vous démarrerez la séance, le cercle Ensō guidera votre souffle sur le rythme 4s Inspiration • 3s Rétention • 5s Expiration (+2 XP par cycle).",
                                    )
                                    .size(11.5)
                                    .color(ZenPalette::TEXT_LIGHT),
                                )
                                .wrap(),
                            );

                            ui.add_space(16.0);

                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(10.0, 10.0);
                                if zen_button(
                                    ui,
                                    "▶ Démarrer la méditation ici (Dojo)",
                                    BtnKind::Primary,
                                    Vec2::new(236.0, 34.0),
                                )
                                .clicked()
                                {
                                    self.start_dojo_zazen();
                                }
                                if zen_button(
                                    ui,
                                    "▶ Méditer sur le Bureau",
                                    BtnKind::Paper,
                                    Vec2::new(172.0, 34.0),
                                )
                                .clicked()
                                {
                                    self.enter_zazen(ctx, false);
                                }
                                if zen_button(
                                    ui,
                                    "▶ Méditer dans le Coin",
                                    BtnKind::Secondary,
                                    Vec2::new(168.0, 34.0),
                                )
                                .clicked()
                                {
                                    self.enter_zazen(ctx, true);
                                }
                            });
                        } else {
                            // ÉTAT 2 : SÉANCE ZAZEN EN COURS DANS LE DOJO
                            let med_secs = self.dojo_zazen_started_at.elapsed().as_secs_f32();
                            let (phase, breath_ratio, cycles) = BreathPhase::compute(med_secs);
                            let haiku = ZEN_HAIKUS[(cycles as usize) % ZEN_HAIKUS.len()];

                            let (phase_txt, phase_col) = match phase {
                                BreathPhase::Inhale => (
                                    "INSPIRATION LENTE PAR LE NEZ (4s)",
                                    ZenPalette::JADE_LIGHT,
                                ),
                                BreathPhase::Hold => {
                                    ("RÉTENTION SEREINE DU SOUFFLE (3s)", ZenPalette::GOLD_LEAF)
                                }
                                BreathPhase::Exhale => (
                                    "EXPIRATION PROFONDE & RELÂCHEMENT (5s)",
                                    ZenPalette::SAKURA,
                                ),
                            };

                            ui.label(RichText::new(phase_txt).size(14.0).color(phase_col));
                            ui.add_space(8.0);
                            let (b_rect, _) =
                                ui.allocate_exact_size(Vec2::new(avail_w, 16.0), Sense::hover());
                            draw_pixel_bar(ui.painter(), b_rect, breath_ratio, phase.color());
                            ui.add_space(10.0);

                            let mins = (med_secs / 60.0).floor() as u32;
                            let secs = (med_secs % 60.0).floor() as u32;
                            ui.label(
                                RichText::new(format!(
                                    "Durée : {:02}:{:02} • {} souffles accomplis (+{} XP Zazen)",
                                    mins,
                                    secs,
                                    cycles,
                                    cycles * 2
                                ))
                                .size(12.0)
                                .color(ZenPalette::GOLD_LEAF),
                            );
                            ui.add_space(6.0);
                            ui.add(
                                egui::Label::new(
                                    RichText::new(haiku).size(12.5).color(ZenPalette::WASHI_BG),
                                )
                                .wrap(),
                            );
                            ui.add_space(16.0);

                            if zen_button(
                                ui,
                                "■ Terminer la séance Zazen",
                                BtnKind::Danger,
                                Vec2::new(200.0, 32.0),
                            )
                            .clicked()
                            {
                                self.stop_dojo_zazen();
                            }
                        }
                    }

                    MenuTab::Eveil => {
                        // =========================================================
                        // 3. DOJO D'ÉVEIL : CHOIX CALME DU SUJET PUIS BOUTON « DÉMARRER »
                        // =========================================================
                        let p_eveil = self.progress.path_level(XpPath::Eveil);
                        let mastery = XpPath::Eveil.mastery_title(p_eveil.level);

                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new("VOIE DE L'ÉVEIL — ENSEIGNEMENT DU SENSEI")
                                    .size(14.5)
                                    .color(ZenPalette::GOLD_LEAF),
                            );
                            ui.label(
                                RichText::new(format!(
                                    "• Nv.{} « {} » ({} / {} XP) • Série : {} (Record : {})",
                                    p_eveil.level,
                                    mastery,
                                    p_eveil.into,
                                    p_eveil.need,
                                    self.quiz.streak,
                                    self.progress.best_streak
                                ))
                                .size(11.5)
                                .color(ZenPalette::JADE_LIGHT),
                            );
                        });
                        ui.add_space(4.0);

                        let (eveil_bar, _) =
                            ui.allocate_exact_size(Vec2::new(avail_w, 8.0), Sense::hover());
                        draw_pixel_bar(
                            ui.painter(),
                            eveil_bar,
                            p_eveil.ratio(),
                            ZenPalette::GOLD_LEAF,
                        );
                        ui.add_space(12.0);

                        if self.quiz.loading {
                            let (wait_rect, _) = ui.allocate_exact_size(
                                Vec2::new(avail_w, 96.0),
                                Sense::hover(),
                            );
                            draw_pixel_plate(
                                ui.painter(),
                                wait_rect,
                                ZenPalette::WASHI_BG,
                                ZenPalette::SUMI_INK,
                                Some(ZenPalette::GOLD_LEAF),
                            );
                            ui.painter().text(
                                wait_rect.center(),
                                Align2::CENTER_CENTER,
                                format!(
                                    "Le Sensei médite et prépare une énigme sur « {} »…",
                                    self.quiz.topic_input
                                ),
                                FontId::new(13.0, FontFamily::Proportional),
                                ZenPalette::SUMI_INK,
                            );
                        } else if let Some(err) = self.quiz.error.clone() {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(format!("Erreur : {}", err))
                                        .size(12.0)
                                        .color(ZenPalette::SAKURA),
                                )
                                .wrap(),
                            );
                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                if zen_button(
                                    ui,
                                    "▶ Réessayer l'énigme",
                                    BtnKind::Primary,
                                    Vec2::new(150.0, 28.0),
                                )
                                .clicked()
                                {
                                    self.request_new_quiz_question(ctx);
                                }
                                if zen_button(
                                    ui,
                                    "← Retour au choix du sujet",
                                    BtnKind::Secondary,
                                    Vec2::new(170.0, 28.0),
                                )
                                .clicked()
                                {
                                    self.quiz.error = None;
                                }
                            });
                        } else if let Some(q) = self.quiz.question.clone() {
                            // ÉTAT 2 : ÉNIGME ACTIVE
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!("Sujet : {}", self.quiz.topic_input))
                                        .size(11.5)
                                        .color(ZenPalette::GOLD_LEAF),
                                );
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if zen_button(
                                        ui,
                                        "← Changer de sujet",
                                        BtnKind::Secondary,
                                        Vec2::new(132.0, 22.0),
                                    )
                                    .clicked()
                                    {
                                        self.quiz.question = None;
                                        self.quiz.selected_option = None;
                                        self.quiz.was_correct = None;
                                    }
                                });
                            });
                            ui.add_space(6.0);

                            let q_card_h = 78.0;
                            let (q_rect, _) = ui.allocate_exact_size(
                                Vec2::new(avail_w, q_card_h),
                                Sense::hover(),
                            );
                            draw_pixel_plate(
                                ui.painter(),
                                q_rect,
                                ZenPalette::WASHI_BG,
                                ZenPalette::SUMI_INK,
                                Some(ZenPalette::VERMILION),
                            );
                            let q_inner = q_rect.shrink2(Vec2::new(12.0, 8.0));
                            let mut q_ui = ui.new_child(
                                egui::UiBuilder::new()
                                    .max_rect(q_inner)
                                    .layout(Layout::top_down(Align::Min)),
                            );
                            q_ui.set_clip_rect(q_inner);
                            let _ = ScrollArea::vertical()
                                .id_salt("dojo_question_scroll")
                                .max_height(q_inner.height())
                                .show(&mut q_ui, |ui| {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&q.question)
                                                .size(13.5)
                                                .color(ZenPalette::SUMI_INK),
                                        )
                                        .wrap(),
                                    );
                                });

                            ui.add_space(10.0);

                            let answered = self.quiz.selected_option.is_some();
                            let mut chosen_idx = None;
                            let two_cols = avail_w >= 460.0;
                            let card_h = 54.0;

                            let option_kind = |idx: usize| -> BtnKind {
                                if let Some(sel) = self.quiz.selected_option {
                                    if idx == q.correct_idx {
                                        BtnKind::Success
                                    } else if idx == sel {
                                        BtnKind::Danger
                                    } else {
                                        BtnKind::Secondary
                                    }
                                } else {
                                    BtnKind::Paper
                                }
                            };

                            if two_cols {
                                let card_w = ((avail_w - 10.0) * 0.5).floor();
                                for pair in [0usize, 1, 2, 3].chunks(2) {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 10.0;
                                        for &idx in pair {
                                            let opt = q
                                                .options
                                                .get(idx)
                                                .map(|s| s.as_str())
                                                .unwrap_or("");
                                            let prefix = ["A", "B", "C", "D"][idx];
                                            if zen_quiz_option_card(
                                                ui,
                                                prefix,
                                                opt,
                                                option_kind(idx),
                                                Vec2::new(card_w, card_h),
                                            )
                                            .clicked()
                                                && !answered
                                            {
                                                chosen_idx = Some(idx);
                                            }
                                        }
                                    });
                                    ui.add_space(8.0);
                                }
                            } else {
                                for idx in 0..4 {
                                    let opt =
                                        q.options.get(idx).map(|s| s.as_str()).unwrap_or("");
                                    let prefix = ["A", "B", "C", "D"][idx];
                                    if zen_quiz_option_card(
                                        ui,
                                        prefix,
                                        opt,
                                        option_kind(idx),
                                        Vec2::new(avail_w, card_h),
                                    )
                                    .clicked()
                                        && !answered
                                    {
                                        chosen_idx = Some(idx);
                                    }
                                    ui.add_space(6.0);
                                }
                            }

                            if let Some(idx) = chosen_idx {
                                self.answer_quiz(idx);
                            }

                            if let Some(correct) = self.quiz.was_correct {
                                ui.add_space(4.0);
                                let (tag, col) = if correct {
                                    (
                                        "Réponse exacte ! La Voie de l'Éveil s'élève.",
                                        ZenPalette::JADE_LIGHT,
                                    )
                                } else {
                                    (
                                        "Réponse incorrecte — chaque essai forge l'esprit (+3 XP).",
                                        ZenPalette::SAKURA,
                                    )
                                };
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(RichText::new(tag).size(12.5).color(col));
                                    if zen_button(
                                        ui,
                                        "▶ Énigme suivante",
                                        BtnKind::Primary,
                                        Vec2::new(142.0, 26.0),
                                    )
                                    .clicked()
                                    {
                                        self.request_new_quiz_question(ctx);
                                    }
                                });
                                ui.add_space(4.0);
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&q.explanation)
                                            .size(12.0)
                                            .color(ZenPalette::TEXT_LIGHT),
                                    )
                                    .wrap(),
                                );
                            }
                        } else {
                            // ÉTAT 1 : PRÉPARATION CALME (Choix du sujet avant de cliquer sur « Démarrer »)
                            ui.label(
                                RichText::new("1. Choisissez un thème ou saisissez votre propre sujet d'étude :")
                                    .size(12.5)
                                    .color(ZenPalette::TEXT_LIGHT),
                            );
                            ui.add_space(6.0);

                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
                                for &(lbl, topic) in &[
                                    ("Zen & Bushidō", "Philosophie Zen & Bushidō"),
                                    ("Rust & Systèmes", "Programmation Rust & Architecture Système"),
                                    ("Japon Féodal", "Histoire et Culture du Japon Féodal"),
                                    ("Linux & Sécurité", "Administration Linux, Réseau & Cybersécurité"),
                                    ("Algorithmique", "Structures de données & Algorithmique"),
                                    ("Sciences & Cosmos", "Physique, Astronomie & Sciences"),
                                ] {
                                    let is_sel = self.quiz.topic_input == topic;
                                    let kind = if is_sel {
                                        BtnKind::Selected(ZenPalette::GOLD_LEAF)
                                    } else {
                                        BtnKind::Secondary
                                    };
                                    if zen_button(ui, lbl, kind, Vec2::new(118.0, 26.0)).clicked() {
                                        self.quiz.topic_input = topic.to_string();
                                    }
                                }
                            });

                            ui.add_space(10.0);

                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("Sujet choisi :")
                                        .size(12.0)
                                        .color(ZenPalette::GOLD_LEAF),
                                );
                                let edit = egui::TextEdit::singleline(&mut self.quiz.topic_input)
                                    .hint_text("Ex: Philosophie Zen, Rust, Linux, Histoire...")
                                    .desired_width((ui.available_width() - 10.0).max(120.0))
                                    .font(FontId::new(12.5, FontFamily::Proportional));
                                let resp = ui.add(edit);
                                if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                                    self.request_new_quiz_question(ctx);
                                }
                            });

                            ui.add_space(16.0);

                            if zen_button(
                                ui,
                                "▶ Démarrer une énigme d'Éveil sur ce sujet",
                                BtnKind::Primary,
                                Vec2::new(268.0, 34.0),
                            )
                            .clicked()
                            {
                                self.request_new_quiz_question(ctx);
                            }
                        }
                    }

                    MenuTab::Bushido => {
                        let p_bushido = self.progress.path_level(XpPath::Bushido);
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new("VOIE DU BUSHIDŌ — ACTION JUSTE & MAÎTRISE SYSTÈME")
                                    .size(14.5)
                                    .color(ZenPalette::CYAN_SPIRIT),
                            );
                            ui.label(
                                RichText::new(format!(
                                    "• Nv.{} « {} » ({} / {} XP)",
                                    p_bushido.level,
                                    XpPath::Bushido.mastery_title(p_bushido.level),
                                    p_bushido.into,
                                    p_bushido.need
                                ))
                                .size(11.5)
                                .color(ZenPalette::SAKURA),
                            );
                        });
                        ui.add_space(6.0);

                        let active_timers = awareness::global_timers()
                            .lock()
                            .map(|reg| reg.active.clone())
                            .unwrap_or_default();
                        if !active_timers.is_empty() {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new("Sabliers en cours :")
                                        .size(11.5)
                                        .color(ZenPalette::JADE_LIGHT),
                                );
                                for t in &active_timers {
                                    let rem = t.remaining().as_secs();
                                    ui.label(
                                        RichText::new(format!(
                                            "[#{} {} — {:02}:{:02}]",
                                            t.id,
                                            t.label,
                                            rem / 60,
                                            rem % 60
                                        ))
                                        .size(11.5)
                                        .color(ZenPalette::GOLD_LEAF),
                                    );
                                }
                            });
                            ui.add_space(6.0);
                        }

                        let groups: &[(&str, &[(&str, &str)])] = &[
                            (
                                "Rituels Zen & Concentration (+XP Bushidō)",
                                &[
                                    ("Pomodoro 25m", "/pomo 25"),
                                    ("Focus Court 15m", "/pomo 15"),
                                    ("Sablier Thé 3m", "/timer 3m Thé"),
                                    ("Annuler Sabliers", "/cancel all"),
                                ],
                            ),
                            (
                                "Vigilance Système & Ressources",
                                &[
                                    ("Bilan OS Complet", "/sys"),
                                    ("Processus & RAM", "/top"),
                                    ("Réseau & IP", "/net"),
                                    ("État Batterie", "/bat"),
                                ],
                            ),
                            (
                                "Harmonie Audio & Multimédia",
                                &[
                                    ("Volume -10%", "/vol -10"),
                                    ("Volume +10%", "/vol +10"),
                                    ("Couper le Son", "/vol mute"),
                                    ("Play / Pause", "/media toggle"),
                                ],
                            ),
                            (
                                "Outils du Sanctuaire & Bureau",
                                &[
                                    ("Capture d'écran", "/shot"),
                                    ("Lister Dossier ~", "/ls"),
                                    ("Ouvrir Terminal", "/open terminal"),
                                    ("Lire Presse-papiers", "/clip"),
                                ],
                            ),
                        ];

                        let cols = if avail_w >= 480.0 { 4.0 } else { 2.0 };
                        let btn_w = ((avail_w - (cols - 1.0) * 8.0) / cols).floor().max(80.0);
                        let mut triggered_cmd: Option<String> = None;

                        for (group_title, items) in groups {
                            ui.label(
                                RichText::new(*group_title)
                                    .size(12.0)
                                    .color(ZenPalette::GOLD_LEAF),
                            );
                            ui.add_space(3.0);
                            ui.horizontal_wrapped(|ui| {
                                ui.spacing_mut().item_spacing = Vec2::new(8.0, 6.0);
                                for (lbl, cmd) in *items {
                                    if zen_button(
                                        ui,
                                        lbl,
                                        BtnKind::Secondary,
                                        Vec2::new(btn_w, 28.0),
                                    )
                                    .clicked()
                                    {
                                        triggered_cmd = Some((*cmd).to_string());
                                    }
                                }
                            });
                            ui.add_space(8.0);
                        }

                        if let Some(cmd) = triggered_cmd {
                            self.submit_input(ctx, cmd);
                        }
                    }

                    MenuTab::Sensei => {
                        // =========================================================
                        // ONGLET SENSEI & IA : RÔLE.MD, FINE-TUNING OLLAMA, AUTONOMIE & SÉCURITÉ
                        // =========================================================
                        ui.label(
                            RichText::new(
                                "ESPRIT DU SENSEI — RÔLE, MODÈLE SPÉCIALISÉ & AUTONOMIE",
                            )
                            .size(14.5)
                            .color(ZenPalette::VERMILION),
                        );
                        ui.add_space(6.0);

                        // 1. Carte Modèle Ollama & Fine-Tuning `kaze-sensei` (Modelfile.sensei)
                        let model_card_h = 88.0;
                        let (m_rect, _) = ui
                            .allocate_exact_size(Vec2::new(avail_w, model_card_h), Sense::hover());
                        draw_pixel_plate(
                            ui.painter(),
                            m_rect,
                            Color32::from_rgba_unmultiplied(24, 28, 42, 238),
                            ZenPalette::VERMILION,
                            Some(ZenPalette::GOLD_LEAF),
                        );
                        let m_in = m_rect.shrink2(Vec2::new(10.0, 7.0));
                        let mut m_ui = ui.new_child(
                            egui::UiBuilder::new()
                                .max_rect(m_in)
                                .layout(Layout::top_down(Align::Min)),
                        );
                        m_ui.set_clip_rect(m_in);

                        let is_sensei_model = self.active_model.starts_with("kaze-sensei");
                        m_ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new(format!("Modèle actif : {}", self.active_model))
                                    .size(12.5)
                                    .color(if is_sensei_model {
                                        ZenPalette::JADE_LIGHT
                                    } else {
                                        ZenPalette::GOLD_LEAF
                                    }),
                            );
                            if zen_button(
                                ui,
                                "Changer de modèle",
                                BtnKind::Secondary,
                                Vec2::new(128.0, 22.0),
                            )
                            .clicked()
                            {
                                self.cycle_model();
                            }
                            let forge_lbl = if self.forging_model {
                                "Forge en cours…"
                            } else if is_sensei_model {
                                "Re-forger kaze-sensei"
                            } else {
                                "Forger kaze-sensei (Fine-tune)"
                            };
                            if zen_button(
                                ui,
                                forge_lbl,
                                BtnKind::Primary,
                                Vec2::new(196.0, 22.0),
                            )
                            .clicked()
                                && !self.forging_model
                            {
                                self.forge_sensei_model_async(ctx);
                            }
                        });
                        m_ui.add_space(4.0);
                        m_ui.add(
                            egui::Label::new(
                                RichText::new(
                                    "« Forger kaze-sensei » compile `Modelfile.sensei` + `ROLE.md` dans Ollama (température 0.32, exemples few-shot Sensei, mémoire autonome `auto_remember`).",
                                )
                                .size(10.5)
                                .color(ZenPalette::TEXT_DIM),
                            )
                            .wrap(),
                        );

                        ui.add_space(10.0);

                        // 2. Autonomie Volontaire & Charte `ROLE.md`
                        ui.horizontal_wrapped(|ui| {
                            let (auto_lbl, auto_kind) = if self.progress.sensei_autonomy {
                                ("Autonomie Sensei : ACTIVE", BtnKind::Success)
                            } else {
                                ("Autonomie Sensei : PAUSE", BtnKind::Secondary)
                            };
                            if zen_button(ui, auto_lbl, auto_kind, Vec2::new(178.0, 26.0)).clicked()
                            {
                                self.progress.sensei_autonomy = !self.progress.sensei_autonomy;
                                self.progress.save();
                            }
                            if zen_button(
                                ui,
                                "Demander une Initiative maintenant",
                                BtnKind::Paper,
                                Vec2::new(210.0, 26.0),
                            )
                            .clicked()
                            {
                                self.trigger_sensei_initiative(ctx);
                            }
                            if zen_button(
                                ui,
                                "Ouvrir ROLE.md",
                                BtnKind::Secondary,
                                Vec2::new(112.0, 26.0),
                            )
                            .clicked()
                            {
                                tasks::open_file_external(&ollama::role_file_path());
                            }
                        });

                        ui.add_space(10.0);

                        // 3. Mémoire Long-Terme Autonome (`memory.json`)
                        ui.label(
                            RichText::new(
                                "MÉMOIRE LONG-TERME DU SENSEI (AUTO-MÉMORISATION & FAITS)",
                            )
                            .size(12.5)
                            .color(ZenPalette::GOLD_LEAF),
                        );
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            let mem_btn_w = 104.0;
                            let mem_edit_w = (ui.available_width() - mem_btn_w - 10.0).max(80.0);
                            let edit = egui::TextEdit::singleline(&mut self.new_memory_fact_input)
                                .hint_text("Confier un fait ou objectif au Sensei (memory.json)...")
                                .desired_width(mem_edit_w)
                                .font(FontId::new(11.5, FontFamily::Proportional));
                            ui.add(edit);
                            if zen_button(
                                ui,
                                "+ Mémoriser",
                                BtnKind::Primary,
                                Vec2::new(mem_btn_w, 22.0),
                            )
                            .clicked()
                            {
                                let fact = std::mem::take(&mut self.new_memory_fact_input);
                                if !fact.trim().is_empty() {
                                    let _ = awareness::remember_fact(&fact);
                                }
                            }
                        });
                        let facts = awareness::load_memory().facts;
                        if facts.is_empty() {
                            ui.label(
                                RichText::new("Le Sensei mémorisera automatiquement vos objectifs et préférences au fil des échanges.")
                                    .size(11.0)
                                    .color(ZenPalette::TEXT_DIM),
                            );
                        } else {
                            for f in facts.iter().rev().take(4) {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(format!("• {}", f))
                                            .size(11.0)
                                            .color(ZenPalette::TEXT_LIGHT),
                                    )
                                    .wrap(),
                                );
                            }
                        }

                        ui.add_space(10.0);

                        // 4. Autorisations d'Action Automatique (Garde-fous)
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new("AUTORISATIONS D'ACTION DE L'AGENT")
                                    .size(12.5)
                                    .color(ZenPalette::GOLD_LEAF),
                            );
                            if zen_button(
                                ui,
                                "Mode Zen (Recommandé)",
                                BtnKind::Paper,
                                Vec2::new(148.0, 22.0),
                            )
                            .clicked()
                            {
                                self.progress.permissions.set_zen_balanced();
                                self.progress.save();
                            }
                            if zen_button(
                                ui,
                                "Mode Strict",
                                BtnKind::Secondary,
                                Vec2::new(98.0, 22.0),
                            )
                            .clicked()
                            {
                                self.progress.permissions.set_strict_all();
                                self.progress.save();
                            }
                        });
                        ui.add_space(6.0);

                        let mut changed = false;
                        let mut perm_row =
                            |ui: &mut egui::Ui, val: &mut bool, label: &str, warn: bool| {
                                ui.horizontal(|ui| {
                                    let (txt, kind) = if *val {
                                        (
                                            "Autorisé (Auto)",
                                            if warn {
                                                BtnKind::Danger
                                            } else {
                                                BtnKind::Success
                                            },
                                        )
                                    } else {
                                        ("Confirmation requise", BtnKind::Secondary)
                                    };
                                    if zen_button(ui, txt, kind, Vec2::new(142.0, 22.0)).clicked() {
                                        *val = !*val;
                                        changed = true;
                                    }
                                    ui.add_space(6.0);
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(label)
                                                .size(11.0)
                                                .color(ZenPalette::TEXT_LIGHT),
                                        )
                                        .wrap(),
                                    );
                                });
                                ui.add_space(3.0);
                            };

                        perm_row(
                            ui,
                            &mut self.progress.permissions.auto_allow_system_info,
                            "Bilans Système (CPU, RAM, Disque, Réseau, Batterie)",
                            false,
                        );
                        perm_row(
                            ui,
                            &mut self.progress.permissions.auto_allow_media_audio,
                            "Contrôle Audio, Multimédia & Captures d'écran",
                            false,
                        );
                        perm_row(
                            ui,
                            &mut self.progress.permissions.auto_allow_timers,
                            "Minuteurs & Sessions Pomodoro Zen",
                            false,
                        );
                        perm_row(
                            ui,
                            &mut self.progress.permissions.auto_allow_read_notes_mem,
                            "Consultation des Notes & Mémoire long-terme",
                            false,
                        );
                        perm_row(
                            ui,
                            &mut self.progress.permissions.auto_allow_read_files,
                            "Exploration & Lecture de fichiers (~)",
                            false,
                        );
                        perm_row(
                            ui,
                            &mut self.progress.permissions.auto_allow_apps_url,
                            "Ouverture d'applications & Liens web",
                            false,
                        );
                        perm_row(
                            ui,
                            &mut self.progress.permissions.auto_allow_shell,
                            "Commandes Shell libres (Sensible — confirmation recommandée)",
                            true,
                        );

                        if changed {
                            self.progress.save();
                        }
                    }
                }
            });

        // =============================================================
        // 5. POIGNÉE DE REDIMENSIONNEMENT (BAS-DROITE & BORDS)
        // =============================================================
        let grip_rect =
            Rect::from_min_size(Pos2::new(w - 26.0, h - 26.0), Vec2::new(26.0, 26.0));
        let grip_resp = ui.allocate_rect(grip_rect, Sense::click_and_drag());
        if grip_resp.hovered() || grip_resp.dragged() {
            ctx.set_cursor_icon(CursorIcon::ResizeNwSe);
        }
        draw_resize_grip(
            ui.painter(),
            win_rect.max,
            grip_resp.hovered() || grip_resp.dragged(),
        );
        if grip_resp.drag_started() {
            ctx.send_viewport_cmd(ViewportCommand::BeginResize(ResizeDirection::SouthEast));
        }
        if grip_resp.dragged() {
            let delta = grip_resp.drag_delta();
            if delta.length_sq() > 0.1 {
                self.dojo_window_size.x = (self.dojo_window_size.x + delta.x).max(MIN_DOJO_WIDTH);
                self.dojo_window_size.y = (self.dojo_window_size.y + delta.y).max(MIN_DOJO_HEIGHT);
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(self.dojo_window_size));
            }
        }

        if should_toggle_size {
            self.toggle_dojo_large_size(ctx);
        }
        if should_close_dojo {
            self.close_dojo_window(ctx);
        }
    }
}

impl eframe::App for KazePetApp {
    /// FOND 100 % TRANSPARENT en mode Compagnon : élimine définitivement le carré noir derrière Kaze !
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(quit_instant) = self.quit_at {
            if Instant::now() >= quit_instant {
                ctx.send_viewport_cmd(ViewportCommand::Close);
                return;
            }
        }

        if !self.kwin_hints_applied {
            self.kwin_hints_applied = true;
            ctx.send_viewport_cmd(ViewportCommand::WindowLevel(WindowLevel::AlwaysOnTop));
            kde::apply_kwin_window_hints(WINDOW_TITLE);
            self.last_topmost_refresh = Instant::now();
        } else if self.last_topmost_refresh.elapsed() >= Duration::from_secs(4) {
            ctx.send_viewport_cmd(ViewportCommand::WindowLevel(WindowLevel::AlwaysOnTop));
            kde::enforce_x11_ewmh_top_and_taskbar(WINDOW_TITLE);
            self.last_topmost_refresh = Instant::now();
        }

        if let Some((_, spawn_time)) = self.bubble_feedback {
            if spawn_time.elapsed() > Duration::from_secs(3) {
                self.bubble_feedback = None;
            }
        }
        self.xp_popups
            .retain(|p| p.spawn_time.elapsed().as_secs_f32() < 2.8);

        if let Ok(mut st) = awareness::global_self_state().lock() {
            st.mood = self.animator.current_mood;
            st.roaming = self.roaming_enabled;
            st.meditating = self.in_zazen;
            st.pos_x = self.window_pos.x;
            st.pos_y = self.window_pos.y;
            st.active_model = self.active_model.clone();
        }

        // Expiration minuteurs / Pomodoros
        let expired_timers = awareness::global_timers()
            .lock()
            .map(|mut reg| reg.poll_expired())
            .unwrap_or_default();

        for exp in expired_timers {
            self.bubble_open = true;
            self.animator
                .set_mood(PetMood::Jump, Some(Duration::from_secs(5)));
            let title = if exp.is_pomodoro {
                self.progress.pomodoros_done += 1;
                self.progress.daily_actions += 1;
                self.grant_xp(XpPath::Bushido, 30, "Pomodoro accompli");
                "Pomodoro Zen terminé !"
            } else {
                self.progress.timers_done += 1;
                self.progress.daily_actions += 1;
                self.grant_xp(XpPath::Bushido, 6, "Minuteur accompli");
                "Minuteur écoulé !"
            };
            let msg = format!("{}\n« {} » vient de s'écouler.", title, exp.label);
            let _ = kde::send_kde_notification(title, &exp.label);
            self.set_bubble_message(msg, true);
        }

        // Réception des événements asynchrones
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                WorkerEvent::PlanReady(res) => {
                    self.is_loading = false;
                    match res {
                        Ok(AgentTurnOutcome::DirectReply { mood, display_text }) => {
                            self.animator.set_mood(mood, Some(Duration::from_secs(4)));
                            self.set_bubble_message(display_text.clone(), true);
                            self.history.push(ChatMessage {
                                role: "assistant".into(),
                                content: display_text,
                            });
                        }
                        Ok(AgentTurnOutcome::NeedsConfirmation(pending)) => {
                            if self
                                .progress
                                .permissions
                                .is_task_auto_approved(&pending.task.action)
                            {
                                self.confirm_pending_action_silently(ctx, pending);
                            } else {
                                self.request_user_confirmation(pending);
                            }
                        }
                        Err(err) => {
                            self.animator.set_mood(PetMood::Idle, None);
                            self.set_bubble_message(format!("Erreur : {}", err), false);
                        }
                    }
                    self.focus_input_next = true;
                }
                WorkerEvent::DecisionDone(res) => {
                    self.is_loading = false;
                    match res {
                        Ok(final_res) => {
                            self.animator
                                .set_mood(final_res.mood, Some(Duration::from_secs(4)));
                            self.set_bubble_message(final_res.display_text.clone(), true);
                            self.history.push(ChatMessage {
                                role: "assistant".into(),
                                content: final_res.display_text,
                            });
                        }
                        Err(err) => {
                            self.animator.set_mood(PetMood::Idle, None);
                            self.set_bubble_message(format!("Erreur : {}", err), false);
                        }
                    }
                    self.focus_input_next = true;
                }
                WorkerEvent::CommandOutput {
                    text,
                    mood,
                    quit_after,
                } => {
                    self.is_loading = false;
                    self.animator.set_mood(mood, Some(Duration::from_secs(3)));
                    self.set_bubble_message(text, true);
                    if quit_after {
                        self.quit_at = Some(Instant::now() + Duration::from_millis(850));
                    } else {
                        self.focus_input_next = true;
                    }
                }
                WorkerEvent::ModelsLoaded(models) => {
                    if !models.is_empty() {
                        if let Some(sensei) =
                            models.iter().find(|m| m.starts_with("kaze-sensei"))
                        {
                            self.active_model = sensei.clone();
                        } else if !models.contains(&self.active_model) {
                            if let Some(qwen) = models.iter().find(|m| m.starts_with("qwen2.5")) {
                                self.active_model = qwen.clone();
                            } else {
                                self.active_model = models[0].clone();
                            }
                        }
                        self.available_models = models;
                    }
                }
                WorkerEvent::QuizReady(res) => {
                    self.quiz.loading = false;
                    match res {
                        Ok(q) => {
                            self.quiz.question = Some(q);
                            self.animator
                                .set_mood(PetMood::Special, Some(Duration::from_secs(2)));
                        }
                        Err(e) => {
                            self.quiz.error = Some(e);
                            self.animator.set_mood(PetMood::Idle, None);
                        }
                    }
                }
                WorkerEvent::SenseiForged(res) => {
                    self.forging_model = false;
                    match res {
                        Ok(msg) => {
                            self.active_model = ollama::SENSEI_MODEL_NAME.to_string();
                            self.animator
                                .set_mood(PetMood::Jump, Some(Duration::from_secs(3)));
                            self.grant_xp(XpPath::Bushido, 20, "Forge du modèle Sensei");
                            self.set_bubble_message(msg, true);
                        }
                        Err(err) => {
                            self.animator.set_mood(PetMood::Idle, None);
                            self.set_bubble_message(format!("Erreur de forge : {}", err), true);
                        }
                    }
                }
            }
        }

        // Impulsion d'autonomie périodique du Sensei (toutes les 4 minutes d'inactivité)
        if self.progress.sensei_autonomy
            && !self.is_loading
            && !self.in_zazen
            && self.pending_confirmation.is_none()
            && self.last_autonomy_pulse.elapsed() >= Duration::from_secs(240)
        {
            self.trigger_sensei_initiative(ctx);
        }

        // Raccourcis clavier globaux
        if self.pending_confirmation.is_some() && !self.is_loading {
            if ctx.input(|i| i.key_pressed(Key::Escape)) {
                self.reject_pending_action(ctx);
            } else if ctx.input(|i| i.key_pressed(Key::Enter)) && self.input_text.trim().is_empty()
            {
                self.confirm_pending_action(ctx);
            }
        } else if ctx.input(|i| i.key_pressed(Key::Escape)) {
            if self.show_control_panel {
                self.close_dojo_window(ctx);
            } else if self.in_zazen {
                self.leave_zazen(ctx);
            } else if self.bubble_open {
                self.bubble_open = false;
            }
        } else if !self.bubble_open
            && !self.show_control_panel
            && !self.in_zazen
            && ctx.input(|i| i.key_pressed(Key::Enter) || i.key_pressed(Key::Space))
        {
            self.bubble_open = true;
            self.focus_input_next = true;
        }

        // Animation fluide d'ouverture et de hauteur de la bulle
        let show_bubble_now = self.bubble_open && !self.in_zazen && !self.show_control_panel;
        let target_anim = if show_bubble_now { 1.0 } else { 0.0 };
        self.bubble_anim += (target_anim - self.bubble_anim) * 0.28;
        if (self.bubble_anim - target_anim).abs() < 0.01 {
            self.bubble_anim = target_anim;
        }

        let line_est = (self.full_text.chars().count() / 42)
            .max(self.full_text.lines().count())
            .clamp(2, 7) as f32;
        let target_h = match self.bubble_view_mode {
            BubbleViewMode::Echange => 245.0,
            BubbleViewMode::Zen => {
                if self.pending_confirmation.is_some() {
                    235.0
                } else {
                    let prompt_extra = if self.last_user_prompt.is_some() { 18.0 } else { 0.0 };
                    (118.0 + line_est * 16.0 + prompt_extra).clamp(158.0, 245.0)
                }
            }
        };
        self.bubble_h_anim += (target_h - self.bubble_h_anim) * 0.25;

        // Machine à écrire
        let total_chars = self.full_text.chars().count();
        if self.revealed_chars < total_chars {
            let now = Instant::now();
            let elapsed_ms = now.duration_since(self.last_typewriter_tick).as_millis() as usize;
            if elapsed_ms >= 10 {
                let step = (elapsed_ms / 10).max(3);
                self.revealed_chars = (self.revealed_chars + step).min(total_chars);
                self.last_typewriter_tick = now;
            }
        }

        self.update_roaming(ctx);

        if let Some(outer_rect) = ctx.input(|i| i.viewport().outer_rect) {
            self.window_pos = outer_rect.min;
        }

        let elapsed_secs = self.start_time.elapsed().as_secs_f32();
        let mood_accent = ZenPalette::mood_accent(self.animator.current_mood);
        let global_lvl = self.progress.global_level();

        let first_timer = awareness::global_timers()
            .lock()
            .ok()
            .and_then(|reg| reg.active.first().cloned());

        let panel_frame = Frame::NONE.fill(Color32::TRANSPARENT);
        egui::CentralPanel::default().frame(panel_frame).show(ctx, |ui| {
            // =============================================================
            // MODE A : GRANDE FENÊTRE REDIMENSIONNABLE DU DOJO (background_dojo.jpeg)
            // =============================================================
            if self.show_control_panel {
                self.draw_dojo_sanctuary_window(ctx, ui);
            } else {
                // =============================================================
                // MODE B : COMPAGNON FLOTTANT TRANSPARENT SUR LE BUREAU
                // =============================================================
                let cx = PET_WIN_WIDTH / 2.0;
                let island_bob = if self.in_zazen {
                    (elapsed_secs * 1.4).sin() * 3.5
                } else {
                    (elapsed_secs * 2.1).sin() * 2.2
                };

                let island_y = 392.0 + island_bob;
                let hat_top_y = (island_y - 84.0).round();

                // 1. Samouraï Kaze sur son îlot flottant évolutif
                let pet_rect = Rect::from_center_size(
                    Pos2::new(cx, island_y - 36.0),
                    Vec2::new(148.0, 112.0),
                );
                let pet_resp = ui.allocate_rect(pet_rect, Sense::click_and_drag());

                if self.in_zazen {
                    let (phase, breath_ratio, _) =
                        BreathPhase::compute(self.meditation_started_at.elapsed().as_secs_f32());
                    let center_halo = Pos2::new(cx, island_y - 40.0);
                    draw_zazen_meditation_halo(
                        ui.painter(),
                        center_halo,
                        elapsed_secs,
                        phase,
                        breath_ratio,
                    );
                    draw_pixel_floating_island(
                        ui.painter(),
                        Pos2::new(cx, island_y),
                        elapsed_secs,
                        phase.color(),
                        global_lvl.level,
                    );
                } else {
                    self.particles.update_and_draw(
                        ui.painter(),
                        Pos2::new(cx, island_y - 40.0),
                        95.0,
                        elapsed_secs,
                    );
                    draw_pixel_floating_island(
                        ui.painter(),
                        Pos2::new(cx, island_y),
                        elapsed_secs,
                        mood_accent,
                        global_lvl.level,
                    );
                }

                let tex_id = self.animator.current_texture().id();
                let sprite_rect = Rect::from_center_size(
                    Pos2::new(cx + 6.0, island_y - 42.0),
                    Vec2::new(124.0, 88.0),
                );
                ui.painter().image(
                    tex_id,
                    sprite_rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    Color32::WHITE,
                );

                if pet_resp.double_clicked() || pet_resp.secondary_clicked() {
                    self.bubble_open = !self.bubble_open;
                    if self.bubble_open {
                        self.focus_input_next = true;
                    }
                } else if pet_resp.clicked() {
                    let next_mood = match self.animator.current_mood {
                        PetMood::Idle => PetMood::Attack,
                        PetMood::Attack => PetMood::Jump,
                        _ => PetMood::Special,
                    };
                    self.animator
                        .set_mood(next_mood, Some(Duration::from_millis(1100)));
                    if self.last_pat_xp.elapsed() >= Duration::from_secs(45) {
                        self.last_pat_xp = Instant::now();
                        self.grant_xp(XpPath::Bushido, 1, "Salut au sabre");
                    }
                }

                if pet_resp.drag_started() {
                    ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                }

                // 2. Talisman d'EXP, Niveau, Titre & Gemmes d'Harmonie sous l'îlot
                let (rank_title, _) = rank_for(global_lvl.level);
                let short_title = self.progress.short_display_title();
                let hud_rect =
                    Rect::from_center_size(Pos2::new(cx, 434.0), Vec2::new(264.0, 32.0));
                let hud_resp = ui.allocate_rect(hud_rect, Sense::click());
                draw_pixel_plate(
                    ui.painter(),
                    hud_rect,
                    ZenPalette::INDIGO_PILL,
                    ZenPalette::SUMI_INK,
                    Some(ZenPalette::GOLD_LEAF),
                );

                let hud_label = format!("Nv.{} • {}", global_lvl.level, short_title);
                draw_outlined_text(
                    ui.painter(),
                    Pos2::new(hud_rect.min.x + 10.0, hud_rect.min.y + 10.0),
                    Align2::LEFT_CENTER,
                    &hud_label,
                    10.5,
                    ZenPalette::GOLD_LEAF,
                );

                let xp_txt = format!("{}%", global_lvl.percent());
                draw_outlined_text(
                    ui.painter(),
                    Pos2::new(hud_rect.max.x - 10.0, hud_rect.min.y + 10.0),
                    Align2::RIGHT_CENTER,
                    &xp_txt,
                    10.0,
                    ZenPalette::TEXT_LIGHT,
                );

                let bar_rect = Rect::from_min_size(
                    Pos2::new(hud_rect.min.x + 10.0, hud_rect.min.y + 19.0),
                    Vec2::new(204.0, 7.0),
                );
                draw_pixel_bar(
                    ui.painter(),
                    bar_rect,
                    global_lvl.ratio(),
                    ZenPalette::GOLD_LEAF,
                );

                let gem_base_x = hud_rect.max.x - 42.0;
                let gem_y = hud_rect.min.y + 19.0;
                let gems = [
                    (self.progress.daily_breaths >= 3, ZenPalette::JADE_LIGHT),
                    (self.progress.daily_quizzes >= 1, ZenPalette::GOLD_LEAF),
                    (self.progress.daily_actions >= 1, ZenPalette::SAKURA),
                ];
                for (i, (done, col)) in gems.iter().enumerate() {
                    let gx = gem_base_x + i as f32 * 11.0;
                    let c = if *done {
                        *col
                    } else {
                        Color32::from_rgb(45, 50, 66)
                    };
                    fill_px(ui.painter(), gx, gem_y, 7.0, 7.0, ZenPalette::SUMI_INK);
                    fill_px(ui.painter(), gx + 1.0, gem_y + 1.0, 5.0, 5.0, c);
                }

                if hud_resp.clicked() {
                    self.open_dojo_window(ctx, MenuTab::Sanctuaire);
                }
                if hud_resp.hovered() {
                    hud_resp.on_hover_text(format!(
                        "Niveau {} • {}\nRang : {}\nExpérience : {} / {} XP ({}% • Total {} XP)\nHarmonie du Jour : Souffles {}/3 • Éveil {}/1 • Action {}/1\nClique pour ouvrir le Sanctuaire du Dojo",
                        global_lvl.level,
                        self.progress.display_title(),
                        rank_title,
                        global_lvl.into,
                        global_lvl.need,
                        global_lvl.percent(),
                        self.progress.total_xp(),
                        self.progress.daily_breaths.min(3),
                        self.progress.daily_quizzes.min(1),
                        self.progress.daily_actions.min(1),
                    ));
                }

                if let Some(ref t) = first_timer {
                    let rem = t.remaining().as_secs();
                    let timer_text = format!("Sablier {:02}:{:02} • {}", rem / 60, rem % 60, t.label);
                    draw_outlined_text(
                        ui.painter(),
                        Pos2::new(cx, 464.0),
                        Align2::CENTER_CENTER,
                        &timer_text,
                        10.5,
                        ZenPalette::JADE_LIGHT,
                    );
                } else if !self.bubble_open && !self.in_zazen {
                    let quick_rect =
                        Rect::from_center_size(Pos2::new(cx, 466.0), Vec2::new(264.0, 20.0));
                    let mut q_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(quick_rect)
                            .layout(Layout::left_to_right(Align::Center)),
                    );
                    q_ui.set_clip_rect(quick_rect);
                    q_ui.spacing_mut().item_spacing.x = 6.0;
                    if zen_button(&mut q_ui, "Parler", BtnKind::Secondary, Vec2::new(62.0, 18.0))
                        .clicked()
                    {
                        self.bubble_open = true;
                        self.focus_input_next = true;
                    }
                    if zen_button(&mut q_ui, "Dojo", BtnKind::Secondary, Vec2::new(62.0, 18.0))
                        .clicked()
                    {
                        self.open_dojo_window(ctx, MenuTab::Sanctuaire);
                    }
                    if zen_button(&mut q_ui, "Zazen", BtnKind::Secondary, Vec2::new(62.0, 18.0))
                        .clicked()
                    {
                        self.open_dojo_window(ctx, MenuTab::Zazen);
                    }
                    if zen_button(&mut q_ui, "Éveil", BtnKind::Secondary, Vec2::new(60.0, 18.0))
                        .clicked()
                    {
                        self.open_dojo_window(ctx, MenuTab::Eveil);
                    }
                }

                // 3. Plaque Zazen ou Bulle Unique Washi juste au-dessus du chapeau de Kaze
                if self.in_zazen {
                    let plate_w = PET_WIN_WIDTH - 44.0;
                    let plate_h = 158.0;
                    let plate_rect = Rect::from_min_size(
                        Pos2::new((PET_WIN_WIDTH - plate_w) * 0.5, hat_top_y - plate_h - 4.0),
                        Vec2::new(plate_w, plate_h),
                    );
                    draw_pixel_plate(
                        ui.painter(),
                        plate_rect,
                        ZenPalette::INDIGO_CARD,
                        ZenPalette::JADE_LIGHT,
                        Some(ZenPalette::JADE_LIGHT),
                    );

                    let inner = plate_rect.shrink2(Vec2::new(14.0, 10.0));
                    let mut zazen_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(inner)
                            .layout(Layout::top_down(Align::Min)),
                    );
                    zazen_ui.set_clip_rect(inner);

                    let med_secs = self.meditation_started_at.elapsed().as_secs_f32();
                    let (phase, breath_ratio, cycles) = BreathPhase::compute(med_secs);
                    let haiku = ZEN_HAIKUS[(cycles as usize) % ZEN_HAIKUS.len()];

                    if cycles > self.last_breath_count {
                        self.last_breath_count = cycles;
                        self.progress.total_breaths += 1;
                        self.progress.daily_breaths += 1;
                        self.grant_xp(XpPath::Zazen, 2, "Souffle Zen");
                    }

                    let (phase_txt, phase_col) = match phase {
                        BreathPhase::Inhale => ("INSPIRATION LENTE (4s)", ZenPalette::JADE_LIGHT),
                        BreathPhase::Hold => ("RÉTENTION SEREINE (3s)", ZenPalette::GOLD_LEAF),
                        BreathPhase::Exhale => ("EXPIRATION PROFONDE (5s)", ZenPalette::SAKURA),
                    };

                    zazen_ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("MÉDITATION ZAZEN")
                                .size(11.0)
                                .color(ZenPalette::JADE_LIGHT),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new(phase_txt).size(11.0).color(phase_col));
                        });
                    });
                    zazen_ui.add_space(6.0);

                    let (bar_rect, _) = zazen_ui
                        .allocate_exact_size(Vec2::new(inner.width(), 8.0), Sense::hover());
                    draw_pixel_bar(ui.painter(), bar_rect, breath_ratio, phase.color());
                    zazen_ui.add_space(6.0);

                    let mins = (med_secs / 60.0).floor() as u32;
                    let secs = (med_secs % 60.0).floor() as u32;
                    zazen_ui.label(
                        RichText::new(format!(
                            "{} souffles (+{} XP) • Durée {:02}:{:02}",
                            cycles,
                            cycles * 2,
                            mins,
                            secs
                        ))
                        .size(11.5)
                        .color(ZenPalette::WASHI_BG),
                    );
                    zazen_ui.add_space(2.0);
                    zazen_ui.label(RichText::new(haiku).size(10.5).color(ZenPalette::TEXT_DIM));
                    zazen_ui.add_space(8.0);

                    zazen_ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        if zen_button(
                            ui,
                            "Sortir de Zazen",
                            BtnKind::Paper,
                            Vec2::new(124.0, 22.0),
                        )
                        .clicked()
                        {
                            self.leave_zazen(ctx);
                        }
                        let corner_lbl = if self.meditation_in_corner {
                            "Recentrer"
                        } else {
                            "Coin écran"
                        };
                        if zen_button(ui, corner_lbl, BtnKind::Secondary, Vec2::new(96.0, 22.0))
                            .clicked()
                        {
                            self.toggle_meditation_corner(ctx);
                        }
                    });
                } else if self.bubble_anim > 0.02 {
                    let anim = self.bubble_anim;
                    let bubble_w = PET_WIN_WIDTH - 24.0;
                    let bubble_h = (self.bubble_h_anim * anim).max(64.0);
                    let bubble_top_y = (hat_top_y - bubble_h).max(6.0);
                    let bubble_rect = Rect::from_min_size(
                        Pos2::new((PET_WIN_WIDTH - bubble_w) * 0.5, bubble_top_y),
                        Vec2::new(bubble_w, bubble_h),
                    );
                    draw_washi_speech_bubble(ui.painter(), bubble_rect, mood_accent, anim);

                    if anim > 0.65 {
                        let box_rect = Rect::from_min_size(
                            bubble_rect.min,
                            Vec2::new(bubble_rect.width(), bubble_rect.height() - 15.0),
                        );
                        let content_rect = box_rect.shrink2(Vec2::new(14.0, 8.0));
                        let mut bubble_ui = ui.new_child(
                            egui::UiBuilder::new()
                                .max_rect(content_rect)
                                .layout(Layout::top_down(Align::Min)),
                        );
                        bubble_ui.set_clip_rect(content_rect);

                        let header_res = bubble_ui.horizontal(|ui| {
                            ui.add_space(18.0);
                            let title = if self.pending_confirmation.is_some() {
                                "CONFIRMATION"
                            } else {
                                self.animator.current_mood.label()
                            };
                            ui.label(
                                RichText::new(format!("KAZE • {}", title))
                                    .size(11.0)
                                    .color(ZenPalette::VERMILION),
                            );

                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.spacing_mut().item_spacing.x = 3.0;
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new("x")
                                                .size(12.0)
                                                .color(ZenPalette::SUMI_MUTED),
                                        )
                                        .frame(false),
                                    )
                                    .clicked()
                                {
                                    self.bubble_open = false;
                                }
                                if zen_button(ui, "Dojo", BtnKind::Paper, Vec2::new(40.0, 18.0))
                                    .clicked()
                                {
                                    self.open_dojo_window(ctx, MenuTab::Sanctuaire);
                                }
                                if let Some(ref md_path) = self.last_exported_md.clone() {
                                    if zen_button(
                                        ui,
                                        "Ouvrir",
                                        BtnKind::Paper,
                                        Vec2::new(42.0, 18.0),
                                    )
                                    .clicked()
                                    {
                                        tasks::open_file_external(md_path);
                                    }
                                }
                                if zen_button(ui, ".md", BtnKind::Paper, Vec2::new(30.0, 18.0))
                                    .clicked()
                                {
                                    self.export_current_response_md("");
                                }
                                if zen_button(ui, "Copier", BtnKind::Paper, Vec2::new(44.0, 18.0))
                                    .clicked()
                                {
                                    self.copy_current_response();
                                }
                                let (mode_lbl, mode_kind) = match self.bubble_view_mode {
                                    BubbleViewMode::Zen => {
                                        ("Zen", BtnKind::Selected(ZenPalette::VERMILION))
                                    }
                                    BubbleViewMode::Echange => {
                                        ("Échange", BtnKind::Selected(ZenPalette::CYAN_SPIRIT))
                                    }
                                };
                                if zen_button(ui, mode_lbl, mode_kind, Vec2::new(50.0, 18.0))
                                    .clicked()
                                {
                                    self.bubble_view_mode = match self.bubble_view_mode {
                                        BubbleViewMode::Zen => BubbleViewMode::Echange,
                                        BubbleViewMode::Echange => BubbleViewMode::Zen,
                                    };
                                }
                            });
                        });
                        if header_res.response.drag_started() {
                            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                        }
                        bubble_ui.add_space(3.0);

                        let body_h = (content_rect.height() - 62.0).max(42.0);

                        match self.bubble_view_mode {
                            BubbleViewMode::Zen => {
                                if let Some(ref prompt) = self.last_user_prompt {
                                    bubble_ui.label(
                                        RichText::new(format!("« {} »", prompt))
                                            .size(10.5)
                                            .color(ZenPalette::SUMI_MUTED),
                                    );
                                }

                                let displayed: String =
                                    self.full_text.chars().take(self.revealed_chars).collect();
                                let scroll_h = if self.pending_confirmation.is_some() {
                                    (body_h - 36.0).max(32.0)
                                } else {
                                    (body_h
                                        - if self.last_user_prompt.is_some() {
                                            16.0
                                        } else {
                                            0.0
                                        })
                                    .max(36.0)
                                };

                                let _ = ScrollArea::vertical()
                                    .id_salt("zen_bubble_scroll")
                                    .max_height(scroll_h)
                                    .auto_shrink([false, false])
                                    .show(&mut bubble_ui, |ui| {
                                        let lbl = ui.add(
                                            egui::Label::new(
                                                RichText::new(&displayed)
                                                    .size(12.5)
                                                    .color(ZenPalette::SUMI_INK),
                                            )
                                            .sense(Sense::click()),
                                        );
                                        if lbl.clicked() && self.revealed_chars < total_chars {
                                            self.revealed_chars = total_chars;
                                        }
                                    });

                                if self.pending_confirmation.is_some() {
                                    bubble_ui.add_space(2.0);
                                    bubble_ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 6.0;
                                        if zen_button(
                                            ui,
                                            "Accepter (Entrée)",
                                            BtnKind::Success,
                                            Vec2::new(155.0, 21.0),
                                        )
                                        .clicked()
                                        {
                                            self.confirm_pending_action(ctx);
                                        }
                                        if zen_button(
                                            ui,
                                            "Refuser (Échap)",
                                            BtnKind::Danger,
                                            Vec2::new(155.0, 21.0),
                                        )
                                        .clicked()
                                        {
                                            self.reject_pending_action(ctx);
                                        }
                                    });
                                }
                            }

                            BubbleViewMode::Echange => {
                                let _ = ScrollArea::vertical()
                                    .id_salt("echange_bubble_scroll")
                                    .max_height(body_h)
                                    .auto_shrink([false, false])
                                    .stick_to_bottom(true)
                                    .show(&mut bubble_ui, |ui| {
                                        if self.history.is_empty() {
                                            ui.label(
                                                RichText::new(&self.full_text)
                                                    .size(12.0)
                                                    .color(ZenPalette::SUMI_INK),
                                            );
                                        } else {
                                            for msg in &self.history {
                                                if msg.role == "user" {
                                                    ui.horizontal_wrapped(|ui| {
                                                        ui.label(
                                                            RichText::new("Vous : ")
                                                                .size(10.5)
                                                                .color(ZenPalette::SUMI_MUTED),
                                                        );
                                                        ui.label(
                                                            RichText::new(&msg.content)
                                                                .size(11.5)
                                                                .color(ZenPalette::SUMI_INK),
                                                        );
                                                    });
                                                } else {
                                                    ui.horizontal_wrapped(|ui| {
                                                        ui.label(
                                                            RichText::new("Kaze : ")
                                                                .size(10.5)
                                                                .color(ZenPalette::VERMILION),
                                                        );
                                                        ui.label(
                                                            RichText::new(&msg.content)
                                                                .size(11.5)
                                                                .color(ZenPalette::SUMI_INK),
                                                        );
                                                    });
                                                }
                                                ui.add_space(2.0);
                                            }
                                        }

                                        if self.is_loading {
                                            ui.label(
                                                RichText::new("Kaze médite sur ta demande…")
                                                    .size(11.0)
                                                    .color(ZenPalette::VERMILION),
                                            );
                                        }

                                        if self.pending_confirmation.is_some() {
                                            ui.add_space(2.0);
                                            ui.horizontal(|ui| {
                                                ui.spacing_mut().item_spacing.x = 6.0;
                                                if zen_button(
                                                    ui,
                                                    "Accepter",
                                                    BtnKind::Success,
                                                    Vec2::new(110.0, 20.0),
                                                )
                                                .clicked()
                                                {
                                                    self.confirm_pending_action(ctx);
                                                }
                                                if zen_button(
                                                    ui,
                                                    "Refuser",
                                                    BtnKind::Danger,
                                                    Vec2::new(110.0, 20.0),
                                                )
                                                .clicked()
                                                {
                                                    self.reject_pending_action(ctx);
                                                }
                                            });
                                        }
                                    });
                            }
                        }

                        if let Some((ref msg, _)) = self.bubble_feedback {
                            bubble_ui.label(RichText::new(msg).size(10.0).color(ZenPalette::JADE));
                        }

                        // Pied de la bulle : Saisie sur Parchemin Washi
                        let input_row_rect = Rect::from_min_size(
                            Pos2::new(content_rect.min.x, content_rect.max.y - 26.0),
                            Vec2::new(content_rect.width(), 24.0),
                        );
                        let box_w = input_row_rect.width() - 34.0;
                        let text_box_rect =
                            Rect::from_min_size(input_row_rect.min, Vec2::new(box_w, 24.0));
                        draw_washi_input_box(ui.painter(), text_box_rect, true);

                        let mut in_ui = ui.new_child(
                            egui::UiBuilder::new()
                                .max_rect(input_row_rect)
                                .layout(Layout::left_to_right(Align::Center)),
                        );
                        in_ui.add_space(6.0);
                        in_ui.label(RichText::new("›").size(14.0).color(ZenPalette::VERMILION));

                        let hint = if self.pending_confirmation.is_some() {
                            "Tape 'oui' ou 'non'…"
                        } else {
                            "Parle à Kaze (/menu, /zazen, /eveil)…"
                        };
                        let edit = egui::TextEdit::singleline(&mut self.input_text)
                            .frame(false)
                            .hint_text(
                                RichText::new(hint)
                                    .size(11.5)
                                    .color(ZenPalette::SUMI_MUTED),
                            )
                            .font(FontId::new(12.0, FontFamily::Proportional))
                            .desired_width(box_w - 24.0)
                            .text_color(ZenPalette::SUMI_INK);

                        let edit_resp = in_ui.add_enabled(!self.is_loading, edit);
                        if self.focus_input_next {
                            edit_resp.request_focus();
                            self.focus_input_next = false;
                        }

                        if edit_resp.has_focus()
                            && in_ui.input(|i| i.key_pressed(Key::ArrowUp))
                            && self.input_text.is_empty()
                        {
                            if let Some(ref last) = self.last_user_prompt {
                                self.input_text = last.clone();
                            }
                        }

                        let enter_pressed =
                            edit_resp.lost_focus() && in_ui.input(|i| i.key_pressed(Key::Enter));

                        in_ui.add_space(4.0);
                        let send_clicked = zen_button(
                            &mut in_ui,
                            if self.is_loading { "…" } else { ">" },
                            BtnKind::Primary,
                            Vec2::new(28.0, 22.0),
                        )
                        .clicked();

                        if (enter_pressed || send_clicked) && !self.is_loading {
                            let msg = std::mem::take(&mut self.input_text);
                            self.submit_input(ctx, msg);
                        }
                    }
                }
            }

            // Popups d'XP flottants (visibles sur le Bureau comme dans le Dojo)
            for p in &self.xp_popups {
                let age = p.spawn_time.elapsed().as_secs_f32();
                let y_offset = age * 26.0;
                let alpha = ((1.0 - (age / 2.8)).clamp(0.0, 1.0) * 255.0) as u8;
                let col = Color32::from_rgba_unmultiplied(
                    p.path.color().r(),
                    p.path.color().g(),
                    p.path.color().b(),
                    alpha,
                );
                draw_outlined_text(
                    ui.painter(),
                    Pos2::new(p.pos.x, p.pos.y - y_offset),
                    Align2::CENTER_CENTER,
                    &p.text,
                    12.5,
                    col,
                );
            }
        });

        ctx.request_repaint_after(Duration::from_millis(33));
    }
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("Kaze (Vent) - Sensei Samouraï & Agent Autonome Desktop");
        println!();
        println!("USAGE:");
        println!("    ai-pet [OPTIONS]");
        println!();
        println!("OPTIONS:");
        println!("    -h, --help               Affiche cette aide");
        println!("    --forge [BASE_MODEL]     Forge (fine-tune) le modèle Ollama `kaze-sensei:latest` via Modelfile.sensei");
        println!("    --cli [PROMPT]           Exécute une requête en mode ligne de commande sans interface graphique");
        println!("    --yes, -y                Auto-accepte les actions demandées en mode CLI");
        println!("    --no, -n                 Refuse les actions système demandées en mode CLI");
        println!("    --wayland                Force le backend Wayland sous Linux (par défaut X11)");
        println!();
        return Ok(());
    }

    if let Some(pos) = args.iter().position(|a| a == "--forge" || a == "--finetune") {
        let base_model = args
            .get(pos + 1)
            .map(|s| s.as_str())
            .unwrap_or(DEFAULT_MODEL);
        println!(
            "Forge du modèle spécialisé `{}` depuis `{}` (Modelfile.sensei + ROLE.md)…",
            ollama::SENSEI_MODEL_NAME,
            base_model
        );
        match ollama::forge_sensei_model(base_model) {
            Ok(msg) => {
                println!("{}", msg);
                return Ok(());
            }
            Err(err) => {
                eprintln!("Erreur lors de la forge : {}", err);
                std::process::exit(1);
            }
        }
    }

    if let Some(pos) = args.iter().position(|a| a == "--cli") {
        let prompt = args
            .get(pos + 1)
            .map(|s| s.as_str())
            .unwrap_or("Salut Kaze ! Liste les fichiers du dossier courant.");
        let auto_yes = args.iter().any(|a| a == "--yes" || a == "-y");
        let auto_no = args.iter().any(|a| a == "--no" || a == "-n");

        println!("Interrogation de Kaze ({}) : « {} »", DEFAULT_MODEL, prompt);
        match ollama::plan_pet_action(DEFAULT_MODEL, &[], prompt) {
            Ok(AgentTurnOutcome::DirectReply { mood, display_text }) => {
                println!("Posture : {:?} ({})", mood, mood.label());
                println!("Réponse directe :\n{}", display_text);
            }
            Ok(AgentTurnOutcome::NeedsConfirmation(pending)) => {
                println!("{}\n", pending.format_confirmation_prompt());
                let accepted = if auto_yes {
                    println!("[Auto-Confirmation : OUI (--yes)]");
                    true
                } else if auto_no {
                    println!("[Auto-Confirmation : NON (--no)]");
                    false
                } else {
                    print!("Votre décision [o/N] : ");
                    let _ = io::stdout().flush();
                    let mut line = String::new();
                    let _ = io::stdin().read_line(&mut line);
                    matches!(
                        line.trim().to_lowercase().as_str(),
                        "o" | "oui" | "y" | "yes"
                    )
                };

                let final_res = if accepted {
                    ollama::execute_confirmed_action(DEFAULT_MODEL, &pending)
                } else {
                    ollama::handle_rejected_action(DEFAULT_MODEL, &pending)
                };

                match final_res {
                    Ok(res) => {
                        println!("\nPosture finale : {:?} ({})", res.mood, res.mood.label());
                        println!("{}", res.display_text);
                    }
                    Err(e) => eprintln!("Erreur : {}", e),
                }
            }
            Err(e) => {
                eprintln!("Erreur : {}", e);
                std::process::exit(1);
            }
        }
        return Ok(());
    }

    if cfg!(target_os = "linux")
        && !args.iter().any(|a| a == "--wayland")
        && std::env::var("DISPLAY").is_ok()
    {
        if let Ok(wl) = std::env::var("WAYLAND_DISPLAY") {
            std::env::set_var("KAZE_WAYLAND_DISPLAY", wl);
        }
        std::env::remove_var("WAYLAND_DISPLAY");
        std::env::set_var("WINIT_UNIX_BACKEND", "x11");
    }

    let viewport = ViewportBuilder::default()
        .with_title(WINDOW_TITLE)
        .with_app_id("ai-pet")
        .with_icon(kde::load_window_icon())
        .with_inner_size([PET_WIN_WIDTH, PET_WIN_HEIGHT])
        .with_position([220.0, 180.0])
        .with_decorations(false)
        .with_transparent(true)
        .with_always_on_top()
        .with_resizable(false)
        .with_taskbar(true);

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        WINDOW_TITLE,
        options,
        Box::new(|cc| Ok(Box::new(KazePetApp::new(cc)))),
    )
}
