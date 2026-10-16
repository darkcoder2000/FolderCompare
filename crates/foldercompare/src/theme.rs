//! The only place colors are defined (light theme, from the WPF app's Colors.xaml plus the colors that were
//! hard-coded there), plus fonts, icons and the egui style.
// The text compare colors are used once the text compare window lands.
#![allow(dead_code)]

use egui::{Color32, FontData, FontDefinitions, FontFamily, FontId, TextStyle};

// ---- Comparison colors (result grid) -----------------------------------------------------------
pub const NEWER: Color32 = Color32::from_rgb(0x1B, 0x7F, 0x2A);
pub const OLDER: Color32 = Color32::from_rgb(0x7A, 0x7A, 0x7A);
pub const ONLY: Color32 = Color32::from_rgb(0x1F, 0x5F, 0xBF);
pub const ERROR: Color32 = Color32::from_rgb(0xC4, 0x2B, 0x1C);
pub const CONFLICT: Color32 = Color32::from_rgb(0xB3, 0x5C, 0x00);
pub const DIFF_MARKER: Color32 = Color32::from_rgb(0xE0, 0x7B, 0x00);
pub const FOLDER_ICON: Color32 = Color32::from_rgb(0xC9, 0x94, 0x00);
pub const FILE_ICON: Color32 = Color32::from_rgb(0x6B, 0x6B, 0x6B);
pub const GLYPH: Color32 = Color32::from_rgb(0x44, 0x44, 0x44);
pub const MESSAGE_BAR: Color32 = Color32::from_rgb(0xFD, 0xE7, 0xE9);
pub const WARNING_BAR: Color32 = Color32::from_rgb(0xFF, 0xF4, 0xCE);
pub const STATUS_COLUMN: Color32 = Color32::from_rgb(0xF3, 0xF3, 0xF3);
pub const TEXT: Color32 = Color32::from_rgb(0x1E, 0x1E, 0x1E);
pub const EXPANDER: Color32 = Color32::from_rgb(0x70, 0x70, 0x70);
pub const EXPANDER_HOVER: Color32 = Color32::from_rgb(0x1F, 0x5F, 0xBF);
pub const SELECTION: Color32 = Color32::from_rgb(0xCC, 0xE4, 0xF7);
pub const SELECTION_FOCUS: Color32 = Color32::from_rgb(0x99, 0xC9, 0xEF);
pub const ROW_HOVER: Color32 = Color32::from_rgb(0xE8, 0xF2, 0xFB);

// ---- Text compare window -------------------------------------------------------------------------
pub const CHANGED_LINE: Color32 = Color32::from_rgb(0xFF, 0xE6, 0xE6);
pub const CHANGED_TEXT: Color32 = Color32::from_rgb(0xFF, 0xAA, 0xAA);
pub const UNIMPORTANT_LINE: Color32 = Color32::from_rgb(0xE6, 0xEE, 0xFF);
pub const UNIMPORTANT_TEXT: Color32 = Color32::from_rgb(0xB8, 0xCF, 0xFF);
pub const FILLER: Color32 = Color32::from_rgb(0xF2, 0xF2, 0xF2);
pub const FILLER_HATCH: Color32 = Color32::from_rgb(0xDD, 0xDD, 0xDD);
pub const CURRENT_SECTION: Color32 = Color32::from_rgb(0xE0, 0x7B, 0x00);
pub const PANE_HEADER: Color32 = Color32::from_rgb(0xF3, 0xF3, 0xF3);
pub const PANE_HEADER_BORDER: Color32 = Color32::from_rgb(0xD0, 0xD0, 0xD0);
pub const LINE_NUMBER: Color32 = Color32::from_rgb(0x8A, 0x8A, 0x8A);
pub const FORMAT_TEXT: Color32 = Color32::from_rgb(0x6B, 0x6B, 0x6B);
pub const CENTER_STRIP: Color32 = Color32::from_rgb(0xF4, 0xF4, 0xF4);
pub const OVERVIEW: Color32 = Color32::from_rgb(0xFA, 0xFA, 0xFA);
pub const OVERVIEW_VIEWPORT_FILL: Color32 = Color32::from_rgba_unmultiplied_const(40, 40, 40, 0x22);
pub const OVERVIEW_VIEWPORT_BORDER: Color32 =
    Color32::from_rgba_unmultiplied_const(40, 40, 40, 0xB0);
pub const EDITOR_BACKGROUND: Color32 = Color32::WHITE;
pub const CARET: Color32 = Color32::from_rgb(0x1E, 0x1E, 0x1E);
pub const EDITOR_SELECTION: Color32 = Color32::from_rgba_unmultiplied_const(0x33, 0x99, 0xFF, 0x50);
pub const WHITESPACE_MARK: Color32 = Color32::from_rgb(0xB0, 0xB0, 0xB0);

/// Icons (Phosphor, MIT) used across the app.
pub mod icons {
    pub use egui_phosphor::regular::{
        ARROW_LEFT, ARROW_RIGHT, ARROWS_LEFT_RIGHT, CARET_DOWN, CARET_RIGHT, CARET_UP, EQUALS,
        FILE, FOLDER, NOT_EQUALS, WARNING,
    };
}

/// Font family with a bold face (falls back to the regular one when the OS has none).
pub fn bold_family() -> FontFamily {
    FontFamily::Name("bold".into())
}

pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

/// Candidate system fonts (regular, bold), tried in order.
fn system_fonts() -> &'static [(&'static str, &'static str)] {
    if cfg!(windows) {
        &[(
            r"C:\Windows\Fonts\segoeui.ttf",
            r"C:\Windows\Fonts\segoeuib.ttf",
        )]
    } else if cfg!(target_os = "macos") {
        &[
            (
                "/System/Library/Fonts/Helvetica.ttc",
                "/System/Library/Fonts/Helvetica.ttc#1",
            ),
            ("/Library/Fonts/Arial.ttf", "/Library/Fonts/Arial Bold.ttf"),
        ]
    } else {
        &[
            (
                "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
                "/usr/share/fonts/truetype/noto/NotoSans-Bold.ttf",
            ),
            (
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
            ),
            (
                "/usr/share/fonts/TTF/DejaVuSans.ttf",
                "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
            ),
            (
                "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
                "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans-Bold.ttf",
            ),
        ]
    }
}

/// Symbol fonts with arrows and the other glyphs some labels use (Windows only; elsewhere the main fonts have them).
fn symbol_fonts() -> &'static [&'static str] {
    if cfg!(windows) {
        &[r"C:\Windows\Fonts\seguisym.ttf"]
    } else {
        &[]
    }
}

/// Loads `path` or `path#index` (a face inside a font collection).
fn load_font(spec: &str) -> Option<FontData> {
    let (path, index) = match spec.rsplit_once('#') {
        Some((p, i)) => (p, i.parse().ok()?),
        None => (spec, 0),
    };
    let bytes = std::fs::read(path).ok()?;
    let mut data = FontData::from_owned(bytes);
    data.index = index;
    Some(data)
}

fn fonts() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let mut regular_name = None;
    let mut bold_name = None;
    for (regular, bold) in system_fonts() {
        if let Some(r) = load_font(regular) {
            fonts.font_data.insert("system".into(), r.into());
            regular_name = Some("system".to_string());
            if let Some(b) = load_font(bold) {
                fonts.font_data.insert("system-bold".into(), b.into());
                bold_name = Some("system-bold".to_string());
            }
            break;
        }
    }
    let mut symbols = Vec::new();
    for (i, path) in symbol_fonts().iter().enumerate() {
        if let Some(s) = load_font(path) {
            let name = format!("symbols-{i}");
            fonts.font_data.insert(name.clone(), s.into());
            symbols.push(name);
        }
    }

    let proportional = fonts
        .families
        .get_mut(&FontFamily::Proportional)
        .expect("default proportional family");
    if let Some(r) = &regular_name {
        proportional.insert(0, r.clone());
    }
    let base: Vec<String> = proportional.clone();
    for s in &symbols {
        proportional.push(s.clone());
    }

    let mut bold = base.clone();
    if let Some(b) = &bold_name {
        bold.insert(0, b.clone());
    }
    bold.extend(symbols.iter().cloned());
    fonts.families.insert(bold_family(), bold);

    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    // The bold family gets the icons too.
    if let Some(b) = fonts.families.get_mut(&bold_family()) {
        b.insert(b.len().min(1), "phosphor".into());
    }
    fonts
}

/// Fonts, light visuals and text sizes. Called once at startup (fonts are shared by all windows).
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(fonts());
    ctx.set_theme(egui::Theme::Light);
    ctx.style_mut_of(egui::Theme::Light, |style| {
        style
            .text_styles
            .insert(TextStyle::Body, FontId::proportional(13.0));
        style
            .text_styles
            .insert(TextStyle::Button, FontId::proportional(13.0));
        style
            .text_styles
            .insert(TextStyle::Small, FontId::proportional(11.0));
        style
            .text_styles
            .insert(TextStyle::Monospace, FontId::monospace(13.0));
        style
            .text_styles
            .insert(TextStyle::Heading, FontId::proportional(16.0));
        style.spacing.item_spacing = egui::vec2(6.0, 4.0);
        style.spacing.button_padding = egui::vec2(6.0, 2.0);
        style.spacing.interact_size.y = 22.0;
        let v = &mut style.visuals;
        v.override_text_color = Some(TEXT);
        v.selection.bg_fill = SELECTION_FOCUS;
        v.selection.stroke.color = TEXT;
        v.window_fill = Color32::from_rgb(0xFA, 0xFA, 0xFA);
        v.panel_fill = Color32::from_rgb(0xF7, 0xF7, 0xF7);
        v.extreme_bg_color = Color32::WHITE;
        v.faint_bg_color = Color32::from_rgb(0xF4, 0xF4, 0xF4);
    });
}
