pub mod channel_view;
pub mod home_view;
pub mod icons;
pub mod osd_controls;
pub mod theme;
pub mod video_widget;
pub mod watch_view;

use crate::cache_manager::{format_size, limit_to_index, CACHE_LIMIT_LABELS};
use crate::config::{PreferredAudioLanguage, PreferredCodec, PreferredResolution};
use crate::power_manager::PowerProfile;
use crate::ui::icons::*;
use crate::ui::theme::*;
use iced::widget::{button, column, container, image, row, slider, text, text_input, Space};
use iced::{Alignment, Element, Length};

pub use crate::config::VideoTab;

pub fn view_header_bar<'a, Message: Clone + 'static>(
    search_query: &'a str,
    _suggestions: &'a [String],
    _show_suggestions: bool,
    _power_profile: PowerProfile,
    is_authenticated: bool,
    user_name: Option<&'a str>,
    user_avatar_handle: Option<&'a iced::widget::image::Handle>,
    is_subscriptions: bool,
    is_history: bool,
    is_home: bool,
    tabs: &'a [VideoTab],
    active_tab_idx: Option<usize>,
    newly_opened_tab: Option<usize>,
    shaking_tab: Option<(usize, f32)>,
    window_width: f32,
    tabs_scroll_x: f32,
    on_search_input: impl Fn(String) -> Message + 'static,
    on_search_submit: Message,
    _on_select_suggestion: impl Fn(String) -> Message + 'static,
    on_go_home: Message,
    on_go_back: Message,
    on_open_subscriptions: Message,
    on_open_history: Message,
    on_toggle_login_modal: Message,
    on_reload: Message,
    on_select_tab: impl Fn(usize) -> Message + Copy + 'static,
    on_close_tab: impl Fn(usize) -> Message + Copy + 'static,
    on_scroll_tabs: impl Fn(iced::widget::scrollable::Viewport) -> Message + Copy + 'static,
    on_scroll_tabs_left: Message,
    on_scroll_tabs_right: Message,
) -> Element<'a, Message> {
    // 1. Left section: Back, Logo (white when away, red when home)
    let back_btn = button(icon_back(18.0))
        .style(ghost_button)
        .padding([6, 10])
        .on_press(on_go_back);

    let logo = button(icon_youtube_logo(20.0, is_home))
        .style(ghost_button)
        .padding([4, 6])
        .on_press(on_go_home);

    let left_section = row![back_btn, logo]
        .spacing(6)
        .align_y(Alignment::Center);

    // 2. Suscripciones (icon-only) + Historial + Search Bar - alineado a la izquierda junto al logo
    let subs_btn = button(icon_subscriptions(16.0))
        .style(move |_t, status| {
            if is_subscriptions {
                button::Style {
                    background: Some(iced::Color::from_rgb8(0xcc, 0x00, 0x00).into()),
                    border: iced::border::rounded(6).color(iced::Color::from_rgb8(0xff, 0x33, 0x33)).width(1.0),
                    ..button::Style::default()
                }
            } else {
                ghost_button(_t, status)
            }
        })
        .padding([6, 8])
        .on_press(on_open_subscriptions);

    let history_btn = button(icon_history(16.0))
        .style(move |_t, status| {
            if is_history {
                button::Style {
                    background: Some(iced::Color::from_rgb8(0xcc, 0x00, 0x00).into()),
                    border: iced::border::rounded(6).color(iced::Color::from_rgb8(0xff, 0x33, 0x33)).width(1.0),
                    ..button::Style::default()
                }
            } else {
                ghost_button(_t, status)
            }
        })
        .padding([6, 8])
        .on_press(on_open_history);

    let search_bar = text_input("Buscar o pegar enlace...", search_query)
        .id(iced::widget::text_input::Id::new("main_search_input"))
        .on_input(on_search_input)
        .on_submit(on_search_submit.clone())
        .style(search_input)
        .padding([6, 12])
        .width(Length::Fixed(260.0));

    let search_btn = button(icon_search(14.0))
        .style(secondary_button)
        .padding([6, 10])
        .on_press(on_search_submit);

    let search_container = row![subs_btn, history_btn, search_bar, search_btn]
        .spacing(4)
        .align_y(Alignment::Center);

    // 3. Center/Horizontal Tabs section: Pestañas de video abiertas con middle-click
    let mut tabs_row = row![].spacing(6).align_y(Alignment::Center);
    for (idx, tab) in tabs.iter().enumerate() {
        let is_active = active_tab_idx == Some(idx);
        let is_newly_opened = newly_opened_tab == Some(idx);
        let shake_elapsed = shaking_tab.and_then(|(s_idx, el)| if s_idx == idx { Some(el) } else { None });

        let tab_title = if tab.title.chars().count() > 20 {
            let mut t: String = tab.title.chars().take(18).collect();
            t.push_str("..");
            t
        } else if tab.title.is_empty() {
            "Nueva pestaña".to_string()
        } else {
            tab.title.clone()
        };

        let select_btn = button(
            text(tab_title)
                .size(11)
                .color(if is_active { iced::Color::WHITE } else { COLOR_TEXT_SECONDARY }),
        )
        .style(ghost_button)
        .padding([4, 6])
        .on_press(on_select_tab(idx));

        let close_btn = button(icon_close(10.0))
            .style(ghost_button)
            .padding([4, 4])
            .on_press(on_close_tab(idx));

        let (shake_offset, border_color, border_width) = if let Some(el) = shake_elapsed {
            let decay = (1.0 - el / 0.6).clamp(0.0, 1.0);
            let offset = (el * 38.0).sin() * 5.0 * decay;
            let c = iced::Color::from_rgba(0.9, 0.3, 0.3, 0.2 + 0.8 * decay);
            (offset, c, 1.5)
        } else if is_active {
            (0.0, iced::Color::from_rgb8(0xff, 0x33, 0x33), 1.5)
        } else if is_newly_opened {
            (0.0, iced::Color::from_rgb8(0xff, 0x55, 0x55), 1.5)
        } else {
            (0.0, COLOR_BORDER, 1.0)
        };

        let pad_l = (4.0 + shake_offset).max(0.0);
        let pad_r = (4.0 - shake_offset).max(0.0);

        let tab_pill = iced::widget::mouse_area(
            container(
                row![select_btn, close_btn]
                    .align_y(Alignment::Center)
                    .spacing(2),
            )
            .style(move |_t| container::Style {
                // Al vibrar una pestaña inactiva se mantiene estrictamente en el tono inactivo (0x18, 0x18, 0x18)
                background: Some(if is_active {
                    iced::Color::from_rgb8(0xcc, 0x00, 0x00).into()
                } else if is_newly_opened {
                    iced::Color::from_rgb8(0x44, 0x11, 0x11).into()
                } else {
                    iced::Color::from_rgb8(0x18, 0x18, 0x18).into()
                }),
                border: iced::border::rounded(6)
                    .color(border_color)
                    .width(border_width),
                ..container::Style::default()
            })
            .padding(iced::Padding {
                top: 1.0,
                bottom: 1.0,
                left: pad_l,
                right: pad_r,
            }),
        )
        .on_middle_press(on_close_tab(idx));

        tabs_row = tabs_row.push(tab_pill);
    }

    let tabs_scrollable = iced::widget::scrollable(tabs_row)
        .direction(iced::widget::scrollable::Direction::Horizontal(
            iced::widget::scrollable::Scrollbar::new()
                .width(0.0)
                .scroller_width(0.0),
        ))
        .style(|_t, _s| iced::widget::scrollable::Style {
            container: container::Style::default(),
            vertical_rail: iced::widget::scrollable::Rail {
                background: None,
                border: iced::border::rounded(0),
                scroller: iced::widget::scrollable::Scroller {
                    color: iced::Color::TRANSPARENT,
                    border: iced::border::rounded(0),
                },
            },
            horizontal_rail: iced::widget::scrollable::Rail {
                background: None,
                border: iced::border::rounded(0),
                scroller: iced::widget::scrollable::Scroller {
                    color: iced::Color::TRANSPARENT,
                    border: iced::border::rounded(0),
                },
            },
            gap: None,
        })
        .id(iced::widget::scrollable::Id::new("tabs_scrollable"))
        .on_scroll(on_scroll_tabs);

    let available_tabs_width = (window_width - 720.0).max(80.0);
    let estimated_tabs_width = tabs.len() as f32 * 145.0;
    let can_scroll_left = tabs_scroll_x > 10.0;
    let can_scroll_right = tabs.len() > 1 && (estimated_tabs_width - tabs_scroll_x) > (available_tabs_width - 20.0);

    let tabs_section: Element<'a, Message> = {
        let now_ms = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() % 1400) as f32 / 1400.0;
        let wave = (now_ms * std::f32::consts::TAU).sin() * 0.5 + 0.5; // 0.0 .. 1.0
        let bounce_x = wave * 3.0;

        let left_btn = if can_scroll_left {
            let border_c = iced::Color::from_rgba(0.9, 0.2, 0.2, 0.35 + wave * 0.45);
            Some(
                button(
                    container(icon_chevron_left(13.0))
                        .padding(iced::Padding {
                            top: 1.0,
                            right: 2.0,
                            bottom: 1.0,
                            left: 2.0 + bounce_x,
                        })
                        .align_x(iced::alignment::Horizontal::Center)
                        .align_y(iced::alignment::Vertical::Center),
                )
                .style(move |_t, status| match status {
                    button::Status::Hovered => button::Style {
                        background: Some(iced::Color::from_rgba8(0x38, 0x18, 0x18, 0.95).into()),
                        border: iced::border::rounded(14).color(COLOR_ACCENT_RED).width(1.5),
                        ..button::Style::default()
                    },
                    _ => button::Style {
                        background: Some(iced::Color::from_rgba8(0x22, 0x22, 0x22, 0.85).into()),
                        border: iced::border::rounded(14).color(border_c).width(1.0),
                        ..button::Style::default()
                    },
                })
                .padding([3, 5])
                .on_press(on_scroll_tabs_left),
            )
        } else {
            None
        };

        let right_btn = if can_scroll_right {
            let border_c = iced::Color::from_rgba(0.9, 0.2, 0.2, 0.35 + wave * 0.45);
            Some(
                button(
                    container(icon_chevron_right(13.0))
                        .padding(iced::Padding {
                            top: 1.0,
                            right: 2.0 + bounce_x,
                            bottom: 1.0,
                            left: 2.0,
                        })
                        .align_x(iced::alignment::Horizontal::Center)
                        .align_y(iced::alignment::Vertical::Center),
                )
                .style(move |_t, status| match status {
                    button::Status::Hovered => button::Style {
                        background: Some(iced::Color::from_rgba8(0x38, 0x18, 0x18, 0.95).into()),
                        border: iced::border::rounded(14).color(COLOR_ACCENT_RED).width(1.5),
                        ..button::Style::default()
                    },
                    _ => button::Style {
                        background: Some(iced::Color::from_rgba8(0x22, 0x22, 0x22, 0.85).into()),
                        border: iced::border::rounded(14).color(border_c).width(1.0),
                        ..button::Style::default()
                    },
                })
                .padding([3, 5])
                .on_press(on_scroll_tabs_right),
            )
        } else {
            None
        };

        let mut row_el = row![].spacing(4).align_y(Alignment::Center);
        if let Some(lb) = left_btn {
            row_el = row_el.push(lb);
        }
        row_el = row_el.push(container(tabs_scrollable).width(Length::Fill));
        if let Some(rb) = right_btn {
            row_el = row_el.push(rb);
        }
        row_el.into()
    };

    // 4. Right section: Account & Reload
    let auth_btn_content: Element<'a, Message> = if is_authenticated {
        let avatar_el: Element<'a, Message> = if let Some(handle) = user_avatar_handle {
            container(
                image(handle.clone())
                    .width(Length::Fixed(24.0))
                    .height(Length::Fixed(24.0))
                    .content_fit(iced::ContentFit::Cover),
            )
            .width(Length::Fixed(24.0))
            .height(Length::Fixed(24.0))
            .style(|_t| container::Style {
                border: iced::border::rounded(12).color(COLOR_BORDER_FOCUS).width(1.5),
                ..container::Style::default()
            })
            .into()
        } else if let Some(name) = user_name {
            view_avatar_badge(name, 24.0, 11.0).into()
        } else {
            icon_user(16.0)
        };

        if let Some(name) = user_name {
            row![
                avatar_el,
                text(name)
                    .size(13)
                    .color(COLOR_TEXT_PRIMARY),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into()
        } else {
            avatar_el
        }
    } else {
        row![icon_user(14.0), text("Iniciar Sesión").size(12)]
            .spacing(6)
            .align_y(Alignment::Center)
            .into()
    };

    let auth_btn = button(auth_btn_content)
        .style(if is_authenticated {
            secondary_button
        } else {
            primary_button
        })
        .padding(if is_authenticated {
            if user_name.is_some() {
                [4, 10]
            } else if user_avatar_handle.is_some() {
                [2, 2]
            } else {
                [6, 8]
            }
        } else {
            [6, 12]
        })
        .on_press(on_toggle_login_modal);

    let reload_btn = button(icon_reload(16.0))
        .style(ghost_button)
        .padding([6, 8])
        .on_press(on_reload);

    let right_section = row![auth_btn, reload_btn]
        .spacing(8)
        .align_y(Alignment::Center);

    container(
        row![
            left_section,
            search_container,
            Space::with_width(Length::Fixed(12.0)),
            container(tabs_section).width(Length::Fill),
            Space::with_width(Length::Fixed(12.0)),
            right_section,
        ]
        .align_y(Alignment::Center)
        .padding([6, 14]),
    )
    .style(|_t| container::Style {
        background: Some(COLOR_BG.into()),
        border: iced::border::color(COLOR_BORDER).width(1.0),
        ..container::Style::default()
    })
    .height(Length::Fixed(56.0))
    .width(Length::Fill)
    .into()
}

pub fn view_suggestions_overlay<'a, Message: Clone + 'static>(
    suggestions: &'a [String],
    on_select_suggestion: impl Fn(String) -> Message + 'static,
) -> Element<'a, Message> {
    let mut sugg_list = column![].spacing(4);
    for s in suggestions.iter().take(6) {
        let s_clone = s.clone();
        let item_btn = button(
            row![icon_search(13.0), text(s).size(13).color(COLOR_TEXT_PRIMARY)]
                .spacing(8)
                .align_y(Alignment::Center),
        )
        .style(ghost_button)
        .width(Length::Fill)
        .on_press(on_select_suggestion(s_clone));
        sugg_list = sugg_list.push(item_btn);
    }

    let dropdown = container(sugg_list)
        .style(card_container)
        .padding(8)
        .width(Length::Fixed(320.0));

    container(
        column![
            Space::with_height(Length::Fixed(52.0)),
            row![
                Space::with_width(Length::Fixed(280.0)),
                dropdown,
                Space::with_width(Length::Fill),
            ],
        ],
    )
    .width(Length::Fill)
    .into()
}

pub fn view_auth_modal<'a, Message: Clone + 'static>(
    raw_cookie_input: &'a str,
    status_msg: &'a str,
    user_name: Option<&'a str>,
    user_avatar_handle: Option<&'a iced::widget::image::Handle>,
    current_res: PreferredResolution,
    current_codec: PreferredCodec,
    current_audio_lang: PreferredAudioLanguage,
    auto_pip_on_unfocus: bool,
    full_buffer_tabs: bool,
    max_cache_size_bytes: u64,
    current_cache_size_bytes: u64,
    on_cookie_input: impl Fn(String) -> Message + 'static,
    on_save_raw_cookie: Message,
    on_import_firefox: Message,
    on_import_chrome: Message,
    on_logout: Message,
    on_open_subscriptions: Message,
    on_open_history: Message,
    on_open_playlists: Message,
    on_select_res: impl Fn(PreferredResolution) -> Message + Copy + 'static,
    on_select_codec: impl Fn(PreferredCodec) -> Message + Copy + 'static,
    on_select_audio_lang: impl Fn(PreferredAudioLanguage) -> Message + Copy + 'static,
    on_toggle_auto_pip: Message,
    on_toggle_full_buffer_tabs: Message,
    on_change_cache_limit: impl Fn(usize) -> Message + Copy + 'static,
    on_clear_cache: Message,
    on_close_modal: Message,
) -> Element<'a, Message> {
    // 1. Profile Header Banner
    let display_name = user_name.unwrap_or("Usuario de YouTube");
    let avatar_widget: Element<'a, Message> = if let Some(handle) = user_avatar_handle {
        container(
            image(handle.clone())
                .width(Length::Fixed(44.0))
                .height(Length::Fixed(44.0))
                .content_fit(iced::ContentFit::Cover),
        )
        .width(Length::Fixed(44.0))
        .height(Length::Fixed(44.0))
        .style(|_t| container::Style {
            border: iced::border::rounded(22).color(COLOR_BORDER_FOCUS).width(2.0),
            ..container::Style::default()
        })
        .into()
    } else if user_name.is_some() {
        view_avatar_badge(display_name, 44.0, 18.0).into()
    } else {
        container(icon_user(22.0))
            .width(Length::Fixed(44.0))
            .height(Length::Fixed(44.0))
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center)
            .style(|_t| container::Style {
                background: Some(iced::Color::from_rgb8(0x28, 0x28, 0x28).into()),
                border: iced::border::rounded(22).color(COLOR_BORDER).width(1.0),
                ..container::Style::default()
            })
            .into()
    };
    let profile_header = row![
        avatar_widget,
        column![
            text(display_name).size(16).color(COLOR_TEXT_PRIMARY),
            text(if status_msg.starts_with("[OK]") {
                "Sesión activa • Recomendaciones e Historial sincronizados"
            } else {
                "Modo Invitado • Inicia sesión para sincronizar tu cuenta"
            })
            .size(12)
            .color(if status_msg.starts_with("[OK]") {
                COLOR_BORDER_FOCUS
            } else {
                COLOR_TEXT_SECONDARY
            }),
        ]
        .spacing(3),
        Space::with_width(Length::Fill),
        button(icon_close(14.0))
            .style(ghost_button)
            .padding([6, 8])
            .on_press(on_close_modal.clone()),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    // 2. Direct Navigation Links (Suscripciones, Historial, Playlists)
    let nav_heading = text("Tu Contenido y Biblioteca")
        .size(13)
        .color(COLOR_TEXT_PRIMARY);

    let subs_nav_btn = button(
        row![
            icon_subscriptions(16.0),
            text("Suscripciones").size(12).color(COLOR_TEXT_PRIMARY),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .style(secondary_button)
    .padding([8, 14])
    .on_press(on_open_subscriptions);

    let history_nav_btn = button(
        row![
            icon_history(16.0),
            text("Historial").size(12).color(COLOR_TEXT_PRIMARY),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .style(secondary_button)
    .padding([8, 14])
    .on_press(on_open_history);

    let playlists_nav_btn = button(
        row![
            icon_playlist(16.0),
            text("Playlists").size(12).color(COLOR_TEXT_PRIMARY),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .style(secondary_button)
    .padding([8, 14])
    .on_press(on_open_playlists);

    let nav_row = row![subs_nav_btn, history_nav_btn, playlists_nav_btn]
        .spacing(10)
        .align_y(Alignment::Center);

    // 3. Section: Default Video Quality & Codec
    let defaults_heading = text("Preferencias de Reproducción Predeterminadas (Default)")
        .size(13)
        .color(COLOR_TEXT_PRIMARY);

    let res_options = [
        PreferredResolution::Res4K,
        PreferredResolution::Res1440p,
        PreferredResolution::Res1080p,
        PreferredResolution::Res720p,
    ];

    let mut res_row = row![].spacing(8).align_y(Alignment::Center);
    for r in res_options {
        let is_selected = r == current_res;
        let btn = button(text(r.label()).size(12))
            .style(if is_selected { primary_button } else { secondary_button })
            .padding([5, 10])
            .on_press(on_select_res(r));
        res_row = res_row.push(btn);
    }

    let codec_options = [
        PreferredCodec::Av1,
        PreferredCodec::Vp9,
        PreferredCodec::H264,
    ];

    let mut codec_row = row![].spacing(8).align_y(Alignment::Center);
    for c in codec_options {
        let is_selected = c == current_codec;
        let btn = button(text(c.label()).size(12))
            .style(if is_selected { primary_button } else { secondary_button })
            .padding([5, 10])
            .on_press(on_select_codec(c));
        codec_row = codec_row.push(btn);
    }

    let audio_options = [
        PreferredAudioLanguage::Original,
        PreferredAudioLanguage::Spanish,
        PreferredAudioLanguage::English,
    ];

    let mut audio_row = row![].spacing(8).align_y(Alignment::Center);
    for a in audio_options {
        let is_selected = a == current_audio_lang;
        let btn = button(text(a.label()).size(12))
            .style(if is_selected { primary_button } else { secondary_button })
            .padding([5, 10])
            .on_press(on_select_audio_lang(a));
        audio_row = audio_row.push(btn);
    }

    let fallback_hint = text(
        "Fallback Inteligente Automático: Si el video no dispone de la calidad o códec seleccionado,\n\
        Mitub descenderá progresivamente en cascada (4K -> 1440p -> 1080p -> 720p y AV1 -> VP9 -> H.264)\n\
        garantizando siempre la máxima calidad soportada sin interrupciones ni fallos.",
    )
    .size(11)
    .color(COLOR_TEXT_SECONDARY);

    // Ventana Flotante PiP Automática (Multitarea)
    let auto_pip_heading = text("Ventana Flotante PiP Automática (Multitarea)")
        .size(13)
        .color(COLOR_TEXT_PRIMARY);

    let auto_pip_btn = button(
        row![
            if auto_pip_on_unfocus {
                icon_check_circle(15.0)
            } else {
                icon_pip(15.0)
            },
            text(if auto_pip_on_unfocus {
                "PiP Automático al cambiar de ventana (Alt+Tab): ACTIVADO"
            } else {
                "PiP Automático al cambiar de ventana (Alt+Tab): Desactivado"
            })
            .size(12),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .style(if auto_pip_on_unfocus { primary_button } else { secondary_button })
    .padding([6, 14])
    .on_press(on_toggle_auto_pip);

    let auto_pip_hint = text(
        "Al cambiar a otra aplicación con Alt+Tab mientras reproduces un video,\n\
        Mitub activará automáticamente la ventana flotante PiP y la restaurará al volver.",
    )
    .size(11)
    .color(COLOR_TEXT_SECONDARY);

    // Modo Pestañas con Buffer Completo
    let buffer_tabs_heading = text("Modo Pestañas con Buffer Completo (Precarga de Sesión)")
        .size(13)
        .color(COLOR_TEXT_PRIMARY);

    let buffer_tabs_btn = button(
        row![
            if full_buffer_tabs {
                icon_check_circle(15.0)
            } else {
                icon_buffer(15.0)
            },
            text(if full_buffer_tabs {
                "Precarga / Buffer Completo en Pestañas: ACTIVADO"
            } else {
                "Precarga / Buffer Completo en Pestañas: Desactivado"
            })
            .size(12),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .style(if full_buffer_tabs { primary_button } else { secondary_button })
    .padding([6, 14])
    .on_press(on_toggle_full_buffer_tabs);

    let buffer_tabs_hint = text(
        "Al abrir un video en una pestaña, descarga y bufferea el video completo en segundo plano.\n\
        Te permite saltar entre pestañas y desplazarte sobre cualquier segundo del video al instante\n\
        sin esperas ni buffering de red durante toda la sesión.",
    )
    .size(11)
    .color(COLOR_TEXT_SECONDARY);

    // 4. Section: Límite y Gestión de Caché
    let cache_heading = text("Almacenamiento y Límite de Caché de Video")
        .size(13)
        .color(COLOR_TEXT_PRIMARY);

    let current_idx = limit_to_index(max_cache_size_bytes);
    let current_limit_label = CACHE_LIMIT_LABELS.get(current_idx).copied().unwrap_or("5 GB (Estándar)");

    let cache_slider = slider(
        0..=5_u8,
        current_idx as u8,
        move |val| on_change_cache_limit(val as usize),
    )
    .step(1_u8)
    .style(youtube_seek_slider)
    .width(Length::Fixed(250.0));

    let clear_cache_btn = button(
        row![
            icon_trash(14.0),
            text(format!("Limpiar caché ({})", format_size(current_cache_size_bytes))).size(12),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .style(secondary_button)
    .padding([6, 12])
    .on_press(on_clear_cache);

    let cache_slider_col = column![
        row![
            text("Tope: ").size(12).color(COLOR_TEXT_SECONDARY),
            text(current_limit_label).size(12).color(COLOR_ACCENT_RED),
            Space::with_width(Length::Fixed(12.0)),
            text(format!("En disco: {}", format_size(current_cache_size_bytes))).size(12).color(COLOR_TEXT_SECONDARY),
        ]
        .align_y(Alignment::Center),
        cache_slider,
        row![
            text("1 GB").size(10).color(COLOR_TEXT_SECONDARY),
            Space::with_width(Length::Fill),
            text("2 GB").size(10).color(COLOR_TEXT_SECONDARY),
            Space::with_width(Length::Fill),
            text("5 GB").size(10).color(COLOR_ACCENT_RED),
            Space::with_width(Length::Fill),
            text("10 GB").size(10).color(COLOR_TEXT_SECONDARY),
            Space::with_width(Length::Fill),
            text("20 GB").size(10).color(COLOR_TEXT_SECONDARY),
            Space::with_width(Length::Fill),
            text("Ilimitado").size(10).color(COLOR_TEXT_SECONDARY),
        ]
        .width(Length::Fixed(250.0)),
    ]
    .spacing(4);

    let cache_controls_row = row![
        cache_slider_col,
        Space::with_width(Length::Fixed(20.0)),
        clear_cache_btn,
    ]
    .align_y(Alignment::Center);

    let cache_hint = text(
        "Almacenamiento persistente en disco (~/.cache/mitub/video_cache).\n\
        Al alcanzar el tope máximo configurado, Mitub purga automáticamente los videos más antiguos (LRU).",
    )
    .size(11)
    .color(COLOR_TEXT_SECONDARY);

    // 5. Section: YouTube Auth / Cookies
    let auth_heading = text("Cuenta de Google / YouTube")
        .size(13)
        .color(COLOR_TEXT_PRIMARY);

    let is_authenticated = status_msg.starts_with("[OK]");
    let auth_action_row: Element<'a, Message> = if is_authenticated {
        let logout_btn = button(
            row![
                icon_logout(13.0),
                text("Cerrar Sesión").size(12).color(COLOR_TEXT_PRIMARY),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .style(secondary_button)
        .padding([6, 14])
        .on_press(on_logout);

        row![
            text("Sesión sincronizada activamente.")
                .size(12)
                .color(COLOR_TEXT_SECONDARY),
            Space::with_width(Length::Fill),
            logout_btn,
        ]
        .align_y(Alignment::Center)
        .into()
    } else {
        let firefox_btn = button(
            row![
                icon_user(13.0),
                text("Importar desde Firefox").size(12).color(COLOR_TEXT_PRIMARY),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .style(secondary_button)
        .padding([6, 12])
        .on_press(on_import_firefox);

        let chrome_btn = button(
            row![
                icon_user(13.0),
                text("Importar desde Chrome").size(12).color(COLOR_TEXT_PRIMARY),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .style(secondary_button)
        .padding([6, 12])
        .on_press(on_import_chrome);

        let input = text_input("O pega aquí tu Cookie: SAPISID=...; HSID=...", raw_cookie_input)
            .on_input(on_cookie_input)
            .style(search_input)
            .padding(8);

        let save_btn = button(text("Guardar Cookies").size(12))
            .style(primary_button)
            .padding([6, 12])
            .on_press(on_save_raw_cookie);

        column![
            text(
                "Inicia sesión para sincronizar tus suscripciones, historial y recomendaciones.\n\
                Compatible con perfiles de Linux, Windows y macOS. Tus credenciales nunca salen de tu equipo.",
            )
            .size(12)
            .color(COLOR_TEXT_SECONDARY),
            row![firefox_btn, chrome_btn].spacing(10).align_y(Alignment::Center),
            input,
            row![save_btn].align_y(Alignment::Center),
        ]
        .spacing(8)
        .into()
    };

    let close_btn = button(text("Cerrar").size(12))
        .style(ghost_button)
        .padding([6, 14])
        .on_press(on_close_modal);

    let status = text(status_msg).size(12).color(COLOR_TEXT_SECONDARY);

    let modal_content = iced::widget::scrollable(
        column![
            profile_header,
            Space::with_height(Length::Fixed(6.0)),
            nav_heading,
            nav_row,
            Space::with_height(Length::Fixed(6.0)),
            defaults_heading,
            text("Calidad por defecto:").size(12).color(COLOR_TEXT_SECONDARY),
            res_row,
            text("Códec por defecto:").size(12).color(COLOR_TEXT_SECONDARY),
            codec_row,
            text("Idioma de Audio por defecto:").size(12).color(COLOR_TEXT_SECONDARY),
            audio_row,
            fallback_hint,
            Space::with_height(Length::Fixed(6.0)),
            auto_pip_heading,
            auto_pip_btn,
            auto_pip_hint,
            Space::with_height(Length::Fixed(6.0)),
            buffer_tabs_heading,
            buffer_tabs_btn,
            buffer_tabs_hint,
            Space::with_height(Length::Fixed(6.0)),
            cache_heading,
            cache_controls_row,
            cache_hint,
            Space::with_height(Length::Fixed(6.0)),
            auth_heading,
            auth_action_row,
            row![Space::with_width(Length::Fill), close_btn].align_y(Alignment::Center),
            status,
        ]
        .spacing(10),
    )
    .height(Length::Shrink);

    let modal_box = container(modal_content)
        .style(card_container)
        .padding(22)
        .max_height(720.0)
        .width(Length::Fixed(680.0));

    container(modal_box)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center)
        .style(|_t| container::Style {
            background: Some(iced::Color::from_rgba(0.0, 0.0, 0.0, 0.7).into()),
            ..container::Style::default()
        })
        .into()
}

