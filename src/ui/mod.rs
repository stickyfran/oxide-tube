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
use iced::{Alignment, Color, Element, Length, Point, Rectangle, Renderer, Theme};

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

fn nav_tab_button<'a, Message: Clone + 'static>(
    icon: Element<'a, Message>,
    is_active: bool,
    on_press: Message,
) -> iced::widget::Button<'a, Message> {
    button(icon)
        .style(move |_t, status| {
            if is_active {
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
        .on_press(on_press)
}

pub fn view_header_bar<'a, Message: Clone + 'static>(
    search_query: &'a str,
    search_bar_width: f32,
    is_search_focused: bool,
    search_focus_glow: f32,
    is_incognito: bool,
    _suggestions: &'a [String],
    _show_suggestions: bool,
    _power_profile: PowerProfile,
    is_authenticated: bool,
    user_name: Option<&'a str>,
    user_avatar_handle: Option<&'a iced::widget::image::Handle>,
    profile_expand: f32,
    is_subscriptions: bool,
    is_history: bool,
    is_downloads: bool,
    is_home: bool,
    tabs: &'a [VideoTab],
    active_tab_idx: Option<usize>,
    hovered_tab_idx: Option<usize>,
    dragging_tab_idx: Option<usize>,
    playing_video_id: Option<&'a str>,
    is_video_playing: bool,
    newly_opened_tab: Option<(usize, f32)>,
    shaking_tab: Option<(usize, f32)>,
    window_width: f32,
    tabs_scroll_x: f32,
    tabs_max_scroll_x: f32,
    custom_titlebar: bool,
    is_maximized: bool,
    on_search_input: impl Fn(String) -> Message + 'static,
    on_search_submit: Message,
    on_search_hover: impl Fn(bool) -> Message + Copy + 'static,
    _on_select_suggestion: impl Fn(String) -> Message + 'static,
    on_go_home: Message,
    on_go_back: Message,
    on_open_subscriptions: Message,
    on_open_history: Message,
    on_open_downloads: Message,
    on_toggle_login_modal: Message,
    on_profile_hover: impl Fn(bool) -> Message + Copy + 'static,
    on_reload: Message,
    on_select_tab: impl Fn(usize) -> Message + Copy + 'static,
    on_tab_press: impl Fn(usize) -> Message + Copy + 'static,
    on_close_tab: impl Fn(usize) -> Message + Copy + 'static,
    on_hover_tab: impl Fn(usize, bool) -> Message + Copy + 'static,
    on_scroll_tabs: impl Fn(iced::widget::scrollable::Viewport) -> Message + Copy + 'static,
    on_scroll_tabs_left: Message,
    on_scroll_tabs_right: Message,
    on_header_drag: Message,
    on_header_menu: Message,
    on_minimize_window: Message,
    on_maximize_window: Message,
    on_close_window: Message,
) -> Element<'a, Message> {
    let fitts_top = custom_titlebar && is_maximized;
    let tab_height: f32 = if fitts_top { 44.0 } else { 38.0 };

    // 1. Left section: Back, Home button (with uniform active styling)
    let back_btn = button(icon_back(18.0))
        .style(ghost_button)
        .padding([7, 10])
        .on_press(on_go_back);

    let home_btn = nav_tab_button(icon_home(16.0), is_home, on_go_home);

    let left_section = container(
        row![back_btn, home_btn]
            .spacing(6)
            .align_y(Alignment::Center),
    )
    .height(Length::Fill)
    .align_y(iced::alignment::Vertical::Center);

    // 2. Suscripciones (icon-only) + Historial + Descargas + Search Bar
    let subs_btn = nav_tab_button(icon_subscriptions(16.0), is_subscriptions, on_open_subscriptions);
    let history_btn = nav_tab_button(icon_history(16.0), is_history, on_open_history);
    let downloads_btn = nav_tab_button(icon_buffer(16.0), is_downloads, on_open_downloads);

    let search_placeholder = if is_search_focused || search_bar_width > 240.0 || !search_query.is_empty() {
        "Buscar o pegar enlace..."
    } else {
        "Buscar..."
    };

    let search_input_widget = text_input(search_placeholder, search_query)
        .id(iced::widget::text_input::Id::new("main_search_input"))
        .on_input(on_search_input)
        .on_submit(on_search_submit.clone())
        .style(animated_search_input(search_focus_glow))
        .padding([7, 12])
        .width(Length::Fixed(search_bar_width));

    let search_btn = button(icon_search(14.0))
        .style(secondary_button)
        .padding([7, 10])
        .on_press(on_search_submit);

    let search_bar_compound = mouse_area(
        row![search_input_widget, search_btn]
            .spacing(4)
            .align_y(Alignment::Center)
    )
    .on_enter(on_search_hover(true))
    .on_exit(on_search_hover(false));

    let search_container = container(
        row![subs_btn, history_btn, downloads_btn, search_bar_compound]
            .spacing(4)
            .align_y(Alignment::Center),
    )
    .height(Length::Fill)
    .align_y(iced::alignment::Vertical::Center);

    // 3. Center/Horizontal Tabs section: Pestañas ergonómicas estilo Firefox (38-44px alto, clic completo)
    let glow_phase = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        % 2200) as f32
        / 2200.0;

    let mut tabs_row = row![]
        .spacing(6)
        .align_y(if fitts_top { Alignment::Start } else { Alignment::Center });
    let mut sum_tabs_width: f32 = 0.0;

    for (idx, tab) in tabs.iter().enumerate() {
        let is_active = active_tab_idx == Some(idx);
        let is_hovered = hovered_tab_idx == Some(idx);
        let is_dragging_this = dragging_tab_idx == Some(idx);
        let is_playing_this_tab =
            is_video_playing && !tab.video_id.is_empty() && playing_video_id == Some(tab.video_id.as_str());
        let open_elapsed = newly_opened_tab.and_then(|(n_idx, el)| if n_idx == idx { Some(el) } else { None });
        let is_newly_opened = open_elapsed.is_some();
        let shake_elapsed = shaking_tab.and_then(|(s_idx, el)| if s_idx == idx { Some(el) } else { None });

        let tab_title = if tab.title.chars().count() > 30 {
            let mut t: String = tab.title.chars().take(28).collect();
            t.push_str("..");
            t
        } else if tab.title.is_empty() {
            "Nueva pestaña".to_string()
        } else {
            tab.title.clone()
        };

        let char_count = tab_title.chars().count() as f32;
        let est_this_tab_w = (char_count * 7.2 + if is_playing_this_tab { 62.0 } else { 46.0 }).clamp(132.0, 248.0);
        sum_tabs_width += est_this_tab_w + if idx > 0 { 6.0 } else { 0.0 };

        let text_color = if is_active || is_hovered || is_playing_this_tab || is_dragging_this {
            iced::Color::WHITE
        } else {
            COLOR_TEXT_PRIMARY
        };

        let label_content: Element<'a, Message> = if is_playing_this_tab {
            row![
                icon_play(11.5),
                text(tab_title)
                    .size(12.5)
                    .wrapping(text::Wrapping::None)
                    .color(text_color),
            ]
            .spacing(6)
            .align_y(Alignment::Center)
            .into()
        } else {
            text(tab_title)
                .size(12.5)
                .wrapping(text::Wrapping::None)
                .color(text_color)
                .into()
        };

        let tab_body = container(label_content)
            .width(Length::Fill)
            .height(Length::Fill)
            .clip(true)
            .align_y(iced::alignment::Vertical::Center)
            .padding(iced::Padding {
                top: if fitts_top { 4.0 } else { 0.0 },
                bottom: 0.0,
                left: 12.0,
                right: 4.0,
            });

        let close_btn = container(
            button(
                container(icon_close(11.0))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(iced::alignment::Horizontal::Center)
                    .align_y(iced::alignment::Vertical::Center),
            )
            .width(Length::Fixed(24.0))
            .height(Length::Fixed(24.0))
            .padding(0)
            .style(tab_close_button)
            .on_press(on_close_tab(idx)),
        )
        .height(Length::Fill)
        .padding(iced::Padding {
            top: if fitts_top { 4.0 } else { 0.0 },
            bottom: 0.0,
            left: 2.0,
            right: 6.0,
        })
        .align_y(iced::alignment::Vertical::Center);

        let (shake_offset, border_color, border_width) = if is_dragging_this {
            (0.0, iced::Color::from_rgb8(0x3e, 0xa6, 0xff), 2.0)
        } else if let Some(el) = shake_elapsed {
            let decay = (1.0 - el / 0.6).clamp(0.0, 1.0);
            let offset = (el * 38.0).sin() * 5.0 * decay;
            let c = iced::Color::from_rgba(0.9, 0.3, 0.3, 0.2 + 0.8 * decay);
            (offset, c, 1.5)
        } else if is_playing_this_tab {
            (0.0, iced::Color::TRANSPARENT, 0.0)
        } else if is_active {
            (0.0, iced::Color::from_rgb8(0xff, 0x3b, 0x3b), 1.5)
        } else if is_newly_opened {
            (0.0, iced::Color::from_rgb8(0xff, 0x55, 0x55), 1.5)
        } else if is_hovered {
            (0.0, iced::Color::from_rgb8(0x52, 0x52, 0x5a), 1.0)
        } else {
            (0.0, color_border(), 1.0)
        };

        let pad_l = shake_offset.max(0.0);
        let pad_r = (-shake_offset).max(0.0);
        let tab_radius = if fitts_top {
            iced::border::Radius {
                top_left: 0.0,
                top_right: 0.0,
                bottom_right: 8.0,
                bottom_left: 8.0,
            }
        } else {
            iced::border::Radius::from(8.0)
        };

        let inner_pill = container(
            row![
                tab_body,
                close_btn,
            ]
            .width(Length::Fill)
            .height(Length::Fill)
            .align_y(Alignment::Center)
            .spacing(0),
        )
        .width(Length::Fixed(est_this_tab_w))
        .height(Length::Fixed(tab_height))
        .style(move |_t| container::Style {
            background: Some(if is_dragging_this {
                iced::Color::from_rgb8(0x22, 0x27, 0x38).into()
            } else if is_active {
                if is_hovered {
                    iced::Color::from_rgb8(0xd9, 0x11, 0x11).into()
                } else {
                    iced::Color::from_rgb8(0xcc, 0x00, 0x00).into()
                }
            } else if is_playing_this_tab {
                if is_hovered {
                    iced::Color::from_rgb8(0x3a, 0x16, 0x16).into()
                } else {
                    iced::Color::from_rgb8(0x2b, 0x10, 0x10).into()
                }
            } else if is_newly_opened {
                iced::Color::from_rgb8(0x44, 0x11, 0x11).into()
            } else if is_hovered {
                iced::Color::from_rgb8(0x28, 0x28, 0x2e).into()
            } else {
                iced::Color::from_rgb8(0x1a, 0x1a, 0x1e).into()
            }),
            border: iced::Border {
                color: border_color,
                width: border_width,
                radius: tab_radius,
            },
            ..container::Style::default()
        })
        .padding(iced::Padding {
            top: 0.0,
            bottom: 0.0,
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
                .width(Length::Fixed(est_this_tab_w))
                .height(Length::Fixed(tab_height))
        } else {
            inner_pill
        };

        // Firefox-style slide-in expansion from left to right when a tab is newly opened (220ms cubic ease-out)
        if let Some(el) = open_elapsed {
            if el < 0.22 {
                let t = (el / 0.22).clamp(0.0, 1.0);
                let ease = 1.0 - (1.0 - t).powi(3);
                let target_w = est_this_tab_w;
                tab_container = tab_container
                    .width(Length::Fixed((target_w * ease).max(2.0)))
                    .clip(true);
            }
        }

        let tab_pill = mouse_area(tab_container)
            .interaction(if is_dragging_this {
                iced::mouse::Interaction::Grabbing
            } else {
                iced::mouse::Interaction::Pointer
            })
            .on_enter(on_hover_tab(idx, true))
            .on_exit(on_hover_tab(idx, false))
            .on_press(on_tab_press(idx))
            .on_release(on_select_tab(idx))
            .on_middle_press(on_close_tab(idx));

        tabs_row = tabs_row.push(tab_pill);
    }

    // Chrome/Firefox overflow detection accounting for CSD window buttons
    let right_account_width = if is_authenticated && user_name.is_some() { 160.0 } else { 125.0 };
    let csd_controls_width = if custom_titlebar { 136.0 + 32.0 } else { 0.0 };
    let non_tabs_width = 240.0 + search_bar_width + right_account_width + csd_controls_width;
    let available_tabs_width = (window_width - non_tabs_width).max(80.0);
    let heuristic_max_scroll = (sum_tabs_width - available_tabs_width).max(0.0);
    let effective_max_scroll = tabs_max_scroll_x.max(heuristic_max_scroll);
    let clamped_scroll_x = tabs_scroll_x.clamp(0.0, effective_max_scroll);

    // Only show scroll controls when tabs actually exceed the viewport
    let has_overflow = effective_max_scroll > 8.0;
    let can_scroll_left = has_overflow && clamped_scroll_x > 4.0;
    let can_scroll_right = has_overflow && (clamped_scroll_x + 6.0) < effective_max_scroll;

    let tabs_scrollable = iced::widget::scrollable(tabs_row)
        .width(Length::Fill)
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

    let tabs_section: Element<'a, Message> = {
        let tabs_align_y = if fitts_top {
            iced::alignment::Vertical::Top
        } else {
            iced::alignment::Vertical::Center
        };

        let free_drag_space: Element<'a, Message> = mouse_area(
            Space::new(Length::Fill, Length::Fill),
        )
        .on_press(on_header_drag.clone())
        .on_right_press(on_header_menu.clone())
        .into();

        let base_row: Element<'a, Message> = if has_overflow {
            container(tabs_scrollable)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_y(tabs_align_y)
                .into()
        } else {
            row![
                container(tabs_scrollable)
                    .width(Length::Fixed(sum_tabs_width))
                    .height(Length::Fill)
                    .align_y(tabs_align_y),
                free_drag_space,
            ]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        };

        let mut layers: Vec<Element<'a, Message>> = vec![base_row];

        if can_scroll_left {
            let left_btn = button(
                container(icon_chevron_left(13.0))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(iced::alignment::Horizontal::Center)
                    .align_y(iced::alignment::Vertical::Center),
            )
            .width(Length::Fixed(26.0))
            .height(Length::Fixed(26.0))
            .padding(0)
            .style(|_t, status| match status {
                button::Status::Hovered | button::Status::Pressed => button::Style {
                    background: Some(iced::Color::from_rgba(0.24, 0.24, 0.24, 0.95).into()),
                    border: iced::border::rounded(13)
                        .color(iced::Color::from_rgb8(0x80, 0x80, 0x80))
                        .width(1.0),
                    ..button::Style::default()
                },
                _ => button::Style {
                    background: Some(iced::Color::from_rgba(0.12, 0.12, 0.12, 0.90).into()),
                    border: iced::border::rounded(13).color(COLOR_BORDER).width(1.0),
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
            .width(Length::Fixed(26.0))
            .height(Length::Fixed(26.0))
            .padding(0)
            .style(|_t, status| match status {
                button::Status::Hovered | button::Status::Pressed => button::Style {
                    background: Some(iced::Color::from_rgba(0.24, 0.24, 0.24, 0.95).into()),
                    border: iced::border::rounded(13)
                        .color(iced::Color::from_rgb8(0x80, 0x80, 0x80))
                        .width(1.0),
                    ..button::Style::default()
                },
                _ => button::Style {
                    background: Some(iced::Color::from_rgba(0.12, 0.12, 0.12, 0.90).into()),
                    border: iced::border::rounded(13).color(COLOR_BORDER).width(1.0),
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

    // 4. Right section: Account, Reload & CSD Window Controls (Minimize, Maximize/Restore, Close)
    let auth_btn: Element<'a, Message> = if is_authenticated {
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

        // Animate the pill from avatar-only to avatar + full name.
        let expand = profile_expand.clamp(0.0, 1.0);
        let name_width = user_name
            .map(|name| (name.chars().count() as f32 * 8.0).clamp(32.0, 200.0))
            .unwrap_or(0.0);
        let collapsed_width = 28.0;
        let expanded_width = 28.0 + 8.0 + name_width;
        let pill_width = collapsed_width + (expanded_width - collapsed_width) * expand;
        let name_alpha = expand.powf(1.35);

        let mut profile_row = row![avatar_el].spacing(8).align_y(Alignment::Center);
        if let Some(name) = user_name {
            profile_row = profile_row.push(
                text(name)
                    .size(13)
                    .color(Color {
                        a: name_alpha,
                        ..COLOR_TEXT_PRIMARY
                    }),
            );
        }

        let profile_content = container(profile_row)
            .width(Length::Fixed((pill_width - 4.0).max(24.0)))
            .height(Length::Fixed(24.0))
            .clip(true)
            .align_x(iced::alignment::Horizontal::Left)
            .align_y(iced::alignment::Vertical::Center);

        let profile_btn = button(profile_content)
            .style(secondary_button)
            .padding([2, 2])
            .width(Length::Fixed(pill_width))
            .on_press(on_toggle_login_modal.clone());

        mouse_area(profile_btn)
            .on_enter(on_profile_hover(true))
            .on_exit(on_profile_hover(false))
            .into()
    } else {
        button(
            row![icon_user(14.0), text("Iniciar Sesión").size(12)]
                .spacing(6)
                .align_y(Alignment::Center),
        )
        .style(primary_button)
        .padding([6, 12])
        .on_press(on_toggle_login_modal.clone())
        .into()
    };

    let incognito_badge: Option<Element<'a, Message>> = if is_incognito {
        Some(
            button(
                row![
                    icon_incognito(14.0),
                    text("Incógnito").size(11).color(Color::from_rgb(0.85, 0.85, 0.85)),
                ]
                .spacing(5)
                .align_y(Alignment::Center),
            )
            .style(|_t, _s| button::Style {
                background: Some(Color::from_rgba(0.25, 0.25, 0.30, 0.85).into()),
                border: iced::border::rounded(14).color(Color::from_rgb(0.45, 0.50, 0.60)).width(1.0),
                ..button::Style::default()
            })
            .padding([4, 8])
            .on_press(on_toggle_login_modal)
            .into(),
        )
    } else {
        None
    };

    let reload_btn = button(icon_reload(16.0))
        .style(ghost_button)
        .padding([6, 8])
        .on_press(on_reload);

    let mut right_row = row![].spacing(8).align_y(Alignment::Center);
    if let Some(badge) = incognito_badge {
        right_row = right_row.push(badge);
    }
    right_row = right_row.push(auth_btn).push(reload_btn);

    let right_tools = container(right_row)
        .height(Length::Fill)
        .align_y(iced::alignment::Vertical::Center);

    let mut main_header_row = row![
        Space::with_width(Length::Fixed(12.0)),
        left_section,
        Space::with_width(Length::Fixed(6.0)),
        search_container,
        Space::with_width(Length::Fixed(8.0)),
        container(tabs_section).width(Length::Fill).height(Length::Fill),
        Space::with_width(Length::Fixed(8.0)),
        right_tools,
    ]
    .height(Length::Fill)
    .align_y(Alignment::Center);

    if custom_titlebar {
        let win_btn_h = if fitts_top { 44.0 } else { 36.0 };
        let win_btn_w = if fitts_top { 44.0 } else { 40.0 };

        let min_btn = button(
            container(icon_win_minimize(13.0))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(iced::alignment::Horizontal::Center)
                .align_y(iced::alignment::Vertical::Center),
        )
        .width(Length::Fixed(win_btn_w))
        .height(Length::Fixed(win_btn_h))
        .padding(0)
        .style(window_control_button(fitts_top))
        .on_press(on_minimize_window);

        let max_icon = if is_maximized {
            icon_win_restore(13.0)
        } else {
            icon_win_maximize(13.0)
        };

        let max_btn = button(
            container(max_icon)
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(iced::alignment::Horizontal::Center)
                .align_y(iced::alignment::Vertical::Center),
        )
        .width(Length::Fixed(win_btn_w))
        .height(Length::Fixed(win_btn_h))
        .padding(0)
        .style(window_control_button(fitts_top))
        .on_press(on_maximize_window);

        let close_btn = button(
            container(icon_win_close(13.0))
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(iced::alignment::Horizontal::Center)
                .align_y(iced::alignment::Vertical::Center),
        )
        .width(Length::Fixed(if fitts_top { 46.0 } else { 40.0 }))
        .height(Length::Fixed(win_btn_h))
        .padding(0)
        .style(window_close_button(fitts_top))
        .on_press(on_close_window);

        let win_controls = container(
            row![min_btn, max_btn, close_btn]
                .spacing(if fitts_top { 0 } else { 2 })
                .align_y(Alignment::Center),
        )
        .height(Length::Fill)
        .padding(iced::Padding {
            top: 0.0,
            bottom: 0.0,
            left: 8.0,
            right: if fitts_top { 0.0 } else { 8.0 },
        })
        .align_y(if fitts_top {
            iced::alignment::Vertical::Top
        } else {
            iced::alignment::Vertical::Center
        });

        main_header_row = main_header_row.push(win_controls);
    } else {
        main_header_row = main_header_row.push(Space::with_width(Length::Fixed(14.0)));
    }

    let header_box = container(main_header_row)
        .style(|_t| container::Style {
            background: Some(color_bg().into()),
            border: iced::border::color(color_border()).width(1.0),
            ..container::Style::default()
        })
        .height(Length::Fixed(56.0))
        .width(Length::Fill);

    if custom_titlebar {
        mouse_area(header_box)
            .on_press(on_header_drag)
            .on_right_press(on_header_menu)
            .into()
    } else {
        header_box.into()
    }
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
    current_theme: crate::config::AppTheme,
    custom_titlebar: bool,
    incognito_mode: bool,
    auto_pip_on_unfocus: bool,
    full_buffer_tabs: bool,
    close_to_tray: bool,
    max_cache_size_bytes: u64,
    current_cache_size_bytes: u64,
    on_cookie_input: impl Fn(String) -> Message + 'static,
    on_save_raw_cookie: Message,
    on_open_browser_login: Message,
    on_sync_browser_session: Message,
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
    on_select_theme: impl Fn(crate::config::AppTheme) -> Message + Copy + 'static,
    on_toggle_custom_titlebar: Message,
    on_toggle_incognito: Message,
    on_toggle_auto_pip: Message,
    on_toggle_full_buffer_tabs: Message,
    on_toggle_close_to_tray: Message,
    on_change_cache_limit: impl Fn(usize) -> Message + Copy + 'static,
    on_clear_cache: Message,
    on_modal_click_trap: Message,
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
            let theme_row = row![
                button(text("Oscuro (YouTube)").size(12))
                    .style(segmented_button(current_theme == crate::config::AppTheme::Dark))
                    .padding([5, 12])
                    .on_press(on_select_theme(crate::config::AppTheme::Dark)),
                button(text("Dark AMOLED (Negro Puro)").size(12))
                    .style(segmented_button(current_theme == crate::config::AppTheme::Amoled))
                    .padding([5, 12])
                    .on_press(on_select_theme(crate::config::AppTheme::Amoled)),
            ]
            .spacing(6);

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

            let row_custom_titlebar = row![
                column![
                    text("Integrar Pestañas en Barra de Título (Estilo Firefox / CSD)").size(13).color(COLOR_TEXT_PRIMARY),
                    text("Unifica las pestañas y los controles de ventana en el header sin barra nativa del sistema.")
                        .size(11)
                        .color(COLOR_TEXT_SECONDARY),
                ]
                .spacing(2)
                .width(Length::Fill),
                render_toggle_switch(custom_titlebar, on_toggle_custom_titlebar),
            ]
            .spacing(12)
            .align_y(Alignment::Center);

            column![
                text("Apariencia y Diseño").size(15).color(COLOR_TEXT_PRIMARY),
                section_card(
                    column![
                        row_custom_titlebar,
                        column![
                            text("Tema de la Interfaz").size(12).color(COLOR_TEXT_SECONDARY),
                            theme_row,
                        ].spacing(6),
                        column![
                            text("Bordes de Miniaturas").size(12).color(COLOR_TEXT_SECONDARY),
                            style_row,
                        ].spacing(6),
                        column![
                            text("Columnas de la Cuadrícula del Home").size(12).color(COLOR_TEXT_SECONDARY),
                            cols_row,
                        ].spacing(6),
                        text("El modo AMOLED apaga totalmente los píxeles negros (#000000) para máxima eficiencia y contraste.")
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
            let row_incognito = row![
                column![
                    text("Modo Incógnito (No dejar rastro)").size(13).color(COLOR_TEXT_PRIMARY),
                    text("No marca videos como vistos ni reporta tiempo a YouTube, y desactiva el historial local.")
                        .size(11)
                        .color(COLOR_TEXT_SECONDARY),
                ]
                .spacing(2)
                .width(Length::Fill),
                render_toggle_switch(incognito_mode, on_toggle_incognito),
            ]
            .spacing(12)
            .align_y(Alignment::Center);

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
                        row_incognito,
                        container(Space::with_height(Length::Fixed(1.0)))
                            .width(Length::Fill)
                            .style(|_t| container::Style {
                                background: Some(iced::Color::from_rgba(1.0, 1.0, 1.0, 0.05).into()),
                                ..container::Style::default()
                            }),
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
            let current_limit_label = CACHE_LIMIT_LABELS.get(current_idx).copied().unwrap_or("10 GB (Estándar)");

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
                    text(format!("Videos en disco: {} / {}", format_size(current_cache_size_bytes), if max_cache_size_bytes == u64::MAX { "sin límite".to_string() } else { format_size(max_cache_size_bytes) })).size(12).color(COLOR_TEXT_SECONDARY),
                ]
                .align_y(Alignment::Center),
                cache_slider,
                row![
                    text("1 GB").size(10).color(COLOR_TEXT_SECONDARY),
                    Space::with_width(Length::Fill),
                    text("2 GB").size(10).color(COLOR_TEXT_SECONDARY),
                    Space::with_width(Length::Fill),
                    text("5 GB").size(10).color(COLOR_TEXT_SECONDARY),
                    Space::with_width(Length::Fill),
                    text("10 GB").size(10).color(COLOR_ACCENT_RED),
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

            let chrome_failed = status_msg.starts_with("Error al importar Chrome")
                || status_msg.contains("No se detectó ninguna sesión")
                || status_msg.contains("App-Bound");

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

                let sync_btn = button(
                    row![
                        icon_user(13.0),
                        text("Re-sincronizar").size(12).color(COLOR_TEXT_PRIMARY),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .style(ghost_button)
                .padding([6, 12])
                .on_press(on_sync_browser_session.clone());

                row![
                    text("Sesión sincronizada activamente.")
                        .size(12)
                        .color(COLOR_TEXT_SECONDARY),
                    Space::with_width(Length::Fill),
                    sync_btn,
                    logout_btn,
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .into()
            } else {
                let browser_login_btn = button(
                    row![
                        icon_user(14.0),
                        text("1. Abrir inicio de sesión en Navegador").size(12).color(iced::Color::WHITE),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .style(primary_button)
                .padding([8, 14])
                .on_press(on_open_browser_login);

                let sync_browser_btn = button(
                    row![
                        icon_play(12.0),
                        text("2. Sincronizar sesión activa").size(12).color(COLOR_TEXT_PRIMARY),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .style(secondary_button)
                .padding([8, 14])
                .on_press(on_sync_browser_session);

                let firefox_btn = button(
                    column![
                        text("Firefox").size(12).color(COLOR_TEXT_PRIMARY),
                        text("Importación automática").size(10).color(COLOR_TEXT_SECONDARY),
                    ]
                    .spacing(2),
                )
                .style(secondary_button)
                .padding([8, 12])
                .width(Length::Fill)
                .on_press(on_import_firefox);

                let chrome_btn = button(
                    column![
                        text("Chrome / Chromium").size(12).color(COLOR_TEXT_PRIMARY),
                        text("Importación automática").size(10).color(COLOR_TEXT_SECONDARY),
                    ]
                    .spacing(2),
                )
                .style(secondary_button)
                .padding([8, 12])
                .width(Length::Fill)
                .on_press(on_import_chrome);

                let chrome_help_card: Element<'a, Message> = container(
                    column![
                        row![
                            icon_settings(13.0),
                            text("¿Chrome no importó tu sesión?")
                                .size(12)
                                .color(COLOR_TEXT_PRIMARY),
                        ]
                        .spacing(6)
                        .align_y(Alignment::Center),
                        text("Sigue estos pasos, en orden:")
                            .size(11)
                            .color(COLOR_TEXT_SECONDARY),
                        text("1. Abre Chrome y confirma en youtube.com que ya estás con tu cuenta.")
                            .size(11)
                            .color(COLOR_TEXT_SECONDARY),
                        text("2. Cierra Chrome por completo (en Windows: icono de la bandeja → Salir).")
                            .size(11)
                            .color(COLOR_TEXT_SECONDARY),
                        text("3. Vuelve aquí y pulsa otra vez “Importar desde Chrome”.")
                            .size(11)
                            .color(COLOR_TEXT_SECONDARY),
                        text("4. Si sigue fallando en Windows, Chrome 127+ usa cifrado App-Bound (v20). Usa Firefox: es la opción más fiable.")
                            .size(11)
                            .color(COLOR_TEXT_SECONDARY),
                        text("5. Plan B: instala la extensión “Get cookies.txt LOCALLY”, exporta youtube.com y pega aquí la línea de SAPISID o el header Cookie completo.")
                            .size(11)
                            .color(COLOR_TEXT_SECONDARY),
                        text("En Linux, además, necesitas yt-dlp instalado y el llavero de GNOME/KWallet desbloqueado.")
                            .size(10)
                            .color(COLOR_TEXT_SECONDARY),
                    ]
                    .spacing(6),
                )
                .style(|_t| container::Style {
                    background: Some(iced::Color::from_rgba(0.16, 0.13, 0.07, 0.95).into()),
                    border: iced::border::rounded(10)
                        .color(iced::Color::from_rgb8(0x8a, 0x6d, 0x2f))
                        .width(1.0),
                    ..container::Style::default()
                })
                .padding(12)
                .width(Length::Fill)
                .into();

                let input = text_input("O pega aquí tu Cookie: SAPISID=...; HSID=...", raw_cookie_input)
                    .on_input(on_cookie_input)
                    .style(search_input)
                    .padding(8);

                let save_btn = button(text("Guardar Cookies").size(12))
                    .style(primary_button)
                    .padding([6, 12])
                    .on_press(on_save_raw_cookie);

                let mut import_section = column![
                    text("Importar directamente desde un navegador")
                        .size(11)
                        .color(COLOR_TEXT_SECONDARY),
                    row![firefox_btn, chrome_btn]
                        .spacing(8)
                        .align_y(Alignment::Center),
                ]
                .spacing(6);

                if chrome_failed {
                    import_section = import_section.push(chrome_help_card);
                }

                column![
                    text(
                        "Inicia sesión para sincronizar tus suscripciones, historial y recomendaciones.\nTus credenciales se conservan localmente y nunca salen de tu equipo.",
                    )
                    .size(11)
                    .color(COLOR_TEXT_SECONDARY),
                    row![browser_login_btn, sync_browser_btn].spacing(8).align_y(Alignment::Center),
                    import_section,
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

    let clickable_modal = mouse_area(modal_box).on_press(on_modal_click_trap);

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



/// A video entry shown in the Downloads view (in-flight or stored in the local cache).
pub struct DownloadItem {
    pub video_id: String,
    pub title: String,
    pub percent: u8,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub speed_bytes_per_sec: u64,
    pub is_complete: bool,
}

pub fn view_downloads<'a, Message: Clone + 'static>(
    items: Vec<DownloadItem>,
    on_open: impl Fn(String) -> Message + Copy + 'static,
    on_delete: impl Fn(String) -> Message + Copy + 'static,
) -> Element<'a, Message> {
    let mut list = Column::new().spacing(8).padding([16, 24]);
    list = list.push(text("Descargas").size(22).color(COLOR_TEXT_PRIMARY));

    if items.is_empty() {
        list = list.push(
            text("No hay videos descargados ni bufferizados localmente.")
                .size(13)
                .color(COLOR_TEXT_SECONDARY),
        );
    }

    for item in items {
        let speed_str = crate::cache_manager::format_speed(item.speed_bytes_per_sec);
        let status_row: Element<'a, Message> = if item.is_complete {
            row![
                icon_check_circle(12.0),
                text(format!("Descargado · {}", format_size(item.downloaded_bytes)))
                    .size(12)
                    .color(iced::Color::from_rgb8(0x2b, 0xa6, 0x40)),
            ]
            .spacing(5)
            .align_y(Alignment::Center)
            .into()
        } else {
            let status_text = if item.total_bytes > 0 {
                if !speed_str.is_empty() {
                    format!(
                        "Descargando {}% · {} / {} · {}",
                        item.percent,
                        format_size(item.downloaded_bytes),
                        format_size(item.total_bytes),
                        speed_str
                    )
                } else {
                    format!(
                        "Descargando {}% · {} / {}",
                        item.percent,
                        format_size(item.downloaded_bytes),
                        format_size(item.total_bytes)
                    )
                }
            } else if !speed_str.is_empty() {
                format!("Descargando {}% · {}", item.percent, speed_str)
            } else {
                format!("Descargando {}%", item.percent)
            };
            text(status_text).size(12).color(COLOR_TEXT_SECONDARY).into()
        };
        let bar_color = if item.is_complete {
            iced::Color::from_rgb8(0x2b, 0xa6, 0x40)
        } else {
            iced::Color::from_rgb8(0x3e, 0xa6, 0xff)
        };
        let content = column![
            text(item.title).size(14).color(COLOR_TEXT_PRIMARY),
            iced::widget::progress_bar(0.0..=100.0, item.percent as f32)
                .height(6)
                .style(move |_t: &Theme| iced::widget::progress_bar::Style {
                    background: COLOR_SURFACE_HOVER.into(),
                    bar: bar_color.into(),
                    border: iced::border::rounded(3),
                }),
            status_row,
        ]
        .spacing(6);

        let open_btn = button(content)
            .style(secondary_button)
            .padding([10, 14])
            .width(Length::Fill)
            .on_press(on_open(item.video_id.clone()));

        let delete_btn = button(
            row![
                icon_trash(14.0),
                text("Eliminar").size(12).color(iced::Color::from_rgb8(0xff, 0x66, 0x66)),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        )
        .style(move |_theme, status| {
            let bg = match status {
                button::Status::Hovered => iced::Color::from_rgb8(0x5a, 0x1a, 0x1a),
                button::Status::Pressed => iced::Color::from_rgb8(0x7a, 0x20, 0x20),
                _ => COLOR_SURFACE,
            };
            let border_col = if status == button::Status::Hovered {
                iced::Color::from_rgb8(0xff, 0x44, 0x44)
            } else {
                COLOR_BORDER
            };
            button::Style {
                background: Some(bg.into()),
                border: iced::border::rounded(10).color(border_col).width(1.0),
                ..button::Style::default()
            }
        })
        .padding([10, 14])
        .on_press(on_delete(item.video_id));

        list = list.push(
            row![open_btn, delete_btn]
                .spacing(8)
                .align_y(Alignment::Center)
                .width(Length::Fill),
        );
    }

    scrollable(list).width(Length::Fill).height(Length::Fill).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    enum TestMsg {
        NoOp,
        Tab(usize),
        Hover(usize, bool),
        Search(String),
        Scroll(iced::widget::scrollable::Viewport),
    }

    #[test]
    fn test_view_header_bar_multiple_tabs() {
        let tabs = vec![
            VideoTab {
                video_id: "v1".into(),
                title: "Rust Async Tutorial".into(),
                channel_name: "RustLang".into(),
                saved_pos_ms: 0,
                thumbnail_url: "".into(),
            },
            VideoTab {
                video_id: "v2".into(),
                title: "KDE Plasma 6 Overview".into(),
                channel_name: "KDE".into(),
                saved_pos_ms: 1000,
                thumbnail_url: "".into(),
            },
            VideoTab {
                video_id: "v3".into(),
                title: "Wayland Protocols Explained".into(),
                channel_name: "Linux".into(),
                saved_pos_ms: 5000,
                thumbnail_url: "".into(),
            },
        ];

        let _element = view_header_bar(
            "",
            200.0,
            false,
            0.0,
            false,
            &[],
            false,
            PowerProfile::Ac,
            false,
            None,
            None,
            0.0,
            false,
            false,
            false,
            false,
            &tabs,
            Some(0),
            None,
            None,
            None,
            false,
            None,
            None,
            1280.0,
            0.0,
            0.0,
            true,
            false,
            TestMsg::Search,
            TestMsg::NoOp,
            |_| TestMsg::NoOp,
            |_| TestMsg::NoOp,
            TestMsg::NoOp,
            TestMsg::NoOp,
            TestMsg::NoOp,
            TestMsg::NoOp,
            TestMsg::NoOp,
            TestMsg::NoOp,
            |_| TestMsg::NoOp,
            TestMsg::NoOp,
            TestMsg::Tab,
            TestMsg::Tab,
            TestMsg::Tab,
            TestMsg::Hover,
            TestMsg::Scroll,
            TestMsg::NoOp,
            TestMsg::NoOp,
            TestMsg::NoOp,
            TestMsg::NoOp,
            TestMsg::NoOp,
            TestMsg::NoOp,
            TestMsg::NoOp,
        );
    }
}
