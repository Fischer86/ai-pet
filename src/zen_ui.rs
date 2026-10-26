use crate::sprites::PetMood;
use eframe::egui::{
    self, Align2, Color32, Context, CornerRadius, FontData, FontDefinitions, FontFamily, FontId,
    Painter, Pos2, Rect, Response, RichText, Stroke, Ui, Vec2,
};
use std::f32::consts::TAU;
use std::sync::Arc;

/// Nouvelle Direction Artistique : « Estampe Ukiyo-e, Papier Washi & Îlot Céleste »
pub struct ZenPalette;

impl ZenPalette {
    // --- Bulle de Dialogue (Papier Washi Ivoire & Encre de Chine Sumi-e) ---
    pub const WASHI_BG: Color32 = Color32::from_rgb(245, 238, 220);
    pub const WASHI_SHADE: Color32 = Color32::from_rgb(228, 217, 192);
    pub const WASHI_BUTTON: Color32 = Color32::from_rgb(236, 226, 202);
    pub const SUMI_INK: Color32 = Color32::from_rgb(26, 23, 21);
    pub const SUMI_MUTED: Color32 = Color32::from_rgb(102, 90, 78);

    // --- Accents Estampe Japonaise (Cinabre Hanko, Jade, Or & Indigo Hokusai) ---
    pub const VERMILION: Color32 = Color32::from_rgb(198, 56, 42);
    pub const JADE: Color32 = Color32::from_rgb(56, 142, 114);
    pub const JADE_LIGHT: Color32 = Color32::from_rgb(118, 204, 172);
    pub const GOLD_LEAF: Color32 = Color32::from_rgb(214, 166, 68);
    pub const SAKURA: Color32 = Color32::from_rgb(224, 122, 142);
    pub const CYAN_SPIRIT: Color32 = Color32::from_rgb(104, 186, 204);

    // --- Médaillons flottants & Talisman Ofuda (Bleu Indigo Nuit d'Estampe) ---
    pub const INDIGO_PILL: Color32 = Color32::from_rgba_premultiplied(22, 26, 38, 235);
    pub const INDIGO_CARD: Color32 = Color32::from_rgba_premultiplied(26, 31, 45, 244);
    pub const TEXT_LIGHT: Color32 = Color32::from_rgb(242, 236, 222);
    pub const TEXT_DIM: Color32 = Color32::from_rgb(168, 162, 150);

    pub fn mood_accent(mood: PetMood) -> Color32 {
        match mood {
            PetMood::Idle => Self::VERMILION,
            PetMood::Run => Self::GOLD_LEAF,
            PetMood::Jump => Self::SAKURA,
            PetMood::Attack => Color32::from_rgb(218, 72, 48),
            PetMood::Swim => Self::JADE,
            PetMood::Special => Self::CYAN_SPIRIT,
        }
    }
}

/// Charge la police Pixel-Art embarquée (`PixelifySans.ttf`) ainsi qu'une police
/// système de secours pour les symboles Unicode / emojis (afin d'éviter tout carré `□`).
pub fn install_pixel_fonts(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    let font_bytes = include_bytes!("../assets/fonts/PixelifySans.ttf");

    fonts.font_data.insert(
        "pixelify".to_owned(),
        Arc::new(FontData::from_static(font_bytes)),
    );

    if let Some(prop) = fonts.families.get_mut(&FontFamily::Proportional) {
        prop.insert(0, "pixelify".to_owned());
    }
    if let Some(mono) = fonts.families.get_mut(&FontFamily::Monospace) {
        mono.insert(0, "pixelify".to_owned());
    }

    // Polices système de secours pour les symboles et icônes Unicode
    let fallback_paths = [
        "/usr/share/fonts/gdouros-symbola/Symbola.ttf",
        "/usr/share/fonts/google-noto/NotoSansSymbols2-Regular.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ];
    for (idx, path) in fallback_paths.iter().enumerate() {
        if let Ok(bytes) = std::fs::read(path) {
            let name = format!("sys_sym_{}", idx);
            fonts
                .font_data
                .insert(name.clone(), Arc::new(FontData::from_owned(bytes)));
            if let Some(prop) = fonts.families.get_mut(&FontFamily::Proportional) {
                prop.push(name.clone());
            }
            if let Some(mono) = fonts.families.get_mut(&FontFamily::Monospace) {
                mono.push(name);
            }
        }
    }

    ctx.set_fonts(fonts);
}

/// Dessine un rectangle plein aligné sur une grille pixel.
#[inline]
pub fn fill_px(painter: &Painter, x: f32, y: f32, w: f32, h: f32, color: Color32) {
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    painter.rect_filled(
        Rect::from_min_size(
            Pos2::new(x.round(), y.round()),
            Vec2::new(w.round(), h.round()),
        ),
        CornerRadius::ZERO,
        color,
    );
}

/// Dessine une vraie bulle de BD / RPG en Pixel-Art sur papier Washi ivoire :
/// - Coins très arrondis en escalier pixel-art (aspect organique de nuage/phylactère, pas une boîte !)
/// - Texte encre de Chine ultra-lisible sur papier Washi clair
/// - Petit sceau rouge Hanko dans l'angle et queue de bulle pointant vers Kaze.
pub fn draw_washi_speech_bubble(
    painter: &Painter,
    rect: Rect,
    accent: Color32,
    alpha_mul: f32,
) {
    let p = 3.0_f32;
    let x = rect.min.x.round();
    let y = rect.min.y.round();
    let w = rect.width().round();
    let tail_h = p * 5.0; // 15px pour la queue de bulle
    let h = (rect.height() - tail_h).max(48.0).round();

    let apply_a = |c: Color32| -> Color32 {
        let [r, g, b, a] = c.to_array();
        Color32::from_rgba_unmultiplied(r, g, b, ((a as f32) * alpha_mul) as u8)
    };

    let ink = apply_a(ZenPalette::SUMI_INK);
    let washi = apply_a(ZenPalette::WASHI_BG);
    let shade = apply_a(ZenPalette::WASHI_SHADE);
    let seal = apply_a(accent);
    let shadow = apply_a(Color32::from_rgba_unmultiplied(12, 14, 22, 110));

    // 1. Ombre portée douce sous la bulle
    fill_px(painter, x + p * 4.0, y + p * 3.0, w - p * 6.0, h - p, shadow);
    fill_px(painter, x + p * 3.0, y + p * 4.0, w - p * 4.0, h - p * 3.0, shadow);

    // 2. Corps de la bulle en papier Washi (coins arrondis à 3 paliers de pixels)
    fill_px(painter, x + p * 3.0, y + p, w - p * 6.0, h - p * 2.0, washi);
    fill_px(painter, x + p * 2.0, y + p * 2.0, w - p * 4.0, h - p * 4.0, washi);
    fill_px(painter, x + p, y + p * 3.0, w - p * 2.0, h - p * 6.0, washi);

    // Léger ombrage parchemin en bas de la bulle
    fill_px(painter, x + p * 3.0, y + h - p * 3.0, w - p * 6.0, p * 2.0, shade);

    // 3. Bordure extérieure Encre de Chine Sumi-e (escalier 3 paliers)
    // Haut & Bas
    fill_px(painter, x + p * 3.0, y, w - p * 6.0, p, ink);
    fill_px(painter, x + p * 3.0, y + h - p, w - p * 6.0, p, ink);
    // Gauche & Droite
    fill_px(painter, x, y + p * 3.0, p, h - p * 6.0, ink);
    fill_px(painter, x + w - p, y + p * 3.0, p, h - p * 6.0, ink);
    // Marches d'arrondi (4 coins × 2 marches)
    fill_px(painter, x + p * 2.0, y + p, p, p, ink);
    fill_px(painter, x + p, y + p * 2.0, p, p, ink);
    fill_px(painter, x + w - p * 3.0, y + p, p, p, ink);
    fill_px(painter, x + w - p * 2.0, y + p * 2.0, p, p, ink);

    fill_px(painter, x + p * 2.0, y + h - p * 2.0, p, p, ink);
    fill_px(painter, x + p, y + h - p * 3.0, p, p, ink);
    fill_px(painter, x + w - p * 3.0, y + h - p * 2.0, p, p, ink);
    fill_px(painter, x + w - p * 2.0, y + h - p * 3.0, p, p, ink);

    // 4. Petit Sceau Rouge Hanko (ハンコ) en pixel-art dans le coin supérieur gauche
    let sx = x + p * 4.0;
    let sy = y + p * 3.0;
    fill_px(painter, sx, sy, p * 4.0, p * 4.0, seal);
    fill_px(painter, sx + p, sy + p, p * 2.0, p, washi);
    fill_px(painter, sx + p, sy + p * 2.5, p, p, washi);

    // 5. Queue de bulle incurvée en Pixel-Art pointant vers Kaze en bas
    let cx = (x + w * 0.5).round();
    let by = y + h - p;
    // Effacer la bordure noire sous l'attache de la queue
    fill_px(painter, cx - p * 2.0, by, p * 4.0, p, shade);
    // Intérieur Washi de la pointe
    fill_px(painter, cx - p * 2.0, by + p, p * 4.0, p, washi);
    fill_px(painter, cx - p, by + p * 2.0, p * 3.0, p, washi);
    fill_px(painter, cx, by + p * 3.0, p * 2.0, p, washi);
    // Contour encre de la pointe
    fill_px(painter, cx - p * 3.0, by + p, p, p, ink);
    fill_px(painter, cx + p * 2.0, by + p, p, p, ink);
    fill_px(painter, cx - p * 2.0, by + p * 2.0, p, p, ink);
    fill_px(painter, cx + p * 2.0, by + p * 2.0, p, p, ink);
    fill_px(painter, cx - p, by + p * 3.0, p, p, ink);
    fill_px(painter, cx + p * 2.0, by + p * 3.0, p, p, ink);
    fill_px(painter, cx, by + p * 4.0, p * 2.0, p, ink);
}

/// Plaque Pixel-Art générique (style talisman Ofuda indigo) : base commune de tous les
/// panneaux de l'interface (barre de modes, HUD de niveau, panneau du mode actif)
/// pour garantir une cohérence visuelle parfaite.
/// `top_accent` dessine un bandeau coloré en haut (couleur du mode actif).
pub fn draw_pixel_plate(
    painter: &Painter,
    rect: Rect,
    fill: Color32,
    border: Color32,
    top_accent: Option<Color32>,
) {
    let p = 3.0_f32;
    let x = rect.min.x.round();
    let y = rect.min.y.round();
    let w = rect.width().round();
    let h = rect.height().round();

    // Ombre douce
    fill_px(
        painter,
        x + p * 2.0,
        y + p * 2.0,
        w - p * 2.0,
        h - p * 2.0,
        Color32::from_rgba_unmultiplied(8, 10, 16, 120),
    );

    // Corps arrondi en escalier
    fill_px(painter, x + p * 2.0, y + p, w - p * 4.0, h - p * 2.0, fill);
    fill_px(painter, x + p, y + p * 2.0, w - p * 2.0, h - p * 4.0, fill);

    // Bandeau d'accent supérieur (couleur du mode)
    if let Some(accent) = top_accent {
        fill_px(painter, x + p * 2.0, y + p, w - p * 4.0, p * 2.0, accent);
    }

    // Bordure pixel-art
    fill_px(painter, x + p * 2.0, y, w - p * 4.0, p, border);
    fill_px(painter, x + p * 2.0, y + h - p, w - p * 4.0, p, border);
    fill_px(painter, x, y + p * 2.0, p, h - p * 4.0, border);
    fill_px(painter, x + w - p, y + p * 2.0, p, h - p * 4.0, border);
    fill_px(painter, x + p, y + p, p, p, border);
    fill_px(painter, x + w - p * 2.0, y + p, p, p, border);
    fill_px(painter, x + p, y + h - p * 2.0, p, p, border);
    fill_px(painter, x + w - p * 2.0, y + h - p * 2.0, p, p, border);
}

/// Jauge segmentée en Pixel-Art (XP, souffle, progression des voies).
pub fn draw_pixel_bar(painter: &Painter, rect: Rect, ratio: f32, color: Color32) {
    let ratio = ratio.clamp(0.0, 1.0);
    let x = rect.min.x.round();
    let y = rect.min.y.round();
    let w = rect.width().round();
    let h = rect.height().round();

    // Cadre encre + fond creusé
    fill_px(painter, x, y, w, h, ZenPalette::SUMI_INK);
    fill_px(
        painter,
        x + 2.0,
        y + 2.0,
        w - 4.0,
        h - 4.0,
        Color32::from_rgb(12, 14, 22),
    );

    // Segments de 5px séparés d'1px
    let inner_w = w - 4.0;
    let seg = 6.0;
    let count = (inner_w / seg).floor().max(1.0) as i32;
    let filled = (ratio * count as f32).round() as i32;
    let highlight = Color32::from_rgba_unmultiplied(255, 255, 255, 70);
    for i in 0..filled {
        let sx = x + 2.0 + i as f32 * seg;
        fill_px(painter, sx, y + 2.0, seg - 1.0, h - 4.0, color);
        fill_px(painter, sx, y + 2.0, seg - 1.0, 1.0, highlight);
    }
}

/// Texte avec contour encre (lisible sur n'importe quel fond : popups d'XP, bannières).
pub fn draw_outlined_text(
    painter: &Painter,
    pos: Pos2,
    align: Align2,
    text: &str,
    size: f32,
    color: Color32,
) {
    let font = FontId::new(size, FontFamily::Proportional);
    let ink = Color32::from_rgba_unmultiplied(14, 12, 18, color.a());
    for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0), (1.0, 1.0)] {
        painter.text(pos + Vec2::new(dx, dy), align, text, font.clone(), ink);
    }
    painter.text(pos, align, text, font, color);
}

/// Styles de boutons unifiés de l'interface (une seule grammaire visuelle partout).
#[derive(Debug, Clone, Copy)]
pub enum BtnKind {
    /// Action principale (cinabre)
    Primary,
    /// Action secondaire (indigo)
    Secondary,
    /// Onglet / option sélectionné(e) (papier Washi + bordure couleur du mode)
    Selected(Color32),
    /// Bouton posé sur la bulle de papier Washi
    Paper,
    /// Validation (jade)
    Success,
    /// Refus / annulation (cinabre)
    Danger,
}

pub fn zen_button(ui: &mut Ui, text: &str, kind: BtnKind, size: Vec2) -> Response {
    let (fill, fg, stroke_col, stroke_w) = match kind {
        BtnKind::Primary => (
            ZenPalette::VERMILION,
            ZenPalette::WASHI_BG,
            ZenPalette::SUMI_INK,
            1.0_f32,
        ),
        BtnKind::Secondary => (
            ZenPalette::INDIGO_PILL,
            ZenPalette::TEXT_LIGHT,
            Color32::from_rgba_unmultiplied(214, 166, 68, 90),
            1.0_f32,
        ),
        BtnKind::Selected(c) => (ZenPalette::WASHI_BG, ZenPalette::SUMI_INK, c, 2.0_f32),
        BtnKind::Paper => (
            ZenPalette::WASHI_BUTTON,
            ZenPalette::SUMI_INK,
            ZenPalette::SUMI_MUTED,
            1.0_f32,
        ),
        BtnKind::Success => (
            ZenPalette::JADE,
            ZenPalette::WASHI_BG,
            ZenPalette::SUMI_INK,
            1.0_f32,
        ),
        BtnKind::Danger => (
            ZenPalette::VERMILION,
            ZenPalette::WASHI_BG,
            ZenPalette::SUMI_INK,
            1.0_f32,
        ),
    };
    ui.add(
        egui::Button::new(RichText::new(text).size(11.5).color(fg))
            .fill(fill)
            .stroke(Stroke::new(stroke_w, stroke_col))
            .corner_radius(CornerRadius::ZERO)
            .min_size(size),
    )
}

/// Dessine un encart de saisie sur parchemin Washi (lisible, doux, sans boîte noire).
pub fn draw_washi_input_box(painter: &Painter, rect: Rect, focused: bool) {
    let x = rect.min.x.round();
    let y = rect.min.y.round();
    let w = rect.width().round();
    let h = rect.height().round();

    let bg = Color32::from_rgb(235, 225, 200);
    let border = if focused {
        ZenPalette::VERMILION
    } else {
        ZenPalette::SUMI_MUTED
    };

    fill_px(painter, x + 2.0, y, w - 4.0, h, bg);
    fill_px(painter, x, y + 2.0, w, h - 4.0, bg);

    // Bordure pixel fine
    fill_px(painter, x + 2.0, y, w - 4.0, 1.0, border);
    fill_px(painter, x + 2.0, y + h - 1.0, w - 4.0, 1.0, border);
    fill_px(painter, x, y + 2.0, 1.0, h - 4.0, border);
    fill_px(painter, x + w - 1.0, y + 2.0, 1.0, h - 4.0, border);
}

/// Dessine un îlot flottant en Pixel-Art sous les pieds de Kaze :
/// strates rocheuses suspendues, mousse de jade, lianes tombantes, lanterne de pierre (Tōrō) animée,
/// et évolution zen progressive du sanctuaire au fil des niveaux (bambous, fleurs, bonsaï, lucioles).
pub fn draw_pixel_floating_island(
    painter: &Painter,
    top_center: Pos2,
    time: f32,
    mood_accent: Color32,
    level: u32,
) {
    let p = 3.0_f32;
    let cx = top_center.x.round();
    let ty = (top_center.y - 10.0).round();

    let moss_top = Color32::from_rgb(92, 166, 118);
    let moss_dark = Color32::from_rgb(54, 115, 82);
    let rock_light = Color32::from_rgb(76, 72, 84);
    let rock_mid = Color32::from_rgb(52, 49, 60);
    let rock_deep = Color32::from_rgb(34, 31, 40);
    let ink = ZenPalette::SUMI_INK;

    // --- 1. Petite Lanterne de Pierre Japonaise (Tōrō) à gauche sur l'îlot ---
    let lx = cx - p * 17.0;
    let ly = ty - p * 9.0;
    // Socle & pilier de la lanterne
    fill_px(painter, lx, ly + p * 6.0, p * 3.0, p * 3.0, rock_mid);
    fill_px(painter, lx + p, ly + p * 4.0, p, p * 2.0, rock_light);
    // Chambre à feu + lueur vacillante
    let glow_alpha = ((time * 4.2).sin() * 40.0 + 195.0) as u8;
    let flame = Color32::from_rgba_unmultiplied(255, 200, 85, glow_alpha);
    let halo = Color32::from_rgba_unmultiplied(
        mood_accent.r(),
        mood_accent.g(),
        mood_accent.b(),
        55,
    );
    fill_px(painter, lx - p * 2.0, ly, p * 7.0, p * 6.0, halo);
    fill_px(painter, lx, ly + p * 2.0, p * 3.0, p * 2.0, flame);
    // Toit de pierre en pagode de la lanterne
    fill_px(painter, lx - p, ly + p, p * 5.0, p, rock_light);
    fill_px(painter, lx, ly, p * 3.0, p, rock_mid);
    fill_px(painter, lx + p, ly - p, p, p, rock_light);

    // --- 2. Évolution Zen de l'îlot à droite selon le Niveau ---
    if level >= 2 {
        // Tiges de bambou souples à droite de l'îlot
        let sway = ((time * 1.8).sin() * 1.5).round();
        let bx = cx + p * 15.0;
        let bamboo_h = if level >= 4 { 8.0 } else { 5.0 };
        fill_px(painter, bx, ty - p * bamboo_h, p, p * bamboo_h, moss_dark);
        fill_px(painter, bx + sway, ty - p * (bamboo_h + 1.0), p * 2.0, p, moss_top);
        if level >= 4 {
            fill_px(painter, bx + p * 2.0, ty - p * 5.0, p, p * 5.0, moss_top);
            fill_px(painter, bx + p + sway, ty - p * 6.0, p * 2.0, p, ZenPalette::JADE_LIGHT);
        }
    }

    if level >= 6 {
        // Petit Bonsaï / Érable japonais Momiji au bord droit
        let tx = cx + p * 14.0;
        let trunk = Color32::from_rgb(88, 60, 46);
        fill_px(painter, tx, ty - p * 4.0, p, p * 4.0, trunk);
        fill_px(painter, tx - p, ty - p * 3.0, p, p, trunk);
        // Feuillage Sakura / Momiji
        fill_px(painter, tx - p * 2.0, ty - p * 6.0, p * 5.0, p * 2.0, ZenPalette::SAKURA);
        fill_px(painter, tx - p, ty - p * 7.0, p * 3.0, p, ZenPalette::VERMILION);
    }

    // --- 3. Tapis de mousse Zen & pétales au sommet de l'îlot ---
    fill_px(painter, cx - p * 20.0, ty, p * 40.0, p * 2.0, moss_top);
    fill_px(painter, cx - p * 21.0, ty + p, p * 42.0, p, moss_dark);
    // Quelques pétales de sakura posés sur l'herbe
    fill_px(painter, cx + p * 12.0, ty, p * 2.0, p, ZenPalette::SAKURA);
    fill_px(painter, cx - p * 8.0, ty, p, p, ZenPalette::SAKURA);
    if level >= 3 {
        fill_px(painter, cx + p * 5.0, ty - p, p, p, ZenPalette::GOLD_LEAF);
    }

    // --- 4. Strates rocheuses flottantes en cône inversé ---
    fill_px(painter, cx - p * 19.0, ty + p * 2.0, p * 38.0, p * 2.0, rock_light);
    fill_px(painter, cx - p * 16.0, ty + p * 4.0, p * 32.0, p * 2.0, rock_mid);
    fill_px(painter, cx - p * 11.0, ty + p * 6.0, p * 22.0, p * 2.0, rock_deep);
    fill_px(painter, cx - p * 6.0, ty + p * 8.0, p * 12.0, p, ink);

    // Lianes de mousse suspendues à droite et à gauche
    fill_px(painter, cx - p * 14.0, ty + p * 2.0, p, p * 5.0, moss_dark);
    fill_px(painter, cx + p * 13.0, ty + p * 2.0, p, p * 6.0, moss_top);
    fill_px(painter, cx + p * 15.0, ty + p * 2.0, p, p * 4.0, moss_dark);

    // Petit rocher satellite flottant sous l'îlot
    let sub_bob = ((time * 2.2).cos() * 2.5).round();
    fill_px(
        painter,
        cx + p * 10.0,
        ty + p * 9.0 + sub_bob,
        p * 4.0,
        p * 2.0,
        rock_mid,
    );
    if level >= 5 {
        let sub_bob2 = ((time * 1.9 + 1.4).sin() * 2.5).round();
        fill_px(
            painter,
            cx - p * 13.0,
            ty + p * 8.0 + sub_bob2,
            p * 3.0,
            p * 2.0,
            rock_light,
        );
    }
}

/// Phase de respiration du Vrai Mode Méditation (Zazen).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreathPhase {
    Inhale, // 4s : Inspirer
    Hold,   // 3s : Retenir
    Exhale, // 5s : Expirer
}

impl BreathPhase {
    /// Calcule la phase actuelle, le ratio d'expansion pulmonaire (0.0 à 1.0)
    /// et le nombre de cycles complets à partir du temps écoulé en méditation.
    pub fn compute(meditation_secs: f32) -> (Self, f32, u32) {
        let cycle_len = 12.0_f32; // 4s inspiration + 3s rétention + 5s expiration
        let cycles = (meditation_secs / cycle_len).floor() as u32;
        let t = meditation_secs % cycle_len;

        if t < 4.0 {
            // Courbe sinusoïdale douce de 0.0 vers 1.0
            let p = t / 4.0;
            let smooth = 0.5 - 0.5 * (p * std::f32::consts::PI).cos();
            (Self::Inhale, smooth, cycles)
        } else if t < 7.0 {
            (Self::Hold, 1.0, cycles)
        } else {
            // Courbe sinusoïdale douce de 1.0 vers 0.0
            let p = (t - 7.0) / 5.0;
            let smooth = 0.5 + 0.5 * (p * std::f32::consts::PI).cos();
            (Self::Exhale, smooth, cycles)
        }
    }

    #[allow(dead_code)]
    pub fn instruction(&self) -> &'static str {
        match self {
            Self::Inhale => "🌬️ Inspirez lentement par le nez…",
            Self::Hold => "✨ Retenez votre souffle en paix…",
            Self::Exhale => "🍃 Expirez doucement et relâchez…",
        }
    }

    pub fn color(&self) -> Color32 {
        match self {
            Self::Inhale => ZenPalette::JADE_LIGHT,
            Self::Hold => ZenPalette::GOLD_LEAF,
            Self::Exhale => ZenPalette::SAKURA,
        }
    }
}

/// Dessine l'Enso (円相 — cercle calligraphié Zen en pixel-art) et l'onde de respiration
/// concentrique autour de Kaze pendant le Mode Méditation Zazen.
pub fn draw_zazen_meditation_halo(
    painter: &Painter,
    center: Pos2,
    time: f32,
    phase: BreathPhase,
    breath_ratio: f32,
) {
    let p = 3.0_f32;
    let phase_col = phase.color();

    // 1. Cercle Enso fixe en calligraphie Pixel-Art (rayon 58px, ouvert en bas à droite comme un vrai Enso)
    let enso_r = 58.0_f32;
    let steps = 56;
    for i in 0..steps {
        let frac = i as f32 / steps as f32;
        // Ouverture traditionnelle du pinceau Enso entre 0.88 et 1.0
        if frac > 0.88 {
            continue;
        }
        let angle = frac * TAU + 0.4;
        let thickness = if frac < 0.15 {
            p * 2.0
        } else if frac > 0.72 {
            p
        } else {
            p * 1.5
        };
        let px = center.x + angle.cos() * enso_r;
        let py = center.y + angle.sin() * enso_r;
        let alpha = ((1.0 - frac * 0.45) * 185.0) as u8;
        let c = Color32::from_rgba_unmultiplied(
            ZenPalette::WASHI_BG.r(),
            ZenPalette::WASHI_BG.g(),
            ZenPalette::WASHI_BG.b(),
            alpha,
        );
        fill_px(painter, px, py, thickness, thickness, c);
    }

    // 2. Anneau de Respiration dynamique (s'élargit à l'inspiration de 28px à 54px, se contracte à l'expiration)
    let breath_r = 28.0 + breath_ratio * 26.0;
    let ring_steps = 36;
    for i in 0..ring_steps {
        let angle = (i as f32 / ring_steps as f32) * TAU - time * 0.35;
        let px = center.x + angle.cos() * breath_r;
        let py = center.y + angle.sin() * breath_r;
        let c = Color32::from_rgba_unmultiplied(
            phase_col.r(),
            phase_col.g(),
            phase_col.b(),
            (110.0 + breath_ratio * 110.0) as u8,
        );
        fill_px(painter, px, py, p, p, c);
    }

    // 3. Particules d'esprit en orbite lente autour du Samouraï méditant
    for i in 0..6 {
        let fi = i as f32;
        let a = time * 0.7 + fi * (TAU / 6.0);
        let r = 66.0 + (time * 1.5 + fi).sin() * 4.0;
        let px = center.x + a.cos() * r;
        let py = center.y + a.sin() * r;
        fill_px(painter, px, py, p, p, phase_col);
    }
}

/// Système de particules Zen en Pixel-Art (pétales de sakura & lucioles dorées).
pub struct ZenParticles {
    particles: Vec<Particle>,
}

struct Particle {
    pos: Pos2,
    vel: Vec2,
    phase: f32,
    size: f32,
    color: Color32,
}

impl ZenParticles {
    pub fn new() -> Self {
        let palette = [
            Color32::from_rgba_unmultiplied(235, 138, 154, 175), // Pétale Sakura
            Color32::from_rgba_unmultiplied(118, 204, 172, 155), // Feuille de Jade
            Color32::from_rgba_unmultiplied(214, 166, 68, 180),  // Luciole dorée
            Color32::from_rgba_unmultiplied(245, 238, 220, 140), // Esprit Washi
        ];
        let mut particles = Vec::with_capacity(10);
        for i in 0..10 {
            let fi = i as f32;
            particles.push(Particle {
                pos: Pos2::new(60.0 + (fi * 31.0) % 280.0, 80.0 + (fi * 23.0) % 180.0),
                vel: Vec2::new(0.14 + (fi % 3.0) * 0.06, -0.18 - (fi % 2.0) * 0.10),
                phase: fi * 0.9,
                size: if i % 2 == 0 { 3.0 } else { 4.0 },
                color: palette[i % palette.len()],
            });
        }
        Self { particles }
    }

    pub fn update_and_draw(&mut self, painter: &Painter, center: Pos2, radius: f32, time: f32) {
        for p in &mut self.particles {
            let sway = (time * 1.4 + p.phase).sin() * 0.35;
            p.pos.x += p.vel.x + sway;
            p.pos.y += p.vel.y;

            if p.pos.y < center.y - radius || p.pos.y > center.y + radius {
                p.pos.y = center.y + radius * 0.6;
                p.pos.x = center.x + ((p.phase * 53.0 + time * 17.0).sin() * radius * 0.85);
            }
            if (p.pos.x - center.x).abs() > radius {
                p.pos.x = center.x - (p.pos.x - center.x).signum() * (radius * 0.8);
            }

            let pulse = ((time * 2.0 + p.phase).sin() * 0.3 + 0.7).clamp(0.25, 1.0);
            let [r, g, b, a] = p.color.to_array();
            let c = Color32::from_rgba_unmultiplied(r, g, b, ((a as f32) * pulse) as u8);

            fill_px(painter, p.pos.x, p.pos.y, p.size, p.size, c);
        }
    }
}
