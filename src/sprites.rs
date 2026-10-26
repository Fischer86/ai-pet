use egui::{ColorImage, Context, TextureHandle, TextureOptions};
use image::DynamicImage;
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PetMood {
    Idle,
    Run,
    Jump,
    Attack,
    Swim,
    Special,
}

impl PetMood {
    pub fn from_str(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "run" | "walk" => Self::Run,
            "jump" | "happy" => Self::Jump,
            "attack" | "slash" | "action" => Self::Attack,
            "swim" | "think" | "thinking" => Self::Swim,
            "special" | "x" | "magic" | "pose" => Self::Special,
            _ => Self::Idle,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Idle => "Sérénité",
            Self::Run => "Exploration",
            Self::Jump => "Joie",
            Self::Attack => "Action",
            Self::Swim => "Méditation",
            Self::Special => "Salut Zen",
        }
    }

    pub fn frame_duration(&self) -> Duration {
        match self {
            Self::Idle => Duration::from_millis(200),
            Self::Run => Duration::from_millis(100),
            Self::Jump => Duration::from_millis(130),
            Self::Attack => Duration::from_millis(85),
            Self::Swim => Duration::from_millis(150),
            Self::Special => Duration::from_millis(240),
        }
    }

    pub fn embedded_frames(&self) -> &'static [&'static [u8]] {
        match self {
            Self::Idle => &[
                include_bytes!("../assets/Samurai/3x/idle_0.png"),
                include_bytes!("../assets/Samurai/3x/idle_1.png"),
                include_bytes!("../assets/Samurai/3x/idle_2.png"),
                include_bytes!("../assets/Samurai/3x/idle_3.png"),
            ],
            Self::Run => &[
                include_bytes!("../assets/Samurai/3x/run_0.png"),
                include_bytes!("../assets/Samurai/3x/run_1.png"),
                include_bytes!("../assets/Samurai/3x/run_2.png"),
                include_bytes!("../assets/Samurai/3x/run_3.png"),
                include_bytes!("../assets/Samurai/3x/run_4.png"),
                include_bytes!("../assets/Samurai/3x/run_5.png"),
            ],
            Self::Jump => &[
                include_bytes!("../assets/Samurai/3x/jump_0.png"),
                include_bytes!("../assets/Samurai/3x/jump_1.png"),
                include_bytes!("../assets/Samurai/3x/jump_2.png"),
                include_bytes!("../assets/Samurai/3x/jump_3.png"),
            ],
            Self::Attack => &[
                include_bytes!("../assets/Samurai/3x/attack_0.png"),
                include_bytes!("../assets/Samurai/3x/attack_1.png"),
                include_bytes!("../assets/Samurai/3x/attack_2.png"),
                include_bytes!("../assets/Samurai/3x/attack_3.png"),
                include_bytes!("../assets/Samurai/3x/attack_4.png"),
                include_bytes!("../assets/Samurai/3x/attack_5.png"),
                include_bytes!("../assets/Samurai/3x/attack_6.png"),
                include_bytes!("../assets/Samurai/3x/attack_7.png"),
            ],
            Self::Swim => &[
                include_bytes!("../assets/Samurai/3x/swim_0.png"),
                include_bytes!("../assets/Samurai/3x/swim_1.png"),
                include_bytes!("../assets/Samurai/3x/swim_2.png"),
                include_bytes!("../assets/Samurai/3x/swim_3.png"),
                include_bytes!("../assets/Samurai/3x/swim_4.png"),
                include_bytes!("../assets/Samurai/3x/swim_5.png"),
            ],
            Self::Special => &[
                include_bytes!("../assets/Samurai/3x/x_0.png"),
                include_bytes!("../assets/Samurai/3x/x_1.png"),
                include_bytes!("../assets/Samurai/3x/x_2.png"),
                include_bytes!("../assets/Samurai/3x/x_3.png"),
            ],
        }
    }
}

pub struct FramePair {
    pub normal: TextureHandle,
    pub flipped: TextureHandle,
}

pub struct SpriteAnimator {
    frames: HashMap<PetMood, Vec<FramePair>>,
    pub current_mood: PetMood,
    pub temporary_until: Option<(Instant, PetMood)>,
    pub frame_idx: usize,
    pub last_tick: Instant,
    pub facing_left: bool,
}

impl SpriteAnimator {
    pub fn load(ctx: &Context) -> Self {
        let mut frames = HashMap::new();

        for mood in [
            PetMood::Idle,
            PetMood::Run,
            PetMood::Jump,
            PetMood::Attack,
            PetMood::Swim,
            PetMood::Special,
        ] {
            let raw_slices = mood.embedded_frames();
            let mut mood_frames = Vec::with_capacity(raw_slices.len());

            for (i, bytes) in raw_slices.iter().enumerate() {
                if let Ok(img) = image::load_from_memory(bytes) {
                    let normal_ci = Self::dynamic_to_color_image(&img);
                    let flipped_img = img.fliph();
                    let flipped_ci = Self::dynamic_to_color_image(&flipped_img);

                    let normal = ctx.load_texture(
                        format!("samurai_{:?}_{}_r", mood, i),
                        normal_ci,
                        TextureOptions::NEAREST,
                    );
                    let flipped = ctx.load_texture(
                        format!("samurai_{:?}_{}_l", mood, i),
                        flipped_ci,
                        TextureOptions::NEAREST,
                    );
                    mood_frames.push(FramePair { normal, flipped });
                }
            }

            if mood_frames.is_empty() {
                let fallback = ColorImage::new([120, 87], egui::Color32::TRANSPARENT);
                let tex = ctx.load_texture(
                    format!("fallback_{:?}", mood),
                    fallback,
                    TextureOptions::NEAREST,
                );
                mood_frames.push(FramePair {
                    normal: tex.clone(),
                    flipped: tex,
                });
            }

            frames.insert(mood, mood_frames);
        }

        Self {
            frames,
            current_mood: PetMood::Idle,
            temporary_until: None,
            frame_idx: 0,
            last_tick: Instant::now(),
            facing_left: false,
        }
    }

    fn dynamic_to_color_image(img: &DynamicImage) -> ColorImage {
        let rgba = img.to_rgba8();
        let size = [rgba.width() as usize, rgba.height() as usize];
        let pixels = rgba.as_flat_samples();
        ColorImage::from_rgba_unmultiplied(size, pixels.as_slice())
    }

    pub fn set_mood(&mut self, mood: PetMood, duration: Option<Duration>) {
        if self.current_mood != mood {
            self.current_mood = mood;
            self.frame_idx = 0;
            self.last_tick = Instant::now();
        }
        if let Some(d) = duration {
            self.temporary_until = Some((Instant::now() + d, PetMood::Idle));
        } else {
            self.temporary_until = None;
        }
    }

    pub fn current_texture(&mut self) -> &TextureHandle {
        let now = Instant::now();
        if let Some((deadline, fallback_mood)) = self.temporary_until {
            if now >= deadline {
                self.current_mood = fallback_mood;
                self.frame_idx = 0;
                self.temporary_until = None;
                self.last_tick = now;
            }
        }

        let frame_dur = self.current_mood.frame_duration();
        let list = self.frames.get(&self.current_mood).unwrap();
        if now.duration_since(self.last_tick) >= frame_dur {
            self.frame_idx = (self.frame_idx + 1) % list.len();
            self.last_tick = now;
        }

        let idx = self.frame_idx.min(list.len().saturating_sub(1));
        if self.facing_left {
            &list[idx].flipped
        } else {
            &list[idx].normal
        }
    }
}

/// Charge l'arrière-plan Pixel-Art du Dojo (`assets/background_dojo.jpeg`) pour le mode Fenêtre d'Éveil / Centre de Contrôle.
pub fn load_dojo_background(ctx: &Context) -> TextureHandle {
    let bytes = include_bytes!("../assets/background_dojo.jpeg");
    if let Ok(img) = image::load_from_memory(bytes) {
        let scaled = img.resize_exact(1376, 768, image::imageops::FilterType::Nearest);
        let rgba = scaled.to_rgba8();
        let size = [rgba.width() as usize, rgba.height() as usize];
        let pixels = rgba.as_flat_samples();
        let ci = ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());
        return ctx.load_texture("dojo_background", ci, TextureOptions::NEAREST);
    }
    let fallback = ColorImage::new([16, 16], egui::Color32::from_rgb(28, 24, 22));
    ctx.load_texture("dojo_background_fallback", fallback, TextureOptions::NEAREST)
}
