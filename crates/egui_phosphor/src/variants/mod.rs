#[cfg(feature = "regular")]
pub mod regular;

#[cfg(not(feature = "regular"))]
compile_error!("The Regular font variant must be selected. When in doubt, use default features.");

#[derive(Debug, Clone, Copy)]
pub enum Variant {
    #[cfg(feature = "regular")]
    Regular,
}

impl Variant {
    pub fn font_bytes(&self) -> &'static [u8] {
        match self {
            #[cfg(feature = "regular")]
            Variant::Regular => &*include_bytes!("../../res/Phosphor.ttf"),
        }
    }

    pub fn font_data(&self) -> egui::FontData {
        egui::FontData::from_static(self.font_bytes())
    }
}
