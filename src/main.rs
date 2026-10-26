mod awareness;
mod kde;
mod ollama;
mod progress;
mod sprites;
mod tasks;
mod zen_ui;

use eframe::egui::{
    self, Align, Align2, Color32, FontFamily, FontId, Frame, Key, Layout, Pos2, Rect, RichText,
    ScrollArea, Sense, Vec2, ViewportBuilder, ViewportCommand, WindowLevel,
};
use ollama::{
    AgentFinalResult, AgentTurnOutcome, ChatMessage, PendingAgentAction, QuizQuestion,
    DEFAULT_MODEL,
};
use progress::{ next_rank, rank_for, Progress, XpPath };
use sprites::{PetMood, SpriteAnimator};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};
use zen_ui::{
    draw_outlined_text, draw_pixel_bar, draw_pixel_floating_island, draw_pixel_plate,
    draw_washi_input_box, draw_washi_speech_bubble, draw_zazen_meditation_halo, fill_px,
    install_pixel_fonts, zen_button, BreathPhase, BtnKind, ZenPalette, ZenParticles,
};

const WINDOW_TITLE: &str = "Kaze AI Pet";
const WIN_WIDTH: f32 = 450.0;
const WIN_HEIGHT: f32 = 490.0;

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

/// Les 6 onglets du Centre de Contrôle (/menu).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuTab {
    Eveil,
    Zazen,
    Actions,
    History,
    Permissions,
    Spirit,
}

impl MenuTab {
    const ALL: [MenuTab; 6] = [
        Self::Eveil,
        Self::Zazen,
        Self::Actions,
        Self::History,
        Self::Permissions,
        Self::Spirit,
    ];

    fn label(&self) -> &'static str {
        match self {
            Self::Eveil => "Éveil",
            Self::Zazen => "Zazen",
            Self::Actions => "Actions",
            Self::History => "Échanges",
            Self::Permissions => "Sécurité",
            Self::Spirit => "Esprit",
        }
    }

    fn accent(&self) -> Color32 {
        match self {
            Self::Eveil => ZenPalette::GOLD_LEAF,
            Self::Zazen => ZenPalette::JADE_LIGHT,
            Self::Actions => ZenPalette::CYAN_SPIRIT,
            Self::History => ZenPalette::SAKURA,
            Self::Permissions => ZenPalette::VERMILION,
            Self::Spirit => ZenPalette::GOLD_LEAF,
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

    // Centre de Contrôle (/menu)
    show_control_panel: bool,
    menu_tab: MenuTab,
    new_memory_fact_input: String,

    // Confirmation d'action d'agent (Human-in-the-Loop)
    pending_confirmation: Option<PendingAgentAction>,

    // Méditation Zazen
    in_zazen: bool,
    meditation_started_at: Instant,
    meditation_in_corner: bool,
    saved_pos_before_meditation: Pos2,
    screen_size: (f32, f32),
    last_breath_count: u32,

    // Dojo d'apprentissage (Éveil)
    quiz: QuizState,

    // Entrée texte intégrée dans la bulle
    input_text: String,
    focus_input_next: bool,
    is_loading: bool,
    active_model: String,
    available_models: Vec<String>,
    history: Vec<ChatMessage>,

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
        let (tx, rx) = mpsc::channel();
        let screen_size = kde::detect_primary_screen_size();

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
            progress.add_xp(XpPath::Bushido, daily_xp);
            progress.save();
            xp_popups.push(XpPopup {
                text: format!("+{} XP Assiduité ({} j)", daily_xp, streak),
                path: XpPath::Bushido,
                spawn_time: Instant::now(),
                pos: Pos2::new(WIN_WIDTH / 2.0, 290.0),
            });
        }

        let welcome = "Bienvenue au sanctuaire de Kaze.\n\
            Double-clique sur moi pour ouvrir ou fermer cette bulle.\n\
            Tape /menu (ou clique sur Menu) pour ouvrir le Centre de Contrôle.";
        let welcome_len = welcome.chars().count();

        Self {
            animator,
            particles: ZenParticles::new(),
            start_time: Instant::now(),
            progress,
            xp_popups,
            last_pat_xp: Instant::now() - Duration::from_secs(60),
            bubble_open: true,
            bubble_anim: 1.0,
            bubble_h_anim: 185.0,
            bubble_view_mode: BubbleViewMode::Zen,
            full_text: welcome.into(),
            revealed_chars: welcome_len,
            last_typewriter_tick: Instant::now(),
            bubble_feedback: None,
            last_exported_md: None,
            last_user_prompt: None,
            last_kaze_reply: Some(welcome.into()),
            show_control_panel: false,
            menu_tab: MenuTab::Eveil,
            new_memory_fact_input: String::new(),
            pending_confirmation: None,
            in_zazen: false,
            meditation_started_at: Instant::now(),
            meditation_in_corner: false,
            saved_pos_before_meditation: Pos2::new(220.0, 180.0),
            screen_size,
            last_breath_count: 0,
            quiz: QuizState::default(),
            input_text: String::new(),
            focus_input_next: true,
            is_loading: false,
            active_model: DEFAULT_MODEL.to_string(),
            available_models: vec![DEFAULT_MODEL.to_string()],
            history: Vec::new(),
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

    /// Octroie de l'XP sur une voie, vérifie l'Harmonie du Jour et affiche une notification flottante.
    fn grant_xp(&mut self, path: XpPath, amount: u64, reason: &str) {
        let old_level = self.progress.global_level().level;
        let old_path_level = self.progress.path_level(path).level;
        self.progress.add_xp(path, amount);

        // Vérifier si l'Harmonie du Jour (3 gestes quotidiens) vient d'être complétée
        let harmony_bonus = if self.progress.check_and_claim_daily_harmony() {
            self.progress.add_xp(XpPath::Zazen, 10);
            self.progress.add_xp(XpPath::Eveil, 10);
            self.progress.add_xp(XpPath::Bushido, 10);
            true
        } else {
            false
        };

        self.progress.save();

        let new_level = self.progress.global_level().level;
        let new_path_level = self.progress.path_level(path).level;

        let popup_text = if !reason.is_empty() {
            format!("+{} XP {} • {}", amount, path.label(), reason)
        } else {
            format!("+{} XP {}", amount, path.label())
        };

        self.xp_popups.push(XpPopup {
            text: popup_text,
            path,
            spawn_time: Instant::now(),
            pos: Pos2::new(WIN_WIDTH / 2.0, 295.0),
        });

        if harmony_bonus {
            self.xp_popups.push(XpPopup {
                text: "+30 XP • Harmonie du Jour accomplie !".into(),
                path: XpPath::Eveil,
                spawn_time: Instant::now(),
                pos: Pos2::new(WIN_WIDTH / 2.0, 270.0),
            });
        }

        if new_level > old_level {
            let (rank, desc) = rank_for(new_level);
            self.animator.set_mood(PetMood::Special, Some(Duration::from_secs(4)));
            self.set_bubble_message(
                format!(
                    "ÉLÉVATION ! Tu atteins le Niveau {} — Rang {} ({}).\nTon îlot sanctuaire s'épanouit.",
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
                self.animator.set_mood(PetMood::Jump, Some(Duration::from_millis(1200)));
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
                self.animator.set_mood(PetMood::Special, Some(Duration::from_secs(2)));
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
        self.in_zazen = true;
        self.show_control_panel = false;
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
            let target_x = (sw - WIN_WIDTH + 60.0).max(20.0);
            let target_y = (sh - WIN_HEIGHT - 36.0).max(20.0);
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
        self.animator.set_mood(PetMood::Jump, Some(Duration::from_secs(3)));

        if self.meditation_in_corner {
            let (sw, sh) = self.screen_size;
            let wake_x = self
                .saved_pos_before_meditation
                .x
                .clamp(20.0, (sw - WIN_WIDTH - 20.0).max(20.0));
            let wake_y = self
                .saved_pos_before_meditation
                .y
                .clamp(20.0, (sh - WIN_HEIGHT - 60.0).max(20.0));
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
                .clamp(20.0, (sw - WIN_WIDTH - 20.0).max(20.0));
            let wake_y = self
                .saved_pos_before_meditation
                .y
                .clamp(20.0, (sh - WIN_HEIGHT - 60.0).max(20.0));
            self.window_pos = Pos2::new(wake_x, wake_y);
            self.meditation_in_corner = false;
        } else {
            self.saved_pos_before_meditation = self.window_pos;
            let target_x = (sw - WIN_WIDTH + 60.0).max(20.0);
            let target_y = (sh - WIN_HEIGHT - 36.0).max(20.0);
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

            self.animator.set_mood(PetMood::Jump, Some(Duration::from_secs(3)));
            self.grant_xp(
                XpPath::Eveil,
                total,
                &format!("Série x{}", self.quiz.streak),
            );
        } else {
            self.quiz.was_correct = Some(false);
            self.quiz.streak = 0;
            self.animator.set_mood(PetMood::Attack, Some(Duration::from_millis(1500)));
            self.grant_xp(XpPath::Eveil, 3, "Persévérance");
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
        self.show_control_panel = false;
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

    fn confirm_pending_action_silently(&mut self, ctx: &egui::Context, pending: PendingAgentAction) {
        self.show_control_panel = false;
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

    fn handle_slash_command(&mut self, ctx: &egui::Context, cmd_line: &str) {
        let mut parts = cmd_line.splitn(2, char::is_whitespace);
        let cmd = parts.next().unwrap_or("").to_lowercase();
        let arg = parts.next().unwrap_or("").trim().to_string();

        match cmd.as_str() {
            "/menu" | "/panel" | "/control" => {
                self.show_control_panel = !self.show_control_panel;
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
                self.bubble_open = false;
            }
            "/balade" | "/roam" => {
                self.roaming_enabled = !self.roaming_enabled;
                let st = if self.roaming_enabled { "activée" } else { "arrêtée" };
                self.set_bubble_message(format!("Balade autonome {}.", st), true);
            }
            "/zazen" | "/meditate" | "/mediter" => {
                let corner = arg.contains("coin") || arg.contains("corner");
                self.enter_zazen(ctx, corner);
            }
            "/eveil" | "/learn" | "/quiz" => {
                self.show_control_panel = true;
                self.menu_tab = MenuTab::Eveil;
                if !arg.is_empty() {
                    self.quiz.topic_input = arg;
                }
                if self.quiz.question.is_none() && !self.quiz.loading {
                    self.request_new_quiz_question(ctx);
                }
            }
            "/xp" | "/niveau" | "/rank" => {
                self.show_control_panel = true;
                self.menu_tab = MenuTab::Spirit;
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
                    self.animator.set_mood(PetMood::Jump, Some(Duration::from_secs(2)));
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
                self.execute_or_confirm_command(ctx, cmd_line, "cancel_timer", &arg, PetMood::Attack);
            }
            "/sys" | "/status" => {
                self.execute_or_confirm_command(ctx, cmd_line, "system_info", "", PetMood::Special);
            }
            "/top" | "/ps" => {
                self.execute_or_confirm_command(ctx, cmd_line, "top_processes", "", PetMood::Attack);
            }
            "/net" | "/ip" => {
                self.execute_or_confirm_command(ctx, cmd_line, "network_info", "", PetMood::Run);
            }
            "/bat" | "/power" => {
                self.execute_or_confirm_command(ctx, cmd_line, "battery_info", "", PetMood::Idle);
            }
            "/vol" => {
                self.execute_or_confirm_command(ctx, cmd_line, "audio_control", &arg, PetMood::Jump);
            }
            "/media" => {
                self.execute_or_confirm_command(ctx, cmd_line, "media_control", &arg, PetMood::Jump);
            }
            "/shot" => {
                self.execute_or_confirm_command(ctx, cmd_line, "screenshot", &arg, PetMood::Attack);
            }
            "/clip" => {
                self.execute_or_confirm_command(ctx, cmd_line, "clipboard_read", "", PetMood::Idle);
            }
            "/copy" => {
                self.execute_or_confirm_command(ctx, cmd_line, "clipboard_write", &arg, PetMood::Jump);
            }
            "/open" => {
                self.execute_or_confirm_command(ctx, cmd_line, "open_app", &arg, PetMood::Run);
            }
            "/ls" => {
                self.execute_or_confirm_command(ctx, cmd_line, "list_files", &arg, PetMood::Run);
            }
            "/aware" => {
                self.animator.set_mood(PetMood::Special, Some(Duration::from_secs(3)));
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
                    • /menu : ouvre le Centre de Contrôle (Éveil, Zazen, Actions, Sécurité, Esprit)\n\
                    • /zen ou /echange : change le mode de la bulle\n\
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
        if !self.roaming_enabled || self.is_loading || self.in_zazen {
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
                    self.animator.set_mood(PetMood::Special, Some(Duration::from_secs(2)));
                }
            }
            let secs = 2 + (now.elapsed().subsec_nanos() % 4);
            self.next_roam_decision = now + Duration::from_secs(secs as u64);
        }

        if self.roam_velocity_x.abs() > 0.01 {
            let (sw, _) = self.screen_size;
            let min_x = 20.0;
            let max_x = (sw - WIN_WIDTH - 20.0).max(min_x);
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
}

impl eframe::App for KazePetApp {
    /// FOND 100 % TRANSPARENT : élimine définitivement le carré noir derrière Kaze !
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
            self.show_control_panel = false;
            self.animator.set_mood(PetMood::Jump, Some(Duration::from_secs(5)));
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
                        if !models.contains(&self.active_model) {
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
            }
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
                self.show_control_panel = false;
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
            let cx = WIN_WIDTH / 2.0;
            let island_bob = if self.in_zazen {
                (elapsed_secs * 1.4).sin() * 3.5
            } else {
                (elapsed_secs * 2.1).sin() * 2.2
            };

            // Point d'ancrage vertical de l'îlot et du chapeau de Kaze
            let island_y = 392.0 + island_bob;
            let hat_top_y = (island_y - 84.0).round();

            // =============================================================
            // 1. ZONE DU SAMOURAÏ & SON ÎLOT SANCTUAIRE ÉVOLUTIF
            // =============================================================
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

            // Interactions sur Kaze :
            // - Double-clic ou clic droit : ouvre / ferme la bulle unique
            // - Clic simple : animation de posture (+1 XP doux avec cooldown)
            // - Glisser : déplace la fenêtre sur le bureau
            if pet_resp.double_clicked() || pet_resp.secondary_clicked() {
                if self.show_control_panel {
                    self.show_control_panel = false;
                    self.bubble_open = true;
                    self.focus_input_next = true;
                } else {
                    self.bubble_open = !self.bubble_open;
                    if self.bubble_open {
                        self.focus_input_next = true;
                    }
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

            // =============================================================
            // 2. TALISMAN DE NIVEAU & GEMMES D'HARMONIE DU JOUR SOUS L'ÎLOT
            // =============================================================
            let (rank_title, _) = rank_for(global_lvl.level);
            let hud_rect = Rect::from_center_size(Pos2::new(cx, 434.0), Vec2::new(226.0, 22.0));
            let hud_resp = ui.allocate_rect(hud_rect, Sense::click());
            draw_pixel_plate(
                ui.painter(),
                hud_rect,
                ZenPalette::INDIGO_PILL,
                ZenPalette::SUMI_INK,
                Some(ZenPalette::GOLD_LEAF),
            );

            let hud_label = format!("Nv.{} {}", global_lvl.level, rank_title);
            draw_outlined_text(
                ui.painter(),
                Pos2::new(hud_rect.min.x + 10.0, hud_rect.center().y + 1.0),
                Align2::LEFT_CENTER,
                &hud_label,
                10.5,
                ZenPalette::GOLD_LEAF,
            );

            let bar_rect = Rect::from_min_size(
                Pos2::new(hud_rect.min.x + 98.0, hud_rect.center().y - 2.0),
                Vec2::new(72.0, 6.0),
            );
            draw_pixel_bar(
                ui.painter(),
                bar_rect,
                global_lvl.ratio(),
                ZenPalette::GOLD_LEAF,
            );

            // 3 petites gemmes de 5x5px pour l'Harmonie du Jour (Zazen, Éveil, Bushidō)
            let gem_base_x = hud_rect.max.x - 42.0;
            let gem_y = hud_rect.center().y - 2.0;
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
                fill_px(ui.painter(), gx, gem_y, 6.0, 6.0, ZenPalette::SUMI_INK);
                fill_px(ui.painter(), gx + 1.0, gem_y + 1.0, 4.0, 4.0, c);
            }

            if hud_resp.clicked() {
                self.show_control_panel = !self.show_control_panel;
                if self.show_control_panel {
                    self.menu_tab = MenuTab::Spirit;
                }
            }
            if hud_resp.hovered() {
                hud_resp.on_hover_text(format!(
                    "Niveau {} ({}) — {} / {} XP\nHarmonie du Jour : Souffles {}/3 • Éveil {}/1 • Action {}/1\nClique pour ouvrir le Centre de Contrôle",
                    global_lvl.level,
                    rank_title,
                    global_lvl.into,
                    global_lvl.need,
                    self.progress.daily_breaths.min(3),
                    self.progress.daily_quizzes.min(1),
                    self.progress.daily_actions.min(1),
                ));
            }

            // Sablier actif ou mini-boutons discrets au survol quand tout est fermé
            if let Some(ref t) = first_timer {
                let rem = t.remaining().as_secs();
                let timer_text = format!("Sablier {:02}:{:02} • {}", rem / 60, rem % 60, t.label);
                draw_outlined_text(
                    ui.painter(),
                    Pos2::new(cx, 460.0),
                    Align2::CENTER_CENTER,
                    &timer_text,
                    10.5,
                    ZenPalette::JADE_LIGHT,
                );
            } else if !self.bubble_open && !self.show_control_panel && !self.in_zazen {
                // Micro-barre d'accès rapide ultra-discrète sous le talisman
                let quick_rect =
                    Rect::from_center_size(Pos2::new(cx, 462.0), Vec2::new(204.0, 20.0));
                let mut q_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(quick_rect)
                        .layout(Layout::left_to_right(Align::Center)),
                );
                q_ui.spacing_mut().item_spacing.x = 6.0;
                if zen_button(&mut q_ui, "Parler", BtnKind::Secondary, Vec2::new(64.0, 18.0))
                    .clicked()
                {
                    self.bubble_open = true;
                    self.focus_input_next = true;
                }
                if zen_button(&mut q_ui, "Zazen", BtnKind::Secondary, Vec2::new(64.0, 18.0))
                    .clicked()
                {
                    self.enter_zazen(ctx, false);
                }
                if zen_button(&mut q_ui, "Menu", BtnKind::Secondary, Vec2::new(64.0, 18.0))
                    .clicked()
                {
                    self.show_control_panel = true;
                }
            }

            // =============================================================
            // 3. ZONE JUSTE AU-DESSUS DE KAZE (ZAZEN / BULLE UNIQUE / MENU)
            // =============================================================
            if self.in_zazen {
                // Plaque Zazen ancrée juste au-dessus du chapeau de Kaze
                let plate_w = WIN_WIDTH - 44.0;
                let plate_h = 158.0;
                let plate_rect = Rect::from_min_size(
                    Pos2::new((WIN_WIDTH - plate_w) * 0.5, hat_top_y - plate_h - 4.0),
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

                let med_secs = self.meditation_started_at.elapsed().as_secs_f32();
                let (phase, breath_ratio, cycles) = BreathPhase::compute(med_secs);
                let haiku = ZEN_HAIKUS[(cycles as usize) % ZEN_HAIKUS.len()];

                if cycles > self.last_breath_count {
                    self.last_breath_count = cycles;
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
            } else if self.show_control_panel {
                // =============================================================
                // CENTRE DE CONTRÔLE (/menu) : VRAIE PETITE FENÊTRE AU-DESSUS DE KAZE
                // =============================================================
                let panel_w = WIN_WIDTH - 16.0;
                let panel_h = 300.0;
                let panel_rect = Rect::from_min_size(
                    Pos2::new((WIN_WIDTH - panel_w) * 0.5, hat_top_y - panel_h - 2.0),
                    Vec2::new(panel_w, panel_h),
                );
                draw_pixel_plate(
                    ui.painter(),
                    panel_rect,
                    ZenPalette::INDIGO_CARD,
                    self.menu_tab.accent(),
                    Some(self.menu_tab.accent()),
                );

                let inner = panel_rect.shrink2(Vec2::new(12.0, 10.0));
                let mut p_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(inner)
                        .layout(Layout::top_down(Align::Min)),
                );

                // Barre de titre de la fenêtre Centre de Contrôle (glissable)
                let title_res = p_ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("KAZE — CENTRE DE CONTRÔLE")
                            .size(11.5)
                            .color(ZenPalette::GOLD_LEAF),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if zen_button(ui, "Fermer x", BtnKind::Danger, Vec2::new(62.0, 18.0))
                            .clicked()
                        {
                            self.show_control_panel = false;
                        }
                        if zen_button(ui, "Bulle", BtnKind::Secondary, Vec2::new(48.0, 18.0))
                            .clicked()
                        {
                            self.show_control_panel = false;
                            self.bubble_open = true;
                            self.focus_input_next = true;
                        }
                    });
                });
                if title_res.response.drag_started() {
                    ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                }
                p_ui.add_space(4.0);

                // Barre des 6 onglets parfaitement calibrée
                p_ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let tab_w = (inner.width() - 20.0) / 6.0;
                    for tab in MenuTab::ALL {
                        let sel = self.menu_tab == tab;
                        let kind = if sel {
                            BtnKind::Selected(tab.accent())
                        } else {
                            BtnKind::Secondary
                        };
                        if zen_button(ui, tab.label(), kind, Vec2::new(tab_w, 22.0)).clicked() {
                            self.menu_tab = tab;
                            if tab == MenuTab::Eveil
                                && self.quiz.question.is_none()
                                && !self.quiz.loading
                            {
                                self.request_new_quiz_question(ctx);
                            }
                        }
                    }
                });
                p_ui.add_space(6.0);

                // Contenu de l'onglet actif
                match self.menu_tab {
                    MenuTab::Eveil => {
                        let p_eveil = self.progress.path_level(XpPath::Eveil);
                        p_ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!(
                                    "Voie de l'Éveil Nv.{} ({} XP) • Série : {} (Record : {})",
                                    p_eveil.level,
                                    self.progress.eveil_xp,
                                    self.quiz.streak,
                                    self.progress.best_streak
                                ))
                                .size(10.5)
                                .color(ZenPalette::GOLD_LEAF),
                            );
                        });
                        p_ui.add_space(3.0);

                        p_ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            let edit = egui::TextEdit::singleline(&mut self.quiz.topic_input)
                                .hint_text("Sujet d'étude...")
                                .desired_width(155.0)
                                .font(FontId::new(11.0, FontFamily::Proportional));
                            ui.add(edit);

                            for &(lbl, topic) in &[
                                ("Zen", "Philosophie Zen & Samouraï"),
                                ("Rust", "Programmation Rust & Systèmes"),
                                ("Japon", "Histoire du Japon féodal"),
                                ("Linux", "Linux & Cybersécurité"),
                            ] {
                                if zen_button(ui, lbl, BtnKind::Secondary, Vec2::new(42.0, 20.0))
                                    .clicked()
                                {
                                    self.quiz.topic_input = topic.to_string();
                                    self.request_new_quiz_question(ctx);
                                }
                            }
                            if zen_button(ui, "Nouveau", BtnKind::Primary, Vec2::new(58.0, 20.0))
                                .clicked()
                            {
                                self.request_new_quiz_question(ctx);
                            }
                        });
                        p_ui.add_space(5.0);

                        if self.quiz.loading {
                            p_ui.add_space(24.0);
                            p_ui.label(
                                RichText::new("Le Sensei prépare une énigme sur ce sujet…")
                                    .size(11.5)
                                    .color(ZenPalette::GOLD_LEAF),
                            );
                        } else if let Some(err) = self.quiz.error.clone() {
                            p_ui.label(
                                RichText::new(format!("Erreur : {}", err))
                                    .size(11.0)
                                    .color(ZenPalette::SAKURA),
                            );
                            if zen_button(
                                &mut p_ui,
                                "Réessayer",
                                BtnKind::Primary,
                                Vec2::new(90.0, 22.0),
                            )
                            .clicked()
                            {
                                self.request_new_quiz_question(ctx);
                            }
                        } else if let Some(q) = self.quiz.question.clone() {
                            p_ui.label(
                                RichText::new(&q.question)
                                    .size(11.5)
                                    .color(ZenPalette::TEXT_LIGHT),
                            );
                            p_ui.add_space(4.0);

                            let answered = self.quiz.selected_option.is_some();
                            let mut chosen_idx = None;
                            let btn_w = (inner.width() - 8.0) * 0.5;

                            for pair in [0usize, 1, 2, 3].chunks(2) {
                                p_ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 6.0;
                                    for &idx in pair {
                                        let opt =
                                            q.options.get(idx).map(|s| s.as_str()).unwrap_or("");
                                        let prefix = match idx {
                                            0 => "A",
                                            1 => "B",
                                            2 => "C",
                                            _ => "D",
                                        };
                                        let label = format!("{}. {}", prefix, opt);
                                        let kind = if let Some(sel) = self.quiz.selected_option {
                                            if idx == q.correct_idx {
                                                BtnKind::Success
                                            } else if idx == sel {
                                                BtnKind::Danger
                                            } else {
                                                BtnKind::Secondary
                                            }
                                        } else {
                                            BtnKind::Secondary
                                        };
                                        if zen_button(ui, &label, kind, Vec2::new(btn_w, 24.0))
                                            .clicked()
                                            && !answered
                                        {
                                            chosen_idx = Some(idx);
                                        }
                                    }
                                });
                                p_ui.add_space(3.0);
                            }

                            if let Some(idx) = chosen_idx {
                                self.answer_quiz(idx);
                            }

                            if let Some(correct) = self.quiz.was_correct {
                                let (tag, col) = if correct {
                                    ("Exact ! (+XP Éveil)", ZenPalette::JADE_LIGHT)
                                } else {
                                    ("Raté ! (+3 XP Persévérance)", ZenPalette::SAKURA)
                                };
                                p_ui.horizontal(|ui| {
                                    ui.label(RichText::new(tag).size(11.0).color(col));
                                    if zen_button(
                                        ui,
                                        "Question suivante >",
                                        BtnKind::Primary,
                                        Vec2::new(125.0, 20.0),
                                    )
                                    .clicked()
                                    {
                                        self.request_new_quiz_question(ctx);
                                    }
                                });
                                p_ui.label(
                                    RichText::new(&q.explanation)
                                        .size(10.5)
                                        .color(ZenPalette::TEXT_DIM),
                                );
                            }
                        } else {
                            p_ui.label(
                                RichText::new("Choisis un sujet et lance une question pour cultiver la Voie de l'Éveil.")
                                    .size(11.0)
                                    .color(ZenPalette::TEXT_DIM),
                            );
                        }
                    }

                    MenuTab::Zazen => {
                        let p_zazen = self.progress.path_level(XpPath::Zazen);
                        p_ui.label(
                            RichText::new(format!(
                                "Voie du Zazen Nv.{} ({} XP) • Total médité : {} min ({} sessions)",
                                p_zazen.level,
                                self.progress.zazen_xp,
                                self.progress.meditation_secs / 60,
                                self.progress.meditation_sessions
                            ))
                            .size(11.0)
                            .color(ZenPalette::JADE_LIGHT),
                        );
                        p_ui.add_space(6.0);
                        p_ui.label(
                            RichText::new(
                                "La méditation Zazen apaise le mental grâce au rythme respiratoire 4-3-5 :\n\
                                • 4s d'inspiration calme • 3s de rétention • 5s d'expiration relâchée.\n\
                                • Chaque cycle complet accorde +2 XP Zazen et fait fleurir votre îlot.",
                            )
                            .size(11.0)
                            .color(ZenPalette::TEXT_LIGHT),
                        );
                        p_ui.add_space(10.0);

                        p_ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            if zen_button(
                                ui,
                                "Méditer ici",
                                BtnKind::Primary,
                                Vec2::new(125.0, 26.0),
                            )
                            .clicked()
                            {
                                self.enter_zazen(ctx, false);
                            }
                            if zen_button(
                                ui,
                                "Méditer dans le coin",
                                BtnKind::Secondary,
                                Vec2::new(145.0, 26.0),
                            )
                            .clicked()
                            {
                                self.enter_zazen(ctx, true);
                            }
                            let roam_lbl = if self.roaming_enabled {
                                "Balade : ON"
                            } else {
                                "Balade : OFF"
                            };
                            if zen_button(ui, roam_lbl, BtnKind::Paper, Vec2::new(95.0, 26.0))
                                .clicked()
                            {
                                self.roaming_enabled = !self.roaming_enabled;
                            }
                        });
                    }

                    MenuTab::Actions => {
                        p_ui.label(
                            RichText::new("Rituels de Concentration & Actions Système")
                                .size(11.0)
                                .color(ZenPalette::CYAN_SPIRIT),
                        );
                        p_ui.add_space(4.0);

                        let groups: &[(&str, &[(&str, &str)])] = &[
                            (
                                "Rituels Zen",
                                &[
                                    ("Pomodoro 25m", "/pomo 25"),
                                    ("Sablier Thé 3m", "/timer 3m Thé"),
                                    ("Pause 5m", "/timer 5m Pause"),
                                    ("Voir Sabliers", "/timers"),
                                ],
                            ),
                            (
                                "Système",
                                &[
                                    ("Bilan OS", "/sys"),
                                    ("Top RAM", "/top"),
                                    ("Réseau & IP", "/net"),
                                    ("Batterie", "/bat"),
                                ],
                            ),
                            (
                                "Audio & Média",
                                &[
                                    ("Volume -10%", "/vol -10"),
                                    ("Volume +10%", "/vol +10"),
                                    ("Muet", "/vol mute"),
                                    ("Play / Pause", "/media toggle"),
                                ],
                            ),
                            (
                                "Bureau",
                                &[
                                    ("Capture écran", "/shot"),
                                    ("Dossier ~", "/ls"),
                                    ("Terminal", "/open terminal"),
                                    ("Presse-papiers", "/clip"),
                                ],
                            ),
                        ];

                        let btn_w = (inner.width() - 18.0) / 4.0;
                        let mut triggered_cmd: Option<String> = None;

                        for (group_title, items) in groups {
                            p_ui.label(
                                RichText::new(*group_title)
                                    .size(10.0)
                                    .color(ZenPalette::GOLD_LEAF),
                            );
                            p_ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 6.0;
                                for (lbl, cmd) in *items {
                                    if zen_button(
                                        ui,
                                        lbl,
                                        BtnKind::Secondary,
                                        Vec2::new(btn_w, 22.0),
                                    )
                                    .clicked()
                                    {
                                        triggered_cmd = Some((*cmd).to_string());
                                    }
                                }
                            });
                            p_ui.add_space(3.0);
                        }

                        if let Some(cmd) = triggered_cmd {
                            self.show_control_panel = false;
                            self.submit_input(ctx, cmd);
                        }
                    }

                    MenuTab::History => {
                        p_ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!(
                                    "Historique ({} messages)",
                                    self.history.len()
                                ))
                                .size(11.0)
                                .color(ZenPalette::SAKURA),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if zen_button(
                                    ui,
                                    "Effacer",
                                    BtnKind::Danger,
                                    Vec2::new(68.0, 20.0),
                                )
                                .clicked()
                                {
                                    self.history.clear();
                                }
                                if zen_button(
                                    ui,
                                    "Exporter .md",
                                    BtnKind::Paper,
                                    Vec2::new(92.0, 20.0),
                                )
                                .clicked()
                                {
                                    self.export_current_response_md("historique_complet");
                                }
                            });
                        });
                        p_ui.add_space(3.0);

                        p_ui.label(
                            RichText::new(kde::get_compact_context())
                                .size(9.5)
                                .color(ZenPalette::TEXT_DIM),
                        );
                        p_ui.add_space(4.0);

                        let _ = ScrollArea::vertical()
                            .id_salt("panel_history_scroll")
                            .max_height(165.0)
                            .stick_to_bottom(true)
                            .show(&mut p_ui, |ui| {
                                if self.history.is_empty() {
                                    ui.label(
                                        RichText::new("Aucun échange enregistré dans cette session.")
                                            .size(11.0)
                                            .color(ZenPalette::TEXT_DIM),
                                    );
                                } else {
                                    for msg in &self.history {
                                        let (who, col) = if msg.role == "user" {
                                            ("Vous", ZenPalette::GOLD_LEAF)
                                        } else {
                                            ("Kaze", ZenPalette::VERMILION)
                                        };
                                        ui.horizontal_wrapped(|ui| {
                                            ui.label(
                                                RichText::new(format!("{} : ", who))
                                                    .size(10.5)
                                                    .color(col),
                                            );
                                            ui.label(
                                                RichText::new(&msg.content)
                                                    .size(10.5)
                                                    .color(ZenPalette::TEXT_LIGHT),
                                            );
                                        });
                                        ui.add_space(2.0);
                                    }
                                }
                            });
                    }

                    MenuTab::Permissions => {
                        p_ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("Exécution Automatique vs Confirmation Humaine")
                                    .size(11.0)
                                    .color(ZenPalette::GOLD_LEAF),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if zen_button(
                                    ui,
                                    "Tout demander",
                                    BtnKind::Secondary,
                                    Vec2::new(92.0, 18.0),
                                )
                                .clicked()
                                {
                                    self.progress.permissions.set_strict_all();
                                    self.progress.save();
                                }
                                if zen_button(
                                    ui,
                                    " Zen (Défaut)",
                                    BtnKind::Paper,
                                    Vec2::new(86.0, 18.0),
                                )
                                .clicked()
                                {
                                    self.progress.permissions.set_zen_balanced();
                                    self.progress.save();
                                }
                            });
                        });
                        p_ui.add_space(4.0);

                        let mut changed = false;
                        let mut perm_row =
                            |ui: &mut egui::Ui, val: &mut bool, label: &str, warn: bool| {
                                ui.horizontal(|ui| {
                                    let (txt, kind) = if *val {
                                        (
                                            "Auto",
                                            if warn {
                                                BtnKind::Danger
                                            } else {
                                                BtnKind::Success
                                            },
                                        )
                                    } else {
                                        ("Demander", BtnKind::Secondary)
                                    };
                                    if zen_button(ui, txt, kind, Vec2::new(74.0, 19.0)).clicked() {
                                        *val = !*val;
                                        changed = true;
                                    }
                                    ui.label(
                                        RichText::new(label)
                                            .size(10.5)
                                            .color(ZenPalette::TEXT_LIGHT),
                                    );
                                });
                            };

                        perm_row(
                            &mut p_ui,
                            &mut self.progress.permissions.auto_allow_system_info,
                            "Bilans Système (CPU, RAM, Réseau, Batterie)",
                            false,
                        );
                        perm_row(
                            &mut p_ui,
                            &mut self.progress.permissions.auto_allow_media_audio,
                            "Contrôle Audio, Média & Captures d'écran",
                            false,
                        );
                        perm_row(
                            &mut p_ui,
                            &mut self.progress.permissions.auto_allow_timers,
                            "Minuteurs & Sessions Pomodoro Zen",
                            false,
                        );
                        perm_row(
                            &mut p_ui,
                            &mut self.progress.permissions.auto_allow_read_notes_mem,
                            "Lecture des Notes & Mémoire long-terme",
                            false,
                        );
                        perm_row(
                            &mut p_ui,
                            &mut self.progress.permissions.auto_allow_read_files,
                            "Exploration & Lecture de fichiers (~)",
                            false,
                        );
                        perm_row(
                            &mut p_ui,
                            &mut self.progress.permissions.auto_allow_apps_url,
                            "Ouverture d'applications & Liens web",
                            false,
                        );
                        perm_row(
                            &mut p_ui,
                            &mut self.progress.permissions.auto_allow_shell,
                            "Commandes Shell libres (Sensible)",
                            true,
                        );

                        if changed {
                            self.progress.save();
                        }
                    }

                    MenuTab::Spirit => {
                        // Les 3 Voies + Harmonie du Jour + Modèle IA + Mémoire
                        let g = self.progress.global_level();
                        let (rk, rk_desc) = rank_for(g.level);
                        let next_str = next_rank(g.level)
                            .map(|(nl, nr)| format!("Prochain rang : {} (Nv.{})", nr, nl))
                            .unwrap_or_else(|| "Rang ultime Satori atteint".into());

                        p_ui.label(
                            RichText::new(format!(
                                "Niveau {} • {} — « {} » ({})",
                                g.level, rk, rk_desc, next_str
                            ))
                            .size(10.5)
                            .color(ZenPalette::GOLD_LEAF),
                        );
                        p_ui.add_space(4.0);

                        for path in XpPath::ALL {
                            let pl = self.progress.path_level(path);
                            p_ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "{:<8} Nv.{:<2}",
                                        path.label(),
                                        pl.level
                                    ))
                                    .size(10.5)
                                    .color(path.color()),
                                );
                                let (b_rect, _) = ui
                                    .allocate_exact_size(Vec2::new(150.0, 7.0), Sense::hover());
                                draw_pixel_bar(ui.painter(), b_rect, pl.ratio(), path.color());
                                ui.label(
                                    RichText::new(format!("{}/{} XP", pl.into, pl.need))
                                        .size(9.5)
                                        .color(ZenPalette::TEXT_DIM),
                                );
                            });
                        }

                        p_ui.add_space(4.0);
                        p_ui.label(
                            RichText::new(format!(
                                "Harmonie du Jour : Souffles {}/3 • Éveil {}/1 • Action {}/1 • Série {} j",
                                self.progress.daily_breaths.min(3),
                                self.progress.daily_quizzes.min(1),
                                self.progress.daily_actions.min(1),
                                self.progress.visit_streak
                            ))
                            .size(10.0)
                            .color(ZenPalette::JADE_LIGHT),
                        );
                        p_ui.add_space(4.0);

                        p_ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("Modèle IA : {}", self.active_model))
                                    .size(10.5)
                                    .color(ZenPalette::CYAN_SPIRIT),
                            );
                            if zen_button(
                                ui,
                                "Changer de modèle",
                                BtnKind::Secondary,
                                Vec2::new(118.0, 19.0),
                            )
                            .clicked()
                            {
                                self.cycle_model();
                            }
                        });
                        p_ui.add_space(4.0);

                        p_ui.horizontal(|ui| {
                            let edit =
                                egui::TextEdit::singleline(&mut self.new_memory_fact_input)
                                    .hint_text("Confier un souvenir durable à Kaze...")
                                    .desired_width(250.0)
                                    .font(FontId::new(11.0, FontFamily::Proportional));
                            ui.add(edit);
                            if zen_button(
                                ui,
                                "+ Mémoriser",
                                BtnKind::Primary,
                                Vec2::new(82.0, 20.0),
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
                        let _ = ScrollArea::vertical()
                            .id_salt("spirit_mem_scroll")
                            .max_height(56.0)
                            .show(&mut p_ui, |ui| {
                                if facts.is_empty() {
                                    ui.label(
                                        RichText::new("Mémoire long-terme vierge.")
                                            .size(10.0)
                                            .color(ZenPalette::TEXT_DIM),
                                    );
                                } else {
                                    for f in &facts {
                                        ui.label(
                                            RichText::new(format!("• {}", f))
                                                .size(10.0)
                                                .color(ZenPalette::TEXT_LIGHT),
                                        );
                                    }
                                }
                            });
                    }
                }
            } else if self.bubble_anim > 0.02 {
                // =============================================================
                // LA BULLE UNIQUE TOUT-EN-UN (ANCRÉE JUSTE AU-DESSUS DU CHAPEAU DE KAZE)
                // =============================================================
                let anim = self.bubble_anim;
                let bubble_w = WIN_WIDTH - 24.0;
                let bubble_h = (self.bubble_h_anim * anim).max(64.0);
                // Le bas de la queue de la bulle touche exactement le sommet du chapeau de Kaze !
                let bubble_top_y = (hat_top_y - bubble_h).max(6.0);
                let bubble_rect = Rect::from_min_size(
                    Pos2::new((WIN_WIDTH - bubble_w) * 0.5, bubble_top_y),
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

                    // 1. En-tête de la bulle Washi
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
                            ui.spacing_mut().item_spacing.x = 4.0;
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
                            if zen_button(ui, "Menu", BtnKind::Paper, Vec2::new(42.0, 18.0))
                                .clicked()
                            {
                                self.show_control_panel = true;
                            }
                            if let Some(ref md_path) = self.last_exported_md.clone() {
                                if zen_button(ui, "Ouvrir", BtnKind::Paper, Vec2::new(44.0, 18.0))
                                    .clicked()
                                {
                                    tasks::open_file_external(md_path);
                                }
                            }
                            if zen_button(ui, ".md", BtnKind::Paper, Vec2::new(32.0, 18.0))
                                .clicked()
                            {
                                self.export_current_response_md("");
                            }
                            if zen_button(ui, "Copier", BtnKind::Paper, Vec2::new(46.0, 18.0))
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
                            if zen_button(ui, mode_lbl, mode_kind, Vec2::new(52.0, 18.0)).clicked()
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

                    // Hauteur disponible pour le corps au-dessus de la barre de saisie (32px)
                    let body_h = (content_rect.height() - 62.0).max(42.0);

                    // 2. Corps de la bulle (Mode Zen vs Mode Échange)
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
                                (body_h - if self.last_user_prompt.is_some() { 16.0 } else { 0.0 })
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

                    // 3. Pied de la bulle : Saisie sur Parchemin Washi (sans rectangle noir !)
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

                    // Flèche Haut pour rappeler la dernière demande
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

            // Popups d'XP flottants
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
        println!("Kaze (Vent) - Compagnon Zen & Samouraï Desktop");
        println!();
        println!("USAGE:");
        println!("    ai-pet [OPTIONS]");
        println!();
        println!("OPTIONS:");
        println!("    -h, --help               Affiche cette aide");
        println!("    --cli [PROMPT]           Exécute une requête en mode ligne de commande sans interface graphique");
        println!("    --yes, -y                Auto-accepte les actions demandées en mode CLI");
        println!("    --no, -n                 Refuse les actions système demandées en mode CLI");
        println!("    --wayland                Force le backend Wayland sous Linux (par défaut X11)");
        println!();
        return Ok(());
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
        .with_inner_size([WIN_WIDTH, WIN_HEIGHT])
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
