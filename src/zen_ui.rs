use crate::sprites::PetMood;
use eframe::egui::{
    Color32, Context, CornerRadius, FontData, FontDefinitions, FontFamily, Painter, Pos2, Rect,
    Vec2,
};
use std::sync::Arc;

/// Palette « Jardin Zen / Parchemin Nocturne »
pub struct ZenPalette;

impl ZenPalette {
    pub const BG_MAIN: Color32 = Color32::from_rgba_premultiplied(30, 28, 36, 245);
    pub const BG_HEADER: Color32 = Color32::from_rgba_premultiplied(38, 35, 46, 245);
    pub const BG_INPUT: Color32 = Color32::from_rgba_premultiplied(20, 19, 25, 245);
    pub const INK_DARK: Color32 = Color32::from_rgb(18, 16, 22);
    pub const TEXT_PARCHMENT: Color32 = Color32::from_rgb(239, 230, 213);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(168, 159, 145);
    pub const BAMBOO_GREEN: Color32 = Color32::from_rgb(143, 184, 150);
    pub const SAKURA_PINK: Color32 = Color32::from_rgb(232, 153, 166);
    pub const LANTERN_GOLD: Color32 = Color32::from_rgb(217, 185, 120);

    pub fn mood_accent(mood: PetMood) -> Color32 {
        match mood {
            PetMood::Idle => Self::LANTERN_GOLD,
            PetMood::Run => Color32::from_rgb(226, 168, 86),
            PetMood::Jump => Self::SAKURA_PINK,
            PetMood::Attack => Color32::from_rgb(224, 122, 95),
            PetMood::Swim => Color32::from_rgb(125, 196, 200),
            PetMood::Special => Color32::from_rgb(181, 155, 224),
        }
    }
}

/// Charge la police Pixel-Art embarquée (`PixelifySans.ttf`) tout en conservant
/// les polices de secours d'egui pour les icônes et emojis.
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

    ctx.set_fonts(fonts);
}

/// Dessine un rectangle plein aligné sur une grille pixel.
#[inline]
fn fill_px(painter: &Painter, x: f32, y: f32, w: f32, h: f32, color: Color32) {
    painter.rect_filled(
        Rect::from_min_size(Pos2::new(x.round(), y.round()), Vec2::new(w.round(), h.round())),
        CornerRadius::ZERO,
        color,
    );
}

/// Dessine une bulle de dialogue en Pixel-Art authentique (coins en escalier de 4px,
/// double bordure encrée + dorure bambou/humeur, ornements d'angle et flèche pixel-art).
pub fn draw_pixel_speech_bubble(
    painter: &Painter,
    rect: Rect,
    accent: Color32,
    alpha_mul: f32,
) {
    let p = 3.0_f32; // Taille d'un "méga-pixel" de bordure (3px)
    let x = rect.min.x.round();
    let y = rect.min.y.round();
    let w = rect.width().round();
    let h = rect.height().round();

    let apply_a = |c: Color32| -> Color32 {
        let [r, g, b, a] = c.to_array();
        Color32::from_rgba_unmultiplied(r, g, b, ((a as f32) * alpha_mul) as u8)
    };

    let ink = apply_a(ZenPalette::INK_DARK);
    let bg = apply_a(ZenPalette::BG_MAIN);
    let border = apply_a(accent);
    let inner_glow = apply_a(Color32::from_rgba_unmultiplied(
        accent.r(),
        accent.g(),
        accent.b(),
        45,
    ));

    // 1. Ombre portée pixel-art douce (décalée de +3px, +4px)
    let shadow = apply_a(Color32::from_rgba_unmultiplied(10, 9, 14, 115));
    fill_px(painter, x + p * 2.0, y + p * 2.0, w - p * 2.0, h - p, shadow);

    // 2. Fond principal (découpé aux 4 coins pour l'effet escalier pixel-art)
    fill_px(painter, x + p * 2.0, y + p, w - p * 4.0, h - p * 2.0, bg);
    fill_px(painter, x + p, y + p * 2.0, w - p * 2.0, h - p * 4.0, bg);

    // Bandeau supérieur subtil (style rouleau kakemono)
    let header_bg = apply_a(ZenPalette::BG_HEADER);
    fill_px(painter, x + p * 2.0, y + p * 2.0, w - p * 4.0, 28.0, header_bg);

    // 3. Contour extérieur Encre de Chine (escalier pixel)
    // Haut & Bas
    fill_px(painter, x + p * 2.0, y, w - p * 4.0, p, ink);
    fill_px(painter, x + p * 2.0, y + h - p, w - p * 4.0, p, ink);
    // Gauche & Droite
    fill_px(painter, x, y + p * 2.0, p, h - p * 4.0, ink);
    fill_px(painter, x + w - p, y + p * 2.0, p, h - p * 4.0, ink);
    // Marches d'escalier des 4 coins (extérieur)
    fill_px(painter, x + p, y + p, p, p, ink);
    fill_px(painter, x + w - p * 2.0, y + p, p, p, ink);
    fill_px(painter, x + p, y + h - p * 2.0, p, p, ink);
    fill_px(painter, x + w - p * 2.0, y + h - p * 2.0, p, p, ink);

    // 4. Bordure intérieure lumineuse (Bambou / Humeur)
    fill_px(painter, x + p * 2.0, y + p, w - p * 4.0, p, border);
    fill_px(painter, x + p * 2.0, y + h - p * 2.0, w - p * 4.0, p, border);
    fill_px(painter, x + p, y + p * 2.0, p, h - p * 4.0, border);
    fill_px(painter, x + w - p * 2.0, y + p * 2.0, p, h - p * 4.0, border);

    // Ornements pixel-art dans les 4 coins intérieurs
    fill_px(painter, x + p * 2.0, y + p * 2.0, p, p, border);
    fill_px(painter, x + w - p * 3.0, y + p * 2.0, p, p, border);
    fill_px(painter, x + p * 2.0, y + h - p * 3.0, p, p, border);
    fill_px(painter, x + w - p * 3.0, y + h - p * 3.0, p, p, border);

    // Ligne de séparation sous l'en-tête
    fill_px(painter, x + p * 3.0, y + 32.0, w - p * 6.0, 2.0, inner_glow);

    // 5. Queue de la bulle en Pixel-Art (pointant vers Kaze en bas au centre)
    let cx = (x + w * 0.5).round();
    let by = y + h - p * 2.0;
    // Remplissage de la pointe en 3 paliers de pixels
    fill_px(painter, cx - p * 2.0, by, p * 4.0, p, bg);
    fill_px(painter, cx - p, by + p, p * 2.0, p, bg);
    // Bordure colorée + encre de la pointe
    fill_px(painter, cx - p * 3.0, by, p, p, border);
    fill_px(painter, cx + p * 2.0, by, p, p, border);
    fill_px(painter, cx - p * 2.0, by + p, p, p, border);
    fill_px(painter, cx + p, by + p, p, p, border);
    fill_px(painter, cx - p, by + p * 2.0, p * 2.0, p, border);
    fill_px(painter, cx - p, by + p * 3.0, p * 2.0, p, ink);
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
            Color32::from_rgba_unmultiplied(232, 153, 166, 150), // Sakura
            Color32::from_rgba_unmultiplied(143, 184, 150, 140), // Feuille de bambou
            Color32::from_rgba_unmultiplied(217, 185, 120, 160), // Luciole dorée
            Color32::from_rgba_unmultiplied(239, 230, 213, 120), // Esprit zen
        ];
        let mut particles = Vec::with_capacity(10);
        for i in 0..10 {
            let fi = i as f32;
            particles.push(Particle {
                pos: Pos2::new(40.0 + (fi * 31.0) % 290.0, 210.0 + (fi * 19.0) % 120.0),
                vel: Vec2::new(0.15 + (fi % 3.0) * 0.08, -0.22 - (fi % 2.0) * 0.12),
                phase: fi * 0.9,
                size: if i % 2 == 0 { 3.0 } else { 4.0 },
                color: palette[i % palette.len()],
            });
        }
        Self { particles }
    }

    pub fn update_and_draw(&mut self, painter: &Painter, bounds: Rect, time: f32) {
        for p in &mut self.particles {
            let sway = (time * 1.4 + p.phase).sin() * 0.35;
            p.pos.x += p.vel.x + sway;
            p.pos.y += p.vel.y;

            if p.pos.y < bounds.max.y - 140.0 {
                p.pos.y = bounds.max.y - 16.0;
                p.pos.x = bounds.center().x + ((p.phase * 53.0 + time * 17.0).sin() * 75.0);
            }
            if p.pos.x < bounds.min.x + 15.0 {
                p.pos.x = bounds.max.x - 20.0;
            } else if p.pos.x > bounds.max.x - 15.0 {
                p.pos.x = bounds.min.x + 20.0;
            }

            // Scintillement doux
            let pulse = ((time * 2.0 + p.phase).sin() * 0.3 + 0.7).clamp(0.2, 1.0);
            let [r, g, b, a] = p.color.to_array();
            let c = Color32::from_rgba_unmultiplied(r, g, b, ((a as f32) * pulse) as u8);

            fill_px(painter, p.pos.x, p.pos.y, p.size, p.size, c);
        }
    }
}
