use super::{EDITOR_FONT_FAMILY, EditorFontPreset, EditorFontSelection, apply_editor_fonts};
use eframe::egui::{self, FontId};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn editor_font() -> FontId {
    FontId::new(16.0, egui::FontFamily::Name(EDITOR_FONT_FAMILY.into()))
}

#[test]
fn bundled_text_and_icons_render_without_system_fonts_on_the_first_pass() {
    let ctx = egui::Context::default();
    ctx.set_font_providers(Vec::new());
    ctx.set_missing_glyph_policy(egui::MissingGlyphPolicy::Panic);
    for preset in EditorFontPreset::ALL {
        apply_editor_fonts(&ctx, &EditorFontSelection::scratchpad(preset), 4.0).unwrap();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.fonts_mut(|fonts| {
                let text = "Latin Ελληνικά Кириллица 日本語 한국어 简体中文 繁體中文 e\u{301}";
                assert!(fonts.has_glyphs(&editor_font(), text));
                let galley =
                    fonts.layout_no_wrap(text.to_owned(), editor_font(), egui::Color32::WHITE);
                assert!(galley.size().x > 0.0 && galley.size().y > 0.0);
                assert!(
                    fonts.has_glyphs(&egui_phosphor::font_id(16.0), egui_phosphor::regular::GEAR)
                );
            });
        })
        .drop_without_applying_deltas();
    }
}

#[test]
fn configured_fonts_take_precedence_and_provider_survives_font_switches() {
    use egui::epaint::text::{FallbackRequest, FontInsert};
    let ctx = egui::Context::default();
    let requests = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&requests);
    // An installed-font stand-in with an otherwise missing private-use glyph.
    // This exercises actual fallback without depending on the host's font set.
    ctx.add_font_provider(Arc::new(move |request: &FallbackRequest<'_>| {
        observed.fetch_add(1, Ordering::Relaxed);
        (request.cluster == egui_phosphor::regular::GEAR).then(|| {
            FontInsert::new(
                "test-discovered-icons",
                egui_phosphor::Variant::Regular.font_data(),
                Vec::new(),
            )
        })
    }));
    for preset in [EditorFontPreset::Standard, EditorFontPreset::Mono] {
        apply_editor_fonts(&ctx, &EditorFontSelection::scratchpad(preset), 4.0).unwrap();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.fonts_mut(|fonts| {
                let before = requests.load(Ordering::Relaxed);
                assert!(fonts.has_glyphs(&editor_font(), "Hello 日本語"));
                assert_eq!(requests.load(Ordering::Relaxed), before);
                assert!(fonts.has_glyphs(&editor_font(), egui_phosphor::regular::GEAR));
                assert!(requests.load(Ordering::Relaxed) > before);
            });
        })
        .drop_without_applying_deltas();
    }
}

#[test]
#[ignore = "requires installed Arabic/Indic and color emoji fonts; run on native font test hosts"]
fn installed_fonts_render_missing_scripts_and_color_emoji() {
    let ctx = egui::Context::default();
    ctx.add_font_provider(Arc::new(
        egui_system_font_provider::SystemFontProvider::new(),
    ));
    ctx.set_missing_glyph_policy(egui::MissingGlyphPolicy::Panic);
    apply_editor_fonts(
        &ctx,
        &EditorFontSelection::scratchpad(EditorFontPreset::Standard),
        4.0,
    )
    .unwrap();
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        ui.label(egui::RichText::new("العربية हिन्दी 🫠 👩🏽‍💻").font(editor_font()));
    });
    let has_color = output.textures_delta.set.values().flatten().any(|delta| {
        let egui::ImageData::Color(image) = &delta.image;
        image
            .pixels
            .iter()
            .any(|pixel| pixel.r() != pixel.g() || pixel.g() != pixel.b())
    });
    output.textures_delta.clear();
    assert!(has_color, "expected a color emoji glyph in the font atlas");
}
