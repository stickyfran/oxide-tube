use iced::widget::{button, container, text_editor, text_input};
use iced::{border, Color};
use std::sync::atomic::{AtomicBool, Ordering};

static IS_AMOLED: AtomicBool = AtomicBool::new(false);

pub fn set_amoled_mode(amoled: bool) {
    IS_AMOLED.store(amoled, Ordering::Relaxed);
}

pub fn is_amoled_mode() -> bool {
    IS_AMOLED.load(Ordering::Relaxed)
}

pub fn color_bg() -> Color {
    if is_amoled_mode() {
        Color::BLACK
    } else {
        COLOR_BG
    }
}

pub fn color_surface() -> Color {
    if is_amoled_mode() {
        Color::from_rgb(0.04, 0.04, 0.04)
    } else {
        COLOR_SURFACE
    }
}

pub fn color_surface_hover() -> Color {
    if is_amoled_mode() {
        Color::from_rgb(0.10, 0.10, 0.10)
    } else {
        COLOR_SURFACE_HOVER
    }
}

pub fn color_input_bg() -> Color {
    if is_amoled_mode() {
        Color::from_rgb(0.03, 0.03, 0.03)
    } else {
        COLOR_INPUT_BG
    }
}

pub fn color_border() -> Color {
    if is_amoled_mode() {
        Color::from_rgb(0.12, 0.12, 0.12)
    } else {
        COLOR_BORDER
    }
}

pub fn selectable_text_editor(_theme: &iced::Theme, _status: text_editor::Status) -> text_editor::Style {
    text_editor::Style {
        background: iced::Background::Color(Color::TRANSPARENT),
        border: border::rounded(0),
        icon: COLOR_TEXT_SECONDARY,
        placeholder: COLOR_TEXT_SECONDARY,
        value: COLOR_TEXT_PRIMARY,
        selection: Color::from_rgba(0.24, 0.65, 1.0, 0.40),
    }
}


pub const COLOR_BG: Color = Color::from_rgb(0.059, 0.059, 0.059); // #0f0f0f
pub const COLOR_SURFACE: Color = Color::from_rgb(0.153, 0.153, 0.153); // #272727
pub const COLOR_SURFACE_HOVER: Color = Color::from_rgb(0.20, 0.20, 0.20);
pub const COLOR_INPUT_BG: Color = Color::from_rgb(0.07, 0.07, 0.07); // #121212
pub const COLOR_BORDER: Color = Color::from_rgb(0.188, 0.188, 0.188); // #303030
pub const COLOR_BORDER_FOCUS: Color = Color::from_rgb(0.11, 0.384, 0.725); // #1c62b9
pub const COLOR_TEXT_PRIMARY: Color = Color::from_rgb(0.945, 0.945, 0.945); // #f1f1f1
pub const COLOR_TEXT_SECONDARY: Color = Color::from_rgb(0.667, 0.667, 0.667); // #aaaaaa
pub const COLOR_ACCENT_RED: Color = Color::from_rgb(1.0, 0.0, 0.0); // #ff0000
pub const COLOR_TRANSPARENT: Color = Color::TRANSPARENT; // rgba(0, 0, 0, 0.0)
pub const COLOR_OSD_BG: Color = Color::from_rgba(0.03, 0.03, 0.03, 0.75); // rgba(8, 8, 8, 0.75)

pub fn main_container(_theme: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(color_bg().into()),
        text_color: Some(COLOR_TEXT_PRIMARY),
        ..container::Style::default()
    }
}

pub fn transparent_player_container(_theme: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(COLOR_TRANSPARENT.into()),
        ..container::Style::default()
    }
}

pub fn card_container(_theme: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(color_surface().into()),
        border: border::rounded(12),
        text_color: Some(COLOR_TEXT_PRIMARY),
        ..container::Style::default()
    }
}

pub fn osd_container(_theme: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(COLOR_OSD_BG.into()),
        border: border::rounded(8),
        text_color: Some(COLOR_TEXT_PRIMARY),
        ..container::Style::default()
    }
}

pub fn youtube_player_control_bar(_theme: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.65).into()),
        border: border::rounded(0),
        text_color: Some(COLOR_TEXT_PRIMARY),
        ..container::Style::default()
    }
}

pub fn primary_button(_theme: &iced::Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered => button::Style {
            background: Some(Color::from_rgb(0.85, 0.85, 0.85).into()),
            text_color: Color::from_rgb(0.06, 0.06, 0.06),
            border: border::rounded(18),
            ..button::Style::default()
        },
        _ => button::Style {
            background: Some(COLOR_TEXT_PRIMARY.into()),
            text_color: color_bg(),
            border: border::rounded(18),
            ..button::Style::default()
        },
    }
}

pub fn secondary_button(_theme: &iced::Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered => button::Style {
            background: Some(color_surface_hover().into()),
            text_color: COLOR_TEXT_PRIMARY,
            border: border::rounded(18),
            ..button::Style::default()
        },
        _ => button::Style {
            background: Some(color_surface().into()),
            text_color: COLOR_TEXT_PRIMARY,
            border: border::rounded(18),
            ..button::Style::default()
        },
    }
}

pub fn ghost_button(_theme: &iced::Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered => button::Style {
            background: Some(Color::from_rgba(1.0, 1.0, 1.0, 0.1).into()),
            text_color: COLOR_TEXT_PRIMARY,
            border: border::rounded(18),
            ..button::Style::default()
        },
        _ => button::Style {
            background: Some(Color::TRANSPARENT.into()),
            text_color: COLOR_TEXT_PRIMARY,
            border: border::rounded(18),
            ..button::Style::default()
        },
    }
}

pub fn osd_control_button(_theme: &iced::Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered => button::Style {
            background: Some(Color::from_rgba(1.0, 1.0, 1.0, 0.16).into()),
            text_color: COLOR_TEXT_PRIMARY,
            border: border::rounded(4),
            ..button::Style::default()
        },
        button::Status::Pressed => button::Style {
            background: Some(Color::from_rgba(1.0, 1.0, 1.0, 0.25).into()),
            text_color: COLOR_TEXT_PRIMARY,
            border: border::rounded(4),
            ..button::Style::default()
        },
        _ => button::Style {
            background: Some(Color::TRANSPARENT.into()),
            text_color: COLOR_TEXT_PRIMARY,
            border: border::rounded(4),
            ..button::Style::default()
        },
    }
}

#[allow(dead_code)]
pub fn video_hitbox_button(_theme: &iced::Theme, _status: button::Status) -> button::Style {
    button::Style {
        background: Some(Color::TRANSPARENT.into()),
        text_color: Color::TRANSPARENT,
        border: border::rounded(0),
        ..button::Style::default()
    }
}

pub fn animated_search_input(glow: f32) -> impl Fn(&iced::Theme, text_input::Status) -> text_input::Style {
    let glow = glow.clamp(0.0, 1.0);
    let base_border = color_border();
    let glow_border = Color::from_rgb(0.24, 0.65, 1.0);
    let current_border = Color {
        r: base_border.r + (glow_border.r - base_border.r) * glow,
        g: base_border.g + (glow_border.g - base_border.g) * glow,
        b: base_border.b + (glow_border.b - base_border.b) * glow,
        a: 1.0,
    };
    let base_bg = color_input_bg();
    let glow_bg = Color::from_rgb(0.12, 0.12, 0.15);
    let current_bg = Color {
        r: base_bg.r + (glow_bg.r - base_bg.r) * glow,
        g: base_bg.g + (glow_bg.g - base_bg.g) * glow,
        b: base_bg.b + (glow_bg.b - base_bg.b) * glow,
        a: 1.0,
    };
    let border_w = 1.0 + 0.5 * glow;

    move |_theme, status| match status {
        text_input::Status::Hovered if glow < 0.05 => text_input::Style {
            background: base_bg.into(),
            border: border::rounded(20).width(1.0).color(Color::from_rgb(0.42, 0.42, 0.42)),
            icon: COLOR_TEXT_SECONDARY,
            placeholder: COLOR_TEXT_SECONDARY,
            value: COLOR_TEXT_PRIMARY,
            selection: Color::from_rgba(0.2, 0.4, 0.8, 0.4),
        },
        _ => text_input::Style {
            background: current_bg.into(),
            border: border::rounded(20).width(border_w).color(current_border),
            icon: if glow > 0.4 { COLOR_TEXT_PRIMARY } else { COLOR_TEXT_SECONDARY },
            placeholder: COLOR_TEXT_SECONDARY,
            value: COLOR_TEXT_PRIMARY,
            selection: Color::from_rgba(0.2, 0.4, 0.8, 0.4),
        },
    }
}

pub fn search_input(theme: &iced::Theme, status: text_input::Status) -> text_input::Style {
    animated_search_input(0.0)(theme, status)
}

#[allow(dead_code)]
pub fn yt_badge_container(_theme: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.8).into()),
        border: border::rounded(4),
        text_color: Some(Color::WHITE),
        ..container::Style::default()
    }
}

#[allow(dead_code)]
pub fn yt_live_badge_container(_theme: &iced::Theme) -> container::Style {
    container::Style {
        background: Some(Color::from_rgb8(0xcc, 0x00, 0x00).into()),
        border: border::rounded(4),
        text_color: Some(Color::WHITE),
        ..container::Style::default()
    }
}

pub fn home_card_button(card_style: crate::config::CardStyle) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    let radius = match card_style {
        crate::config::CardStyle::Squircle => 12,
        crate::config::CardStyle::Rectangular => 0,
    };
    // Hover estilo YouTube: fondo plano translúcido, sin borde ni sombra.
    move |_theme, status| match status {
        button::Status::Hovered | button::Status::Pressed => button::Style {
            background: Some(Color::from_rgba(1.0, 1.0, 1.0, 0.10).into()),
            border: border::rounded(radius),
            ..button::Style::default()
        },
        _ => button::Style {
            background: Some(Color::TRANSPARENT.into()),
            border: border::rounded(radius),
            ..button::Style::default()
        },
    }
}

pub fn compact_card_button(card_style: crate::config::CardStyle) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    let radius = match card_style {
        crate::config::CardStyle::Squircle => 12,
        crate::config::CardStyle::Rectangular => 0,
    };
    move |_theme, status| match status {
        button::Status::Hovered | button::Status::Pressed => button::Style {
            background: Some(Color::from_rgba(1.0, 1.0, 1.0, 0.10).into()),
            border: border::rounded(radius),
            ..button::Style::default()
        },
        _ => button::Style {
            background: Some(Color::TRANSPARENT.into()),
            border: border::rounded(radius),
            ..button::Style::default()
        },
    }
}

pub fn video_card_button(_theme: &iced::Theme, _status: button::Status) -> button::Style {
    button::Style {
        background: Some(Color::TRANSPARENT.into()),
        border: border::rounded(0),
        ..button::Style::default()
    }
}

#[allow(dead_code)]
pub fn youtube_seek_slider(_theme: &iced::Theme, _status: iced::widget::slider::Status) -> iced::widget::slider::Style {
    iced::widget::slider::Style {
        rail: iced::widget::slider::Rail {
            backgrounds: (COLOR_ACCENT_RED.into(), Color::from_rgba(1.0, 1.0, 1.0, 0.25).into()),
            width: 4.0,
            border: border::rounded(2),
        },
        handle: iced::widget::slider::Handle {
            shape: iced::widget::slider::HandleShape::Circle { radius: 6.0 },
            background: COLOR_ACCENT_RED.into(),
            border_width: 0.0,
            border_color: Color::TRANSPARENT,
        },
    }
}

pub fn settings_category_button(is_active: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_theme, status| {
        if is_active {
            button::Style {
                background: Some(Color::from_rgb8(0xcc, 0x00, 0x00).into()),
                text_color: Color::WHITE,
                border: border::rounded(10)
                    .color(Color::from_rgb8(0xff, 0x33, 0x33))
                    .width(1.0),
                ..button::Style::default()
            }
        } else {
            match status {
                button::Status::Hovered => button::Style {
                    background: Some(Color::from_rgba(1.0, 1.0, 1.0, 0.08).into()),
                    text_color: COLOR_TEXT_PRIMARY,
                    border: border::rounded(10),
                    ..button::Style::default()
                },
                _ => button::Style {
                    background: Some(Color::TRANSPARENT.into()),
                    text_color: COLOR_TEXT_SECONDARY,
                    border: border::rounded(10),
                    ..button::Style::default()
                },
            }
        }
    }
}

pub fn segmented_button(is_selected: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_theme, status| {
        if is_selected {
            button::Style {
                background: Some(Color::from_rgb8(0xcc, 0x00, 0x00).into()),
                text_color: Color::WHITE,
                border: border::rounded(8)
                    .color(Color::from_rgb8(0xff, 0x33, 0x33))
                    .width(1.0),
                ..button::Style::default()
            }
        } else {
            match status {
                button::Status::Hovered => button::Style {
                    background: Some(Color::from_rgba(1.0, 1.0, 1.0, 0.10).into()),
                    text_color: COLOR_TEXT_PRIMARY,
                    border: border::rounded(8).color(COLOR_BORDER).width(1.0),
                    ..button::Style::default()
                },
                _ => button::Style {
                    background: Some(Color::from_rgb8(0x1e, 0x1e, 0x22).into()),
                    text_color: COLOR_TEXT_SECONDARY,
                    border: border::rounded(8).color(COLOR_BORDER).width(1.0),
                    ..button::Style::default()
                },
            }
        }
    }
}

#[allow(dead_code)]
pub fn tab_select_button(_theme: &iced::Theme, _status: button::Status) -> button::Style {
    button::Style {
        background: Some(Color::TRANSPARENT.into()),
        text_color: COLOR_TEXT_PRIMARY,
        border: border::rounded(0),
        ..button::Style::default()
    }
}

pub fn tab_close_button(_theme: &iced::Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered => button::Style {
            background: Some(Color::from_rgba(1.0, 1.0, 1.0, 0.22).into()),
            text_color: Color::WHITE,
            border: border::rounded(12),
            ..button::Style::default()
        },
        button::Status::Pressed => button::Style {
            background: Some(Color::from_rgba(1.0, 0.25, 0.25, 0.45).into()),
            text_color: Color::WHITE,
            border: border::rounded(12),
            ..button::Style::default()
        },
        _ => button::Style {
            background: Some(Color::TRANSPARENT.into()),
            text_color: COLOR_TEXT_SECONDARY,
            border: border::rounded(12),
            ..button::Style::default()
        },
    }
}

pub fn window_control_button(is_maximized: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    let radius = if is_maximized { 0 } else { 6 };
    move |_theme, status| match status {
        button::Status::Hovered => button::Style {
            background: Some(Color::from_rgba(1.0, 1.0, 1.0, 0.13).into()),
            text_color: COLOR_TEXT_PRIMARY,
            border: border::rounded(radius),
            ..button::Style::default()
        },
        button::Status::Pressed => button::Style {
            background: Some(Color::from_rgba(1.0, 1.0, 1.0, 0.20).into()),
            text_color: COLOR_TEXT_PRIMARY,
            border: border::rounded(radius),
            ..button::Style::default()
        },
        _ => button::Style {
            background: Some(Color::TRANSPARENT.into()),
            text_color: COLOR_TEXT_SECONDARY,
            border: border::rounded(radius),
            ..button::Style::default()
        },
    }
}

pub fn window_close_button(is_maximized: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    let radius = if is_maximized { 0 } else { 6 };
    move |_theme, status| match status {
        button::Status::Hovered => button::Style {
            background: Some(Color::from_rgb8(0xe8, 0x11, 0x23).into()),
            text_color: Color::WHITE,
            border: border::rounded(radius),
            ..button::Style::default()
        },
        button::Status::Pressed => button::Style {
            background: Some(Color::from_rgb8(0xc5, 0x0f, 0x1f).into()),
            text_color: Color::WHITE,
            border: border::rounded(radius),
            ..button::Style::default()
        },
        _ => button::Style {
            background: Some(Color::TRANSPARENT.into()),
            text_color: COLOR_TEXT_SECONDARY,
            border: border::rounded(radius),
            ..button::Style::default()
        },
    }
}


