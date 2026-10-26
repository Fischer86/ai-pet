# 🎋 Kaze (風) — Compagnon AI Pet Samouraï Zen & Centre de Contrôle en Rust

![Samurai Sprite](assets/Samurai/5x/idle_0.png) ![Samurai Run](assets/Samurai/5x/run_1.png) ![Samurai Jump](assets/Samurai/5x/jump_2.png) ![Samurai Attack](assets/Samurai/5x/attack_3.png) ![Samurai Meditate](assets/Samurai/5x/swim_1.png)

**Kaze (風)** est un compagnon de bureau (**AI Pet**) écrit en **Rust** (`eframe`/`egui`, `sysinfo`, `arboard`, `x11rb`) doté d'une interface **Pixel-Art Zen** épurée, d'une **bulle de dialogue unique tout-en-un**, d'un **Centre de Contrôle (/menu)** sous forme de vraie fenêtre dédiée, d'un **système de progression RPG/Zen (XP, Niveaux, Rangs Samouraï)**, d'un **Dojo d'apprentissage interactif piloté par l'IA**, d'un **mode de méditation guidée Zazen**, et d'un **gestionnaire d'autorisations (Human-in-the-Loop)** sécurisé compatible **Linux (KDE Plasma Wayland/X11), macOS et Windows**.

---

## ✨ Fonctionnalités Clés

### 🏮 1. Expérience de Bureau Épurée & Bulle Unique Tout-en-Un
- **Présence discrète sur le bureau** : Au repos, vous ne voyez **que Kaze** sur son îlot flottant pixel-art avec sa jauge d'XP sous lui. Aucune grande boîte permanente n'encombre l'écran.
- **Double-clic sur Kaze** : Ouvre / ferme **une seule bulle de dialogue** située **juste au-dessus du personnage**, sa queue descendant précisément vers son chapeau de paille sans aucun vide.
- **Tout dans la même bulle** :
  - **En-tête** : Sceau Hanko, humeur, boutons d'action (`[Copier]`, `[.md]`, `[Ouvrir]`, `[Menu]`, `[x]`).
  - **Bascule de mode d'affichage** :
    - **Mode Zen** : Affiche uniquement la dernière demande et la réponse actuelle (zéro distraction).
    - **Mode Échange** : Affiche le fil de discussion complet défilant.
  - **Zone de saisie intégrée** : Champ de commande et de dialogue directement au bas de la bulle (`Parle à Kaze (/menu, /zazen, /eveil)...`) avec touche Entrée ou bouton `>`.

### 🎴 2. Le Centre de Contrôle (`/menu`) — Vraie Fenêtre Dédiée
Accessible par la commande `/menu` ou en cliquant sur le bouton **`[Menu]`** de la bulle / de la jauge d'XP. Ouvre une vraie fenêtre de contrôle avec 6 onglets complets :
1. **🏯 Éveil (Dojo d'apprentissage IA)** : Choix de sujets (Philosophie Zen, Rust, Histoire, Linux...), génération de quiz à 4 choix via Ollama, feedback instantané avec explication détaillée et récompenses en XP.
2. **🧘 Zazen (Méditation guidée)** : Suivi en direct du cycle 4-3-5 (Inspiration 4s, Rétention 3s, Expiration 5s), Enso calligraphié, chrono, bouton pour envoyer Kaze dans un coin d'écran (`[Coin écran]`) ou le recentrer (`[Recentrer]`).
3. **🎴 Actions Rapides** : Pomodoros 25m, Sablier thé 3m, Pause 5m, Bilans système (CPU, RAM, Réseau, Batterie), Audio (Volume +/-, Muet), Capture d'écran, Terminal, Presse-papiers.
4. **📜 Historique & Contexte** : Consultation de tous les échanges, export complet en `.md`, vidage d'historique et état environnemental en direct.
5. **🛡️ Autorisations (Permissions & Auto-Approbation)** : Configuration détaillée des actions autorisées automatiquement sans pop-up de confirmation vs celles requérant une validation explicite.
6. **🧠 Mémoire & Compétences** : Mémorisation durable de faits (`memory.json`), consultation des souvenirs, changement de modèle IA Ollama actif et catalogue des skills.

### 🏆 3. Progression, Rangs Samouraï & 3 Voies de Sagesse
- **Niveaux (1 à 50+) & 8 Rangs** : Progression de `Deshi (Disciple)` à `Satori (Éveillé)`.
- **3 Voies d'XP Dédiées** :
  - 🧘 **Voie du Zazen** : +2 XP par souffle complet complété, bonus de session.
  - 📚 **Voie de l'Éveil** : +15 XP par bonne réponse au Dojo, bonus de série (streaks).
  - ⚔️ **Voie du Bushidō** : +30 XP par cycle Pomodoro, +6 XP par minuteur, actions système et fidélité quotidienne.
- **Popups d'XP Flottants** animés au-dessus de l'îlot.

---

## 📦 Prérequis & Installation

1. **Rust** (1.75+) :
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```
2. **Ollama** avec le modèle léger par défaut `qwen2.5:3b` :
   ```bash
   ollama pull qwen2.5:3b
   ```

---

## 🚀 Compilation & Lancement

```bash
# Lancer l'application
cargo run

# Compiler un binaire optimisé pour la production
cargo build --release
./target/release/ai-pet

# Aide en ligne de commande
cargo run -- --help

# Mode CLI sans interface graphique
cargo run -- --cli "Liste les fichiers du dossier courant" --yes
```

---

## 📜 Licence

Distribué sous licence **MIT**. Voir le fichier [LICENSE](LICENSE).
