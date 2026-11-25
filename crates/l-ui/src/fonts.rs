//! Font registration: embedded `JetBrains` Mono + Lucide icon font.

use std::sync::Arc;

use egui::epaint::text::{FontInsert, FontPriority, InsertFontFamily};
use egui::{FontData, FontFamily};

const JBM_REGULAR: &[u8] = include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf");
const JBM_MEDIUM: &[u8] = include_bytes!("../../../assets/fonts/JetBrainsMono-Medium.ttf");
const JBM_BOLD: &[u8] = include_bytes!("../../../assets/fonts/JetBrainsMono-Bold.ttf");
const LUCIDE_FONT: &[u8] = lucide_icons::LUCIDE_FONT_BYTES;

fn font_data(data: &'static [u8]) -> Arc<FontData> {
    Arc::new(FontData::from_static(data))
}

/// Register all bundled fonts. Call once at startup before first frame.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    fonts.font_data.insert("jbm".into(), font_data(JBM_REGULAR));
    fonts
        .font_data
        .insert("jbm-medium".into(), font_data(JBM_MEDIUM));
    fonts
        .font_data
        .insert("jbm-bold".into(), font_data(JBM_BOLD));
    fonts
        .font_data
        .insert("lucide".into(), font_data(LUCIDE_FONT));

    // JetBrains Mono serves both families; Lucide is appended last so icon
    // codepoints resolve while text still uses JetBrains Mono.
    let jbm = vec!["jbm".to_string(), "lucide".to_string()];
    fonts.families.insert(FontFamily::Proportional, jbm.clone());
    fonts.families.insert(FontFamily::Monospace, jbm);
    fonts.families.insert(
        FontFamily::Name("jbm-medium".into()),
        vec!["jbm-medium".to_string(), "lucide".to_string()],
    );
    fonts.families.insert(
        FontFamily::Name("jbm-bold".into()),
        vec!["jbm-bold".to_string(), "lucide".to_string()],
    );

    ctx.set_fonts(fonts);

    // Extra safety: register Lucide as the lowest-priority fallback so icon
    // chars render even inside widgets that override text styles.
    ctx.add_font(FontInsert {
        name: "lucide-fallback".to_string(),
        data: FontData::from_static(LUCIDE_FONT),
        families: vec![
            InsertFontFamily {
                family: FontFamily::Monospace,
                priority: FontPriority::Lowest,
            },
            InsertFontFamily {
                family: FontFamily::Proportional,
                priority: FontPriority::Lowest,
            },
        ],
    });
}
