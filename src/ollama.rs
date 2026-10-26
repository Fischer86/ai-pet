use crate::awareness;
use crate::sprites::PetMood;
use crate::tasks::{self, PetTask};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::process::{Command, Stdio};
use std::time::Duration;

const OLLAMA_BASE: &str = "http://127.0.0.1:11434";
pub const DEFAULT_MODEL: &str = "qwen2.5:3b";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize)]
struct StructuredAgentPlan {
    #[serde(default)]
    user_intent: String,
    #[serde(default)]
    planned_action: String,
    #[serde(default)]
    fallback_if_refused: String,
    #[serde(default)]
    mood: String,
    #[serde(default)]
    reply: String,
    #[serde(default)]
    task: PetTask,
}

/// Proposition d'action en attente de validation explicite par l'utilisateur (Human-in-the-Loop).
#[derive(Debug, Clone)]
pub struct PendingAgentAction {
    pub user_input: String,
    pub user_intent: String,
    pub planned_action: String,
    pub fallback_if_refused: String,
    pub mood: PetMood,
    pub proposal_reply: String,
    pub task: PetTask,
}

impl PendingAgentAction {
    /// Crée une demande de confirmation pour une commande slash ou un bouton rapide.
    pub fn from_direct_command(cmd_label: &str, action: &str, arg: &str, mood: PetMood) -> Self {
        let desc = tasks::describe_task_human(action, arg);
        Self {
            user_input: cmd_label.to_string(),
            user_intent: format!("Exécuter la commande « {} ».", cmd_label),
            planned_action: desc.clone(),
            fallback_if_refused:
                "Je n'exécuterai aucune commande et resterai à votre écoute pour discuter."
                    .to_string(),
            mood,
            proposal_reply: format!(
                "Je m'apprête à effectuer cette action : {}. Me donnez-vous votre accord ?",
                desc
            ),
            task: PetTask {
                action: action.to_string(),
                arg: arg.to_string(),
            },
        }
    }

    pub fn format_confirmation_prompt(&self) -> String {
        let arg_display = if self.task.arg.trim().is_empty() {
            "(aucun argument)".to_string()
        } else {
            format!("« {} »", self.task.arg)
        };
        format!(
            "🛡️ CONFIRMATION D'ACTION REQUISE\n\
            ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n\
            🎯 1. Que veut l'utilisateur ?\n   {}\n\
            🛠️ 2. Que vais-je faire ?\n   {} [Action: `{}` | Arg: {}]\n\
            🍃 3. Et si vous refusez ?\n   {}\n\
            ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n\
            💬 Kaze : « {} »\n\n\
            👉 Acceptez-vous cette action ? (Boutons ci-dessous ou [Entrée = Oui / Échap = Non])",
            self.user_intent,
            self.planned_action,
            self.task.action,
            arg_display,
            self.fallback_if_refused,
            self.proposal_reply
        )
    }
}

#[derive(Debug, Clone)]
pub enum AgentTurnOutcome {
    /// Simple réponse conversationnelle sans action système.
    DirectReply {
        mood: PetMood,
        display_text: String,
    },
    /// L'agent souhaite effectuer une action (même mondaine) et demande la permission à l'utilisateur.
    NeedsConfirmation(PendingAgentAction),
}

#[derive(Debug, Clone)]
pub struct AgentFinalResult {
    pub mood: PetMood,
    pub display_text: String,
}

pub fn check_ollama_running() -> bool {
    ureq::get(&format!("{}/api/tags", OLLAMA_BASE))
        .timeout(Duration::from_millis(800))
        .call()
        .map(|r| r.status() == 200)
        .unwrap_or(false)
}

pub fn list_models() -> Result<Vec<(String, f64)>, String> {
    let resp = ureq::get(&format!("{}/api/tags", OLLAMA_BASE))
        .timeout(Duration::from_secs(3))
        .call()
        .map_err(|e| format!("Ollama injoignable : {}", e))?;

    let val: Value = resp.into_json().map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    if let Some(arr) = val.get("models").and_then(|m| m.as_array()) {
        for m in arr {
            let name = m
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("inconnu")
                .to_string();
            let size_gb = m
                .get("size")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0)
                / (1024.0 * 1024.0 * 1024.0);
            out.push((name, size_gb));
        }
    }
    Ok(out)
}

pub fn ensure_ollama_started() -> Result<String, String> {
    if check_ollama_running() {
        return format_models_summary();
    }

    Command::new("ollama")
        .arg("serve")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Impossible de lancer `ollama serve` : {}", e))?;

    for _ in 0..25 {
        std::thread::sleep(Duration::from_millis(250));
        if check_ollama_running() {
            return format_models_summary();
        }
    }

    Err("Ollama a été lancé mais ne répond pas encore sur 127.0.0.1:11434.".into())
}

pub fn stop_ollama() {
    if cfg!(target_os = "windows") {
        let _ = Command::new("taskkill")
            .args(["/F", "/IM", "ollama.exe"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    } else {
        let _ = Command::new("pkill")
            .args(["-f", "ollama serve"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

pub fn format_models_summary() -> Result<String, String> {
    let models = list_models()?;
    if models.is_empty() {
        return Ok(
            "🟢 Ollama est actif, mais aucun modèle n'est installé.\n(Ex: `ollama pull qwen2.5:3b`)"
                .into(),
        );
    }
    let lines: Vec<String> = models
        .iter()
        .map(|(name, gb)| format!("• {} ({:.1} Go)", name, gb))
        .collect();
    Ok(format!(
        "🟢 Sanctuaire Ollama actif !\nModèles présents ({}) :\n{}",
        models.len(),
        lines.join("\n")
    ))
}

/// Étape 1 du Processus d'Agent :
/// Le modèle analyse :
/// 1) Que veut l'utilisateur ? (`user_intent`)
/// 2) Que vais-je faire ? (`planned_action` + `task`)
/// 3) S'il refuse, comment puis-je continuer ? (`fallback_if_refused`)
/// Si `task.action != "none"`, retourne `AgentTurnOutcome::NeedsConfirmation` pour demander la permission.
pub fn plan_pet_action(
    model: &str,
    history: &[ChatMessage],
    user_input: &str,
) -> Result<AgentTurnOutcome, String> {
    if !check_ollama_running() {
        let _ = ensure_ollama_started()?;
    }

    let deep_awareness = awareness::build_deep_awareness_context();
    let system_prompt = format!(
        "Tu es Kaze (風), un petit samouraï zen, loyal et prudent qui vit sur le bureau de l'utilisateur.\n\
        Tu suis un PROCESSUS D'AGENT RIGOUREUX avant toute action :\n\
        1. `user_intent` : Que veut l'utilisateur exactement ?\n\
        2. `planned_action` : Que vais-je faire concrètement pour lui répondre ?\n\
        3. `fallback_if_refused` : Si l'utilisateur refuse cette action, comment puis-je continuer à l'aider autrement (ex: explication théorique, autre piste) ?\n\
        4. `reply` : Ta phrase chaleureuse demandant la permission d'agir (si `task.action != \"none\"`) ou ta réponse directe (si `task.action == \"none\"`).\n\n\
        {}\n\n\
        CAPACITÉS DISPONIBLES (`task.action` et `task.arg`) :\n\
        - \"none\" : simple discussion ou réponse directe à partir des données DÉJÀ présentes dans ta conscience temps réel\n\
        - \"timer_status\" : afficher le détail des minuteurs et Pomodoros actifs (arg: \"\")\n\
        - \"set_timer\" : créer un nouveau minuteur (arg: \"<durée> <nom>\", ex \"45s Thé vert\" ou \"5m Pause\")\n\
        - \"pomodoro\" : lancer une session Pomodoro Zen en minutes (arg: ex \"25\")\n\
        - \"cancel_timer\" : annuler un minuteur ou Pomodoro (arg: \"all\" ou nom/numéro)\n\
        - \"remember_fact\" : mémoriser durablement un fait sur l'utilisateur (arg: fait à retenir)\n\
        - \"recall_memory\" : afficher toute la mémoire long-terme (arg: \"\")\n\
        - \"forget_memory\" : oublier un souvenir (arg: mot-clé ou \"all\")\n\
        - \"write_note\" : écrire une note sur le parchemin (arg: texte)\n\
        - \"read_notes\" : lire toutes les notes du parchemin (arg: \"\")\n\
        - \"clear_notes\" : effacer toutes les notes (arg: \"\")\n\
        - \"system_info\" : bilan complet CPU, RAM, Disque, Audio, Uptime (arg: \"\")\n\
        - \"top_processes\" : liste détaillée des applications utilisant le plus de mémoire (arg: \"\")\n\
        - \"network_info\" : état réseau et IP (arg: \"\")\n\
        - \"battery_info\" : niveau de batterie et alimentation (arg: \"\")\n\
        - \"audio_control\" : régler ou couper le son (arg: \"+10\", \"-10\", \"50\", \"mute\", \"unmute\", \"status\")\n\
        - \"media_control\" : contrôler la musique/vidéo (arg: \"play\", \"pause\", \"next\", \"prev\")\n\
        - \"screenshot\" : prendre une capture d'écran (arg: \"\")\n\
        - \"kde_action\" : action bureau (arg: \"lock\", \"krunner\", \"settings\", \"monitor\")\n\
        - \"clipboard_read\" : lire le presse-papiers complet (arg: \"\")\n\
        - \"clipboard_write\" : copier un texte dans le presse-papiers (arg: texte)\n\
        - \"open_app\" : lancer une application (arg: ex \"dolphin\", \"konsole\", \"kate\", \"firefox\", \"kcalc\")\n\
        - \"open_url\" : ouvrir un site ou une recherche web (arg: URL ou recherche)\n\
        - \"list_files\" : lister les fichiers d'un dossier (arg: chemin ex \"~\", \"~/Documents\", \".\")\n\
        - \"find_files\" : chercher un fichier par nom (arg: motif)\n\
        - \"read_file\" : lire le contenu d'un fichier texte (arg: chemin)\n\
        - \"run_shell\" : exécuter une commande shell (arg: commande)\n\
        - \"notify\" : envoyer une notification système (arg: message)\n\n\
        RÈGLES ESSENTIELLES :\n\
        1. TOUTE action (`task.action` différent de \"none\"), même la commande la plus mondaine (lister un dossier, lire un fichier, ouvrir une appli, lancer un minuteur, voir le bilan système), demandera confirmation à l'utilisateur : formule donc `reply` comme une proposition polie expliquant ce que tu t'apprêtes à faire.\n\
        2. Remplis toujours `user_intent`, `planned_action` et `fallback_if_refused` en français clair.\n\
        3. Choisis `mood` parmi : \"idle\", \"run\", \"jump\", \"attack\", \"swim\", \"special\".",
        deep_awareness
    );

    let mut messages = Vec::with_capacity(history.len() + 2);
    messages.push(json!({
        "role": "system",
        "content": system_prompt
    }));

    let start_idx = history.len().saturating_sub(6);
    for msg in &history[start_idx..] {
        messages.push(json!({
            "role": msg.role,
            "content": msg.content
        }));
    }

    messages.push(json!({
        "role": "user",
        "content": user_input
    }));

    let schema = json!({
        "type": "object",
        "properties": {
            "user_intent": {
                "type": "string",
                "description": "Que veut l'utilisateur ? (Objectif compris)"
            },
            "planned_action": {
                "type": "string",
                "description": "Que vais-je faire concrètement ? (Action technique prévue)"
            },
            "fallback_if_refused": {
                "type": "string",
                "description": "Si l'utilisateur refuse cette action, comment continuer à l'aider sans exécuter la commande ?"
            },
            "mood": {
                "type": "string",
                "enum": ["idle", "run", "jump", "attack", "swim", "special"]
            },
            "reply": {
                "type": "string",
                "description": "Message poli demandant à l'utilisateur s'il accepte l'action"
            },
            "task": {
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": [
                            "none",
                            "timer_status",
                            "set_timer",
                            "pomodoro",
                            "cancel_timer",
                            "remember_fact",
                            "recall_memory",
                            "forget_memory",
                            "write_note",
                            "read_notes",
                            "clear_notes",
                            "system_info",
                            "top_processes",
                            "network_info",
                            "battery_info",
                            "audio_control",
                            "media_control",
                            "screenshot",
                            "kde_action",
                            "clipboard_read",
                            "clipboard_write",
                            "open_app",
                            "open_url",
                            "list_files",
                            "find_files",
                            "read_file",
                            "run_shell",
                            "notify"
                        ]
                    },
                    "arg": { "type": "string" }
                },
                "required": ["action", "arg"]
            }
        },
        "required": ["user_intent", "planned_action", "fallback_if_refused", "mood", "reply", "task"]
    });

    let payload = json!({
        "model": model,
        "stream": false,
        "think": false,
        "format": schema,
        "options": {
            "temperature": 0.25
        },
        "messages": messages
    });

    let resp = ureq::post(&format!("{}/api/chat", OLLAMA_BASE))
        .timeout(Duration::from_secs(45))
        .send_json(payload)
        .map_err(|e| format!("Erreur requête Ollama ({}) : {}", model, e))?;

    let body: Value = resp.into_json().map_err(|e| e.to_string())?;
    let raw_content = body
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("{}");

    let parsed: StructuredAgentPlan =
        serde_json::from_str(raw_content).unwrap_or_else(|_| StructuredAgentPlan {
            user_intent: "Échanger avec Kaze".into(),
            planned_action: "Répondre directement".into(),
            fallback_if_refused: "Continuer la discussion".into(),
            mood: "idle".into(),
            reply: raw_content.to_string(),
            task: PetTask::default(),
        });

    let mood = PetMood::from_str(&parsed.mood);
    let action_clean = parsed.task.action.trim().to_lowercase();

    if action_clean.is_empty() || action_clean == "none" {
        let text = if parsed.reply.trim().is_empty() {
            "Je suis à votre écoute avec sérénité.".to_string()
        } else {
            parsed.reply.trim().to_string()
        };
        return Ok(AgentTurnOutcome::DirectReply {
            mood,
            display_text: text,
        });
    }

    // Une action est envisagée : on exige systématiquement la confirmation de l'utilisateur !
    let human_desc = tasks::describe_task_human(&action_clean, &parsed.task.arg);
    let user_intent = if parsed.user_intent.trim().is_empty() {
        format!("Demande : « {} »", user_input)
    } else {
        parsed.user_intent.trim().to_string()
    };
    let planned_action = if parsed.planned_action.trim().is_empty() {
        human_desc.clone()
    } else {
        format!("{} ({})", parsed.planned_action.trim(), human_desc)
    };
    let fallback = if parsed.fallback_if_refused.trim().is_empty() {
        "Continuer la conversation sans exécuter l'action et proposer une explication ou alternative."
            .to_string()
    } else {
        parsed.fallback_if_refused.trim().to_string()
    };
    let proposal_reply = if parsed.reply.trim().is_empty() {
        format!(
            "Souhaitez-vous que j'exécute : {} ?",
            human_desc
        )
    } else {
        parsed.reply.trim().to_string()
    };

    Ok(AgentTurnOutcome::NeedsConfirmation(PendingAgentAction {
        user_input: user_input.to_string(),
        user_intent,
        planned_action,
        fallback_if_refused: fallback,
        mood,
        proposal_reply,
        task: PetTask {
            action: action_clean,
            arg: parsed.task.arg.trim().to_string(),
        },
    }))
}

/// Étape 2A : L'utilisateur a ACCEPTÉ l'action proposée par l'agent.
/// Exécute la tâche puis synthétise le résultat réel.
pub fn execute_confirmed_action(
    model: &str,
    pending: &PendingAgentAction,
) -> Result<AgentFinalResult, String> {
    let task_output = tasks::execute_task(&pending.task)
        .unwrap_or_else(|| "(Action effectuée)".to_string());

    let deep_awareness = awareness::build_deep_awareness_context();
    let mut commentary = "✅ Action confirmée et accomplie avec soin.".to_string();

    if pending.task.needs_synthesis() {
        if let Ok(synth) =
            synthesize_tool_output(model, &deep_awareness, &pending.user_input, &task_output)
        {
            if !synth.trim().is_empty() {
                commentary = synth;
            }
        }
    } else {
        commentary = format!("✅ Accord reçu ! {}", pending.proposal_reply);
    }

    let mood = match pending.task.action.as_str() {
        "open_app" | "open_url" | "list_files" | "find_files" => PetMood::Run,
        "run_shell" | "screenshot" | "top_processes" => PetMood::Attack,
        _ => pending.mood,
    };

    Ok(AgentFinalResult {
        mood,
        display_text: format!("{}\n\n{}", commentary, task_output),
    })
}

/// Étape 2B : L'utilisateur a REFUSÉ l'action proposée par l'agent.
/// L'agent n'exécute PAS l'action et rebondit avec bienveillance en suivant son Plan B (`fallback_if_refused`).
pub fn handle_rejected_action(
    model: &str,
    pending: &PendingAgentAction,
) -> Result<AgentFinalResult, String> {
    let prompt = format!(
        "Demande initiale de l'utilisateur : « {} »\n\
        Action que tu avais proposée : {} (`{}` `{}`)\n\
        Décision de l'utilisateur : REFUSÉ (l'utilisateur ne souhaite pas que cette action soit exécutée).\n\
        Ton plan B prévu : « {} »\n\n\
        En tant que Kaze (petit samouraï zen), prends acte avec respect et sérénité du refus de l'utilisateur (confirme qu'aucune action n'a été lancée) et applique ton plan B ou propose-lui de continuer autrement en 1 ou 2 phrases courtes en français.",
        pending.user_input,
        pending.planned_action,
        pending.task.action,
        pending.task.arg,
        pending.fallback_if_refused
    );

    let payload = json!({
        "model": model,
        "stream": false,
        "think": false,
        "options": {
            "temperature": 0.3,
            "num_predict": 120
        },
        "messages": [
            {"role": "user", "content": prompt}
        ]
    });

    if let Ok(resp) = ureq::post(&format!("{}/api/chat", OLLAMA_BASE))
        .timeout(Duration::from_secs(12))
        .send_json(payload)
    {
        if let Ok(body) = resp.into_json::<Value>() {
            if let Some(txt) = body
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
            {
                let trimmed = txt.trim();
                if !trimmed.is_empty() {
                    return Ok(AgentFinalResult {
                        mood: PetMood::Idle,
                        display_text: format!(
                            "🛑 Action annulée à votre demande.\n\n💬 Kaze : {}",
                            trimmed
                        ),
                    });
                }
            }
        }
    }

    Ok(AgentFinalResult {
        mood: PetMood::Idle,
        display_text: format!(
            "🛑 Action annulée à votre demande (aucune commande exécutée).\n\n🍃 Plan alternatif : {}",
            pending.fallback_if_refused
        ),
    })
}

fn synthesize_tool_output(
    model: &str,
    deep_awareness: &str,
    user_input: &str,
    tool_output: &str,
) -> Result<String, String> {
    let prompt = format!(
        "{}\n\nDemande de l'utilisateur : « {} »\nRésultat de l'outil exécuté (après accord de l'utilisateur) :\n{}\n\n\
        Tu es Kaze (le petit samouraï zen ; l'utilisateur est ton compagnon décrit dans [MÉMOIRE LONG-TERME UTILISATEUR]). Réponds en 1 ou 2 phrases courtes et utiles en français à partir de ta conscience et de ce résultat réel.",
        deep_awareness, user_input, tool_output
    );

    let payload = json!({
        "model": model,
        "stream": false,
        "think": false,
        "options": {
            "temperature": 0.25,
            "num_predict": 120
        },
        "messages": [
            {"role": "user", "content": prompt}
        ]
    });

    let resp = ureq::post(&format!("{}/api/chat", OLLAMA_BASE))
        .timeout(Duration::from_secs(15))
        .send_json(payload)
        .map_err(|e| e.to_string())?;

    let body: Value = resp.into_json().map_err(|e| e.to_string())?;
    Ok(body
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .trim()
        .to_string())
}
