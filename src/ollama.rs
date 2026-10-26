use crate::awareness;
use crate::kde;
use crate::sprites::PetMood;
use crate::tasks::{self, PetTask};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

const OLLAMA_BASE: &str = "http://127.0.0.1:11434";
pub const DEFAULT_MODEL: &str = "qwen2.5:3b";
pub const SENSEI_MODEL_NAME: &str = "kaze-sensei:latest";

const EMBEDDED_ROLE_MD: &str = include_str!("../ROLE.md");
const EMBEDDED_MODELFILE_SENSEI: &str = include_str!("../Modelfile.sensei");

/// Retourne le chemin du fichier `ROLE.md` (priorité au fichier local `./ROLE.md` s'il existe,
/// sinon copie modifiable dans le dossier applicatif `app_data_dir()/ROLE.md`).
pub fn role_file_path() -> PathBuf {
    let local = PathBuf::from("ROLE.md");
    if local.exists() {
        return local;
    }
    let app_role = kde::app_data_dir().join("ROLE.md");
    if !app_role.exists() {
        let _ = fs::write(&app_role, EMBEDDED_ROLE_MD);
    }
    app_role
}

/// Charge le contenu actif de `ROLE.md` définissant la personnalité et la pédagogie du Sensei.
pub fn load_role_markdown() -> String {
    let path = role_file_path();
    fs::read_to_string(path).unwrap_or_else(|_| EMBEDDED_ROLE_MD.to_string())
}

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
    auto_remember: String,
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
                "Je n'exécuterai aucune commande et resterai à votre écoute au Dojo."
                    .to_string(),
            mood,
            proposal_reply: format!(
                "Disciple, je m'apprête à accomplir cette action : {}. Me donnez-vous votre accord ?",
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
            "CONFIRMATION DU SENSEI REQUISE\n\
            ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n\
            1. Objectif compris : {}\n\
            2. Action prévue : {} [`{}` {}]\n\
            3. Plan alternatif : {}\n\
            ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n\
            Sensei Kaze : « {} »\n\n\
            Acceptez-vous cette action ? ([Entrée = Oui / Échap = Non])",
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
    /// L'agent souhaite effectuer une action et demande la permission à l'utilisateur.
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

/// Forge (fine-tune via Modelfile Ollama) le modèle spécialisé `kaze-sensei:latest`
/// à partir du modèle de base choisi et de la charte `ROLE.md`.
pub fn forge_sensei_model(base_model: &str) -> Result<String, String> {
    if !check_ollama_running() {
        let _ = ensure_ollama_started()?;
    }

    let clean_base = if base_model.starts_with("kaze-sensei") {
        DEFAULT_MODEL
    } else {
        base_model
    };

    let role_md = load_role_markdown();
    let custom_modelfile = format!(
        "FROM {}\n\
        PARAMETER temperature 0.32\n\
        PARAMETER top_p 0.88\n\
        PARAMETER top_k 40\n\
        PARAMETER repeat_penalty 1.12\n\
        PARAMETER num_ctx 4096\n\n\
        SYSTEM \"\"\"\n\
        {}\n\
        \"\"\"\n\n\
        MESSAGE user \"Je commence à travailler sur mon projet en Rust, mais j'ai du mal à rester concentré.\"\n\
        MESSAGE assistant \"{{\\\"user_intent\\\":\\\"Se concentrer efficacement sur son projet en Rust\\\",\\\"planned_action\\\":\\\"Lancer une session Pomodoro Zen de 25 minutes\\\",\\\"fallback_if_refused\\\":\\\"Proposer trois respirations Zazen ou un découpage de sa tâche en étapes simples\\\",\\\"auto_remember\\\":\\\"Développe un projet en Rust\\\",\\\"mood\\\":\\\"special\\\",\\\"reply\\\":\\\"Un sabre ne se forge pas dans la dispersion, Disciple. Je te propose d'allumer un sablier Pomodoro de 25 minutes consacré uniquement à ton code Rust. Ouvrons-nous ce cycle de discipline ?\\\",\\\"task\\\":{{\\\"action\\\":\\\"pomodoro\\\",\\\"arg\\\":\\\"25\\\"}}}}\"\n\
        MESSAGE user \"Explique-moi la différence entre Stack et Heap simplement.\"\n\
        MESSAGE assistant \"{{\\\"user_intent\\\":\\\"Comprendre la différence entre la pile (Stack) et le tas (Heap)\\\",\\\"planned_action\\\":\\\"Répondre directement par une métaphore martiale et technique limpide\\\",\\\"fallback_if_refused\\\":\\\"Continuer l'explication avec un exemple en Rust\\\",\\\"auto_remember\\\":\\\"Étudie la gestion mémoire (Stack et Heap)\\\",\\\"mood\\\":\\\"idle\\\",\\\"reply\\\":\\\"Imagine la Pile (Stack) comme le râtelier de sabres à portée de main : chaque lame de taille fixe y est posée et reprise instantanément dans l'ordre inverse (LIFO). Le Tas (Heap) est le grand entrepôt du Dojo : on y réserve de l'espace sur mesure pour des objets dynamiques, mais il faut un pointeur sur le râtelier pour retrouver leur emplacement.\\\",\\\"task\\\":{{\\\"action\\\":\\\"none\\\",\\\"arg\\\":\\\"\\\"}}}}\"\n",
        clean_base,
        role_md.replace("\"\"\"", "'''")
    );

    let modelfile_path = kde::app_data_dir().join("Modelfile.sensei");
    fs::write(&modelfile_path, &custom_modelfile)
        .unwrap_or_else(|_| {
            let _ = fs::write(&modelfile_path, EMBEDDED_MODELFILE_SENSEI);
        });

    let output = Command::new("ollama")
        .args([
            "create",
            "kaze-sensei",
            "-f",
            &modelfile_path.to_string_lossy(),
        ])
        .output()
        .map_err(|e| format!("Impossible d'exécuter `ollama create kaze-sensei` : {}", e))?;

    if output.status.success() {
        Ok(format!(
            "Modèle spécialisé `{}` forgé avec succès à partir de `{}` et `ROLE.md` !\nSensei Kaze utilise désormais cet esprit affûté.",
            SENSEI_MODEL_NAME, clean_base
        ))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(format!(
            "Échec de `ollama create kaze-sensei` : {}",
            if stderr.is_empty() {
                "vérifiez que le modèle de base est bien téléchargé (`ollama pull qwen2.5:3b`)."
            } else {
                &stderr
            }
        ))
    }
}

pub fn generate_quiz_question(model: &str, topic: &str) -> Result<QuizQuestion, String> {
    if !check_ollama_running() {
        let _ = ensure_ollama_started()?;
    }

    let role_md = load_role_markdown();
    let prompt = format!(
        "En tant que Sensei Kaze, génère une question d'apprentissage stimulante sur le sujet : « {} ».\n\
        Fournis exactement 4 options de réponse (A, B, C, D) courtes, claires et plausibles.\n\
        Indique l'index de la bonne réponse (0 pour A, 1 pour B, 2 pour C, 3 pour D).\n\
        Fournis une explication pédagogique de Sensei (1 ou 2 phrases claires et inspirantes).",
        if topic.trim().is_empty() {
            "Philosophie Zen, Bushidō ou Architecture Informatique"
        } else {
            topic.trim()
        }
    );

    let schema = json!({
        "type": "object",
        "properties": {
            "question": { "type": "string" },
            "options": {
                "type": "array",
                "items": { "type": "string" },
                "minItems": 4,
                "maxItems": 4
            },
            "correct_idx": { "type": "integer", "minimum": 0, "maximum": 3 },
            "explanation": { "type": "string" }
        },
        "required": ["question", "options", "correct_idx", "explanation"]
    });

    let payload = json!({
        "model": model,
        "stream": false,
        "think": false,
        "format": schema,
        "options": {
            "temperature": 0.55
        },
        "messages": [
            {
                "role": "system",
                "content": format!(
                    "Tu es le Sensei du Dojo d'Éveil de Kaze.\n{}\nTu poses des questions captivantes en français avec 4 choix concis et une explication lumineuse.",
                    role_md
                )
            },
            {
                "role": "user",
                "content": prompt
            }
        ]
    });

    let resp = ureq::post(&format!("{}/api/chat", OLLAMA_BASE))
        .timeout(Duration::from_secs(30))
        .send_json(payload)
        .map_err(|e| format!("Erreur Ollama ({}) : {}", model, e))?;

    let body: Value = resp.into_json().map_err(|e| e.to_string())?;
    let raw_content = body
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("{}");

    let parsed: QuizQuestion = serde_json::from_str(raw_content).map_err(|e| {
        format!(
            "Impossible de parser la question du Dojo : {} (brut: {})",
            e, raw_content
        )
    })?;

    if parsed.options.len() != 4 || parsed.question.trim().is_empty() {
        return Err("La question générée n'a pas 4 options valides.".into());
    }

    Ok(parsed)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuizQuestion {
    pub question: String,
    pub options: Vec<String>,
    pub correct_idx: usize,
    pub explanation: String,
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
            "Sanctuaire Ollama actif, mais aucun modèle n'est installé.\n(Ex: `ollama pull qwen2.5:3b`)"
                .into(),
        );
    }
    let lines: Vec<String> = models
        .iter()
        .map(|(name, gb)| format!("• {} ({:.1} Go)", name, gb))
        .collect();
    Ok(format!(
        "Sanctuaire Ollama actif !\nModèles présents ({}) :\n{}",
        models.len(),
        lines.join("\n")
    ))
}

fn agent_json_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "user_intent": {
                "type": "string",
                "description": "Intention ou besoin actuel du disciple"
            },
            "planned_action": {
                "type": "string",
                "description": "Action concrète choisie par Sensei Kaze"
            },
            "fallback_if_refused": {
                "type": "string",
                "description": "Alternative bienveillante si le disciple refuse l'action"
            },
            "auto_remember": {
                "type": "string",
                "description": "Fait nouveau et durable appris sur l'utilisateur à mémoriser automatiquement (ou chaîne vide \"\")"
            },
            "mood": {
                "type": "string",
                "enum": ["idle", "run", "jump", "attack", "swim", "special"]
            },
            "reply": {
                "type": "string",
                "description": "Parole de Sensei Kaze en français (2 à 4 phrases ciselées)"
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
        "required": ["user_intent", "planned_action", "fallback_if_refused", "auto_remember", "mood", "reply", "task"]
    })
}

fn build_sensei_system_prompt(deep_awareness: &str) -> String {
    let role_md = load_role_markdown();
    format!(
        "{}\n\n\
        {}\n\n\
        CAPACITÉS D'ACTION SYSTÈME (`task.action` et `task.arg`) :\n\
        - \"none\" : discussion, enseignement ou réponse directe à partir de ta conscience temps réel\n\
        - \"timer_status\" : afficher le détail des sabliers et Pomodoros actifs (arg: \"\")\n\
        - \"set_timer\" : créer un nouveau sablier (arg: \"<durée> <nom>\", ex \"3m Thé vert\" ou \"5m Pause\")\n\
        - \"pomodoro\" : lancer une session Pomodoro Zen en minutes (arg: ex \"25\")\n\
        - \"cancel_timer\" : arrêter un minuteur ou Pomodoro (arg: \"all\" ou nom/numéro)\n\
        - \"remember_fact\" : mémoriser durablement un fait sur le disciple (arg: fait à retenir)\n\
        - \"recall_memory\" : consulter toute la mémoire long-terme (arg: \"\")\n\
        - \"forget_memory\" : oublier un souvenir (arg: mot-clé ou \"all\")\n\
        - \"write_note\" : graver une note sur le parchemin (arg: texte)\n\
        - \"read_notes\" : lire les notes du parchemin (arg: \"\")\n\
        - \"clear_notes\" : effacer toutes les notes (arg: \"\")\n\
        - \"system_info\" : bilan complet CPU, RAM, Disque, Audio, Uptime (arg: \"\")\n\
        - \"top_processes\" : inspecter les applications utilisant le plus de mémoire (arg: \"\")\n\
        - \"network_info\" : état réseau et IP (arg: \"\")\n\
        - \"battery_info\" : niveau de batterie et alimentation (arg: \"\")\n\
        - \"audio_control\" : régler ou couper le son (arg: \"+10\", \"-10\", \"50\", \"mute\", \"unmute\", \"status\")\n\
        - \"media_control\" : contrôler la musique/vidéo (arg: \"play\", \"pause\", \"next\", \"prev\")\n\
        - \"screenshot\" : prendre une capture d'écran (arg: \"\")\n\
        - \"kde_action\" : action bureau (arg: \"lock\", \"krunner\", \"settings\", \"monitor\")\n\
        - \"clipboard_read\" : lire le presse-papiers complet (arg: \"\")\n\
        - \"clipboard_write\" : copier un texte dans le presse-papiers (arg: texte)\n\
        - \"open_app\" : lancer une application (arg: \"fichiers\", \"terminal\", \"editeur\", \"parametres\", \"moniteur\")\n\
        - \"open_url\" : ouvrir un site ou une recherche web (arg: URL ou recherche)\n\
        - \"list_files\" : lister les fichiers d'un dossier (arg: chemin ex \"~\", \"~/Documents\", \".\")\n\
        - \"find_files\" : chercher un fichier par nom (arg: motif)\n\
        - \"read_file\" : lire le contenu d'un fichier texte (arg: chemin)\n\
        - \"run_shell\" : exécuter une commande shell ou PowerShell (arg: commande)\n\
        - \"notify\" : envoyer une notification bureau (arg: message)\n\n\
        RÈGLES DU SENSEI :\n\
        1. Parle comme un véritable Sensei Samouraï Zen : calme, volontaire, structuré et bienveillant.\n\
        2. Si l'utilisateur révèle un fait important sur lui (son projet, son langage préféré, son objectif), inscris-le dans `auto_remember` pour le retenir automatiquement.\n\
        3. Choisis `mood` parmi : \"idle\", \"run\", \"jump\", \"attack\", \"swim\", \"special\".",
        role_md, deep_awareness
    )
}

/// Étape 1 du Processus d'Agent :
/// Analyse la demande avec la charte `ROLE.md`, mémorise spontanément tout fait nouveau (`auto_remember`),
/// et prépare la réponse ou l'action système.
pub fn plan_pet_action(
    model: &str,
    history: &[ChatMessage],
    user_input: &str,
) -> Result<AgentTurnOutcome, String> {
    if !check_ollama_running() {
        let _ = ensure_ollama_started()?;
    }

    let deep_awareness = awareness::build_deep_awareness_context();
    let system_prompt = build_sensei_system_prompt(&deep_awareness);

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

    let payload = json!({
        "model": model,
        "stream": false,
        "think": false,
        "format": agent_json_schema(),
        "options": {
            "temperature": 0.3
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

    parse_agent_plan_outcome(raw_content, user_input)
}

/// Génère une prise de parole ou proposition d'action **volontaire et autonome** du Sensei
/// à partir de l'état de progression du disciple et de la conscience temps réel du système.
pub fn generate_sensei_initiative(
    model: &str,
    progress_summary: &str,
) -> Result<AgentTurnOutcome, String> {
    if !check_ollama_running() {
        let _ = ensure_ollama_started()?;
    }

    let deep_awareness = awareness::build_deep_awareness_context();
    let system_prompt = build_sensei_system_prompt(&deep_awareness);

    let initiative_prompt = format!(
        "[INITIATIVE AUTONOME DU SENSEI]\n\
        Progression actuelle du disciple : {}\n\n\
        En tant que Sensei Kaze, prends volontairement la parole de ta propre initiative !\n\
        Observe l'état du système (RAM, CPU, minuteurs actifs), les souvenirs du disciple et son avancement sur les Trois Voies (Zazen, Éveil, Bushidō).\n\
        - Si aucun minuteur n'est actif et qu'il travaille, tu peux lui proposer un Pomodoro (`action: \"pomodoro\"`, `arg: \"25\"`) ou un sablier court.\n\
        - Si la mémoire RAM est très sollicitée (>80%), propose d'inspecter les processus (`action: \"top_processes\"`).\n\
        - Sinon (`action: \"none\"`), offre-lui un conseil martial/technique précis, un Kōan court, ou invite-le à accomplir son prochain geste d'Harmonie du Jour au Dojo.",
        progress_summary
    );

    let payload = json!({
        "model": model,
        "stream": false,
        "think": false,
        "format": agent_json_schema(),
        "options": {
            "temperature": 0.45
        },
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": initiative_prompt }
        ]
    });

    let resp = ureq::post(&format!("{}/api/chat", OLLAMA_BASE))
        .timeout(Duration::from_secs(35))
        .send_json(payload)
        .map_err(|e| format!("Erreur initiative Sensei ({}) : {}", model, e))?;

    let body: Value = resp.into_json().map_err(|e| e.to_string())?;
    let raw_content = body
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("{}");

    parse_agent_plan_outcome(raw_content, "Initiative volontaire du Sensei")
}

fn parse_agent_plan_outcome(raw_content: &str, user_input: &str) -> Result<AgentTurnOutcome, String> {
    let parsed: StructuredAgentPlan =
        serde_json::from_str(raw_content).unwrap_or_else(|_| StructuredAgentPlan {
            user_intent: "Échanger avec Sensei Kaze".into(),
            planned_action: "Répondre directement".into(),
            fallback_if_refused: "Continuer la discussion".into(),
            auto_remember: String::new(),
            mood: "idle".into(),
            reply: raw_content.to_string(),
            task: PetTask::default(),
        });

    // Apprentissage autonome : si le Sensei a détecté un fait durable sur le disciple, il le mémorise spontanément
    let mem_fact = parsed.auto_remember.trim();
    if !mem_fact.is_empty() && mem_fact.len() >= 4 && mem_fact != "none" && mem_fact != "aucun" {
        let _ = awareness::remember_fact(mem_fact);
    }

    let mood = PetMood::from_str(&parsed.mood);
    let action_clean = parsed.task.action.trim().to_lowercase();

    if action_clean.is_empty() || action_clean == "none" {
        let text = if parsed.reply.trim().is_empty() {
            "Je veille sur ton étude avec sérénité, Disciple.".to_string()
        } else {
            parsed.reply.trim().to_string()
        };
        return Ok(AgentTurnOutcome::DirectReply {
            mood,
            display_text: text,
        });
    }

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
        "Poursuivre l'échange au Dojo et proposer une approche alternative.".to_string()
    } else {
        parsed.fallback_if_refused.trim().to_string()
    };
    let proposal_reply = if parsed.reply.trim().is_empty() {
        format!("Souhaites-tu que j'accomplisse : {} ?", human_desc)
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

/// Étape 2A : L'utilisateur (ou les autorisations automatiques) a validé l'action.
/// Exécute la tâche puis synthétise le résultat réel avec le regard du Sensei.
pub fn execute_confirmed_action(
    model: &str,
    pending: &PendingAgentAction,
) -> Result<AgentFinalResult, String> {
    let task_output =
        tasks::execute_task(&pending.task).unwrap_or_else(|| "(Action accomplie)".to_string());

    let deep_awareness = awareness::build_deep_awareness_context();
    let mut commentary = "Action accomplie avec maîtrise.".to_string();

    if pending.task.needs_synthesis() {
        if let Ok(synth) =
            synthesize_tool_output(model, &deep_awareness, &pending.user_input, &task_output)
        {
            if !synth.trim().is_empty() {
                commentary = synth;
            }
        }
    } else {
        commentary = pending.proposal_reply.clone();
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
pub fn handle_rejected_action(
    model: &str,
    pending: &PendingAgentAction,
) -> Result<AgentFinalResult, String> {
    let prompt = format!(
        "Demande initiale : « {} »\n\
        Action proposée : {} (`{}` `{}`)\n\
        Décision du disciple : REFUSÉ.\n\
        Plan alternatif prévu : « {} »\n\n\
        En tant que Sensei Kaze, prends acte avec noblesse et sérénité du refus (confirme qu'aucune action n'a été lancée) et applique ton plan alternatif en 1 ou 2 phrases courtes en français.",
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
                        display_text: format!("Action retenue.\n\nSensei Kaze : {}", trimmed),
                    });
                }
            }
        }
    }

    Ok(AgentFinalResult {
        mood: PetMood::Idle,
        display_text: format!(
            "Action retenue (aucune commande exécutée).\n\nVoie alternative : {}",
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
        "{}\n\nDemande du disciple : « {} »\nRésultat de l'outil exécuté :\n{}\n\n\
        Tu es Sensei Kaze. Analyse ce résultat et réponds en 1 ou 2 phrases claires, utiles et volontaires en français, en proposant si pertinent la prochaine étape logique.",
        deep_awareness, user_input, tool_output
    );

    let payload = json!({
        "model": model,
        "stream": false,
        "think": false,
        "options": {
            "temperature": 0.25,
            "num_predict": 140
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
