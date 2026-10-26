# 🎋 Kaze (風) — Sensei Samouraï Pixel-Art & Agent Autonome de Bureau

**Kaze** (*« Le Vent »*) est un compagnon de bureau **Pixel-Art Zen** et un **agent IA autonome** écrit en **Rust** (`eframe` / `egui`) propulsé localement par **Ollama**.

À la fois **Sensei attentif**, **maître de méditation Zazen**, **précepteur d'apprentissage (Dojo d'Éveil)** et **gardien système (Bushidō)**, Kaze vit directement sur votre bureau en transparence totale ou vous accueille dans la grande fenêtre redimensionnable de son **Sanctuaire Dojo**.

---

## ✨ Fonctionnalités Principales

### 1. 🏯 Deux Modes Visuels Complémentaires
- **Mode Compagnon Flottant (Bureau 100 % Transparent)** :
  - Kaze flotte sur son îlot pixel-art évolutif (bonsaï, lanterne *Tōrō*, torii, autel d'or selon votre niveau) sans aucun fond ni cadre opaque.
  - **Double-clic** (ou clic droit) sur Kaze : déploie une **bulle unique en papier Washi** juste au-dessus de son chapeau *kasa*, regroupant la réponse du Sensei, le champ de saisie, ainsi que les boutons **Copier**, **`.md`** (export Markdown) et **Dojo**.
  - Deux modes de lecture dans la bulle : **Mode Zen** (uniquement la demande et la réponse actuelles) ou **Mode Échange** (historique complet).
- **Grande Fenêtre Redimensionnable du Sanctuaire Dojo (`/menu`, `/dojo` ou `/eveil`)** :
  - Véritable fenêtre redimensionnable avec l'arrière-plan pixel-art `assets/background_dojo.jpeg` (tatami, jardin de bambous et alcôve *Tokonoma*).
  - **Interface 100 % responsive** : chaque panneau, barre d'onglets, jauge d'XP et carte de réponse QCM s'adapte dynamiquement à la largeur de la fenêtre sans jamais déborder.

### 2. 🧭 Menu Cohérent & Volontaire (5 Piliers + Directive du Sensei)
Dès l'ouverture du Dojo, une **Bannière de Directive Volontaire du Sensei** analyse votre équilibre du jour et vous propose une action immédiate en 1 clic (**« Suivre > »** ou **« Initiative Sensei »**), suivie de **5 onglets cohérents** :
1. **Éveil** : Enseignement interactif et énigmes QCM générées par le Sensei sur le sujet de votre choix (*Philosophie Zen, Rust, Linux, Histoire, Sciences…*).
2. **Zazen** : Méditation guidée sur le rythme respiratoire **4s Inspiration • 3s Rétention • 5s Expiration**, avec halo *Ensō* animé, haïkus et gain d'XP à chaque cycle de souffle.
3. **Bushidō** : Rituels de concentration (**Pomodoro**, minuteurs), bilans système temps réel (CPU, RAM, réseau, batterie), contrôle audio/média et outils de bureau.
4. **Voies & Titres** : Vue complète de votre **Niveau Global**, des **3 Voies de Sagesse**, galerie des **12 Titres d'Honneur (*Shōgō*) équipables**, journal des derniers gains d'XP et export `.md` des chroniques.
5. **Sensei & IA** : Charte [`ROLE.md`](./ROLE.md), forge en 1 clic du modèle spécialisé `kaze-sensei`, activation de l'**autonomie proactive**, mémoire long-terme (`memory.json`) et garde-fous de sécurité.

### 3. 📜 Rôle de Sensei (`ROLE.md`) & Modèle Fine-Tuné (`Modelfile.sensei`)
- **[`ROLE.md`](./ROLE.md)** définit la charte complète du Sensei : identité, ton calme et volontaire, pédagogie des Trois Voies, mémorisation autonome et protocole d'action JSON.
- **[`Modelfile.sensei`](./Modelfile.sensei)** permet de créer dans Ollama le modèle spécialisé **`kaze-sensei:latest`** avec :
  - Paramètres d'inférence calibrés (`temperature 0.32`, `top_p 0.88`, `repeat_penalty 1.12`, `num_ctx 4096`).
  - Instructions système du Sensei et exemples *few-shot* intégrés pour garantir des réponses structurées et sobres.
- **Forge en 1 clic** depuis l'onglet **Sensei & IA** (bouton *« Forger kaze-sensei »*), via la commande `/forge`, ou en ligne de commande :
  ```bash
  cargo run --release -- --forge
  ```

### 4. ⚡ Agent Plus Autonome
- **Auto-Mémorisation (`auto_remember`)** : Lorsqu'au fil d'une conversation Kaze découvre un fait important sur vous (langage préféré, projet en cours, objectif d'apprentissage), il l'extrait et l'enregistre de lui-même dans sa mémoire durable (`memory.json`).
- **Initiatives Volontaires du Sensei** : Lorsque l'autonomie est active, le Sensei observe périodiquement votre état (ou répond immédiatement au bouton *« Initiative Sensei »* / `/sensei`) pour vous conseiller ou vous proposer une action concrète (ex. lancer un Pomodoro ou inspecter les processus gourmands).
- **Exécution Automatique Configurable** : Choisissez quelles familles d'actions (*Bilans système, Audio/Média, Minuteurs, Lecture de notes/fichiers, Applications, Shell*) s'exécutent directement sans confirmation manuelle.

### 5. ⭐ Progression, Titres d'Honneur & Sauvegarde Atomique
- **3 Voies d'Expérience** :
  - **Voie du Zazen** (Jade) : Souffles conscients et sessions de méditation.
  - **Voie de l'Éveil** (Or) : Énigmes du Sensei résolues et séries de bonnes réponses.
  - **Voie du Bushidō** (Sakura) : Pomodoros accomplis, minuteurs et maîtrise du système.
- **10 Rangs Globaux** (*Tamago* → *Deshi* → *Ashigaru* → *Bushi* → *Rōnin* → *Samouraï* → *Hatamoto* → *Kensei* → *Shōgun* → *Satori*) + **12 Titres d'Honneur (*Shōgō*)** déblocables et équipables à tout moment.
- **Conservation Garantie (`progress.json`)** : Sauvegarde atomique via fichier temporaire (`progress.json.tmp` puis renommage) doublée d'une copie de secours automatique (`progress.backup.json`) restaurée en cas d'arrêt brusque.

---

## 🖥️ Compatibilité Multi-Plateforme (Linux, macOS, Windows)

Kaze est conçu pour fonctionner nativement sur **Linux**, **macOS** et **Windows** :

| Fonctionnalité | 🐧 Linux (X11 / Wayland / KDE / GNOME) | 🍎 macOS ( Intel & Apple Silicon ) | 🪟 Windows (10 / 11) |
| :--- | :--- | :--- | :--- |
| **Fenêtre & Transparence** | `eframe` + règles KWin / X11 EWMH | Fenêtre transparente Cocoa Always-On-Top | Fenêtre transparente Win32 / DirectX |
| **Polices & Symboles** | DejaVu Sans / Noto Sans Symbols | Apple Symbols / Arial Unicode | Segoe UI Symbol / Segoe UI |
| **Contrôle Audio (`/vol`)** | `wpctl` / `pactl` / `amixer` | `osascript` (volume relatif/absolu & mute) | Touches média Win32 via PowerShell |
| **Contrôle Média (`/media`)** | `playerctl` | `osascript` (`Spotify` & `Music`) | Touches multimédias virtuelles Windows |
| **Capture d'écran (`/shot`)** | `spectacle` / `grim` / `gnome-screenshot` | `screencapture` natif | Presse-papiers / outils natifs |
| **Ouverture Fichiers / URLs** | `xdg-open` | `open` | `cmd /C start` / `explorer` |
| **Données Persistantes** | `~/.local/share/ai-pet/` | `~/Library/Application Support/ai-pet/` | `%APPDATA%\ai-pet\` |

---

## 🚀 Installation & Démarrage

### Prérequis
1. **[Rust](https://rustup.rs/)** (édition 2021, stable).
2. **[Ollama](https://ollama.com/)** installé sur votre machine.
3. Un modèle local téléchargé dans Ollama (par défaut `qwen2.5:7b` ou tout autre modèle) :
   ```bash
   ollama pull qwen2.5:7b
   ```

### Compilation & Lancement
```bash
# 1. Compiler et lancer Kaze en mode graphique
cargo run --release

# 2. (Recommandé) Forger le modèle spécialisé Sensei (`kaze-sensei:latest`)
cargo run --release -- --forge
```

---

## ⌨️ Commandes Slash & Raccourcis

Depuis la bulle de Kaze ou la barre de discussion du Dojo :

| Commande | Description |
| :--- | :--- |
| `/menu` ou `/dojo` | Ouvre ou ferme la grande fenêtre redimensionnable du Sanctuaire Dojo |
| `/eveil [sujet]` | Ouvre le Dojo d'Éveil et lance une énigme sur le sujet indiqué |
| `/zazen [coin]` | Lance la méditation Zazen guidée (ajoutez `coin` pour méditer dans le coin de l'écran) |
| `/xp` ou `/titres` | Ouvre directement l'onglet **Voies & Titres** (EXP, rangs et *Shōgō*) |
| `/sensei` | Demande immédiatement une **initiative volontaire** au Sensei |
| `/forge` | Forge / fine-tune le modèle Ollama `kaze-sensei:latest` depuis [`Modelfile.sensei`](./Modelfile.sensei) |
| `/role` | Ouvre l'onglet **Sensei & IA** pour consulter [`ROLE.md`](./ROLE.md) et régler l'autonomie |
| `/pomo [minutes]` | Démarre une session Pomodoro Zen (défaut : 25 min, +30 XP Bushidō à la fin) |
| `/timer 5m [nom]` | Lance un sablier personnalisé |
| `/sys`, `/top`, `/net`, `/bat` | Affiche le bilan système, les processus gourmands, le réseau ou la batterie |
| `/vol +10`, `/vol -10`, `/vol mute` | Ajuste ou coupe le volume système (Linux, macOS, Windows) |
| `/media toggle`, `/media next` | Contrôle le lecteur multimédia actif |
| `/zen` / `/echange` | Bascule la bulle entre le mode Zen épuré et le fil d'échange complet |
| `/balade` | Active ou désactive la marche autonome de Kaze sur le bureau |
| `/export [nom]` | Exporte la dernière réponse ou l'historique en fichier Markdown `.md` |
| `/quit` | Salue le Sensei et referme proprement le sanctuaire |

---

## 📂 Architecture du Projet

- [`ROLE.md`](./ROLE.md) : Charte identitaire, pédagogique et comportementale du Sensei Kaze.
- [`Modelfile.sensei`](./Modelfile.sensei) : Modelfile Ollama spécialisé (paramètres calibrés + *few-shot* Sensei).
- [`src/main.rs`](./src/main.rs) : Boucle d'application `eframe`, mode Compagnon transparent, fenêtre redimensionnable du Dojo, bannière volontaire du Sensei et gestion des 5 piliers.
- [`src/zen_ui.rs`](./src/zen_ui.rs) : Moteur graphique Pixel-Art Washi/Laque/Or, boutons et cartes QCM 100 % responsives, halo Zazen *Ensō* et particules.
- [`src/progress.rs`](./src/progress.rs) : Système d'EXP (3 Voies), 10 rangs, 12 Titres d'Honneur (*Shōgō*), calcul de la Directive du Sensei et sauvegarde atomique `progress.json` + backup.
- [`src/ollama.rs`](./src/ollama.rs) : Client Ollama, injection dynamique de [`ROLE.md`](./ROLE.md), forge de `kaze-sensei`, auto-mémorisation (`auto_remember`) et initiatives proactives.
- [`src/tasks.rs`](./src/tasks.rs) : Actions système multi-plateformes (Linux, macOS, Windows).
- [`src/awareness.rs`](./src/awareness.rs) : Conscience profonde du système, registre de minuteurs/Pomodoros, notes et mémoire long-terme (`memory.json`).
- [`src/sprites.rs`](./src/sprites.rs) : Découpage et animation des spritesheets Pixel-Art du samouraï et chargement de `assets/background_dojo.jpeg`.
