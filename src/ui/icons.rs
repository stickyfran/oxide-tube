use iced::widget::{container, image, svg, text};
use iced::{Element, Length};

use crate::ui::theme::{COLOR_SURFACE_HOVER, COLOR_TEXT_PRIMARY};

use std::sync::OnceLock;

fn render_cached_svg<'a, Message: 'static>(
    cache: &'static OnceLock<svg::Handle>,
    raw_svg: &'static [u8],
    size: f32,
) -> Element<'a, Message> {
    let handle = cache.get_or_init(|| svg::Handle::from_memory(raw_svg));
    svg(handle.clone())
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .into()
}

pub fn icon_back<'a, Message: 'static>(size: f32) -> Element<'a, Message> {
    static HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#f1f1f1" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M19 12H5"/><path d="M12 19l-7-7 7-7"/></svg>"##;
    render_cached_svg(&HANDLE, SVG, size)
}

#[allow(dead_code)]
pub fn icon_app_logo<'a, Message: 'static>(size: f32) -> Element<'a, Message> {
    static LOGO_HANDLE: OnceLock<image::Handle> = OnceLock::new();
    let handle = LOGO_HANDLE.get_or_init(|| {
        const ICON_BYTES: &[u8] = include_bytes!("../../assets/app_logo.png");
        image::Handle::from_bytes(ICON_BYTES)
    });
    image(handle.clone())
        .width(Length::Fixed(size * 1.43))
        .height(Length::Fixed(size))
        .content_fit(iced::ContentFit::Contain)
        .into()
}

pub fn icon_youtube_logo<'a, Message: 'static>(size: f32, is_active: bool) -> Element<'a, Message> {
    static RED_HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    static WHITE_HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    const SVG_RED: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="1.5" y="4.5" width="21" height="15" rx="4.5" fill="#ff0000"/><polygon points="10,8.5 16.5,12 10,15.5" fill="#ffffff"/></svg>"##;
    const SVG_WHITE: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="1.5" y="4.5" width="21" height="15" rx="4.5" fill="#f1f1f1"/><polygon points="10,8.5 16.5,12 10,15.5" fill="#0f0f0f"/></svg>"##;
    let handle = if is_active {
        RED_HANDLE.get_or_init(|| svg::Handle::from_memory(SVG_RED))
    } else {
        WHITE_HANDLE.get_or_init(|| svg::Handle::from_memory(SVG_WHITE))
    };
    svg(handle.clone())
        .width(Length::Fixed(size * 1.35))
        .height(Length::Fixed(size))
        .into()
}

pub fn icon_search<'a, Message: 'static>(size: f32) -> Element<'a, Message> {
    static HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#f1f1f1" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="7"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg>"##;
    render_cached_svg(&HANDLE, SVG, size)
}

pub fn icon_reload<'a, Message: 'static>(size: f32) -> Element<'a, Message> {
    static HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#f1f1f1" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="23 4 23 10 17 10"/><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10"/></svg>"##;
    render_cached_svg(&HANDLE, SVG, size)
}

pub fn icon_user<'a, Message: 'static>(size: f32) -> Element<'a, Message> {
    static HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#f1f1f1" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/></svg>"##;
    render_cached_svg(&HANDLE, SVG, size)
}

pub fn icon_check_circle<'a, Message: 'static>(size: f32) -> Element<'a, Message> {
    static HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#2ba640" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/></svg>"##;
    render_cached_svg(&HANDLE, SVG, size)
}

pub fn icon_play<'a, Message: 'static>(size: f32) -> Element<'a, Message> {
    static HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="#f1f1f1"><polygon points="6 4 20 12 6 20 6 4"/></svg>"##;
    render_cached_svg(&HANDLE, SVG, size)
}

pub fn icon_pause<'a, Message: 'static>(size: f32) -> Element<'a, Message> {
    static HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="#f1f1f1"><rect x="6" y="4" width="4" height="16" rx="1"/><rect x="14" y="4" width="4" height="16" rx="1"/></svg>"##;
    render_cached_svg(&HANDLE, SVG, size)
}

pub fn icon_volume<'a, Message: 'static>(size: f32) -> Element<'a, Message> {
    static HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#f1f1f1" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5" fill="#f1f1f1"/><path d="M15.54 8.46a5 5 0 0 1 0 7.07"/><path d="M19.07 4.93a10 10 0 0 1 0 14.14"/></svg>"##;
    render_cached_svg(&HANDLE, SVG, size)
}

pub fn icon_volume_low<'a, Message: 'static>(size: f32) -> Element<'a, Message> {
    static HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#f1f1f1" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5" fill="#f1f1f1"/><path d="M15.54 8.46a5 5 0 0 1 0 7.07"/></svg>"##;
    render_cached_svg(&HANDLE, SVG, size)
}

pub fn icon_volume_mute<'a, Message: 'static>(size: f32) -> Element<'a, Message> {
    static HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#f1f1f1" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5" fill="#f1f1f1"/><line x1="23" y1="9" x2="17" y2="15"/><line x1="17" y1="9" x2="23" y2="15"/></svg>"##;
    render_cached_svg(&HANDLE, SVG, size)
}

pub fn icon_settings<'a, Message: 'static>(size: f32) -> Element<'a, Message> {
    static HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#f1f1f1" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>"##;
    render_cached_svg(&HANDLE, SVG, size)
}

pub fn icon_subtitles<'a, Message: 'static>(size: f32) -> Element<'a, Message> {
    static HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#f1f1f1" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="4" width="20" height="16" rx="2"/><path d="M7 15h4M15 15h2M7 11h2M13 11h4"/></svg>"##;
    render_cached_svg(&HANDLE, SVG, size)
}

pub fn icon_theater<'a, Message: 'static>(size: f32) -> Element<'a, Message> {
    static HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#f1f1f1" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="6" width="20" height="12" rx="2"/></svg>"##;
    render_cached_svg(&HANDLE, SVG, size)
}

pub fn icon_fullscreen<'a, Message: 'static>(size: f32) -> Element<'a, Message> {
    static HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#f1f1f1" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M8 3H5a2 2 0 0 0-2 2v3m18 0V5a2 2 0 0 0-2-2h-3m0 18h3a2 2 0 0 0 2-2v-3M3 16v3a2 2 0 0 0 2 2h3"/></svg>"##;
    render_cached_svg(&HANDLE, SVG, size)
}

pub fn icon_exit_fullscreen<'a, Message: 'static>(size: f32) -> Element<'a, Message> {
    static HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#f1f1f1" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M8 3v3a2 2 0 0 1-2 2H3m18 0h-3a2 2 0 0 1-2-2V3m0 18v-3a2 2 0 0 1 2-2h3M3 16h3a2 2 0 0 1 2 2v3"/></svg>"##;
    render_cached_svg(&HANDLE, SVG, size)
}

pub fn icon_thumb_up<'a, Message: 'static>(size: f32) -> Element<'a, Message> {
    static HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#f1f1f1" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M14 9V5a3 3 0 0 0-3-3l-4 9v11h11.28a2 2 0 0 0 2-1.7l1.38-9a2 2 0 0 0-2-2.3zM7 22H4a2 2 0 0 1-2-2v-7a2 2 0 0 1 2-2h3"/></svg>"##;
    render_cached_svg(&HANDLE, SVG, size)
}

pub fn icon_thumb_down<'a, Message: 'static>(size: f32) -> Element<'a, Message> {
    static HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#f1f1f1" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M10 15v4a3 3 0 0 0 3 3l4-9V2H5.72a2 2 0 0 0-2 1.7l-1.38 9a2 2 0 0 0 2 2.3zm7-13h3a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2h-3"/></svg>"##;
    render_cached_svg(&HANDLE, SVG, size)
}

pub fn icon_close<'a, Message: 'static>(size: f32) -> Element<'a, Message> {
    static HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#f1f1f1" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"></line><line x1="6" y1="6" x2="18" y2="18"></line></svg>"##;
    render_cached_svg(&HANDLE, SVG, size)
}

pub fn icon_pip<'a, Message: 'static>(size: f32) -> Element<'a, Message> {
    static HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#f1f1f1" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="3" width="20" height="15" rx="2"/><rect x="12" y="9" width="8" height="7" rx="1" fill="#f1f1f1"/></svg>"##;
    render_cached_svg(&HANDLE, SVG, size)
}

pub fn icon_copy<'a, Message: 'static>(size: f32) -> Element<'a, Message> {
    static HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#f1f1f1" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>"##;
    render_cached_svg(&HANDLE, SVG, size)
}

pub fn icon_history<'a, Message: 'static>(size: f32) -> Element<'a, Message> {
    static HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#f1f1f1" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12 8v4l3 3"/><circle cx="12" cy="12" r="9"/></svg>"##;
    render_cached_svg(&HANDLE, SVG, size)
}

pub fn icon_live_dot<'a, Message: 'static>(size: f32) -> Element<'a, Message> {
    static HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 12 12"><circle cx="6" cy="6" r="5" fill="#ff0000"/></svg>"##;
    render_cached_svg(&HANDLE, SVG, size)
}

pub fn icon_audio_track<'a, Message: 'static>(size: f32) -> Element<'a, Message> {
    static HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#f1f1f1" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 18v-6a9 9 0 0 1 18 0v6"/><path d="M21 19a2 2 0 0 1-2 2h-1a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3zM3 19a2 2 0 0 0 2 2h1a2 2 0 0 0 2-2v-3a2 2 0 0 0-2-2H3z"/></svg>"##;
    render_cached_svg(&HANDLE, SVG, size)
}

pub fn icon_subscriptions<'a, Message: 'static>(size: f32) -> Element<'a, Message> {
    static HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#f1f1f1" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="7" width="20" height="14" rx="2"/><path d="M5 4h14"/><path d="M8 1h8"/><polygon points="10 11 16 14 10 17 10 11" fill="#f1f1f1"/></svg>"##;
    render_cached_svg(&HANDLE, SVG, size)
}

pub fn icon_queue_add<'a, Message: 'static>(size: f32) -> Element<'a, Message> {
    static HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#f1f1f1" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="3" y1="6" x2="17" y2="6"/><line x1="3" y1="12" x2="13" y2="12"/><line x1="3" y1="18" x2="11" y2="18"/><line x1="17" y1="14" x2="17" y2="20"/><line x1="14" y1="17" x2="20" y2="17"/></svg>"##;
    render_cached_svg(&HANDLE, SVG, size)
}

/// Renders a circular channel/author avatar using the first ASCII alphanumeric character
pub fn view_avatar_badge<'a, Message: 'static>(name: &str, diameter: f32, font_size: f32) -> iced::widget::Container<'a, Message> {
    let initial = name
        .chars()
        .find(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase().to_string())
        .unwrap_or_else(|| "Y".to_string());

    container(text(initial).size(font_size).color(COLOR_TEXT_PRIMARY))
        .width(Length::Fixed(diameter))
        .height(Length::Fixed(diameter))
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center)
        .style(move |_theme| container::Style {
            background: Some(COLOR_SURFACE_HOVER.into()),
            border: iced::border::rounded(diameter / 2.0),
            ..container::Style::default()
        })
}

pub fn view_avatar_image_or_badge_with_radius<'a, Message: 'static>(
    name: &str,
    avatar_handle: Option<&'a iced::widget::image::Handle>,
    diameter: f32,
    font_size: f32,
    border_radius: f32,
) -> Element<'a, Message> {
    if let Some(h) = avatar_handle {
        container(
            iced::widget::image(h.clone())
                .width(Length::Fixed(diameter))
                .height(Length::Fixed(diameter))
                .content_fit(iced::ContentFit::Cover),
        )
        .width(Length::Fixed(diameter))
        .height(Length::Fixed(diameter))
        .style(move |_theme| container::Style {
            border: iced::border::rounded(border_radius),
            ..container::Style::default()
        })
        .into()
    } else {
        let initial = name
            .chars()
            .find(|c| c.is_ascii_alphanumeric())
            .map(|c| c.to_ascii_uppercase().to_string())
            .unwrap_or_else(|| "Y".to_string());

        container(text(initial).size(font_size).color(COLOR_TEXT_PRIMARY))
            .width(Length::Fixed(diameter))
            .height(Length::Fixed(diameter))
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center)
            .style(move |_theme| container::Style {
                background: Some(COLOR_SURFACE_HOVER.into()),
                border: iced::border::rounded(border_radius),
                ..container::Style::default()
            })
            .into()
    }
}

/// Renders a circular avatar from downloaded image handle, or falls back to view_avatar_badge
pub fn view_avatar_image_or_badge<'a, Message: 'static>(
    name: &str,
    avatar_handle: Option<&'a iced::widget::image::Handle>,
    diameter: f32,
    font_size: f32,
) -> Element<'a, Message> {
    view_avatar_image_or_badge_with_radius(name, avatar_handle, diameter, font_size, diameter / 2.0)
}

pub fn icon_chevron_right<'a, Message: 'static>(size: f32) -> Element<'a, Message> {
    static HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#f1f1f1" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 18 15 12 9 6"/></svg>"##;
    render_cached_svg(&HANDLE, SVG, size)
}

pub fn icon_link<'a, Message: 'static>(size: f32) -> Element<'a, Message> {
    static HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#3ea6ff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/></svg>"##;
    render_cached_svg(&HANDLE, SVG, size)
}

pub fn icon_playlist<'a, Message: 'static>(size: f32) -> Element<'a, Message> {
    static HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#f1f1f1" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15V6M18.5 18a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5zM12 12H3M16 6H3M12 18H3"/></svg>"##;
    render_cached_svg(&HANDLE, SVG, size)
}

pub fn icon_buffer<'a, Message: 'static>(size: f32) -> Element<'a, Message> {
    static HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#f1f1f1" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>"##;
    render_cached_svg(&HANDLE, SVG, size)
}

pub fn icon_share<'a, Message: 'static>(size: f32) -> Element<'a, Message> {
    static HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#f1f1f1" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M15 17l5-5-5-5"/><path d="M4 19v-3a6 6 0 0 1 6-6h10"/></svg>"##;
    render_cached_svg(&HANDLE, SVG, size)
}

pub fn icon_trash<'a, Message: 'static>(size: f32) -> Element<'a, Message> {
    static HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#f1f1f1" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/><line x1="10" y1="11" x2="10" y2="17"/><line x1="14" y1="11" x2="14" y2="17"/></svg>"##;
    render_cached_svg(&HANDLE, SVG, size)
}

pub fn icon_chevron_left<'a, Message: 'static>(size: f32) -> Element<'a, Message> {
    static HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#f1f1f1" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 18 9 12 15 6"/></svg>"##;
    render_cached_svg(&HANDLE, SVG, size)
}

pub fn icon_logout<'a, Message: 'static>(size: f32) -> Element<'a, Message> {
    static HANDLE: OnceLock<svg::Handle> = OnceLock::new();
    const SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#f1f1f1" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/><polyline points="16 17 21 12 16 7"/><line x1="21" y1="12" x2="9" y2="12"/></svg>"##;
    render_cached_svg(&HANDLE, SVG, size)
}




