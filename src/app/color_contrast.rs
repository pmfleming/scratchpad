use eframe::egui::Color32;

pub fn optimal_text_color(background: Color32) -> Color32 {
    let mut best_color_candidate = None;
    let mut best_color_ratio = 0.0;

    for candidate in complementary_text_color_candidates(background) {
        let ratio = contrast_ratio(background, candidate);
        let chroma_ok = is_chromatically_distinct(background, candidate);

        if ratio >= WCAG_AA_CONTRAST && chroma_ok && ratio > best_color_ratio {
            best_color_ratio = ratio;
            best_color_candidate = Some(candidate);
        }
    }

    // Fallback to Black or White if no colorful candidate is distinct/readable enough
    best_color_candidate.unwrap_or_else(|| best_grayscale_text_color(background))
}

fn complementary_text_color_candidates(background: Color32) -> [Color32; 6] {
    let hsl = Hsl::from_color32(background);

    // Polarize saturation: if background is dull, make text vivid.
    // If background is vivid, keep text clean but saturated.
    let target_saturation = if hsl.saturation < 0.4 { 0.9 } else { 0.7 };
    let [preferred_l, alternate_l] = contrasting_lightnesses(hsl.lightness);

    // Offsets: 0.5 (Complementary), 5/12 and 7/12 (Split-Complementary)
    let offsets = [0.5, 5.0 / 12.0, 7.0 / 12.0];
    let mut candidates = [Color32::TRANSPARENT; 6];

    for (i, &offset) in offsets.iter().enumerate() {
        candidates[i * 2] = hsl
            .with_hue_offset(offset)
            .with_saturation(target_saturation)
            .with_lightness(preferred_l)
            .to_color32();
        candidates[i * 2 + 1] = hsl
            .with_hue_offset(offset)
            .with_saturation(target_saturation)
            .with_lightness(alternate_l)
            .to_color32();
    }
    candidates
}

fn contrasting_lightnesses(lightness: f32) -> [f32; 2] {
    if lightness > 0.5 {
        [0.20, 0.10]
    } else {
        [0.85, 0.93]
    }
}

fn is_chromatically_distinct(a: Color32, b: Color32) -> bool {
    let ha = Hsl::from_color32(a);
    let hb = Hsl::from_color32(b);

    if ha.saturation < 0.1 || hb.saturation < 0.1 {
        return visual_distance(a, b) > MIN_CHROMA_DISTANCE;
    }

    let hue_diff = (ha.hue - hb.hue).abs();
    let hue_diff = hue_diff.min(1.0 - hue_diff);
    hue_diff > 0.15
}

fn visual_distance(a: Color32, b: Color32) -> f32 {
    let dr = (a.r() as f32 - b.r() as f32).powi(2);
    let dg = (a.g() as f32 - b.g() as f32).powi(2);
    let db = (a.b() as f32 - b.b() as f32).powi(2);
    (dr + dg + db).sqrt()
}

fn best_grayscale_text_color(background: Color32) -> Color32 {
    if relative_luminance(background) > 0.179 {
        Color32::BLACK
    } else {
        Color32::WHITE
    }
}

fn contrast_ratio(left: Color32, right: Color32) -> f32 {
    let l1 = relative_luminance(left) + 0.05;
    let l2 = relative_luminance(right) + 0.05;
    if l1 > l2 { l1 / l2 } else { l2 / l1 }
}

fn relative_luminance(color: Color32) -> f32 {
    let r = linearize(color.r());
    let g = linearize(color.g());
    let b = linearize(color.b());
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

fn linearize(channel: u8) -> f32 {
    let s = channel as f32 / 255.0;
    if s <= 0.03928 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}

#[derive(Clone, Copy)]
struct Hsl {
    hue: f32,
    saturation: f32,
    lightness: f32,
}

impl Hsl {
    fn from_color32(c: Color32) -> Self {
        let r = c.r() as f32 / 255.0;
        let g = c.g() as f32 / 255.0;
        let b = c.b() as f32 / 255.0;
        let max = r.max(g.max(b));
        let min = r.min(g.min(b));
        let delta = max - min;
        let l = (max + min) / 2.0;
        let s = if delta == 0.0 {
            0.0
        } else {
            delta / (1.0 - (2.0 * l - 1.0).abs())
        };
        let mut h = if delta == 0.0 {
            0.0
        } else if max == r {
            (g - b) / delta
        } else if max == g {
            (b - r) / delta + 2.0
        } else {
            (r - g) / delta + 4.0
        };
        h /= 6.0;
        Self {
            hue: h.rem_euclid(1.0),
            saturation: s,
            lightness: l,
        }
    }

    fn with_hue_offset(self, o: f32) -> Self {
        Self {
            hue: (self.hue + o).rem_euclid(1.0),
            ..self
        }
    }
    fn with_saturation(self, s: f32) -> Self {
        Self {
            saturation: s.clamp(0.0, 1.0),
            ..self
        }
    }
    fn with_lightness(self, l: f32) -> Self {
        Self {
            lightness: l.clamp(0.0, 1.0),
            ..self
        }
    }

    fn to_color32(self) -> Color32 {
        let c = (1.0 - (2.0 * self.lightness - 1.0).abs()) * self.saturation;
        let h_prime = self.hue * 6.0;
        let x = c * (1.0 - (h_prime.rem_euclid(2.0) - 1.0).abs());
        let (r1, g1, b1) = match h_prime.floor() as i32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let m = self.lightness - c / 2.0;
        Color32::from_rgb(
            ((r1 + m) * 255.0).round() as u8,
            ((g1 + m) * 255.0).round() as u8,
            ((b1 + m) * 255.0).round() as u8,
        )
    }
}

const WCAG_AA_CONTRAST: f32 = 4.5;
const MIN_CHROMA_DISTANCE: f32 = 60.0;
