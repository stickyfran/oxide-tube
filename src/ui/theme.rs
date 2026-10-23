use iced::widget::{button, container, text_editor, text_input};
use iced::{border, Color};

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
        background: Some(COLOR_BG.into()),
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
        background: Some(COLOR_SURFACE.into()),
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
            text_color: COLOR_BG,
            border: border::rounded(18),
            ..button::Style::default()
        },
    }
}

pub fn secondary_button(_theme: &iced::Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered => button::Style {
            background: Some(COLOR_SURFACE_HOVER.into()),
            text_color: COLOR_TEXT_PRIMARY,
            border: border::rounded(18),
            ..button::Style::default()
        },
        _ => button::Style {
            background: Some(COLOR_SURFACE.into()),
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

pub fn search_input(_theme: &iced::Theme, status: text_input::Status) -> text_input::Style {
    match status {
        text_input::Status::Focused => text_input::Style {
            background: COLOR_INPUT_BG.into(),
            border: border::rounded(20).width(1.0).color(COLOR_BORDER_FOCUS),
            icon: COLOR_TEXT_SECONDARY,
            placeholder: COLOR_TEXT_SECONDARY,
            value: COLOR_TEXT_PRIMARY,
            selection: Color::from_rgba(0.2, 0.4, 0.8, 0.4),
        },
        _ => text_input::Style {
            background: COLOR_INPUT_BG.into(),
            border: border::rounded(20).width(1.0).color(COLOR_BORDER),
            icon: COLOR_TEXT_SECONDARY,
            placeholder: COLOR_TEXT_SECONDARY,
            value: COLOR_TEXT_PRIMARY,
            selection: Color::from_rgba(0.2, 0.4, 0.8, 0.4),
        },
    }
}

pub fn video_card_button(_theme: &iced::Theme, status: button::Status) -> button::Style {
    match status {
        button::Status::Hovered => button::Style {
            background: Some(Color::from_rgba(1.0, 1.0, 1.0, 0.04).into()),
            border: border::rounded(12),
            ..button::Style::default()
        },
        _ => button::Style {
            background: Some(Color::TRANSPARENT.into()),
            border: border::rounded(12),
            ..button::Style::default()
        },
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
