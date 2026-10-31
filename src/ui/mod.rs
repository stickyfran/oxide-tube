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
use iced::widget::{button, canvas, column, container, image, mouse_area, row, scrollable, slider, stack, text, text_input, Column, Space};
use iced::{Alignment, Element, Length, Point, Rectangle, Renderer, Theme};

pub use crate::config::VideoTab;

/// Renders a smooth travelling perimeter glow around the tab that is currently playing a video.
#[derive(Debug, Clone, Copy)]
struct PlayingTabGlowBorder {
    phase: f32,
    is_active_tab: bool,
}

impl<Message> canvas::Program<Message> for PlayingTabGlowBorder {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let w = bounds.width;
        let h = bounds.height;
        if w < 4.0 || h < 4.0 {
            return vec![];
        }

        let center = Point::new(w * 0.5, h * 0.5);
        let angle = self.phase * std::f32::consts::TAU;
        let radius = (w.hypot(h)) * 0.55;
        let start = Point::new(center.x - angle.cos() * radius, center.y - angle.sin() * radius);
        let end = Point::new(center.x + angle.cos() * radius, center.y + angle.sin() * radius);

        let base_alpha = if self.is_active_tab { 0.45 } else { 0.25 };
        let base_color = iced::Color::from_rgba(1.0, 0.20, 0.20, base_alpha);
        let mid_glow = iced::Color::from_rgba(1.0, 0.28, 0.28, 0.92);
        let peak_glow = iced::Color::from_rgba(1.0, 0.78, 0.78, 1.0);

        let grad = canvas::gradient::Linear::new(start, end)
            .add_stop(0.0, base_color)
            .add_stop(0.32, base_color)
            .add_stop(0.46, mid_glow)
            .add_stop(0.50, peak_glow)
            .add_stop(0.54, mid_glow)
            .add_stop(0.68, base_color)
            .add_stop(1.0, base_color);

        // Soft outer halo stroke
        let outer_halo = canvas::gradient::Linear::new(start, end)
            .add_stop(0.0, iced::Color::TRANSPARENT)
            .add_stop(0.35, iced::Color::TRANSPARENT)
            .add_stop(0.50, iced::Color::from_rgba(1.0, 0.22, 0.22, 0.42))
            .add_stop(0.65, iced::Color::TRANSPARENT)
            .add_stop(1.0, iced::Color::TRANSPARENT);

        let halo_path = canvas::Path::rounded_rectangle(
            Point::new(0.75, 0.75),
            iced::Size::new((w - 1.5).max(1.0), (h - 1.5).max(1.0)),
            6.0.into(),
        );
        frame.stroke(
            &halo_path,
            canvas::Stroke {
                style: canvas::Style::Gradient(canvas::Gradient::Linear(outer_halo)),
                width: 3.2,
                ..canvas::Stroke::default()
            },
        );

        let border_path = canvas::Path::rounded_rectangle(
            Point::new(1.0, 1.0),
            iced::Size::new((w - 2.0).max(1.0), (h - 2.0).max(1.0)),
            6.0.into(),
        );
        frame.stroke(
            &border_path,
            canvas::Stroke {
                style: canvas::Style::Gradient(canvas::Gradient::Linear(grad)),
                width: 1.8,
                ..canvas::Stroke::default()
            },
        );

        vec![frame.into_geometry()]
    }
}

pub fn view_header_bar<'a, Message: Clone + 'static>(
    search_query: &'a str,
    search_bar_width: f32,
    is_search_focused: bool,
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
    playing_video_id: Option<&'a str>,
    is_video_playing: bool,
    newly_opened_tab: Option<(usize, f32)>,
    shaking_tab: Option<(usize, f32)>,
    window_width: f32,
    tabs_scroll_x: f32,
    tabs_max_scroll_x: f32,
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
    on_hover_tab: impl Fn(usize, bool) -> Message + Copy + 'static,
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

    let search_placeholder = if is_search_focused || !search_query.is_empty() {
        "Buscar o pegar enlace..."
    } else {
        "Buscar..."
    };

    let search_bar = text_input(search_placeholder, search_query)
        .id(iced::widget::text_input::Id::new("main_search_input"))
        .on_input(on_search_input)
        .on_submit(on_search_submit.clone())
        .style(search_input)
        .padding([6, 12])
        .width(Length::Fixed(search_bar_width));

    let search_btn = button(icon_search(14.0))
        .style(secondary_button)
        .padding([6, 10])
        .on_press(on_search_submit);

    let search_container = row![subs_btn, history_btn, search_bar, search_btn]
        .spacing(4)
        .align_y(Alignment::Center);

    // 3. Center/Horizontal Tabs section: Pestañas de video abiertas con middle-click
    let glow_phase = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        % 2200) as f32
        / 2200.0;

    let mut tabs_row = row![].spacing(6).align_y(Alignment::Center);
    let mut sum_tabs_width: f32 = 0.0;

    for (idx, tab) in tabs.iter().enumerate() {
        let is_active = active_tab_idx == Some(idx);
        let is_playing_this_tab =
            is_video_playing && !tab.video_id.is_empty() && playing_video_id == Some(tab.video_id.as_str());
        let open_elapsed = newly_opened_tab.and_then(|(n_idx, el)| if n_idx == idx { Some(el) } else { None });
        let is_newly_opened = open_elapsed.is_some();
        let shake_elapsed = shaking_tab.and_then(|(s_idx, el)| if s_idx == idx { Some(el) } else { None });

        let tab_title = if tab.title.chars().count() > 32 {
            let mut t: String = tab.title.chars().take(30).collect();
            t.push_str("..");
            t
        } else if tab.title.is_empty() {
            "Nueva pestaña".to_string()
        } else {
            tab.title.clone()
        };

        let char_count = tab_title.chars().count() as f32;
        let est_this_tab_w = (char_count * 6.5 + if is_playing_this_tab { 48.0 } else { 34.0 }).clamp(110.0, 235.0);
        sum_tabs_width += est_this_tab_w + if idx > 0 { 6.0 } else { 0.0 };

        let label_content: Element<'a, Message> = if is_playing_this_tab {
            row![
                icon_play(10.0),
                text(tab_title)
                    .size(11)
                    .wrapping(text::Wrapping::None)
                    .color(iced::Color::WHITE),
            ]
            .spacing(5)
            .align_y(Alignment::Center)
            .into()
        } else {
            text(tab_title)
                .size(11)
                .wrapping(text::Wrapping::None)
                .color(if is_active { iced::Color::WHITE } else { COLOR_TEXT_SECONDARY })
                .into()
        };

        let select_btn = button(label_content)
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
        } else if is_playing_this_tab {
            (0.0, iced::Color::TRANSPARENT, 0.0)
        } else if is_active {
            (0.0, iced::Color::from_rgb8(0xff, 0x33, 0x33), 1.5)
        } else if is_newly_opened {
            (0.0, iced::Color::from_rgb8(0xff, 0x55, 0x55), 1.5)
        } else {
            (0.0, COLOR_BORDER, 1.0)
        };

        let pad_l = (4.0 + shake_offset).max(0.0);
        let pad_r = (4.0 - shake_offset).max(0.0);

        let inner_pill = container(
            row![select_btn, close_btn]
                .align_y(Alignment::Center)
                .spacing(2),
        )
        .style(move |_t| container::Style {
            background: Some(if is_active {
                iced::Color::from_rgb8(0xcc, 0x00, 0x00).into()
            } else if is_playing_this_tab {
                iced::Color::from_rgb8(0x2b, 0x10, 0x10).into()
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
        });

        let mut tab_container: container::Container<'a, Message> = if is_playing_this_tab {
            let glow_canvas = canvas(PlayingTabGlowBorder {
                phase: glow_phase,
                is_active_tab: is_active,
            })
            .width(Length::Fill)
            .height(Length::Fill);

            container(stack![inner_pill, glow_canvas])
        } else {
            inner_pill
        };

        // Firefox-style slide-in expansion from left to right when a tab is newly opened (220ms cubic ease-out)
        if let Some(el) = open_elapsed {
            if el < 0.22 {
                let t = (el / 0.22).clamp(0.0, 1.0);
                let ease = 1.0 - (1.0 - t).powi(3);
                let target_w = 210.0;
                tab_container = tab_container
                    .width(Length::Fixed((target_w * ease).max(2.0)))
                    .clip(true);
            }
        }

        let tab_pill = mouse_area(tab_container)
            .on_enter(on_hover_tab(idx, true))
            .on_exit(on_hover_tab(idx, false))
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

    // Chrome-like overflow detection:
    // Real occupied width by non-tab elements in header:
    // left_section (~116) + search_container (buttons ~72 + search_bar) + margins/spacings (~52) + right_section (~160)
    let right_account_width = if is_authenticated && user_name.is_some() { 160.0 } else { 125.0 };
    let non_tabs_width = 240.0 + search_bar_width + right_account_width;
    let available_tabs_width = (window_width - non_tabs_width).max(80.0);
    let heuristic_max_scroll = (sum_tabs_width - available_tabs_width).max(0.0);
    let effective_max_scroll = tabs_max_scroll_x.max(heuristic_max_scroll);
    let clamped_scroll_x = tabs_scroll_x.clamp(0.0, effective_max_scroll);

    // Only show scroll controls when tabs actually exceed the viewport
    let has_overflow = effective_max_scroll > 8.0;
    let can_scroll_left = has_overflow && clamped_scroll_x > 4.0;
    let can_scroll_right = has_overflow && (clamped_scroll_x + 6.0) < effective_max_scroll;

    let tabs_section: Element<'a, Message> = {
        let base_scrollable = container(tabs_scrollable)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_y(iced::alignment::Vertical::Center);

        let mut layers: Vec<Element<'a, Message>> = vec![base_scrollable.into()];

        if can_scroll_left {
            let left_btn = button(
                container(icon_chevron_left(13.0))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(iced::alignment::Horizontal::Center)
                    .align_y(iced::alignment::Vertical::Center),
            )
            .width(Length::Fixed(24.0))
            .height(Length::Fixed(24.0))
            .padding(0)
            .style(|_t, status| match status {
                button::Status::Hovered | button::Status::Pressed => button::Style {
                    background: Some(iced::Color::from_rgba(0.24, 0.24, 0.24, 0.95).into()),
                    border: iced::border::rounded(12)
                        .color(iced::Color::from_rgb8(0x80, 0x80, 0x80))
                        .width(1.0),
                    ..button::Style::default()
                },
                _ => button::Style {
                    background: Some(iced::Color::from_rgba(0.12, 0.12, 0.12, 0.90).into()),
                    border: iced::border::rounded(12).color(COLOR_BORDER).width(1.0),
                    ..button::Style::default()
                },
            })
            .on_press(on_scroll_tabs_left);

            let overlay_left = container(left_btn)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(iced::alignment::Horizontal::Left)
                .align_y(iced::alignment::Vertical::Center);

            layers.push(overlay_left.into());
        }

        if can_scroll_right {
            let right_btn = button(
                container(icon_chevron_right(13.0))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(iced::alignment::Horizontal::Center)
                    .align_y(iced::alignment::Vertical::Center),
            )
            .width(Length::Fixed(24.0))
            .height(Length::Fixed(24.0))
            .padding(0)
            .style(|_t, status| match status {
                button::Status::Hovered | button::Status::Pressed => button::Style {
                    background: Some(iced::Color::from_rgba(0.24, 0.24, 0.24, 0.95).into()),
                    border: iced::border::rounded(12)
                        .color(iced::Color::from_rgb8(0x80, 0x80, 0x80))
                        .width(1.0),
                    ..button::Style::default()
                },
                _ => button::Style {
                    background: Some(iced::Color::from_rgba(0.12, 0.12, 0.12, 0.90).into()),
                    border: iced::border::rounded(12).color(COLOR_BORDER).width(1.0),
                    ..button::Style::default()
                },
            })
            .on_press(on_scroll_tabs_right);

            let overlay_right = container(right_btn)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(iced::alignment::Horizontal::Right)
                .align_y(iced::alignment::Vertical::Center);

            layers.push(overlay_right.into());
        }

        stack(layers).width(Length::Fill).height(Length::Fill).into()
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
    search_bar_width: f32,
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
        .width(Length::Fixed(search_bar_width.max(260.0)));

    container(
        column![
            Space::with_height(Length::Fixed(52.0)),
            row![
                Space::with_width(Length::Fixed(168.0)),
                dropdown,
                Space::with_width(Length::Fill),
            ],
        ],
    )
    .width(Length::Fill)
    .into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsCategory {
    #[default]
    Playback,
    Appearance,
    System,
    Storage,
    Account,
}

impl SettingsCategory {
    pub fn label(&self) -> &'static str {
        match self {
            SettingsCategory::Playback => "Reproducción",
            SettingsCategory::Appearance => "Apariencia",
            SettingsCategory::System => "Sistema",
            SettingsCategory::Storage => "Almacenamiento",
            SettingsCategory::Account => "Cuenta",
        }
    }
}

pub fn render_toggle_switch<'a, Message: Clone + 'static>(
    enabled: bool,
    on_toggle: Message,
) -> Element<'a, Message> {
    let track_bg = if enabled {
        iced::Color::from_rgb8(0xcc, 0x00, 0x00)
    } else {
        iced::Color::from_rgb8(0x38, 0x38, 0x40)
    };
    let thumb = container(Space::with_width(Length::Fixed(0.0)))
        .width(Length::Fixed(16.0))
        .height(Length::Fixed(16.0))
        .style(|_t| container::Style {
            background: Some(iced::Color::WHITE.into()),
            border: iced::border::rounded(8),
            ..container::Style::default()
        });

    let left_spacer: Element<'a, Message> = if enabled {
        Space::with_width(Length::Fixed(18.0)).into()
    } else {
        Space::with_width(Length::Fixed(0.0)).into()
    };
    let right_spacer: Element<'a, Message> = if !enabled {
        Space::with_width(Length::Fixed(18.0)).into()
    } else {
        Space::with_width(Length::Fixed(0.0)).into()
    };

    let switch_pill = container(
        row![
            left_spacer,
            thumb,
            right_spacer,
        ]
        .align_y(Alignment::Center),
    )
    .width(Length::Fixed(42.0))
    .height(Length::Fixed(22.0))
    .padding([3, 4])
    .style(move |_t| container::Style {
        background: Some(track_bg.into()),
        border: iced::border::rounded(11),
        ..container::Style::default()
    });

    button(switch_pill)
        .style(ghost_button)
        .padding(0)
        .on_press(on_toggle)
        .into()
}

pub fn view_auth_modal<'a, Message: Clone + 'static>(
    selected_category: SettingsCategory,
    on_select_category: impl Fn(SettingsCategory) -> Message + Copy + 'static,
    raw_cookie_input: &'a str,
    status_msg: &'a str,
    user_name: Option<&'a str>,
    user_avatar_handle: Option<&'a iced::widget::image::Handle>,
    current_res: PreferredResolution,
    current_codec: PreferredCodec,
    current_audio_lang: PreferredAudioLanguage,
    current_card_style: crate::config::CardStyle,
    current_columns: crate::config::GridColumnsConfig,
    auto_pip_on_unfocus: bool,
    full_buffer_tabs: bool,
    close_to_tray: bool,
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
    on_select_card_style: impl Fn(crate::config::CardStyle) -> Message + Copy + 'static,
    on_select_home_columns: impl Fn(crate::config::GridColumnsConfig) -> Message + Copy + 'static,
    on_toggle_auto_pip: Message,
    on_toggle_full_buffer_tabs: Message,
    on_toggle_close_to_tray: Message,
    on_change_cache_limit: impl Fn(usize) -> Message + Copy + 'static,
    on_clear_cache: Message,
    on_close_modal: Message,
) -> Element<'a, Message> {
    // 1. Profile Header Banner
    let display_name = user_name.unwrap_or("Usuario de YouTube");
    let avatar_widget: Element<'a, Message> = if let Some(handle) = user_avatar_handle {
        container(
            image(handle.clone())
                .width(Length::Fixed(40.0))
                .height(Length::Fixed(40.0))
                .content_fit(iced::ContentFit::Cover),
        )
        .width(Length::Fixed(40.0))
        .height(Length::Fixed(40.0))
        .style(|_t| container::Style {
            border: iced::border::rounded(20).color(COLOR_BORDER_FOCUS).width(2.0),
            ..container::Style::default()
        })
        .into()
    } else if user_name.is_some() {
        view_avatar_badge(display_name, 40.0, 16.0).into()
    } else {
        container(icon_user(20.0))
            .width(Length::Fixed(40.0))
            .height(Length::Fixed(40.0))
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center)
            .style(|_t| container::Style {
                background: Some(iced::Color::from_rgb8(0x28, 0x28, 0x28).into()),
                border: iced::border::rounded(20).color(COLOR_BORDER).width(1.0),
                ..container::Style::default()
            })
            .into()
    };

    let is_authenticated = status_msg.starts_with("[OK]");
    let profile_header = row![
        avatar_widget,
        column![
            text(display_name).size(15).color(COLOR_TEXT_PRIMARY),
            text(if is_authenticated {
                "Sesión activa • Recomendaciones e Historial sincronizados"
            } else {
                "Modo Invitado • Inicia sesión para sincronizar tu cuenta"
            })
            .size(11)
            .color(if is_authenticated {
                COLOR_BORDER_FOCUS
            } else {
                COLOR_TEXT_SECONDARY
            }),
        ]
        .spacing(2),
        Space::with_width(Length::Fill),
        button(icon_close(14.0))
            .style(ghost_button)
            .padding([6, 8])
            .on_press(on_close_modal.clone()),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    // 2. Sidebar Navigation Buttons
    let sidebar_btn = |cat: SettingsCategory, icon: Element<'a, Message>| {
        let is_selected = selected_category == cat;
        button(
            row![
                icon,
                text(cat.label()).size(13),
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .style(settings_category_button(is_selected))
        .padding([9, 14])
        .width(Length::Fill)
        .on_press(on_select_category(cat))
    };

    let sidebar = column![
        sidebar_btn(SettingsCategory::Playback, icon_play(15.0)),
        sidebar_btn(SettingsCategory::Appearance, icon_theater(15.0)),
        sidebar_btn(SettingsCategory::System, icon_pip(15.0)),
        sidebar_btn(SettingsCategory::Storage, icon_trash(15.0)),
        sidebar_btn(SettingsCategory::Account, icon_user(15.0)),
        Space::with_height(Length::Fill),
    ]
    .spacing(4)
    .width(Length::Fixed(180.0));

    // Section Card Helper
    let section_card = |content: Column<'a, Message>| -> Element<'a, Message> {
        container(content)
            .style(|_t| container::Style {
                background: Some(iced::Color::from_rgb8(0x18, 0x18, 0x1b).into()),
                border: iced::border::rounded(12)
                    .color(iced::Color::from_rgba(1.0, 1.0, 1.0, 0.08))
                    .width(1.0),
                ..container::Style::default()
            })
            .padding(14)
            .width(Length::Fill)
            .into()
    };

    // 3. Tab Contents
    let content: Element<'a, Message> = match selected_category {
        SettingsCategory::Playback => {
            let res_options = [
                PreferredResolution::Res8K,
                PreferredResolution::Res4K,
                PreferredResolution::Res1440p,
                PreferredResolution::Res1080p,
                PreferredResolution::Res720p,
            ];
            let mut res_row = row![].spacing(6).align_y(Alignment::Center);
            for r in res_options {
                let is_selected = r == current_res;
                let btn = button(text(r.label()).size(12))
                    .style(segmented_button(is_selected))
                    .padding([5, 10])
                    .on_press(on_select_res(r));
                res_row = res_row.push(btn);
            }

            let codec_options = [
                PreferredCodec::Av1,
                PreferredCodec::Vp9,
                PreferredCodec::H264,
            ];
            let mut codec_row = row![].spacing(6).align_y(Alignment::Center);
            for c in codec_options {
                let is_selected = c == current_codec;
                let btn = button(text(c.label()).size(12))
                    .style(segmented_button(is_selected))
                    .padding([5, 10])
                    .on_press(on_select_codec(c));
                codec_row = codec_row.push(btn);
            }

            let audio_options = [
                PreferredAudioLanguage::Original,
                PreferredAudioLanguage::Spanish,
                PreferredAudioLanguage::English,
            ];
            let mut audio_row = row![].spacing(6).align_y(Alignment::Center);
            for a in audio_options {
                let is_selected = a == current_audio_lang;
                let btn = button(text(a.label()).size(12))
                    .style(segmented_button(is_selected))
                    .padding([5, 10])
                    .on_press(on_select_audio_lang(a));
                audio_row = audio_row.push(btn);
            }

            let fallback_hint = text(
                "Fallback Inteligente: Si el video no dispone de la calidad o códec seleccionado, Mitub desciende en cascada (8K -> 4K -> 1440p -> 1080p -> 720p y AV1 -> VP9 -> H.264) asegurando máxima fidelidad sin interrupciones.",
            )
            .size(11)
            .color(COLOR_TEXT_SECONDARY);

            column![
                text("Preferencias de Reproducción").size(15).color(COLOR_TEXT_PRIMARY),
                section_card(
                    column![
                        column![
                            text("Resolución Máxima").size(12).color(COLOR_TEXT_SECONDARY),
                            res_row,
                        ].spacing(6),
                        column![
                            text("Códec Preferido").size(12).color(COLOR_TEXT_SECONDARY),
                            codec_row,
                        ].spacing(6),
                        column![
                            text("Pista de Audio Predeterminada").size(12).color(COLOR_TEXT_SECONDARY),
                            audio_row,
                        ].spacing(6),
                        fallback_hint,
                    ]
                    .spacing(12),
                ),
            ]
            .spacing(12)
            .into()
        }

        SettingsCategory::Appearance => {
            let style_row = row![
                button(text("Squircle (YouTube)").size(12))
                    .style(segmented_button(current_card_style == crate::config::CardStyle::Squircle))
                    .padding([5, 12])
                    .on_press(on_select_card_style(crate::config::CardStyle::Squircle)),
                button(text("Rectangular").size(12))
                    .style(segmented_button(current_card_style == crate::config::CardStyle::Rectangular))
                    .padding([5, 12])
                    .on_press(on_select_card_style(crate::config::CardStyle::Rectangular)),
            ]
            .spacing(6);

            let cols_row = row![
                button(text("4 (Original)").size(12))
                    .style(segmented_button(current_columns == crate::config::GridColumnsConfig::Cols4))
                    .padding([5, 10])
                    .on_press(on_select_home_columns(crate::config::GridColumnsConfig::Cols4)),
                button(text("Auto").size(12))
                    .style(segmented_button(current_columns == crate::config::GridColumnsConfig::Auto))
                    .padding([5, 10])
                    .on_press(on_select_home_columns(crate::config::GridColumnsConfig::Auto)),
                button(text("2").size(12))
                    .style(segmented_button(current_columns == crate::config::GridColumnsConfig::Cols2))
                    .padding([5, 10])
                    .on_press(on_select_home_columns(crate::config::GridColumnsConfig::Cols2)),
                button(text("5").size(12))
                    .style(segmented_button(current_columns == crate::config::GridColumnsConfig::Cols5))
                    .padding([5, 10])
                    .on_press(on_select_home_columns(crate::config::GridColumnsConfig::Cols5)),
                button(text("6").size(12))
                    .style(segmented_button(current_columns == crate::config::GridColumnsConfig::Cols6))
                    .padding([5, 10])
                    .on_press(on_select_home_columns(crate::config::GridColumnsConfig::Cols6)),
            ]
            .spacing(6);

            column![
                text("Diseño y Cuadrícula").size(15).color(COLOR_TEXT_PRIMARY),
                section_card(
                    column![
                        column![
                            text("Bordes de Miniaturas").size(12).color(COLOR_TEXT_SECONDARY),
                            style_row,
                        ].spacing(6),
                        column![
                            text("Columnas de la Cuadrícula del Home").size(12).color(COLOR_TEXT_SECONDARY),
                            cols_row,
                        ].spacing(6),
                        text("YouTube Desktop usa 4 columnas por defecto con esquinas Squircle (12px). Puedes personalizar a rectangulares o cambiar columnas sin saltos de layout.")
                            .size(11)
                            .color(COLOR_TEXT_SECONDARY),
                    ]
                    .spacing(12),
                ),
            ]
            .spacing(12)
            .into()
        }

        SettingsCategory::System => {
            let row_auto_pip = row![
                column![
                    text("PiP Automático al cambiar de ventana (Alt+Tab)").size(13).color(COLOR_TEXT_PRIMARY),
                    text("Activa la ventana flotante automáticamente al cambiar a otra app y la restaura al volver.")
                        .size(11)
                        .color(COLOR_TEXT_SECONDARY),
                ]
                .spacing(2)
                .width(Length::Fill),
                render_toggle_switch(auto_pip_on_unfocus, on_toggle_auto_pip),
            ]
            .spacing(12)
            .align_y(Alignment::Center);

            let row_buffer_tabs = row![
                column![
                    text("Buffer y Precarga Completa en Pestañas").size(13).color(COLOR_TEXT_PRIMARY),
                    text("Descarga el video completo en segundo plano para saltar de pestaña y buscar con 0ms de espera.")
                        .size(11)
                        .color(COLOR_TEXT_SECONDARY),
                ]
                .spacing(2)
                .width(Length::Fill),
                render_toggle_switch(full_buffer_tabs, on_toggle_full_buffer_tabs),
            ]
            .spacing(12)
            .align_y(Alignment::Center);

            let row_tray = row![
                column![
                    text("Cerrar a la Bandeja del Sistema (Segundo plano)").size(13).color(COLOR_TEXT_PRIMARY),
                    text("Al pulsar la X de la ventana, mantiene la app y el audio sonando en la bandeja del sistema.")
                        .size(11)
                        .color(COLOR_TEXT_SECONDARY),
                ]
                .spacing(2)
                .width(Length::Fill),
                render_toggle_switch(close_to_tray, on_toggle_close_to_tray),
            ]
            .spacing(12)
            .align_y(Alignment::Center);

            column![
                text("Multitarea y Sistema").size(15).color(COLOR_TEXT_PRIMARY),
                section_card(
                    column![
                        row_auto_pip,
                        container(Space::with_height(Length::Fixed(1.0)))
                            .width(Length::Fill)
                            .style(|_t| container::Style {
                                background: Some(iced::Color::from_rgba(1.0, 1.0, 1.0, 0.05).into()),
                                ..container::Style::default()
                            }),
                        row_buffer_tabs,
                        container(Space::with_height(Length::Fixed(1.0)))
                            .width(Length::Fill)
                            .style(|_t| container::Style {
                                background: Some(iced::Color::from_rgba(1.0, 1.0, 1.0, 0.05).into()),
                                ..container::Style::default()
                            }),
                        row_tray,
                    ]
                    .spacing(10),
                ),
            ]
            .spacing(12)
            .into()
        }

        SettingsCategory::Storage => {
            let current_idx = limit_to_index(max_cache_size_bytes);
            let current_limit_label = CACHE_LIMIT_LABELS.get(current_idx).copied().unwrap_or("5 GB (Estándar)");

            let cache_slider = slider(
                0..=5_u8,
                current_idx as u8,
                move |val| on_change_cache_limit(val as usize),
            )
            .step(1_u8)
            .style(youtube_seek_slider)
            .width(Length::Fixed(240.0));

            let clear_cache_btn = button(
                row![
                    icon_trash(13.0),
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
                .width(Length::Fixed(240.0)),
            ]
            .spacing(4);

            let cache_controls_row = row![
                cache_slider_col,
                Space::with_width(Length::Fixed(16.0)),
                clear_cache_btn,
            ]
            .align_y(Alignment::Center);

            column![
                text("Almacenamiento y Caché").size(15).color(COLOR_TEXT_PRIMARY),
                section_card(
                    column![
                        text("Límite de Caché de Video").size(12).color(COLOR_TEXT_SECONDARY),
                        cache_controls_row,
                        text("Almacenamiento persistente en disco (~/.cache/mitub/video_cache). Al alcanzar el tope configurado, Mitub purga automáticamente los videos más antiguos (LRU).")
                            .size(11)
                            .color(COLOR_TEXT_SECONDARY),
                    ]
                    .spacing(10),
                ),
            ]
            .spacing(12)
            .into()
        }

        SettingsCategory::Account => {
            let subs_nav_btn = button(
                row![
                    icon_subscriptions(15.0),
                    text("Suscripciones").size(12).color(COLOR_TEXT_PRIMARY),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .style(secondary_button)
            .padding([7, 12])
            .on_press(on_open_subscriptions);

            let history_nav_btn = button(
                row![
                    icon_history(15.0),
                    text("Historial").size(12).color(COLOR_TEXT_PRIMARY),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .style(secondary_button)
            .padding([7, 12])
            .on_press(on_open_history);

            let playlists_nav_btn = button(
                row![
                    icon_playlist(15.0),
                    text("Playlists").size(12).color(COLOR_TEXT_PRIMARY),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .style(secondary_button)
            .padding([7, 12])
            .on_press(on_open_playlists);

            let nav_card = section_card(
                column![
                    text("Tu Biblioteca de YouTube").size(13).color(COLOR_TEXT_PRIMARY),
                    row![subs_nav_btn, history_nav_btn, playlists_nav_btn]
                        .spacing(8)
                        .align_y(Alignment::Center),
                ]
                .spacing(8),
            );

            let auth_action_content: Element<'a, Message> = if is_authenticated {
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
                        "Inicia sesión para sincronizar tus suscripciones, historial y recomendaciones.\nTus credenciales se conservan localmente y nunca salen de tu equipo.",
                    )
                    .size(11)
                    .color(COLOR_TEXT_SECONDARY),
                    row![firefox_btn, chrome_btn].spacing(8).align_y(Alignment::Center),
                    input,
                    row![save_btn].align_y(Alignment::Center),
                ]
                .spacing(8)
                .into()
            };

            let auth_card = section_card(
                column![
                    text("Gestión de Cuenta").size(13).color(COLOR_TEXT_PRIMARY),
                    auth_action_content,
                ]
                .spacing(8),
            );

            column![
                text("Cuenta y Biblioteca").size(15).color(COLOR_TEXT_PRIMARY),
                nav_card,
                auth_card,
                text(status_msg).size(11).color(COLOR_TEXT_SECONDARY),
            ]
            .spacing(12)
            .into()
        }
    };

    let content_scroll = scrollable(
        container(content)
            .padding(iced::Padding { top: 0.0, right: 8.0, bottom: 0.0, left: 0.0 })
            .width(Length::Fill)
    )
    .height(Length::Fill);

    let modal_body = row![
        sidebar,
        container(Space::with_width(Length::Fixed(1.0)))
            .height(Length::Fill)
            .style(|_t| container::Style {
                background: Some(iced::Color::from_rgba(1.0, 1.0, 1.0, 0.08).into()),
                ..container::Style::default()
            }),
        container(content_scroll)
            .width(Length::Fill)
            .height(Length::Fill),
    ]
    .spacing(14)
    .height(Length::Fill);

    let modal_content = column![
        profile_header,
        container(Space::with_height(Length::Fixed(1.0)))
            .width(Length::Fill)
            .style(|_t| container::Style {
                background: Some(iced::Color::from_rgba(1.0, 1.0, 1.0, 0.08).into()),
                ..container::Style::default()
            }),
        modal_body,
    ]
    .spacing(12)
    .height(Length::Fill);

    let modal_box = container(modal_content)
        .style(card_container)
        .padding(20)
        .width(Length::Fixed(760.0))
        .height(Length::Fixed(520.0));

    let clickable_modal = mouse_area(modal_box);

    let backdrop = mouse_area(
        container(clickable_modal)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center)
            .style(|_t| container::Style {
                background: Some(iced::Color::from_rgba(0.0, 0.0, 0.0, 0.7).into()),
                ..container::Style::default()
            })
    )
    .on_press(on_close_modal);

    backdrop.into()
}

