# 🎋 Kaze (風) — Compagnon AI Pet Samouraï Zen en Rust

![Samurai Sprite](assets/Samurai/5x/idle_0.png) ![Samurai Run](assets/Samurai/5x/run_1.png) ![Samurai Jump](assets/Samurai/5x/jump_2.png) ![Samurai Attack](assets/Samurai/5x/attack_3.png) ![Samurai Meditate](assets/Samurai/5x/swim_1.png)

**Kaze (風)** est un compagnon de bureau (**AI Pet**) écrit en **Rust** (`eframe`/`egui`, `sysinfo`, `arboard`, `x11rb`) doté d'une interface **Pixel-Art Zen** sur-mesure, d'un **processus d'agent sécurisé avec confirmation humaine (Human-in-the-Loop)**, d'une **conscience temps réel approfondie** et d'une compatibilité native **Linux (KDE Plasma Wayland/X11), macOS et Windows**.

---

## ✨ Fonctionnalités Principales

- 🏮 **Interface Pixel-Art Zen & Police Dédiée** :
  - Bulle de dialogue Pixel-Art dessinée au pixel près (coins en escalier, double bordure dynamique selon l'humeur, ornements d'angle, flèche de bulle).
  - Police Pixel-Art `PixelifySans.ttf` et sprites 100 % embarqués dans l'exécutable (`include_bytes!`).
  - Particules flottantes (pétales de sakura, feuilles de bambou, lucioles dorées) et effet machine à écrire RPG.
- 🛡️ **Processus d'Agent avec Confirmation Systématique (Human-in-the-Loop)** :
  - Pour toute action (même la commande la plus mondaine), le modèle structure sa réflexion :
    1. **🎯 Que veut l'utilisateur ?** (`user_intent`)
    2. **🛠️ Que vais-je faire ?** (`planned_action` + `task`)
    3. **🍃 Et si l'utilisateur refuse ?** (`fallback_if_refused` — Plan B sans exécution)
  - Affiche deux boutons interactifs : **`✅ Accepter & Exécuter (Entrée)`** et **`❌ Refuser & Continuer (Échap)`**.
  - Si vous refusez, l'action est annulée et Kaze poursuit la conversation sereinement en suivant son Plan B.
- 📌 **Toujours au Premier Plan & Présent dans la Barre des Tâches** :
  - Maintien actif au-dessus de toutes les fenêtres via le protocole EWMH (`x11rb` `_NET_WM_STATE_ABOVE`, `_NET_WM_STATE_STICKY`) et règles compositeur KWin (`~/.config/kwinrulesrc`) sur KDE Plasma, ainsi que `WindowLevel::AlwaysOnTop` sur macOS et Windows.
  - Entrée d'application avec icône Samouraï dans la barre des tâches.
- 🧘 **Mode Méditation (Réduction en bas à droite & Réveil à la demande)** :
  - Cliquez sur **`🧘 Méditer`** (ou réduisez la fenêtre depuis la barre des tâches) : Kaze replie son parchemin et se place en bas à droite de votre écran en posture de méditation.
  - Cliquez sur **`🌅 Réveiller Kaze`** (ou sur le Samouraï) pour le ramener instantanément à sa position de travail.
- 👁️ **Conscience Temps Réel (`src/awareness.rs`)** :
  - Conscient de ses **minuteurs et sessions Pomodoro actifs** à la seconde près (avec barre de progression en direct et réveil automatique à l'échéance).
  - Conscient de la **RAM, du CPU, des disques, du réseau, de la batterie, du volume audio, des processus principaux actifs, de la branche Git courante, du presse-papiers, des notes** et de sa **mémoire long-terme persistante (`memory.json`)**.

---

## 📦 Prérequis & Installation

1. **Rust** (1.75+) :
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```
2. **Ollama** avec le modèle léger par défaut `qwen2.5:3b` (ou `phi4-mini` / `flint`) :
   ```bash
   ollama pull qwen2.5:3b
   ```

---

## 🚀 Compilation & Lancement (Linux, macOS, Windows)

```bash
# Cloner le dépôt
git clone https://github.com/Fischer86/ai-pet.git
cd ai-pet

# Lancer en mode développement
cargo run

# Ou compiler un binaire optimisé pour la production
cargo build --release
./target/release/ai-pet
```

### Mode Test / Ligne de Commande (`--cli`)

```bash
# Tester le processus d'agent interactif dans le terminal
cargo run -- --cli "Liste les fichiers du dossier courant"

# Simuler l'acceptation (--yes) ou le refus (--no) de l'action proposée
cargo run -- --cli "Exécute uname -a" --yes
cargo run -- --cli "Exécute uname -a" --no
```

---

## 🗂️ Architecture du Projet

```text
ai-pet/
├── Cargo.toml          # Dépendances multi-plateformes (egui, eframe, sysinfo, arboard, ureq, x11rb)
├── Cargo.lock          # Verrouillage exact des dépendances
├── LICENSE             # Licence MIT
├── README.md           # Documentation
├── assets/
│   ├── fonts/          # Police Pixel-Art PixelifySans.ttf
│   └── Samurai/        # Sprites pixel-art (1x à 5x : idle, run, jump, attack, swim, x)
└── src/
    ├── main.rs         # Boucle graphique, Mode Méditation, UI de confirmation Agent & CLI
    ├── zen_ui.rs       # Rendu de la bulle Pixel-Art en escalier, palette Zen & particules
    ├── sprites.rs      # Chargeur et animateur de sprites embarqués (include_bytes!)
    ├── ollama.rs       # Planificateur d'Agent, confirmation Human-in-the-Loop & synthèse 2 passes
    ├── awareness.rs    # Conscience temps réel (TimerRegistry, MemoryBank, état système/Git)
    ├── tasks.rs        # Exécuteur des 22 capacités (système, audio, média, fichiers, notes…)
    └── kde.rs          # Intégration multi-OS (KWin/EWMH x11rb sur Linux, macOS, Windows)
```

---

## 📜 Licence

Distribué sous licence **MIT**. Voir le fichier [LICENSE](LICENSE).
