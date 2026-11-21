use gpui_kit::{App, Hsla, component::{ActiveTheme as _, Theme, ThemeMode}, px, rgb};

pub(crate) fn active_palette(cx: &App) -> SkinPalette {
    let t = cx.theme();
    SkinPalette {
        dark: t.mode.is_dark(),
        canvas: t.background,
        surface: t.popover,
        surface_header: t.muted,
        rail_bg: t.sidebar,
        subtle: t.secondary,
        subtle_strong: t.secondary_hover,
        text: t.foreground,
        text_body: t.secondary_foreground,
        text_muted: t.muted_foreground,
        accent: t.primary,
        accent_fill: t.button_primary,
        on_accent: t.primary_foreground,
        accent_soft: t.accent,
        accent_border: t.list_active_border,
        border: t.border,
        success: t.success,
        danger: t.danger,
        warning: t.warning,
        panel_radius: 12.0,
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct SkinPalette {
    pub(crate) dark: bool,
    pub(crate) canvas: Hsla,
    pub(crate) surface: Hsla,
    pub(crate) surface_header: Hsla,
    pub(crate) rail_bg: Hsla,
    pub(crate) subtle: Hsla,
    pub(crate) subtle_strong: Hsla,
    pub(crate) text: Hsla,
    pub(crate) text_body: Hsla,
    pub(crate) text_muted: Hsla,
    pub(crate) accent: Hsla,
    pub(crate) accent_fill: Hsla,
    pub(crate) on_accent: Hsla,
    pub(crate) accent_soft: Hsla,
    pub(crate) accent_border: Hsla,
    pub(crate) border: Hsla,
    pub(crate) success: Hsla,
    pub(crate) danger: Hsla,
    pub(crate) warning: Hsla,
    pub(crate) panel_radius: f32,
}

impl SkinPalette {
    /// Active Islands tab / rail pill fill (`--desktop-tab` / `--skin-accent-container`).
    pub(crate) fn tab_active(&self) -> Hsla {
        self.accent_soft
    }
}

/// Scale a pixel font size according to the user's appearance scale (80%–120%).
pub(crate) fn scaled_px(base: f32, scale: u16) -> gpui_kit::Pixels {
    px((base * (f32::from(scale.clamp(80, 140)) / 100.0)).round())
}

fn c(hex: u32) -> Hsla {
    rgb(hex).into()
}

pub(crate) fn palette_for(skin_id: &str) -> SkinPalette {
    match skin_id {
        "islands-light" => SkinPalette {
            dark: false,
            canvas: c(0xebecf0),
            surface: c(0xffffff),
            surface_header: c(0xf7f8fa),
            rail_bg: c(0xf2f3f5),
            subtle: c(0xf2f3f5),
            subtle_strong: c(0xe4e6eb),
            text: c(0x27282e),
            text_body: c(0x383a42),
            text_muted: c(0x666974),
            accent: c(0x315fbd),
            accent_fill: c(0x3574f0),
            on_accent: c(0xffffff),
            accent_soft: c(0xedf3ff),
            accent_border: c(0xb5c9f4),
            border: c(0xdfe1e5),
            success: c(0x397453),
            danger: c(0xb34232),
            warning: c(0x997023),
            panel_radius: 12.0,
        },
        "islands-dark" => SkinPalette {
            dark: true,
            canvas: c(0x2b2d30),
            surface: c(0x1e1f22),
            surface_header: c(0x25262a),
            rail_bg: c(0x25262a),
            subtle: c(0x25262a),
            subtle_strong: c(0x393b40),
            text: c(0xdfe1e5),
            text_body: c(0xced0d6),
            text_muted: c(0xa0a3ad),
            accent: c(0x88adf7),
            accent_fill: c(0x3574f0),
            on_accent: c(0xffffff),
            accent_soft: c(0x293549),
            accent_border: c(0x4b689b),
            border: c(0x393b40),
            success: c(0x9cceaf),
            danger: c(0xf0aaa0),
            warning: c(0xe7c18a),
            panel_radius: 12.0,
        },
        "apple" => SkinPalette {
            dark: false,
            canvas: c(0xf2f2f7),
            surface: c(0xffffff),
            surface_header: c(0xf9f9fb),
            rail_bg: c(0xeaeaef),
            subtle: c(0xf5f5f7),
            subtle_strong: c(0xe8e8ed),
            text: c(0x1d1d1f),
            text_body: c(0x3a3a3c),
            text_muted: c(0x6e6e73),
            accent: c(0x0066cc),
            accent_fill: c(0x007aff),
            on_accent: c(0xffffff),
            accent_soft: c(0xeaf2fe),
            accent_border: c(0xa5c8f0),
            border: c(0xe5e5ea),
            success: c(0x248a3d),
            danger: c(0xd70015),
            warning: c(0xb25000),
            panel_radius: 14.0,
        },
        "frosted-glass" => SkinPalette {
            dark: false,
            canvas: c(0xe7edf7),
            surface: c(0xf9fbff),
            surface_header: c(0xf3f7fd),
            rail_bg: c(0xedf2fa),
            subtle: c(0xedf2fa),
            subtle_strong: c(0xdfe7f3),
            text: c(0x24324b),
            text_body: c(0x3e4e67),
            text_muted: c(0x5c6c83),
            accent: c(0x315caa),
            accent_fill: c(0x3b6dc6),
            on_accent: c(0xffffff),
            accent_soft: c(0xeaf0fb),
            accent_border: c(0xb7c9e7),
            border: c(0xd7e1ef),
            success: c(0x397453),
            danger: c(0xb34232),
            warning: c(0x997023),
            panel_radius: 16.0,
        },
        "jade" => SkinPalette {
            dark: false,
            canvas: c(0xf2f8f5),
            surface: c(0xfcfffd),
            surface_header: c(0xf4faf7),
            rail_bg: c(0xedf5f0),
            subtle: c(0xeaf2ed),
            subtle_strong: c(0xdbe7e0),
            text: c(0x273f35),
            text_body: c(0x395247),
            text_muted: c(0x5b7368),
            accent: c(0x21745b),
            accent_fill: c(0x21745b),
            on_accent: c(0xffffff),
            accent_soft: c(0xe5f3ee),
            accent_border: c(0xa9d2c4),
            border: c(0xd8e6df),
            success: c(0x247750),
            danger: c(0xb34232),
            warning: c(0x997023),
            panel_radius: 12.0,
        },
        "apricot" => SkinPalette {
            dark: false,
            canvas: c(0xfbf6ee),
            surface: c(0xfffdf8),
            surface_header: c(0xfaf3e8),
            rail_bg: c(0xf5ecdf),
            subtle: c(0xf5ecdf),
            subtle_strong: c(0xeadcc9),
            text: c(0x4a3b2b),
            text_body: c(0x5c4c3b),
            text_muted: c(0x7c6b58),
            accent: c(0x956019),
            accent_fill: c(0x956019),
            on_accent: c(0xffffff),
            accent_soft: c(0xf6ecdd),
            accent_border: c(0xdbc09b),
            border: c(0xe8dccb),
            success: c(0x397453),
            danger: c(0xb34232),
            warning: c(0x997023),
            panel_radius: 12.0,
        },
        "wisteria" => SkinPalette {
            dark: false,
            canvas: c(0xf6f4fb),
            surface: c(0xfefcff),
            surface_header: c(0xf7f4fc),
            rail_bg: c(0xeeeaf6),
            subtle: c(0xeeeaf6),
            subtle_strong: c(0xe1daf0),
            text: c(0x3e3450),
            text_body: c(0x514665),
            text_muted: c(0x706684),
            accent: c(0x7353a6),
            accent_fill: c(0x7353a6),
            on_accent: c(0xffffff),
            accent_soft: c(0xf1ebf9),
            accent_border: c(0xc9b9e3),
            border: c(0xe3ddee),
            success: c(0x397453),
            danger: c(0xb34232),
            warning: c(0x997023),
            panel_radius: 12.0,
        },
        "midnight" => SkinPalette {
            dark: true,
            canvas: c(0x111827),
            surface: c(0x1b2536),
            surface_header: c(0x202c40),
            rail_bg: c(0x172030),
            subtle: c(0x222f43),
            subtle_strong: c(0x2c3c55),
            text: c(0xe3ecfa),
            text_body: c(0xcdd9ec),
            text_muted: c(0x9cb0cc),
            accent: c(0xa6c8ff),
            accent_fill: c(0x5c96f5),
            on_accent: c(0x152c4b),
            accent_soft: c(0x263854),
            accent_border: c(0x4c6b99),
            border: c(0x2d3b52),
            success: c(0x9cceaf),
            danger: c(0xf0aaa0),
            warning: c(0xe7c18a),
            panel_radius: 12.0,
        },
        "graphite" => SkinPalette {
            dark: true,
            canvas: c(0x191d20),
            surface: c(0x242a2e),
            surface_header: c(0x2a3136),
            rail_bg: c(0x1f2428),
            subtle: c(0x2c3438),
            subtle_strong: c(0x384247),
            text: c(0xe3ebe8),
            text_body: c(0xcfdad6),
            text_muted: c(0xa2b2ac),
            accent: c(0x92d4bd),
            accent_fill: c(0x4ba587),
            on_accent: c(0x163a2e),
            accent_soft: c(0x283a36),
            accent_border: c(0x4b7467),
            border: c(0x374046),
            success: c(0x9cceaf),
            danger: c(0xf0aaa0),
            warning: c(0xe7c18a),
            panel_radius: 12.0,
        },
        "vscode-dark-2026" => SkinPalette {
            dark: true,
            canvas: c(0x191a1b),
            surface: c(0x121314),
            surface_header: c(0x1e1f20),
            rail_bg: c(0x151617),
            subtle: c(0x202122),
            subtle_strong: c(0x242526),
            text: c(0xbfbfbf),
            text_body: c(0xbbbebf),
            text_muted: c(0x8c8c8c),
            accent: c(0x48a0c7),
            accent_fill: c(0x297aa0),
            on_accent: c(0xffffff),
            accent_soft: c(0x1e2b32),
            accent_border: c(0x3994bc),
            border: c(0x2a2b2c),
            success: c(0x73c991),
            danger: c(0xf48771),
            warning: c(0xe5ba7d),
            panel_radius: 10.0,
        },
        "vscode-light-2026" => SkinPalette {
            dark: false,
            canvas: c(0xfafafd),
            surface: c(0xffffff),
            surface_header: c(0xf4f4f8),
            rail_bg: c(0xf3f3f6),
            subtle: c(0xeaeaea),
            subtle_strong: c(0xececec),
            text: c(0x202020),
            text_body: c(0x202020),
            text_muted: c(0x606060),
            accent: c(0x0069cc),
            accent_fill: c(0x0069cc),
            on_accent: c(0xffffff),
            accent_soft: c(0xe8f1fb),
            accent_border: c(0x8bbbe8),
            border: c(0xe2e2e6),
            success: c(0x397453),
            danger: c(0xb34232),
            warning: c(0x997023),
            panel_radius: 10.0,
        },
        // "default"
        _ => SkinPalette {
            dark: false,
            canvas: c(0xf8fafd),
            surface: c(0xffffff),
            surface_header: c(0xf5f8fc),
            rail_bg: c(0xf3f6fb),
            subtle: c(0xf0f4fa),
            subtle_strong: c(0xe3eaf4),
            text: c(0x2b3a52),
            text_body: c(0x49566a),
            text_muted: c(0x5e6e83),
            accent: c(0x0b57d0),
            accent_fill: c(0x0b57d0),
            on_accent: c(0xffffff),
            accent_soft: c(0xeaf2fe),
            accent_border: c(0xbfd0e7),
            border: c(0xe3e9f1),
            success: c(0x397453),
            danger: c(0xb34232),
            warning: c(0x997023),
            panel_radius: 12.0,
        },
    }
}

pub(crate) fn apply_skin_palette(skin_id: &str, scale: u16, cx: &mut App) {
    let palette = palette_for(skin_id);
    let mode = if palette.dark {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    };
    Theme::change(mode, None, cx);
    let theme = Theme::global_mut(cx);
    theme.font_size = px(13.5 * (f32::from(scale.clamp(80, 140)) / 100.0));
    theme.radius = px(8.0);
    theme.radius_lg = px(palette.panel_radius);

    let colors = &mut theme.colors;
    colors.background = palette.canvas;
    colors.foreground = palette.text;
    colors.muted = palette.subtle;
    colors.muted_foreground = palette.text_muted;
    colors.border = palette.border;
    colors.input = palette.border;
    colors.popover = palette.surface;
    colors.popover_foreground = palette.text;
    colors.list = palette.surface;
    colors.list_hover = palette.subtle;
    colors.list_active = palette.accent_soft;
    colors.list_active_border = palette.accent_border;
    colors.secondary = palette.subtle;
    colors.secondary_hover = palette.subtle_strong;
    colors.secondary_active = palette.subtle_strong;
    colors.secondary_foreground = palette.text_body;
    colors.accent = palette.accent_soft;
    colors.accent_foreground = palette.accent;
    colors.primary = palette.accent;
    colors.primary_hover = palette.accent_fill;
    colors.primary_active = palette.accent_fill;
    colors.primary_foreground = palette.on_accent;
    colors.button_primary = palette.accent_fill;
    colors.button_primary_hover = palette.accent;
    colors.button_primary_active = palette.accent;
    colors.button_primary_foreground = palette.on_accent;
    colors.sidebar = palette.rail_bg;
    colors.sidebar_border = palette.border;
    colors.sidebar_foreground = palette.text_body;
    colors.sidebar_accent = palette.accent_soft;
    colors.sidebar_accent_foreground = palette.accent;
    colors.title_bar = palette.canvas;
    colors.title_bar_border = palette.border;
    colors.ring = palette.accent;
    colors.drag_border = palette.accent;
    colors.drop_target = palette.accent_soft;
    colors.success = palette.success;
    colors.danger = palette.danger;
    colors.warning = palette.warning;
    colors.red = palette.danger;
    colors.green = palette.success;
    colors.yellow = palette.warning;
}
