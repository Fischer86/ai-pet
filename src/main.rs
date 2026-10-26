mod awareness;
mod kde;
mod ollama;
mod sprites;
mod tasks;
mod zen_ui;

use eframe::egui::{
    self, Align, Color32, CornerRadius, FontFamily, FontId, Frame, Key, Layout, Margin, Pos2,
    Rect, RichText, ScrollArea, Sense, Stroke, Vec2, ViewportBuilder, ViewportCommand, WindowLevel,
};
use ollama::{
    AgentFinalResult, AgentTurnOutcome, ChatMessage, PendingAgentAction, DEFAULT_MODEL,
};
use sprites::{PetMood, SpriteAnimator};
use std::io::{self, Write};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};
use zen_ui::{draw_pixel_speech_bubble, install_pixel_fonts, ZenPalette, ZenParticles};

const WINDOW_TITLE: &str = "Kaze AI Pet";
const WIN_WIDTH: f32 = 396.0;
const WIN_HEIGHT: f32 = 400.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QuickTab {
    Zen,
    System,
    MediaKde,
    Tools,
}

enum WorkerEvent {
    PlanReady(Result<AgentTurnOutcome, String>),
    DecisionDone(Result<AgentFinalResult, String>),
    CommandOutput {
        text: String,
        mood: PetMood,
        quit_after: bool,
    },
    ModelsLoaded(Vec<String>),
}

struct KazePetApp {
    animator: SpriteAnimator,
    particles: ZenParticles,
    start_time: Instant,

    // État de la bulle & animations fluides
    bubble_open: bool,
    bubble_anim: f32,
    full_text: String,
    revealed_chars: usize,
    last_typewriter_tick: Instant,

    // Processus d'Agent : Action en attente de confirmation humaine
    pending_confirmation: Option<PendingAgentAction>,

    // Mode Méditation (réduit en bas à droite de l'écran, toujours au-dessus)
    is_meditating: bool,
    saved_pos_before_meditation: Pos2,
    screen_size: (f32, f32),

    input_text: String,
    focus_input_next: bool,
    is_loading: bool,
    active_tab: QuickTab,
    active_model: String,
    available_models: Vec<String>,
    history: Vec<ChatMessage>,

    // Communication asynchrone avec les threads Ollama / tâches
    tx: Sender<WorkerEvent>,
    rx: Receiver<WorkerEvent>,

    // Mode balade autonome sur le bureau
    roaming_enabled: bool,
    window_pos: Pos2,
    roam_velocity_x: f32,
    next_roam_decision: Instant,

    // Fermeture différée après /quit
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
        visuals.override_text_color = Some(ZenPalette::TEXT_PARCHMENT);
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

        let welcome = "🍵 Bienvenue dans mon jardin zen, voyageur.\n\
            Je suis Kaze (風), ton compagnon Samouraï toujours au-dessus de tes fenêtres.\n\
            • 🛡️ Sécurité Agent : je te demanderai toujours confirmation avant la moindre action ou commande.\n\
            • 🧘 Méditation : clique sur « 🧘 » en haut à droite pour me réduire en bas à droite de ton écran !";
        let welcome_len = welcome.chars().count();

        Self {
            animator,
            particles: ZenParticles::new(),
            start_time: Instant::now(),
            bubble_open: true,
            bubble_anim: 1.0,
            full_text: welcome.into(),
            revealed_chars: welcome_len,
            last_typewriter_tick: Instant::now(),
            pending_confirmation: None,
            is_meditating: false,
            saved_pos_before_meditation: Pos2::new(220.0, 220.0),
            screen_size,
            input_text: String::new(),
            focus_input_next: true,
            is_loading: false,
            active_tab: QuickTab::Zen,
            active_model: DEFAULT_MODEL.to_string(),
            available_models: vec![DEFAULT_MODEL.to_string()],
            history: Vec::new(),
            tx,
            rx,
            roaming_enabled: false,
            window_pos: Pos2::new(220.0, 220.0),
            roam_velocity_x: 0.0,
            next_roam_decision: Instant::now() + Duration::from_secs(4),
            quit_at: None,
            kwin_hints_applied: false,
            last_topmost_refresh: Instant::now(),
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
        self.full_text = s;
    }

    /// Active le Mode Méditation : Kaze replie son parchemin, se place en bas à droite de l'écran
    /// (toujours au-dessus des fenêtres et visible dans la barre des tâches) et médite jusqu'au réveil.
    fn enter_meditation(&mut self, ctx: &egui::Context) {
        if !self.is_meditating {
            self.saved_pos_before_meditation = self.window_pos;
        }
        self.is_meditating = true;
        self.roaming_enabled = false;
        self.roam_velocity_x = 0.0;
        self.bubble_open = false;
        self.animator.set_mood(PetMood::Swim, None);

        // Détecter la taille de l'écran via egui ou fallback OS
        if let Some(mon) = ctx.input(|i| i.viewport().monitor_size) {
            if mon.x >= 800.0 && mon.y >= 600.0 {
                self.screen_size = (mon.x, mon.y);
            }
        }
        let (sw, sh) = self.screen_size;
        // Positionner la fenêtre pour que Kaze (situé en bas au centre du viewport) apparaisse
        // pile dans le coin inférieur droit, au-dessus de la barre des tâches
        let target_x = (sw - WIN_WIDTH + 75.0).max(20.0);
        let target_y = (sh - WIN_HEIGHT - 46.0).max(20.0);
        self.window_pos = Pos2::new(target_x, target_y);

        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::WindowLevel(WindowLevel::AlwaysOnTop));
        ctx.send_viewport_cmd(ViewportCommand::OuterPosition(self.window_pos));
        kde::apply_kwin_window_hints(WINDOW_TITLE);
    }

    /// Réveille Kaze de sa méditation en bas à droite et rouvre son parchemin.
    fn wake_from_meditation(&mut self, ctx: &egui::Context) {
        self.is_meditating = false;
        self.bubble_open = true;
        self.focus_input_next = true;
        self.animator
            .set_mood(PetMood::Jump, Some(Duration::from_secs(3)));

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

        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(ViewportCommand::WindowLevel(WindowLevel::AlwaysOnTop));
        ctx.send_viewport_cmd(ViewportCommand::OuterPosition(self.window_pos));
        ctx.send_viewport_cmd(ViewportCommand::Focus);
        kde::apply_kwin_window_hints(WINDOW_TITLE);

        if self.pending_confirmation.is_none() {
            self.set_bubble_message(
                "🌅 Kaze sort de sa méditation, l'esprit clair et apaisé !\nQue souhaitez-vous accomplir ensemble ?",
                true,
            );
        }
    }

    /// Soumet une action à la confirmation explicite de l'utilisateur.
    fn request_user_confirmation(&mut self, pending: PendingAgentAction) {
        self.is_meditating = false;
        self.bubble_open = true;
        self.animator.set_mood(PetMood::Swim, None);
        let prompt_txt = pending.format_confirmation_prompt();
        self.pending_confirmation = Some(pending);
        self.set_bubble_message(prompt_txt, true);
    }

    /// L'utilisateur clique sur « ✅ Accepter & Exécuter ».
    fn confirm_pending_action(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.pending_confirmation.take() else {
            return;
        };
        self.is_loading = true;
        self.animator.set_mood(PetMood::Attack, None);
        self.set_bubble_message(
            format!(
                "⚔️ Accord confirmé ! Kaze exécute : {}…",
                pending.planned_action
            ),
            false,
        );

        let tx = self.tx.clone();
        let model = self.active_model.clone();
        let ctx_clone = ctx.clone();

        std::thread::spawn(move || {
            let res = ollama::execute_confirmed_action(&model, &pending);
            let _ = tx.send(WorkerEvent::DecisionDone(res));
            ctx_clone.request_repaint();
        });
    }

    /// L'utilisateur clique sur « ❌ Refuser & Continuer ».
    fn reject_pending_action(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.pending_confirmation.take() else {
            return;
        };
        self.is_loading = true;
        self.animator.set_mood(PetMood::Idle, None);
        self.set_bubble_message(
            "🍃 Refus pris en compte (aucune action exécutée). Kaze adapte sa réponse…",
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

    fn submit_input(&mut self, ctx: &egui::Context, raw: String) {
        let trimmed = raw.trim().to_string();
        if trimmed.is_empty() || self.is_loading {
            return;
        }

        if self.is_meditating {
            self.wake_from_meditation(ctx);
        } else {
            self.bubble_open = true;
        }

        if let Ok(mut st) = awareness::global_self_state().lock() {
            st.interactions += 1;
        }

        // Si une confirmation est en attente et que l'utilisateur tape oui/non dans le champ
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
            // Sinon on annule l'ancienne proposition et on traite la nouvelle demande
            self.pending_confirmation = None;
        }

        if trimmed.starts_with('/') {
            self.handle_slash_command(ctx, &trimmed);
            return;
        }

        self.is_loading = true;
        self.animator.set_mood(PetMood::Swim, None);
        self.set_bubble_message(
            format!(
                "🍃 Processus d'agent ({}) : Kaze analyse votre intention et prépare son plan…\n« {} »",
                self.active_model.split(':').next().unwrap_or(&self.active_model),
                trimmed
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

    /// Même les commandes slash ou boutons rapides passent par le prompt de confirmation d'action !
    fn propose_quick_task(&mut self, cmd_label: &str, action: &str, arg: &str, mood: PetMood) {
        let pending = PendingAgentAction::from_direct_command(cmd_label, action, arg, mood);
        self.request_user_confirmation(pending);
    }

    fn handle_slash_command(&mut self, ctx: &egui::Context, cmd_line: &str) {
        let mut parts = cmd_line.splitn(2, char::is_whitespace);
        let cmd = parts.next().unwrap_or("").to_lowercase();
        let arg = parts.next().unwrap_or("").trim().to_string();

        match cmd.as_str() {
            "/meditate" | "/mediter" | "/zen" => {
                self.enter_meditation(ctx);
            }
            "/wake" | "/reveil" => {
                self.wake_from_meditation(ctx);
            }
            "/serve" | "/models" => {
                self.is_loading = true;
                self.animator.set_mood(PetMood::Swim, None);
                self.set_bubble_message(
                    "⏳ Éveil du sanctuaire Ollama et lecture des modèles…",
                    false,
                );
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
                        Err(err) => (format!("⚠️ {}", err), PetMood::Idle),
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
                        format!(
                            "🧠 Esprit actif : {}\nModèles présents : {}\nAstuce : tape `/model <nom>` ou clique sur le badge en haut.",
                            self.active_model, list
                        ),
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
                        format!("✨ Esprit d'IA changé pour : {}", matched),
                        true,
                    );
                }
            }
            "/aware" | "/conscience" => {
                self.animator
                    .set_mood(PetMood::Special, Some(Duration::from_secs(3)));
                self.set_bubble_message(awareness::build_deep_awareness_context(), true);
            }
            // Toutes les commandes d'action passent par le processus de confirmation !
            "/sys" | "/status" => {
                self.propose_quick_task(cmd_line, "system_info", "", PetMood::Special)
            }
            "/top" | "/ps" => {
                self.propose_quick_task(cmd_line, "top_processes", "", PetMood::Attack)
            }
            "/net" | "/ip" => self.propose_quick_task(cmd_line, "network_info", "", PetMood::Run),
            "/bat" | "/power" => {
                self.propose_quick_task(cmd_line, "battery_info", "", PetMood::Idle)
            }
            "/vol" | "/volume" => {
                self.propose_quick_task(cmd_line, "audio_control", &arg, PetMood::Jump)
            }
            "/media" | "/music" => {
                self.propose_quick_task(cmd_line, "media_control", &arg, PetMood::Jump)
            }
            "/shot" | "/screenshot" => {
                self.propose_quick_task(cmd_line, "screenshot", &arg, PetMood::Attack)
            }
            "/kde" => self.propose_quick_task(cmd_line, "kde_action", &arg, PetMood::Special),
            "/clip" => self.propose_quick_task(cmd_line, "clipboard_read", "", PetMood::Idle),
            "/copy" => self.propose_quick_task(cmd_line, "clipboard_write", &arg, PetMood::Jump),
            "/open" => self.propose_quick_task(cmd_line, "open_app", &arg, PetMood::Run),
            "/web" | "/url" => self.propose_quick_task(cmd_line, "open_url", &arg, PetMood::Run),
            "/ls" | "/files" => self.propose_quick_task(cmd_line, "list_files", &arg, PetMood::Run),
            "/find" => self.propose_quick_task(cmd_line, "find_files", &arg, PetMood::Run),
            "/cat" | "/read" => self.propose_quick_task(cmd_line, "read_file", &arg, PetMood::Idle),
            "/run" | "/sh" => self.propose_quick_task(cmd_line, "run_shell", &arg, PetMood::Attack),
            "/note" => self.propose_quick_task(cmd_line, "write_note", &arg, PetMood::Jump),
            "/notes" => self.propose_quick_task(cmd_line, "read_notes", "", PetMood::Idle),
            "/clearnotes" => {
                self.propose_quick_task(cmd_line, "clear_notes", "", PetMood::Special)
            }
            "/remember" => self.propose_quick_task(cmd_line, "remember_fact", &arg, PetMood::Jump),
            "/memory" => self.propose_quick_task(cmd_line, "recall_memory", "", PetMood::Special),
            "/forget" => self.propose_quick_task(cmd_line, "forget_memory", &arg, PetMood::Idle),
            "/timers" => self.propose_quick_task(cmd_line, "timer_status", "", PetMood::Idle),
            "/timer" => self.propose_quick_task(cmd_line, "set_timer", &arg, PetMood::Jump),
            "/cancel" => self.propose_quick_task(cmd_line, "cancel_timer", &arg, PetMood::Attack),
            "/pomo" | "/pomodoro" => {
                self.propose_quick_task(cmd_line, "pomodoro", &arg, PetMood::Special)
            }
            "/clear" => {
                self.history.clear();
                self.set_bubble_message("🍃 L'esprit de Kaze est apaisé (historique effacé).", true);
            }
            "/quit" | "/exit" => {
                self.is_loading = true;
                self.animator.set_mood(PetMood::Special, None);
                self.set_bubble_message(
                    "🎋 Au revoir, voyageur ! Arrêt d'Ollama et repos du Samouraï…",
                    false,
                );
                let tx = self.tx.clone();
                let ctx_clone = ctx.clone();
                std::thread::spawn(move || {
                    ollama::stop_ollama();
                    let _ = tx.send(WorkerEvent::CommandOutput {
                        text: "🌸 Que la paix accompagne votre session !".into(),
                        mood: PetMood::Special,
                        quit_after: true,
                    });
                    ctx_clone.request_repaint();
                });
            }
            "/help" | _ => {
                self.set_bubble_message(
                    "📜 Capacités & Processus d'Agent de Kaze :\n\
                    • 🛡️ Toute action demande votre confirmation (Accepter / Refuser).\n\
                    • 🧘 Méditation : /meditate (réduire en bas à droite), /wake (réveiller)\n\
                    • Conscience & Mémoire : /aware, /remember <fait>, /memory, /forget\n\
                    • Minuteurs & Pomodoro : /timer <45s Thé>, /pomo [25], /timers, /cancel\n\
                    • Système : /sys, /top, /net, /bat\n\
                    • Audio, Média & Capture : /vol <+10|-10|mute>, /media <play|next>, /shot\n\
                    • Fichiers & Apps : /open <app>, /web <recherche>, /ls, /find <nom>, /cat <fichier>, /run <cmd>",
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
        self.set_bubble_message(
            format!("🧠 Esprit Ollama sélectionné : {}", self.active_model),
            true,
        );
    }

    fn update_roaming(&mut self, ctx: &egui::Context) {
        if !self.roaming_enabled || self.is_loading || self.is_meditating {
            return;
        }
        let now = Instant::now();
        if now >= self.next_roam_decision {
            let phase = (now.elapsed().subsec_nanos() / 100) % 4;
            match phase {
                0 => {
                    self.roam_velocity_x = 1.5;
                    self.animator.facing_left = false;
                    self.animator.set_mood(PetMood::Run, None);
                }
                1 => {
                    self.roam_velocity_x = -1.5;
                    self.animator.facing_left = true;
                    self.animator.set_mood(PetMood::Run, None);
                }
                2 => {
                    self.roam_velocity_x = 0.0;
                    self.animator
                        .set_mood(PetMood::Jump, Some(Duration::from_millis(900)));
                }
                _ => {
                    self.roam_velocity_x = 0.0;
                    self.animator.set_mood(PetMood::Idle, None);
                }
            }
            self.next_roam_decision = now + Duration::from_millis(2600);
        }

        if self.roam_velocity_x.abs() > 0.01 {
            let max_x = (self.screen_size.0 - WIN_WIDTH).max(200.0);
            self.window_pos.x = (self.window_pos.x + self.roam_velocity_x).clamp(20.0, max_x);
            if self.window_pos.x <= 25.0 {
                self.roam_velocity_x = 1.5;
                self.animator.facing_left = false;
            } else if self.window_pos.x >= max_x - 5.0 {
                self.roam_velocity_x = -1.5;
                self.animator.facing_left = true;
            }
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(self.window_pos));
        }
    }
}

impl eframe::App for KazePetApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.kwin_hints_applied {
            self.kwin_hints_applied = true;
            ctx.send_viewport_cmd(ViewportCommand::WindowLevel(WindowLevel::AlwaysOnTop));
            kde::apply_kwin_window_hints(WINDOW_TITLE);
        }

        // Maintenir AlwaysOnTop en permanence et intercepter toute minimisation de la barre des tâches
        // pour basculer automatiquement en Mode Méditation en bas à droite !
        if ctx.input(|i| i.viewport().minimized.unwrap_or(false)) {
            self.enter_meditation(ctx);
        }
        if self.last_topmost_refresh.elapsed() >= Duration::from_secs(3) {
            self.last_topmost_refresh = Instant::now();
            ctx.send_viewport_cmd(ViewportCommand::WindowLevel(WindowLevel::AlwaysOnTop));
            kde::enforce_x11_ewmh_top_and_taskbar(WINDOW_TITLE);
        }

        if let Some(deadline) = self.quit_at {
            if Instant::now() >= deadline {
                ctx.send_viewport_cmd(ViewportCommand::Close);
                return;
            }
        }

        // Synchroniser la conscience interne de Kaze
        if let Ok(mut st) = awareness::global_self_state().lock() {
            st.mood = self.animator.current_mood;
            st.roaming = self.roaming_enabled;
            st.meditating = self.is_meditating;
            st.pos_x = self.window_pos.x;
            st.pos_y = self.window_pos.y;
            st.active_model = self.active_model.clone();
        }

        // Vérifier l'expiration des minuteurs / Pomodoros en temps réel
        let expired_timers = awareness::global_timers()
            .lock()
            .map(|mut reg| reg.poll_expired())
            .unwrap_or_default();

        for exp in expired_timers {
            if self.is_meditating {
                self.wake_from_meditation(ctx);
            } else {
                self.bubble_open = true;
            }
            self.animator
                .set_mood(PetMood::Jump, Some(Duration::from_secs(5)));
            let title = if exp.is_pomodoro {
                "🍵 Fin de session Pomodoro Zen !"
            } else {
                "⏰ Gong du Dojo — Minuteur écoulé !"
            };
            let msg = format!(
                "{}\nVotre sablier « {} » ({}m {:02}s) vient de s'écouler !",
                title,
                exp.label,
                exp.total_duration.as_secs() / 60,
                exp.total_duration.as_secs() % 60
            );
            let _ = kde::send_kde_notification(title, &exp.label);
            self.set_bubble_message(msg, true);
        }

        // Réception des événements asynchrones (Planification Agent / Décision / Modèles)
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
                            self.request_user_confirmation(pending);
                        }
                        Err(err) => {
                            self.animator.set_mood(PetMood::Idle, None);
                            self.set_bubble_message(format!("⚠️ {}", err), false);
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
                            self.set_bubble_message(format!("⚠️ {}", err), false);
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
                            } else if let Some(flint) =
                                models.iter().find(|m| m.starts_with("flint"))
                            {
                                self.active_model = flint.clone();
                            } else {
                                self.active_model = models[0].clone();
                            }
                        }
                        self.available_models = models;
                    }
                }
            }
        }

        // Raccourcis clavier pour confirmer / refuser ou replier la bulle
        if self.pending_confirmation.is_some() && !self.is_loading {
            if ctx.input(|i| i.key_pressed(Key::Escape)) {
                self.reject_pending_action(ctx);
            } else if ctx.input(|i| i.key_pressed(Key::Enter)) && self.input_text.trim().is_empty()
            {
                self.confirm_pending_action(ctx);
            }
        } else if ctx.input(|i| i.key_pressed(Key::Escape)) && self.bubble_open {
            self.bubble_open = false;
        }

        // Animation fluide d'ouverture/fermeture de la bulle
        let target_anim = if self.bubble_open && !self.is_meditating {
            1.0
        } else {
            0.0
        };
        self.bubble_anim += (target_anim - self.bubble_anim) * 0.28;
        if (self.bubble_anim - target_anim).abs() < 0.01 {
            self.bubble_anim = target_anim;
        }

        // Avancement de l'effet Machine à écrire
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

        let first_timer = awareness::global_timers()
            .lock()
            .ok()
            .and_then(|reg| reg.active.first().cloned());

        let panel_frame = Frame::NONE.fill(Color32::TRANSPARENT);
        egui::CentralPanel::default()
            .frame(panel_frame)
            .show(ctx, |ui| {
                let full_rect = ui.max_rect();

                // Particules Zen flottantes
                self.particles
                    .update_and_draw(ui.painter(), full_rect, elapsed_secs);

                ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
                    ui.add_space(4.0);

                    // --- 1. SAMOURAÏ PIXEL-ART & SOCLE ZEN ---
                    let tex_id = self.animator.current_texture().id();
                    let bob_y = if self.is_meditating {
                        (elapsed_secs * 1.4).sin() * 2.2
                    } else {
                        (elapsed_secs * 2.5).sin() * 1.5
                    };
                    let pet_size = Vec2::new(138.0, 100.0);
                    let (rect, resp) = ui.allocate_exact_size(pet_size, Sense::click_and_drag());

                    let shadow_rect = Rect::from_center_size(
                        Pos2::new(rect.center().x, rect.max.y - 6.0),
                        Vec2::new(66.0, 8.0),
                    );
                    ui.painter().rect_filled(
                        shadow_rect,
                        CornerRadius::same(2),
                        Color32::from_rgba_unmultiplied(12, 11, 16, 130),
                    );

                    let sprite_rect = rect.translate(Vec2::new(0.0, bob_y));
                    ui.painter().image(
                        tex_id,
                        sprite_rect,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                        Color32::WHITE,
                    );

                    // --- AFFICHAGE DU CARTOUCHE EN MODE MÉDITATION (EN BAS À DROITE) ---
                    if self.is_meditating {
                        ui.add_space(4.0);
                        let med_frame = Frame::NONE
                            .fill(ZenPalette::BG_MAIN)
                            .stroke(Stroke::new(1.5_f32, ZenPalette::BAMBOO_GREEN))
                            .inner_margin(Margin::symmetric(10, 6));
                        med_frame.show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    RichText::new("🧘 Kaze médite en paix…")
                                        .size(11.5)
                                        .color(ZenPalette::BAMBOO_GREEN),
                                );
                                if let Some(ref t) = first_timer {
                                    let rem = t.remaining().as_secs();
                                    ui.label(
                                        RichText::new(format!(
                                            "⏳ {:02}:{:02} • {}",
                                            rem / 60,
                                            rem % 60,
                                            t.label
                                        ))
                                        .size(10.5)
                                        .color(ZenPalette::LANTERN_GOLD),
                                    );
                                }
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new("🌅 Réveiller Kaze")
                                                .size(11.0)
                                                .color(ZenPalette::INK_DARK),
                                        )
                                        .fill(ZenPalette::LANTERN_GOLD)
                                        .corner_radius(CornerRadius::ZERO),
                                    )
                                    .clicked()
                                {
                                    self.wake_from_meditation(ctx);
                                }
                            });
                        });
                    } else if !self.bubble_open {
                        // Badge sablier ou infobulle quand la bulle est simplement fermée
                        if let Some(ref t) = first_timer {
                            let rem = t.remaining().as_secs();
                            let badge_rect = Rect::from_center_size(
                                Pos2::new(rect.center().x, rect.min.y - 12.0),
                                Vec2::new(150.0, 20.0),
                            );
                            ui.painter().rect_filled(
                                badge_rect,
                                CornerRadius::ZERO,
                                ZenPalette::BG_MAIN,
                            );
                            ui.painter().rect_stroke(
                                badge_rect,
                                CornerRadius::ZERO,
                                Stroke::new(1.0_f32, ZenPalette::BAMBOO_GREEN),
                                egui::StrokeKind::Inside,
                            );
                            ui.painter().text(
                                badge_rect.center(),
                                egui::Align2::CENTER_CENTER,
                                format!("⏳ {:02}:{:02} • {}", rem / 60, rem % 60, t.label),
                                FontId::new(10.5, FontFamily::Proportional),
                                ZenPalette::BAMBOO_GREEN,
                            );
                        } else if resp.hovered() {
                            let badge_rect = Rect::from_center_size(
                                Pos2::new(rect.center().x, rect.min.y - 12.0),
                                Vec2::new(210.0, 22.0),
                            );
                            ui.painter().rect_filled(
                                badge_rect,
                                CornerRadius::ZERO,
                                ZenPalette::BG_MAIN,
                            );
                            ui.painter().rect_stroke(
                                badge_rect,
                                CornerRadius::ZERO,
                                Stroke::new(1.0_f32, mood_accent),
                                egui::StrokeKind::Inside,
                            );
                            ui.painter().text(
                                badge_rect.center(),
                                egui::Align2::CENTER_CENTER,
                                "🎋 Double-clic : Parchemin • Glisser : Déplacer",
                                FontId::new(11.0, FontFamily::Proportional),
                                ZenPalette::LANTERN_GOLD,
                            );
                        }
                    }

                    if resp.drag_started() {
                        ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                    }
                    if resp.double_clicked() || resp.secondary_clicked() {
                        if self.is_meditating {
                            self.wake_from_meditation(ctx);
                        } else {
                            self.bubble_open = !self.bubble_open;
                            if self.bubble_open {
                                self.focus_input_next = true;
                                self.animator
                                    .set_mood(PetMood::Jump, Some(Duration::from_millis(1200)));
                            }
                        }
                    } else if resp.clicked() {
                        if self.is_meditating {
                            self.wake_from_meditation(ctx);
                        } else {
                            let next_mood = match self.animator.current_mood {
                                PetMood::Idle => PetMood::Attack,
                                PetMood::Attack => PetMood::Jump,
                                _ => PetMood::Special,
                            };
                            self.animator
                                .set_mood(next_mood, Some(Duration::from_millis(1100)));
                        }
                    }

                    ui.add_space(10.0);

                    // --- 2. BULLE DE TEXTE PIXEL-ART ZEN ---
                    if self.bubble_anim > 0.02 && !self.is_meditating {
                        let anim = self.bubble_anim;
                        ui.with_layout(Layout::top_down(Align::Center), |ui| {
                            let bubble_w = WIN_WIDTH - 16.0;
                            let bubble_h = 266.0 * anim;
                            let (bubble_rect, _) = ui.allocate_exact_size(
                                Vec2::new(bubble_w, bubble_h),
                                Sense::hover(),
                            );

                            let border_accent = if self.pending_confirmation.is_some() {
                                ZenPalette::SAKURA_PINK
                            } else {
                                mood_accent
                            };

                            draw_pixel_speech_bubble(
                                ui.painter(),
                                bubble_rect,
                                border_accent,
                                anim,
                            );

                            if anim > 0.65 {
                                let content_rect = bubble_rect.shrink2(Vec2::new(14.0, 10.0));
                                let mut child_ui = ui.new_child(
                                    egui::UiBuilder::new()
                                        .max_rect(content_rect)
                                        .layout(Layout::top_down(Align::Min)),
                                );

                                // --- EN-TÊTE DU PARCHEMIN ---
                                let header_resp = child_ui
                                    .horizontal(|ui| {
                                        let status_label = if self.pending_confirmation.is_some() {
                                            "🛡️ CONFIRMATION"
                                        } else {
                                            self.animator.current_mood.label()
                                        };
                                        ui.label(
                                            RichText::new(format!("🎋 KAZE • {}", status_label))
                                                .size(12.5)
                                                .color(border_accent),
                                        );

                                        let short_model = self
                                            .active_model
                                            .split(':')
                                            .next()
                                            .unwrap_or(&self.active_model);
                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    RichText::new(format!("🏮 {}", short_model))
                                                        .size(10.5)
                                                        .color(ZenPalette::BAMBOO_GREEN),
                                                )
                                                .fill(Color32::from_rgba_unmultiplied(
                                                    143, 184, 150, 28,
                                                ))
                                                .corner_radius(CornerRadius::ZERO),
                                            )
                                            .on_hover_text(
                                                "Changer de modèle Ollama (qwen2.5 / flint / phi4-mini)",
                                            )
                                            .clicked()
                                        {
                                            self.cycle_model();
                                        }

                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    RichText::new("📋")
                                                        .size(11.0)
                                                        .color(ZenPalette::TEXT_MUTED),
                                                )
                                                .fill(Color32::TRANSPARENT)
                                                .corner_radius(CornerRadius::ZERO),
                                            )
                                            .on_hover_text("Copier le texte dans le presse-papiers")
                                            .clicked()
                                        {
                                            let _ = tasks::write_clipboard(&self.full_text);
                                        }

                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    RichText::new("👁")
                                                        .size(11.0)
                                                        .color(ZenPalette::LANTERN_GOLD),
                                                )
                                                .fill(Color32::TRANSPARENT)
                                                .corner_radius(CornerRadius::ZERO),
                                            )
                                            .on_hover_text(
                                                "Afficher la Conscience Temps Réel de Kaze (/aware)",
                                            )
                                            .clicked()
                                        {
                                            self.submit_input(ctx, "/aware".into());
                                        }

                                        ui.with_layout(
                                            Layout::right_to_left(Align::Center),
                                            |ui| {
                                                if ui
                                                    .add(
                                                        egui::Button::new(
                                                            RichText::new("✕")
                                                                .size(12.5)
                                                                .color(ZenPalette::SAKURA_PINK),
                                                        )
                                                        .frame(false),
                                                    )
                                                    .on_hover_text("Replier le parchemin (Échap)")
                                                    .clicked()
                                                {
                                                    self.bubble_open = false;
                                                }

                                                // Bouton Mode Méditation (réduire en bas à droite)
                                                if ui
                                                    .add(
                                                        egui::Button::new(
                                                            RichText::new("🧘 Méditer")
                                                                .size(10.5)
                                                                .color(ZenPalette::BAMBOO_GREEN),
                                                        )
                                                        .fill(Color32::from_rgba_unmultiplied(
                                                            143, 184, 150, 35,
                                                        ))
                                                        .corner_radius(CornerRadius::ZERO),
                                                    )
                                                    .on_hover_text(
                                                        "Réduire Kaze en bas à droite en état de méditation",
                                                    )
                                                    .clicked()
                                                {
                                                    self.enter_meditation(ctx);
                                                }
                                            },
                                        );
                                    })
                                    .response;

                                if header_resp.interact(Sense::drag()).drag_started() {
                                    ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                                }

                                child_ui.add_space(5.0);

                                // Barre de progression en direct pour tout minuteur ou Pomodoro actif
                                if let Some(ref t) = first_timer {
                                    let rem_secs = t.remaining().as_secs();
                                    let ratio = t.progress_ratio();
                                    child_ui.horizontal(|ui| {
                                        let icon = if t.is_pomodoro { "🍵" } else { "⏳" };
                                        ui.label(
                                            RichText::new(format!(
                                                "{} {:02}:{:02} {}",
                                                icon,
                                                rem_secs / 60,
                                                rem_secs % 60,
                                                t.label
                                            ))
                                            .size(10.5)
                                            .color(ZenPalette::BAMBOO_GREEN),
                                        );
                                        let bar = egui::ProgressBar::new(ratio)
                                            .desired_width(120.0)
                                            .desired_height(6.0)
                                            .fill(ZenPalette::BAMBOO_GREEN);
                                        ui.add(bar);
                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    RichText::new("✕")
                                                        .size(9.5)
                                                        .color(ZenPalette::SAKURA_PINK),
                                                )
                                                .frame(false),
                                            )
                                            .on_hover_text("Annuler ce minuteur")
                                            .clicked()
                                        {
                                            self.propose_quick_task(
                                                "/cancel",
                                                "cancel_timer",
                                                &t.id.to_string(),
                                                PetMood::Attack,
                                            );
                                        }
                                    });
                                    child_ui.add_space(2.0);
                                }

                                // --- CORPS DU TEXTE (AVEC EFFET MACHINE À ÉCRIRE) ---
                                let displayed: String = self
                                    .full_text
                                    .chars()
                                    .take(self.revealed_chars)
                                    .collect();

                                let _ = ScrollArea::vertical()
                                    .max_height(if first_timer.is_some() { 94.0 } else { 112.0 })
                                    .auto_shrink([false, false])
                                    .stick_to_bottom(true)
                                    .show(&mut child_ui, |ui| {
                                        let lbl = ui.add(
                                            egui::Label::new(
                                                RichText::new(&displayed)
                                                    .size(12.5)
                                                    .color(ZenPalette::TEXT_PARCHMENT),
                                            )
                                            .sense(Sense::click()),
                                        );
                                        if lbl.clicked() && self.revealed_chars < total_chars {
                                            self.revealed_chars = total_chars;
                                        }
                                    });

                                child_ui.add_space(4.0);

                                // --- ZONE D'ACTION : SOIT BOUTONS DE CONFIRMATION D'AGENT, SOIT ONGLETS RAPIDES ---
                                if self.pending_confirmation.is_some() {
                                    child_ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 8.0;
                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    RichText::new(
                                                        "✅ Accepter & Exécuter (Entrée)",
                                                    )
                                                    .size(11.5)
                                                    .color(ZenPalette::INK_DARK),
                                                )
                                                .fill(ZenPalette::BAMBOO_GREEN)
                                                .corner_radius(CornerRadius::ZERO),
                                            )
                                            .clicked()
                                        {
                                            self.confirm_pending_action(ctx);
                                        }

                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    RichText::new("❌ Refuser & Continuer (Échap)")
                                                        .size(11.5)
                                                        .color(ZenPalette::INK_DARK),
                                                )
                                                .fill(ZenPalette::SAKURA_PINK)
                                                .corner_radius(CornerRadius::ZERO),
                                            )
                                            .clicked()
                                        {
                                            self.reject_pending_action(ctx);
                                        }
                                    });
                                    child_ui.add_space(6.0);
                                } else {
                                    // --- ONGLETS DE CAPACITÉS PIXEL-ART ---
                                    child_ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 4.0;
                                        for (tab, label) in [
                                            (QuickTab::Zen, "🍵 Zen"),
                                            (QuickTab::System, "📊 Système"),
                                            (QuickTab::MediaKde, "🎵 Bureau"),
                                            (QuickTab::Tools, "📂 Outils"),
                                        ] {
                                            let active = self.active_tab == tab;
                                            let fill = if active {
                                                Color32::from_rgba_unmultiplied(
                                                    mood_accent.r(),
                                                    mood_accent.g(),
                                                    mood_accent.b(),
                                                    60,
                                                )
                                            } else {
                                                Color32::from_rgba_unmultiplied(45, 42, 54, 180)
                                            };
                                            let text_col = if active {
                                                mood_accent
                                            } else {
                                                ZenPalette::TEXT_MUTED
                                            };
                                            if ui
                                                .add(
                                                    egui::Button::new(
                                                        RichText::new(label)
                                                            .size(11.0)
                                                            .color(text_col),
                                                    )
                                                    .fill(fill)
                                                    .corner_radius(CornerRadius::ZERO),
                                                )
                                                .clicked()
                                            {
                                                self.active_tab = tab;
                                            }
                                        }

                                        let roam_col = if self.roaming_enabled {
                                            ZenPalette::BAMBOO_GREEN
                                        } else {
                                            ZenPalette::TEXT_MUTED
                                        };
                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    RichText::new("🚶 Balade")
                                                        .size(11.0)
                                                        .color(roam_col),
                                                )
                                                .fill(if self.roaming_enabled {
                                                    Color32::from_rgba_unmultiplied(
                                                        143, 184, 150, 55,
                                                    )
                                                } else {
                                                    Color32::from_rgba_unmultiplied(
                                                        45, 42, 54, 180,
                                                    )
                                                })
                                                .corner_radius(CornerRadius::ZERO),
                                            )
                                            .on_hover_text("Promenade autonome de Kaze sur l'écran")
                                            .clicked()
                                        {
                                            self.roaming_enabled = !self.roaming_enabled;
                                            if !self.roaming_enabled {
                                                self.roam_velocity_x = 0.0;
                                                self.animator.set_mood(PetMood::Idle, None);
                                            } else {
                                                self.next_roam_decision = Instant::now();
                                            }
                                        }
                                    });

                                    child_ui.add_space(3.0);

                                    // --- ACTIONS DE L'ONGLET SÉLECTIONNÉ ---
                                    child_ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 5.0;
                                        let actions: &[(&str, &str)] = match self.active_tab {
                                            QuickTab::Zen => &[
                                                ("🍵 Pomo 25m", "/pomo 25"),
                                                ("🧘 Méditer", "/meditate"),
                                                ("🧠 Mémoire", "/memory"),
                                                ("📒 Notes", "/notes"),
                                                ("⏹ Repos", "/quit"),
                                            ],
                                            QuickTab::System => &[
                                                ("📊 Bilan", "/sys"),
                                                ("🔥 Top RAM", "/top"),
                                                ("🌐 Réseau", "/net"),
                                                ("🔋 Batterie", "/bat"),
                                            ],
                                            QuickTab::MediaKde => &[
                                                ("🔊 Vol +", "/vol +10"),
                                                ("🔉 Vol -", "/vol -10"),
                                                ("🎵 Play/Pause", "/media toggle"),
                                                ("📸 Capture", "/shot"),
                                                ("🔍 Launcher", "/kde krunner"),
                                            ],
                                            QuickTab::Tools => &[
                                                ("📁 Fichiers", "/open fichiers"),
                                                ("💻 Terminal", "/open terminal"),
                                                ("📋 Presse-pap.", "/clip"),
                                                ("▶ Ollama", "/serve"),
                                            ],
                                        };

                                        for (label, cmd) in actions {
                                            if ui
                                                .add(
                                                    egui::Button::new(
                                                        RichText::new(*label)
                                                            .size(10.5)
                                                            .color(ZenPalette::LANTERN_GOLD),
                                                    )
                                                    .fill(Color32::from_rgba_unmultiplied(
                                                        217, 185, 120, 26,
                                                    ))
                                                    .stroke(Stroke::new(
                                                        1.0_f32,
                                                        Color32::from_rgba_unmultiplied(
                                                            217, 185, 120, 70,
                                                        ),
                                                    ))
                                                    .corner_radius(CornerRadius::ZERO),
                                                )
                                                .clicked()
                                            {
                                                self.submit_input(ctx, (*cmd).to_string());
                                            }
                                        }
                                    });
                                }

                                child_ui.add_space(4.0);

                                // --- CHAMP DE SAISIE PIXEL-ART ---
                                child_ui.horizontal(|ui| {
                                    let edit_frame = Frame::NONE
                                        .fill(ZenPalette::BG_INPUT)
                                        .stroke(Stroke::new(
                                            1.0_f32,
                                            Color32::from_rgba_unmultiplied(
                                                border_accent.r(),
                                                border_accent.g(),
                                                border_accent.b(),
                                                110,
                                            ),
                                        ))
                                        .inner_margin(Margin::symmetric(6, 4));

                                    let mut enter_pressed = false;
                                    let hint = if self.pending_confirmation.is_some() {
                                        "Tape 'oui' ou 'non' (ou utilise les boutons)…"
                                    } else {
                                        "Confie une tâche à Kaze ou tape /help…"
                                    };

                                    edit_frame.show(ui, |ui| {
                                        let edit = egui::TextEdit::singleline(&mut self.input_text)
                                            .frame(false)
                                            .hint_text(hint)
                                            .font(FontId::new(12.5, FontFamily::Proportional))
                                            .desired_width(WIN_WIDTH - 100.0)
                                            .text_color(ZenPalette::TEXT_PARCHMENT);

                                        let edit_resp = ui.add_enabled(!self.is_loading, edit);
                                        if self.focus_input_next {
                                            edit_resp.request_focus();
                                            self.focus_input_next = false;
                                        }
                                        enter_pressed = edit_resp.lost_focus()
                                            && ui.input(|i| i.key_pressed(Key::Enter));
                                    });

                                    let send_clicked = ui
                                        .add_enabled(
                                            !self.is_loading,
                                            egui::Button::new(
                                                RichText::new(if self.is_loading {
                                                    "…"
                                                } else {
                                                    "➤"
                                                })
                                                .size(13.0)
                                                .color(ZenPalette::INK_DARK),
                                            )
                                            .fill(border_accent)
                                            .corner_radius(CornerRadius::ZERO),
                                        )
                                        .clicked();

                                    if enter_pressed || send_clicked {
                                        let msg = std::mem::take(&mut self.input_text);
                                        self.submit_input(ctx, msg);
                                    }
                                });
                            }
                        });
                    }
                });
            });

        ctx.request_repaint_after(Duration::from_millis(33));
    }
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if let Some(pos) = args.iter().position(|a| a == "--cli") {
        let prompt = args
            .get(pos + 1)
            .map(|s| s.as_str())
            .unwrap_or("Salut Kaze ! Liste les fichiers du dossier courant.");
        let auto_yes = args.iter().any(|a| a == "--yes" || a == "-y");
        let auto_no = args.iter().any(|a| a == "--no" || a == "-n");

        println!("🎋 Interrogation de Kaze ({}) : « {} »", DEFAULT_MODEL, prompt);
        match ollama::plan_pet_action(DEFAULT_MODEL, &[], prompt) {
            Ok(AgentTurnOutcome::DirectReply { mood, display_text }) => {
                println!("🎭 Posture : {:?} ({})", mood, mood.label());
                println!("💬 Réponse directe :\n{}", display_text);
            }
            Ok(AgentTurnOutcome::NeedsConfirmation(pending)) => {
                println!("{}\n", pending.format_confirmation_prompt());
                let accepted = if auto_yes {
                    println!("👉 [Auto-Confirmation : OUI (--yes)]");
                    true
                } else if auto_no {
                    println!("👉 [Auto-Confirmation : NON (--no)]");
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
                        println!("\n🎭 Posture finale : {:?} ({})", res.mood, res.mood.label());
                        println!("{}", res.display_text);
                    }
                    Err(e) => eprintln!("❌ Erreur : {}", e),
                }
            }
            Err(e) => {
                eprintln!("❌ Erreur : {}", e);
                std::process::exit(1);
            }
        }
        return Ok(());
    }

    // Sur Linux (KDE Plasma Wayland avec XWayland actif), winit 0.30 privilégie WAYLAND_DISPLAY
    // (où xdg-shell ne permet ni AlwaysOnTop ni le positionnement libre en bas à droite).
    // On bascule donc sur X11/XWayland (en conservant KAZE_WAYLAND_DISPLAY pour wl-copy/wl-paste).
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
        .with_position([220.0, 220.0])
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
