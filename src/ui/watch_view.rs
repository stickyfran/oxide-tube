use crate::config::PreferredCodec;
use crate::innertube::{VideoCard, WatchDetails};
use crate::live_chat::ChatMessage;
use crate::ui::icons::*;
use crate::ui::osd_controls::{
    view_control_bar, view_subtitle_overlay, view_video_hitbox_and_popups, BufferSpinnerProgram, OsdState, PlayPauseRippleProgram,
    SeekFeedbackProgram,
};
use crate::ui::theme::*;
use iced::widget::{button, canvas, column, container, mouse_area, row, scrollable, stack, text, text_editor, Space};
use iced::{Alignment, Element, Length};

pub fn extract_unique_urls(text: &str) -> Vec<String> {
    let mut urls = Vec::new();
    for word in text.split_whitespace() {
        if word.starts_with("http://")
            || word.starts_with("https://")
            || word.starts_with("www.")
            || word.starts_with("youtu.be/")
        {
            let url = if word.starts_with("www.") {
                format!("https://{word}")
            } else {
                word.to_string()
            };
            let url_clean = url.trim_end_matches(&[',', '.', ')', ']', ';', '>', '<', '"', '\''][..]).to_string();
            if !url_clean.is_empty() && !urls.contains(&url_clean) {
                urls.push(url_clean);
            }
        }
    }
    urls
}

pub fn extract_timestamps(text: &str) -> Vec<(String, u64)> {
    let mut list = Vec::new();
    for word in text.split_whitespace() {
        let clean = word.trim_matches(&['(', ')', '[', ']', ',', '.', ';', ':', '-', '"', '\''][..]);
        let parts: Vec<&str> = clean.split(':').collect();
        if parts.len() == 2 {
            if let (Ok(m), Ok(s)) = (parts[0].parse::<u64>(), parts[1].parse::<u64>()) {
                if s < 60 && m < 600 {
                    let total_secs = m * 60 + s;
                    let label = format!("{:02}:{:02}", m, s);
                    if !list.iter().any(|(l, _)| l == &label) {
                        list.push((label, total_secs));
                    }
                }
            }
        } else if parts.len() == 3 {
            if let (Ok(h), Ok(m), Ok(s)) = (parts[0].parse::<u64>(), parts[1].parse::<u64>(), parts[2].parse::<u64>()) {
                if m < 60 && s < 60 && h < 100 {
                    let total_secs = h * 3600 + m * 60 + s;
                    let label = format!("{}:{:02}:{:02}", h, m, s);
                    if !list.iter().any(|(l, _)| l == &label) {
                        list.push((label, total_secs));
                    }
                }
            }
        }
    }
    list
}

fn read_only_text_editor_bindings<Message>(event: text_editor::KeyPress) -> Option<text_editor::Binding<Message>> {
    let text_editor::KeyPress { ref key, modifiers, .. } = event;
    if modifiers.command() {
        match key.as_ref() {
            iced::keyboard::Key::Character("c") | iced::keyboard::Key::Character("C") => Some(text_editor::Binding::Copy),
            iced::keyboard::Key::Character("a") | iced::keyboard::Key::Character("A") => Some(text_editor::Binding::SelectAll),
            _ => None,
        }
    } else if let iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape) = key.as_ref() {
        Some(text_editor::Binding::Unfocus)
    } else if modifiers.shift() {
        text_editor::Binding::from_key_press(event).filter(|b| matches!(b, text_editor::Binding::Select(_)))
    } else {
        None
    }
}

pub fn render_rich_text_with_links<'a, Message: Clone + 'static>(
    raw_text: &str,
    font_size: f32,
    base_color: iced::Color,
    on_open_url: impl Fn(String) -> Message + Copy + 'static,
) -> Element<'a, Message> {
    let mut main_col = column![].spacing(4);
    for line in raw_text.lines() {
        if line.trim().is_empty() {
            main_col = main_col.push(Space::with_height(Length::Fixed(4.0)));
            continue;
        }

        let words: Vec<&str> = line.split_whitespace().collect();
        let has_links = words.iter().any(|w| {
            w.starts_with("http://")
                || w.starts_with("https://")
                || w.starts_with("www.")
                || w.starts_with("youtu.be/")
        });

        if !has_links {
            main_col = main_col.push(text(line.to_string()).size(font_size).color(base_color));
        } else {
            let mut line_row = row![].spacing(4).align_y(Alignment::Center);
            for word in words {
                if word.starts_with("http://")
                    || word.starts_with("https://")
                    || word.starts_with("www.")
                    || word.starts_with("youtu.be/")
                {
                    let url = if word.starts_with("www.") {
                        format!("https://{word}")
                    } else {
                        word.to_string()
                    };
                    let url_clean = url.trim_end_matches(&[',', '.', ')', ']', ';'][..]).to_string();
                    let link_btn = button(
                        text(word.to_string())
                            .size(font_size)
                            .color(iced::Color::from_rgb8(0x3e, 0xa6, 0xff)),
                    )
                    .style(ghost_button)
                    .padding(0)
                    .on_press(on_open_url(url_clean));
                    line_row = line_row.push(link_btn);
                } else {
                    line_row = line_row.push(text(word.to_string()).size(font_size).color(base_color));
                }
            }
            main_col = main_col.push(line_row);
        }
    }
    main_col.into()
}

#[allow(clippy::too_many_arguments)]
pub fn view_watch_page<'a, Message: Clone + 'static>(
    details: &'a WatchDetails,
    osd_state: &'a OsdState,
    live_messages: &'a [ChatMessage],
    playback_queue: &'a [VideoCard],
    autoplay_next: bool,
    thumbnail_cache: &'a std::collections::HashMap<String, iced::widget::image::Handle>,
    blurred_cache: &'a std::collections::HashMap<String, iced::widget::image::Handle>,
    blurred_poster: Option<&'a iced::widget::image::Handle>,
    video_frame: Option<crate::player::VideoFrameBuffer>,
    blur_amount: f32,
    theater_mode: bool,
    is_fullscreen: bool,
    is_pip_active: bool,
    description_expanded: bool,
    chat_collapsed: bool,
    _description_content: &'a iced::widget::text_editor::Content,
    comment_contents: &'a [iced::widget::text_editor::Content],
    player_viewport_width: u16,
    player_viewport_height: u16,
    _window_height: f32,
    on_video_hitbox_click: Message,
    on_close_menus: Message,
    on_play_pause: Message,
    on_retry_playback: Message,
    on_seek: impl Fn(u64) -> Message + Copy + 'static,
    on_volume: impl Fn(f32) -> Message + Copy + 'static,
    on_toggle_mute: Message,
    on_toggle_quality_menu: Message,
    on_select_quality: impl Fn(u32) -> Message + Copy + 'static,
    on_toggle_save_default_quality: Message,
    on_set_preferred_codec: impl Fn(PreferredCodec) -> Message + Copy + 'static,
    on_toggle_audio_menu: Message,
    on_select_audio: impl Fn(usize) -> Message + Copy + 'static,
    on_toggle_speed_menu: Message,
    on_select_speed: impl Fn(f64) -> Message + Copy + 'static,
    on_toggle_captions: Message,
    on_select_caption_track: impl Fn(Option<usize>) -> Message + Copy + 'static,
    on_toggle_pip: Message,
    on_toggle_theater: Message,
    on_toggle_fullscreen: Message,
    on_toggle_subscribe: Message,
    on_like: Message,
    on_dislike: Message,
    buffer_status: Option<(u8, bool)>,
    on_trigger_full_buffer: Message,
    on_open_video: impl Fn(String) -> Message + Copy + 'static,
    on_open_video_new_tab: impl Fn(String) -> Message + Copy + 'static,
    on_open_channel: impl Fn(String) -> Message + Copy + 'static,
    on_add_to_queue: impl Fn(VideoCard) -> Message + Copy + 'static,
    on_remove_from_queue: impl Fn(usize) -> Message + Copy + 'static,
    on_clear_queue: Message,
    on_toggle_autoplay: Message,
    on_open_url: impl Fn(String) -> Message + Copy + 'static,
    on_copy_text: impl Fn(String) -> Message + Copy + 'static,
    on_toggle_description: Message,
    on_toggle_chat: Message,
    _on_description_action: impl Fn(iced::widget::text_editor::Action) -> Message + Copy + 'static,
    on_comment_action: impl Fn(usize, iced::widget::text_editor::Action) -> Message + Copy + 'static,
    on_scroll: impl Fn(iced::widget::scrollable::Viewport) -> Message + Copy + 'static,
) -> Element<'a, Message> {
    let col_w = player_viewport_width as f32;
    let blur = blur_amount.clamp(0.0, 1.0);
    let hide_cursor = !osd_state.is_visible && osd_state.is_playing && !is_pip_active;

    // 1. Transparent Underlay Punch-Through Video Container:
    let hitbox_msg = if osd_state.has_open_menu() {
        on_close_menus.clone()
    } else {
        on_video_hitbox_click
    };
    let video_hitbox = view_video_hitbox_and_popups(
        hitbox_msg.clone(),
        hide_cursor,
    );

    let controls_bar = view_control_bar(
        osd_state,
        &details.video_streams,
        &details.audio_streams,
        &details.caption_tracks,
        &details.chapters,
        theater_mode,
        is_fullscreen,
        on_close_menus.clone(),
        on_play_pause.clone(),
        on_seek,
        on_volume,
        on_toggle_mute,
        on_toggle_quality_menu,
        on_select_quality,
        on_toggle_save_default_quality,
        on_set_preferred_codec,
        on_toggle_audio_menu,
        on_select_audio,
        on_toggle_speed_menu,
        on_select_speed,
        on_toggle_captions,
        on_select_caption_track,
        on_toggle_pip.clone(),
        on_toggle_theater,
        on_toggle_fullscreen,
    );

    let show_poster = video_frame.is_none() || !osd_state.first_frame_rendered;
    let thumb_key = format!("https://i.ytimg.com/vi/{}/hqdefault.jpg", &details.video_id);
    let poster_img = blurred_poster
        .or_else(|| blurred_cache.get(&thumb_key))
        .or_else(|| thumbnail_cache.get(&thumb_key))
        .or_else(|| {
            details
                .recommended_videos
                .iter()
                .find(|v| v.video_id == details.video_id)
                .and_then(|v| {
                    blurred_cache
                        .get(&v.thumbnail_url)
                        .or_else(|| thumbnail_cache.get(&v.thumbnail_url))
                })
        });

    let loading_badge = mouse_area(
        container(
            column![
                Space::with_height(Length::Fill),
                row![
                    Space::with_width(Length::Fill),
                    if osd_state.has_playback_error {
                        container(
                            column![
                                text("No se pudo reproducir este formato o stream.")
                                    .size(13)
                                    .color(COLOR_TEXT_PRIMARY),
                                button(
                                    row![
                                        icon_reload(12.0),
                                        text("Reintentar reproducción").size(12),
                                    ]
                                    .spacing(6)
                                    .align_y(Alignment::Center),
                                )
                                .style(primary_button)
                                .padding([6, 12])
                                .on_press(on_retry_playback),
                            ]
                            .spacing(8)
                            .align_x(Alignment::Center),
                        )
                        .padding([12, 18])
                        .style(osd_container)
                    } else if osd_state.is_buffering {
                        container(
                            column![
                                canvas(BufferSpinnerProgram {
                                    buffering_percent: osd_state.buffering_percent,
                                    is_buffering: osd_state.is_buffering,
                                })
                                .width(Length::Fixed(64.0))
                                .height(Length::Fixed(64.0)),
                                if osd_state.buffering_percent < 100 {
                                    container(
                                        text(format!("{}%", osd_state.buffering_percent))
                                            .size(12)
                                            .color(COLOR_TEXT_SECONDARY),
                                    )
                                    .padding([2, 6])
                                    .style(osd_container)
                                } else {
                                    container(Space::with_height(Length::Fixed(0.0)))
                                },
                            ]
                            .spacing(6)
                            .align_x(Alignment::Center),
                        )
                        .style(transparent_player_container)
                    } else if !osd_state.is_playing {
                        container(
                            button(
                                container(icon_play(36.0))
                                    .align_x(iced::alignment::Horizontal::Center)
                                    .align_y(iced::alignment::Vertical::Center)
                            )
                            .style(move |_t, status| match status {
                                button::Status::Hovered => button::Style {
                                    background: Some(iced::Color::from_rgba(0.9, 0.1, 0.1, 0.95).into()),
                                    border: iced::border::rounded(32).color(COLOR_ACCENT_RED).width(2.0),
                                    shadow: iced::Shadow {
                                        color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.5),
                                        offset: iced::Vector::new(0.0, 4.0),
                                        blur_radius: 12.0,
                                    },
                                    ..button::Style::default()
                                },
                                _ => button::Style {
                                    background: Some(iced::Color::from_rgba(0.1, 0.1, 0.1, 0.8).into()),
                                    border: iced::border::rounded(32).color(iced::Color::WHITE).width(1.5),
                                    shadow: iced::Shadow {
                                        color: iced::Color::from_rgba(0.0, 0.0, 0.0, 0.4),
                                        offset: iced::Vector::new(0.0, 3.0),
                                        blur_radius: 8.0,
                                    },
                                    ..button::Style::default()
                                },
                            })
                            .padding([14, 18])
                            .on_press(on_play_pause)
                        )
                        .style(transparent_player_container)
                    } else {
                        container(Space::with_height(Length::Fixed(0.0)))
                    },
                    Space::with_width(Length::Fill),
                ],
                Space::with_height(Length::Fill),
            ],
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_t| container::Style {
            background: Some(iced::Color::from_rgba(0.0, 0.0, 0.0, 0.35).into()),
            ..container::Style::default()
        })
    )
    .on_press(hitbox_msg);

    let player_content: Element<'a, Message> = if is_pip_active {
        let pip_card = container(
            column![
                Space::with_height(Length::Fill),
                row![
                    Space::with_width(Length::Fill),
                    container(
                        column![
                            row![
                                Space::with_width(Length::Fill),
                                icon_pip(28.0),
                                Space::with_width(Length::Fill),
                            ],
                            text("Reproduciendo en ventana flotante (PiP)")
                                .size(16)
                                .color(COLOR_TEXT_PRIMARY),
                            text("El video está activo en la miniventana Picture-in-Picture.")
                                .size(13)
                                .color(COLOR_TEXT_SECONDARY),
                            Space::with_height(Length::Fixed(6.0)),
                            row![
                                Space::with_width(Length::Fill),
                                button(
                                    row![
                                        icon_pip(15.0),
                                        text("Reacoplar video aquí").size(13).color(COLOR_TEXT_PRIMARY),
                                    ]
                                    .spacing(8)
                                    .align_y(Alignment::Center)
                                )
                                .style(primary_button)
                                .padding([8, 18])
                                .on_press(on_toggle_pip),
                                Space::with_width(Length::Fill),
                            ],
                        ]
                        .spacing(8)
                        .align_x(Alignment::Center)
                    )
                    .padding([24, 32])
                    .style(card_container),
                    Space::with_width(Length::Fill),
                ],
                Space::with_height(Length::Fill),
            ]
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_t| container::Style {
            background: Some(iced::Color::from_rgb8(0x12, 0x12, 0x12).into()),
            border: iced::border::rounded(12).color(COLOR_BORDER).width(1.0),
            ..container::Style::default()
        });
        pip_card.into()
    } else if let Some(frame) = video_frame {
        let mut frame_layers: Vec<Element<'a, Message>> = vec![
            container(Space::new(Length::Fill, Length::Fill))
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_t| container::Style {
                    background: Some(iced::Color::BLACK.into()),
                    ..container::Style::default()
                })
                .into(),
            container(crate::ui::video_widget::video_shader(frame))
                .width(Length::Fill)
                .height(Length::Fixed(player_viewport_height as f32))
                .into(),
        ];
        if osd_state.has_playback_error {
            frame_layers.push(loading_badge.into());
        }
        frame_layers.push(video_hitbox);
        stack(frame_layers).into()
    } else if show_poster {
        if let Some(h) = poster_img {
            stack![
                container(Space::new(Length::Fill, Length::Fill))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(|_t| container::Style {
                        background: Some(iced::Color::BLACK.into()),
                        ..container::Style::default()
                    }),
                iced::widget::image(h.clone())
                    .width(Length::Fill)
                    .height(Length::Fixed(player_viewport_height as f32))
                    .content_fit(iced::ContentFit::Cover),
                video_hitbox,
                loading_badge,
            ]
            .into()
        } else {
            stack![
                container(Space::new(Length::Fill, Length::Fill))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(|_t| container::Style {
                        background: Some(iced::Color::BLACK.into()),
                        ..container::Style::default()
                    }),
                video_hitbox,
                loading_badge,
            ]
            .into()
        }
    } else {
        container(video_hitbox)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_t| container::Style {
                background: Some(iced::Color::BLACK.into()),
                ..container::Style::default()
            })
            .into()
    };

    let make_ripple = || -> Option<Element<'a, Message>> {
        if let Some((start_time, is_playing)) = osd_state.play_pause_anim {
            if start_time.elapsed().as_secs_f32() < 0.50 {
                return Some(
                    container(
                        canvas(PlayPauseRippleProgram {
                            start_time,
                            is_playing,
                        })
                        .width(Length::Fill)
                        .height(Length::Fill),
                    )
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(transparent_player_container)
                    .into(),
                );
            }
        }
        None
    };

    let make_seek_feedback = || -> Option<Element<'a, Message>> {
        if let Some(anim) = osd_state.seek_anim {
            if anim.last_tap_time.elapsed().as_secs_f32() < 0.65 {
                return Some(
                    container(
                        canvas(SeekFeedbackProgram { anim })
                            .width(Length::Fill)
                            .height(Length::Fill),
                    )
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(transparent_player_container)
                    .into(),
                );
            }
        }
        None
    };

    let make_buffering_spinner = || -> Option<Element<'a, Message>> {
        if osd_state.is_buffering && !osd_state.has_playback_error {
            Some(
                container(
                    column![
                        Space::with_height(Length::Fill),
                        row![
                            Space::with_width(Length::Fill),
                            column![
                                canvas(BufferSpinnerProgram {
                                    buffering_percent: osd_state.buffering_percent,
                                    is_buffering: true,
                                })
                                .width(Length::Fixed(64.0))
                                .height(Length::Fixed(64.0)),
                                if osd_state.buffering_percent < 100 {
                                    container(
                                        text(format!("{}%", osd_state.buffering_percent))
                                            .size(12)
                                            .color(COLOR_TEXT_SECONDARY),
                                    )
                                    .padding([2, 6])
                                    .style(osd_container)
                                } else {
                                    container(Space::with_height(Length::Fixed(0.0)))
                                },
                            ]
                            .spacing(6)
                            .align_x(Alignment::Center),
                            Space::with_width(Length::Fill),
                        ],
                        Space::with_height(Length::Fill),
                    ],
                )
                .width(Length::Fill)
                .height(Length::Fill)
                .style(transparent_player_container)
                .into(),
            )
        } else {
            None
        }
    };

    if is_fullscreen {
        let fs_player_container = container(player_content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(if is_pip_active { main_container } else { transparent_player_container });

        let mut fs_layers = vec![fs_player_container.into()];
        if let Some(ripple) = make_ripple() {
            fs_layers.push(ripple);
        }
        if let Some(seek_fx) = make_seek_feedback() {
            fs_layers.push(seek_fx);
        }
        if let Some(buf_spinner) = make_buffering_spinner() {
            fs_layers.push(buf_spinner);
        }
        if osd_state.has_open_menu() {
            let dismiss_backdrop = mouse_area(
                container(Space::new(Length::Fill, Length::Fill))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(transparent_player_container)
            )
            .on_press(on_close_menus.clone())
            .into();
            fs_layers.push(dismiss_backdrop);
        }
        if osd_state.is_visible {
            fs_layers.push(
                container(controls_bar)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_y(iced::alignment::Vertical::Bottom)
                    .into()
            );
        }
        if !osd_state.current_subtitle_text.is_empty() && !is_pip_active {
            fs_layers.push(view_subtitle_overlay(
                &osd_state.current_subtitle_text,
                osd_state.is_visible,
            ));
        }

        let fs_content: Element<'a, Message> = stack(fs_layers)
            .width(Length::Fill)
            .height(Length::Fill)
            .into();

        return mouse_area(
            container(fs_content)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(if is_pip_active { main_container } else { transparent_player_container })
        )
        .interaction(if hide_cursor {
            iced::mouse::Interaction::None
        } else {
            iced::mouse::Interaction::Idle
        })
        .into();
    }

    let normal_player_container = container(player_content)
        .width(if theater_mode { Length::Fill } else { Length::Fixed(col_w as f32) })
        .height(Length::Fixed(player_viewport_height as f32))
        .style(if is_pip_active { main_container } else { transparent_player_container });

    let mut normal_layers = vec![normal_player_container.into()];
    if !is_pip_active {
        if let Some(ripple) = make_ripple() {
            normal_layers.push(ripple);
        }
        if let Some(seek_fx) = make_seek_feedback() {
            normal_layers.push(seek_fx);
        }
        if let Some(buf_spinner) = make_buffering_spinner() {
            normal_layers.push(buf_spinner);
        }
        if osd_state.has_open_menu() {
            let dismiss_backdrop = mouse_area(
                container(Space::new(Length::Fill, Length::Fill))
                    .width(if theater_mode { Length::Fill } else { Length::Fixed(col_w as f32) })
                    .height(Length::Fixed(player_viewport_height as f32))
                    .style(transparent_player_container)
            )
            .on_press(on_close_menus.clone())
            .into();
            normal_layers.push(dismiss_backdrop);
        }
        if osd_state.is_visible {
            normal_layers.push(
                container(controls_bar)
                    .width(if theater_mode { Length::Fill } else { Length::Fixed(col_w as f32) })
                    .height(Length::Fill)
                    .align_y(iced::alignment::Vertical::Bottom)
                    .into()
            );
        }
        if !osd_state.current_subtitle_text.is_empty() {
            normal_layers.push(view_subtitle_overlay(
                &osd_state.current_subtitle_text,
                osd_state.is_visible,
            ));
        }
    }

    let player_stack: Element<'a, Message> = stack(normal_layers)
        .width(if theater_mode { Length::Fill } else { Length::Fixed(col_w as f32) })
        .height(Length::Fixed(player_viewport_height as f32))
        .into();

    let player_area = mouse_area(player_stack)
        .interaction(if hide_cursor {
            iced::mouse::Interaction::None
        } else {
            iced::mouse::Interaction::Idle
        });

    // 2. Video Metadata & Details:
    let title: Element<'a, Message> = if details.title.is_empty() {
        container(Space::with_width(Length::Fixed(420.0)))
            .width(Length::Fixed(420.0))
            .height(Length::Fixed(22.0))
            .style(|_t| container::Style {
                background: Some(iced::Color::from_rgb8(0x24, 0x24, 0x24).into()),
                border: iced::border::rounded(6),
                ..container::Style::default()
            })
            .into()
    } else {
        text(&details.title)
            .size(20)
            .color(COLOR_TEXT_PRIMARY)
            .into()
    };

    let avatar_handle = thumbnail_cache.get(&details.channel_avatar);
    let channel_avatar = view_avatar_image_or_badge(&details.channel_name, avatar_handle, 40.0, 16.0);

    let channel_name_label = if details.channel_name.is_empty() {
        "Canal"
    } else {
        &details.channel_name
    };

    let channel_info = column![
        button(text(channel_name_label).size(15).color(COLOR_TEXT_PRIMARY))
            .style(ghost_button)
            .padding(0)
            .on_press(on_open_channel(details.channel_id.clone())),
        text(if details.subscriber_count.is_empty() {
            "Canal de YouTube"
        } else {
            &details.subscriber_count
        })
        .size(12)
        .color(COLOR_TEXT_SECONDARY),
    ]
    .spacing(2);

    let sub_btn = button(
        row![
            if details.is_subscribed {
                icon_check_circle(14.0)
            } else {
                Space::with_width(Length::Fixed(0.0)).into()
            },
            text(if details.is_subscribed { "Suscrito" } else { "Suscribirse" }).size(14)
        ]
        .spacing(6)
        .align_y(Alignment::Center)
    )
    .style(if details.is_subscribed { secondary_button } else { primary_button })
    .padding([8, 16])
    .on_press(on_toggle_subscribe);

    let is_liked = details.like_status == "LIKE";
    let is_disliked = details.like_status == "DISLIKE";

    let like_label = if details.likes_str.is_empty() || details.likes_str == "Me gusta" {
        "Me gusta".to_string()
    } else {
        details.likes_str.clone()
    };

    let like_btn = button(
        row![
            icon_thumb_up(14.0),
            text(like_label)
                .size(13)
                .color(if is_liked { COLOR_ACCENT_RED } else { COLOR_TEXT_PRIMARY })
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .style(secondary_button)
    .padding([8, 14])
    .on_press(on_like);

    let dislike_btn = button(
        row![
            icon_thumb_down(14.0),
            text(if is_disliked { "No me gusta" } else { "" })
                .size(13)
                .color(if is_disliked { COLOR_ACCENT_RED } else { COLOR_TEXT_PRIMARY })
        ]
        .spacing(4)
        .align_y(Alignment::Center),
    )
    .style(secondary_button)
    .padding([8, 12])
    .on_press(on_dislike);

    let is_buffered = buffer_status.as_ref().map(|s| s.1).unwrap_or(false);
    let buf_pct = buffer_status.as_ref().map(|s| s.0).unwrap_or(0);
    let is_loading = buf_pct > 0 && !is_buffered;

    let (buf_label, buf_color, buf_icon) = if is_buffered {
        (
            "Buffer listo ✓".to_string(),
            iced::Color::from_rgb8(0x2b, 0xa6, 0x40),
            icon_check_circle(14.0),
        )
    } else if is_loading {
        (
            format!("Buffereando {buf_pct}%"),
            iced::Color::from_rgb8(0x3e, 0xa6, 0xff),
            icon_reload(14.0),
        )
    } else {
        (
            "Buffer completo".to_string(),
            COLOR_TEXT_PRIMARY,
            icon_buffer(14.0),
        )
    };

    let buffer_btn = button(
        row![
            buf_icon,
            text(buf_label).size(13).color(buf_color),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .style(move |_theme, status| {
        let bg = match status {
            button::Status::Hovered => COLOR_SURFACE_HOVER,
            button::Status::Pressed => iced::Color::from_rgb8(0x35, 0x35, 0x35),
            _ => COLOR_SURFACE,
        };
        let (border_color, border_width) = if is_buffered {
            (iced::Color::from_rgb8(0x2b, 0xa6, 0x40), 1.2)
        } else if is_loading {
            (iced::Color::from_rgb8(0x3e, 0xa6, 0xff), 1.2)
        } else {
            (iced::Color::TRANSPARENT, 0.0)
        };
        button::Style {
            background: Some(bg.into()),
            border: iced::border::rounded(18).color(border_color).width(border_width),
            ..button::Style::default()
        }
    })
    .padding([8, 14])
    .on_press(on_trigger_full_buffer);

    let share_url = format!("https://youtu.be/{}", details.video_id);
    let share_btn = button(
        row![
            icon_share(14.0),
            text("Compartir")
                .size(13)
                .color(COLOR_TEXT_PRIMARY),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .style(secondary_button)
    .padding([8, 14])
    .on_press(on_copy_text(share_url));

    // Channel action row: clean, with Compartir placed to the left of Me gusta!
    let channel_row = row![
        channel_avatar,
        channel_info,
        sub_btn,
        Space::with_width(Length::Fill),
        share_btn,
        like_btn,
        dislike_btn,
        buffer_btn,
    ]
    .spacing(10)
    .align_y(Alignment::Center);

    // Description Box
    let description_box = if details.description.is_empty() && details.video_streams.is_empty() {
        container(
            column![
                container(Space::with_width(Length::Fill))
                    .width(Length::Fill)
                    .height(Length::Fixed(14.0))
                    .style(|_t| container::Style {
                        background: Some(iced::Color::from_rgb8(0x22, 0x22, 0x22).into()),
                        border: iced::border::rounded(4),
                        ..container::Style::default()
                    }),
                container(Space::with_width(Length::Fixed(280.0)))
                    .width(Length::Fixed(280.0))
                    .height(Length::Fixed(14.0))
                    .style(|_t| container::Style {
                        background: Some(iced::Color::from_rgb8(0x1e, 0x1e, 0x1e).into()),
                        border: iced::border::rounded(4),
                        ..container::Style::default()
                    }),
            ]
            .spacing(8),
        )
        .style(card_container)
        .padding(14)
        .width(Length::Fill)
    } else {
        let copy_desc_btn = button(
            row![icon_copy(12.0), text("Copiar").size(11)]
                .spacing(4)
                .align_y(Alignment::Center),
        )
        .style(ghost_button)
        .padding([2, 6])
        .on_press(on_copy_text(details.description.clone()));

        let desc_toggle_btn = button(
            text(if description_expanded { "Mostrar menos" } else { "Mostrar más" })
                .size(12)
                .color(COLOR_TEXT_PRIMARY),
        )
        .style(secondary_button)
        .padding([4, 8])
        .on_press(on_toggle_description);

        let desc_header = row![
            text("Descripción").size(13).color(COLOR_TEXT_SECONDARY),
            Space::with_width(Length::Fill),
            copy_desc_btn,
        ]
        .align_y(Alignment::Center);

        let mut desc_col = column![desc_header].spacing(8);

        if !details.chapters.is_empty() {
            let active_start = details
                .chapters
                .iter()
                .rev()
                .find(|c| c.start_secs <= osd_state.current_pos_secs)
                .or_else(|| details.chapters.first())
                .map(|c| c.start_secs);

            let mut ch_row = row![].spacing(8).align_y(Alignment::Center).padding([2, 0]);
            for ch in &details.chapters {
                let is_active = active_start == Some(ch.start_secs);
                let ts_str = crate::ui::osd_controls::format_duration(ch.start_secs);
                let ch_title = if ch.title.chars().count() > 34 {
                    let mut s: String = ch.title.chars().take(31).collect();
                    s.push_str("...");
                    s
                } else {
                    ch.title.clone()
                };
                let pill_btn = button(
                    row![
                        text(ts_str)
                            .size(11)
                            .color(if is_active { COLOR_ACCENT_RED } else { iced::Color::from_rgb8(0x3e, 0xa6, 0xff) }),
                        text(ch_title)
                            .size(12)
                            .color(if is_active { COLOR_TEXT_PRIMARY } else { COLOR_TEXT_SECONDARY }),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                )
                .style(if is_active { secondary_button } else { ghost_button })
                .padding([5, 10])
                .on_press(on_seek(ch.start_secs));
                ch_row = ch_row.push(pill_btn);
            }

            let ch_scroll = scrollable(ch_row).direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::new().width(0.0).scroller_width(0.0),
            ));
            desc_col = desc_col.push(ch_scroll);
        }

        let desc_text = if description_expanded || details.description.chars().count() <= 240 {
            details.description.clone()
        } else {
            let truncated: String = details.description.chars().take(240).collect();
            format!("{truncated}...")
        };
        let desc_widget = text(desc_text)
            .size(13)
            .color(COLOR_TEXT_PRIMARY);
        desc_col = desc_col.push(desc_widget);

        let desc_urls = extract_unique_urls(&details.description);
        let desc_timestamps = extract_timestamps(&details.description);

        if !desc_timestamps.is_empty() || !desc_urls.is_empty() {
            let mut chip_row = row![].spacing(6).align_y(Alignment::Center);
            for (label, secs) in desc_timestamps.into_iter().take(12) {
                let ts_btn = button(
                    row![
                        icon_play(9.0),
                        text(label).size(11).color(COLOR_TEXT_PRIMARY),
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center),
                )
                .style(secondary_button)
                .padding([3, 8])
                .on_press(on_seek(secs * 1000));
                chip_row = chip_row.push(ts_btn);
            }
            for url in desc_urls.into_iter().take(12) {
                let display_label = if url.len() > 36 {
                    let mut s: String = url.chars().take(34).collect();
                    s.push_str("..");
                    s
                } else {
                    url.clone()
                };
                let link_btn = button(
                    row![
                        icon_link(10.0),
                        text(display_label).size(11).color(iced::Color::from_rgb8(0x3e, 0xa6, 0xff)),
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center),
                )
                .style(secondary_button)
                .padding([3, 8])
                .on_press(on_open_url(url));
                chip_row = chip_row.push(link_btn);
            }
            let chip_scroll = scrollable(chip_row).direction(scrollable::Direction::Horizontal(
                scrollable::Scrollbar::new().width(0.0).scroller_width(0.0),
            ));
            desc_col = desc_col.push(chip_scroll);
        }

        desc_col = desc_col.push(desc_toggle_btn);

        container(desc_col)
            .style(card_container)
            .padding(14)
            .width(Length::Fill)
    };

    // Comments Section
    let mut comments_col = column![
        text(if details.comments.is_empty() {
            "Comentarios".to_string()
        } else {
            format!("Comentarios ({})", details.comments.len())
        })
        .size(16)
        .color(COLOR_TEXT_PRIMARY)
    ]
    .spacing(14);

    if details.comments.is_empty() && details.video_streams.is_empty() {
        for _ in 0..4 {
            comments_col = comments_col.push(view_skeleton_comment());
        }
    } else {
        for (c_idx, c) in details.comments.iter().enumerate() {
            let comment_avatar_handle = thumbnail_cache.get(&c.author_avatar);
            let comment_avatar = view_avatar_image_or_badge(&c.author, comment_avatar_handle, 32.0, 13.0);

            let copy_comment_btn = button(icon_copy(11.0))
                .style(ghost_button)
                .padding([2, 4])
                .on_press(on_copy_text(c.text.clone()));

            let comment_text_widget: Element<'a, Message> = if let Some(content) = comment_contents.get(c_idx) {
                let comment_urls = extract_unique_urls(&c.text);
                let comment_timestamps = extract_timestamps(&c.text);
                let ed = text_editor(content)
                    .style(selectable_text_editor)
                    .padding(0)
                    .size(13)
                    .key_binding(read_only_text_editor_bindings)
                    .on_action(move |a| on_comment_action(c_idx, a));

                if comment_urls.is_empty() && comment_timestamps.is_empty() {
                    ed.into()
                } else {
                    let mut col = column![ed].spacing(4);
                    let mut chip_row = row![].spacing(6).align_y(Alignment::Center);
                    for (label, secs) in comment_timestamps.into_iter().take(6) {
                        let ts_btn = button(
                            row![
                                icon_play(9.0),
                                text(label).size(11).color(COLOR_TEXT_PRIMARY),
                            ]
                            .spacing(3)
                            .align_y(Alignment::Center),
                        )
                        .style(secondary_button)
                        .padding([2, 6])
                        .on_press(on_seek(secs * 1000));
                        chip_row = chip_row.push(ts_btn);
                    }
                    for url in comment_urls.into_iter().take(6) {
                        let display_label = if url.len() > 32 {
                            let mut s: String = url.chars().take(30).collect();
                            s.push_str("..");
                            s
                        } else {
                            url.clone()
                        };
                        let link_btn = button(
                            row![
                                icon_link(10.0),
                                text(display_label).size(11).color(iced::Color::from_rgb8(0x3e, 0xa6, 0xff)),
                            ]
                            .spacing(4)
                            .align_y(Alignment::Center),
                        )
                        .style(secondary_button)
                        .padding([2, 6])
                        .on_press(on_open_url(url));
                        chip_row = chip_row.push(link_btn);
                    }
                    let chip_scroll = scrollable(chip_row).direction(scrollable::Direction::Horizontal(
                        scrollable::Scrollbar::new().width(0.0).scroller_width(0.0),
                    ));
                    col = col.push(chip_scroll);
                    col.into()
                }
            } else {
                render_rich_text_with_links(&c.text, 13.0, COLOR_TEXT_PRIMARY, on_open_url)
            };

            let mut comment_body = column![
                row![
                    text(&c.author).size(13).color(COLOR_TEXT_PRIMARY),
                    text(&c.published_time).size(11).color(COLOR_TEXT_SECONDARY),
                    Space::with_width(Length::Fill),
                    copy_comment_btn,
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                comment_text_widget,
            ]
            .spacing(4)
            .width(Length::Fill);

            if !c.likes_count.is_empty() && c.likes_count != "0" {
                comment_body = comment_body.push(
                    row![
                        icon_thumb_up(11.0),
                        text(&c.likes_count).size(11).color(COLOR_TEXT_SECONDARY)
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center),
                );
            }

            comments_col = comments_col.push(row![comment_avatar, comment_body].spacing(10));
        }
    }

    let opaque_bg_style = |_t: &iced::Theme| container::Style {
        background: Some(COLOR_BG.into()),
        ..container::Style::default()
    };

    let metadata_container = container(
        column![
            title,
            channel_row,
            description_box,
            comments_col,
        ]
        .spacing(16)
        .padding(iced::Padding {
            top: if theater_mode { 12.0 } else { 0.0 },
            right: 16.0,
            bottom: 24.0,
            left: if theater_mode { 16.0 } else { 24.0 },
        }),
    )
    .style(opaque_bg_style)
    .width(Length::Fill);

    let gap_12 = container(Space::with_height(Length::Fixed(12.0)))
        .width(Length::Fill)
        .height(Length::Fixed(12.0))
        .style(opaque_bg_style);

    let left_column = if theater_mode {
        column![
            player_area,
            gap_12,
            metadata_container,
        ]
        .spacing(0)
        .width(Length::Fill)
    } else {
        let player_box = column![
            container(Space::with_height(Length::Fixed(12.0)))
                .width(Length::Fill)
                .height(Length::Fixed(12.0))
                .style(opaque_bg_style),
            row![
                container(Space::with_width(Length::Fixed(24.0)))
                    .width(Length::Fixed(24.0))
                    .height(Length::Fixed(player_viewport_height as f32))
                    .style(opaque_bg_style),
                player_area,
            ]
            .spacing(0),
        ]
        .spacing(0);

        column![
            player_box,
            gap_12,
            metadata_container,
        ]
        .spacing(0)
        .width(Length::Fixed(col_w + 24.0))
    };

    // 3. Right Column: Live Chat + Recommended Videos
    let mut right_col = column![].spacing(16).width(Length::Fill);

    // Live Chat Panel
    if details.is_live || details.live_chat_continuation.is_some() {
        let chat_header = row![
            icon_live_dot(8.0),
            text("Chat en vivo").size(14).color(COLOR_TEXT_PRIMARY),
            Space::with_width(Length::Fill),
            button(text(if chat_collapsed { "Abrir" } else { "Ocultar" }).size(12))
                .style(ghost_button)
                .on_press(on_toggle_chat),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .padding([8, 12]);

        let chat_box = if !chat_collapsed {
            let mut messages_list = column![].spacing(8);
            for m in live_messages {
                let sender_color = if m.is_owner {
                    COLOR_ACCENT_RED
                } else if m.is_moderator {
                    COLOR_BORDER_FOCUS
                } else {
                    COLOR_TEXT_SECONDARY
                };

                messages_list = messages_list.push(
                    row![
                        text(&m.author_name).size(12).color(sender_color),
                        text(&m.message).size(12).color(COLOR_TEXT_PRIMARY),
                    ]
                    .spacing(6),
                );
            }

            let scroll_messages = scrollable(messages_list)
                .height(Length::Fixed(400.0));

            container(column![chat_header, scroll_messages])
                .style(card_container)
                .width(Length::Fill)
        } else {
            container(chat_header)
                .style(card_container)
                .width(Length::Fill)
        };

        right_col = right_col.push(chat_box);
    }

    // Playback Queue & Continuous Autoplay Panel ("Cola de reproducción")
    let make_autoplay_btn = || {
        button(
            text(if autoplay_next { "Auto-reproducir: ACTIVADO" } else { "Auto-reproducir: DESACTIVADO" })
                .size(11)
                .color(if autoplay_next { COLOR_ACCENT_RED } else { COLOR_TEXT_SECONDARY }),
        )
        .style(secondary_button)
        .padding([4, 8])
        .on_press(on_toggle_autoplay.clone())
    };

    if !playback_queue.is_empty() {
        let queue_header = row![
            icon_queue_add(15.0),
            text(format!("Cola de reproducción ({})", playback_queue.len()))
                .size(14)
                .color(COLOR_TEXT_PRIMARY),
            Space::with_width(Length::Fill),
            button(text("Vaciar").size(11).color(COLOR_TEXT_SECONDARY))
                .style(ghost_button)
                .padding([3, 6])
                .on_press(on_clear_queue),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .padding([8, 12]);

        let mut q_items = column![].spacing(6).padding([4, 10]);
        for (q_idx, q_vid) in playback_queue.iter().enumerate() {
            let q_vid_id = q_vid.video_id.clone();
            let q_title = if q_vid.title.chars().count() > 42 {
                let mut t: String = q_vid.title.chars().take(39).collect();
                t.push_str("...");
                t
            } else {
                q_vid.title.clone()
            };
            let play_q_btn = button(
                row![
                    text(format!("{}.", q_idx + 1)).size(11).color(COLOR_ACCENT_RED),
                    column![
                        text(q_title).size(12).color(COLOR_TEXT_PRIMARY),
                        text(&q_vid.channel_name).size(10).color(COLOR_TEXT_SECONDARY),
                    ]
                    .spacing(1)
                    .width(Length::Fill),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .style(ghost_button)
            .padding([4, 6])
            .width(Length::Fill)
            .on_press(on_open_video(q_vid_id));

            let rm_q_btn = button(icon_close(11.0))
                .style(ghost_button)
                .padding([4, 6])
                .on_press(on_remove_from_queue(q_idx));

            q_items = q_items.push(
                row![play_q_btn, rm_q_btn]
                    .spacing(4)
                    .align_y(Alignment::Center),
            );
        }

        let queue_box = container(
            column![
                queue_header,
                container(scrollable(q_items)).max_height(220.0),
                container(make_autoplay_btn()).padding([6, 12]),
            ]
            .spacing(4),
        )
        .style(card_container)
        .width(Length::Fill);

        right_col = right_col.push(queue_box);
    }

    // Recommended Videos (Anti-Shorts Compact Cards)
    let recs_header = row![
        text("Videos recomendados").size(16).color(COLOR_TEXT_PRIMARY),
        Space::with_width(Length::Fill),
        if playback_queue.is_empty() {
            make_autoplay_btn().into()
        } else {
            Element::from(Space::with_width(Length::Shrink))
        },
    ]
    .align_y(Alignment::Center);

    let mut recs_list = column![recs_header].spacing(10);
    if details.recommended_videos.is_empty() {
        for _ in 0..8 {
            recs_list = recs_list.push(view_skeleton_compact_card());
        }
    } else {
        for video in &details.recommended_videos {
            let thumb = thumbnail_cache.get(&video.thumbnail_url);
            let blurred_thumb = blurred_cache.get(&video.thumbnail_url);
            recs_list = recs_list.push(view_compact_card(
                video,
                thumb,
                blurred_thumb,
                blur,
                on_open_video,
                on_open_video_new_tab,
                on_open_channel,
                on_add_to_queue,
            ));
        }
    }
    right_col = right_col.push(recs_list);

    let right_col_wrapped = container(right_col)
        .style(opaque_bg_style)
        .width(Length::Fill)
        .padding(iced::Padding {
            top: 12.0,
            right: if theater_mode { 16.0 } else { 24.0 },
            bottom: 24.0,
            left: if theater_mode { 16.0 } else { 24.0 },
        });

    let bottom_cap = container(Space::with_height(Length::Fixed(32.0)))
        .width(Length::Fill)
        .height(Length::Fixed(32.0))
        .style(opaque_bg_style);

    // Layout arrangement:
    if theater_mode {
        let below_theater = container(
            row![
                Space::with_width(Length::Fixed(16.0)),
                right_col_wrapped,
                Space::with_width(Length::Fixed(16.0)),
            ]
            .spacing(0),
        )
        .width(Length::Fill)
        .style(opaque_bg_style);

        let content = column![
            left_column,
            below_theater,
            bottom_cap,
        ]
        .spacing(0)
        .width(Length::Fill);

        container(
            scrollable(content)
                .id(iced::widget::scrollable::Id::new("watch_scrollable"))
                .on_scroll(on_scroll)
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(main_container)
        .into()
    } else {
        let content_row = container(
            row![left_column, right_col_wrapped].spacing(0)
        )
        .width(Length::Fill)
        .style(main_container);

        let content = column![
            content_row,
            bottom_cap,
        ]
        .spacing(0)
        .width(Length::Fill);

        container(
            scrollable(content)
                .id(iced::widget::scrollable::Id::new("watch_scrollable"))
                .on_scroll(on_scroll)
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(main_container)
        .into()
    }
}

fn view_skeleton_comment<'a, Message: 'static>() -> Element<'a, Message> {
    let avatar = container(Space::with_width(Length::Fixed(32.0)))
        .width(Length::Fixed(32.0))
        .height(Length::Fixed(32.0))
        .style(|_t| container::Style {
            background: Some(iced::Color::from_rgb8(0x22, 0x22, 0x22).into()),
            border: iced::border::rounded(16),
            ..container::Style::default()
        });

    let line1 = container(Space::with_width(Length::Fixed(120.0)))
        .width(Length::Fixed(120.0))
        .height(Length::Fixed(12.0))
        .style(|_t| container::Style {
            background: Some(iced::Color::from_rgb8(0x24, 0x24, 0x24).into()),
            border: iced::border::rounded(4),
            ..container::Style::default()
        });

    let line2 = container(Space::with_width(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fixed(13.0))
        .style(|_t| container::Style {
            background: Some(iced::Color::from_rgb8(0x1e, 0x1e, 0x1e).into()),
            border: iced::border::rounded(4),
            ..container::Style::default()
        });

    row![avatar, column![line1, line2].spacing(6).width(Length::Fill)]
        .spacing(10)
        .into()
}

fn view_skeleton_compact_card<'a, Message: 'static>() -> Element<'a, Message> {
    let thumb = container(Space::with_width(Length::Fixed(168.0)))
        .width(Length::Fixed(168.0))
        .height(Length::Fixed(94.0))
        .style(|_t| container::Style {
            background: Some(iced::Color::from_rgb8(0x1a, 0x1a, 0x1a).into()),
            border: iced::border::rounded(8).color(COLOR_BORDER).width(1.0),
            ..container::Style::default()
        });

    let line1 = container(Space::with_width(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fixed(13.0))
        .style(|_t| container::Style {
            background: Some(iced::Color::from_rgb8(0x24, 0x24, 0x24).into()),
            border: iced::border::rounded(4),
            ..container::Style::default()
        });

    let line2 = container(Space::with_width(Length::Fixed(110.0)))
        .width(Length::Fixed(110.0))
        .height(Length::Fixed(11.0))
        .style(|_t| container::Style {
            background: Some(iced::Color::from_rgb8(0x1e, 0x1e, 0x1e).into()),
            border: iced::border::rounded(4),
            ..container::Style::default()
        });

    row![thumb, column![line1, line2].spacing(6).width(Length::Fill)]
        .spacing(10)
        .align_y(Alignment::Start)
        .into()
}

#[allow(clippy::too_many_arguments)]
pub fn view_compact_card<'a, Message: Clone + 'static>(
    video: &'a VideoCard,
    thumb_handle: Option<&'a iced::widget::image::Handle>,
    blurred_thumb_handle: Option<&'a iced::widget::image::Handle>,
    blur_amount: f32,
    on_open_video: impl Fn(String) -> Message + 'static,
    on_open_video_new_tab: impl Fn(String) -> Message + 'static,
    on_open_channel: impl Fn(String) -> Message + 'static,
    on_add_to_queue: impl Fn(VideoCard) -> Message + 'static,
) -> Element<'a, Message> {
    let vid_id = video.video_id.clone();
    let chan_id = video.channel_id.clone();
    let video_clone = video.clone();
    let blur = blur_amount.clamp(0.0, 1.0);
    let text_alpha = (1.0 - blur * 0.62).clamp(0.25, 1.0);

    let badge_content: Element<'a, Message> = if video.is_live {
        row![
            icon_live_dot(7.0),
            text("EN VIVO")
                .size(10)
                .color(iced::Color { a: text_alpha, ..COLOR_TEXT_PRIMARY })
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .into()
    } else {
        text(&video.duration_str)
            .size(10)
            .color(iced::Color { a: text_alpha, ..COLOR_TEXT_PRIMARY })
            .into()
    };

    let thumb_badge = container(badge_content)
        .padding([2, 4])
        .style(osd_container);

    let badge_overlay = container(
        column![
            Space::with_height(Length::Fill),
            row![Space::with_width(Length::Fill), thumb_badge],
        ],
    )
    .width(Length::Fixed(168.0))
    .height(Length::Fixed(94.0))
    .padding(4);

    let active_thumb = if blur > 0.20 {
        blurred_thumb_handle.or(thumb_handle)
    } else {
        thumb_handle
    };

    let base_thumb: Element<'a, Message> = if let Some(h) = active_thumb {
        iced::widget::image(h.clone())
            .width(Length::Fixed(168.0))
            .height(Length::Fixed(94.0))
            .content_fit(iced::ContentFit::Cover)
            .into()
    } else {
        container(Space::with_width(Length::Fixed(168.0)))
            .width(Length::Fixed(168.0))
            .height(Length::Fixed(94.0))
            .style(|_t| container::Style {
                background: Some(iced::Color::from_rgb8(0x18, 0x18, 0x18).into()),
                border: iced::border::rounded(8).color(COLOR_BORDER).width(1.0),
                ..container::Style::default()
            })
            .into()
    };

    let thumb_element: Element<'a, Message> = if blur > 0.02 {
        let frost_alpha = (blur * 0.42).clamp(0.0, 0.55);
        let frost_layer = container(Space::with_width(Length::Fixed(168.0)).height(Length::Fixed(94.0)))
            .width(Length::Fixed(168.0))
            .height(Length::Fixed(94.0))
            .style(move |_t| container::Style {
                background: Some(iced::Color::from_rgba(0.07, 0.07, 0.09, frost_alpha).into()),
                border: iced::border::rounded(8),
                ..container::Style::default()
            });
        container(stack![base_thumb, frost_layer, badge_overlay])
            .width(Length::Fixed(168.0))
            .height(Length::Fixed(94.0))
            .style(|_t| container::Style {
                border: iced::border::rounded(8),
                ..container::Style::default()
            })
            .into()
    } else {
        container(stack![base_thumb, badge_overlay])
            .width(Length::Fixed(168.0))
            .height(Length::Fixed(94.0))
            .style(|_t| container::Style {
                border: iced::border::rounded(8),
                ..container::Style::default()
            })
            .into()
    };

    let thumb_btn = button(thumb_element)
        .style(ghost_button)
        .padding(0)
        .on_press(on_open_video(vid_id.clone()));

    let title_cow: std::borrow::Cow<'a, str> = if video.title.chars().count() > 50 {
        let mut t: String = video.title.chars().take(47).collect();
        t.push_str("...");
        std::borrow::Cow::Owned(t)
    } else {
        std::borrow::Cow::Borrowed(&video.title)
    };

    let title_color = iced::Color {
        a: text_alpha,
        ..COLOR_TEXT_PRIMARY
    };
    let secondary_color = iced::Color {
        a: text_alpha,
        ..COLOR_TEXT_SECONDARY
    };

    let title_btn = button(
        text(title_cow)
            .size(13)
            .line_height(iced::widget::text::LineHeight::Relative(1.2))
            .color(title_color),
    )
    .style(ghost_button)
    .padding(0)
    .on_press(on_open_video(vid_id.clone()));

    let chan_btn = button(text(&video.channel_name).size(12).color(secondary_color))
        .style(ghost_button)
        .padding(0)
        .on_press(on_open_channel(chan_id));

    let meta_cow: std::borrow::Cow<'a, str> = if video.published_str.is_empty() {
        std::borrow::Cow::Borrowed(&video.views_str)
    } else if video.views_str.is_empty() {
        std::borrow::Cow::Borrowed(&video.published_str)
    } else {
        std::borrow::Cow::Owned(format!("{}  -  {}", &video.views_str, &video.published_str))
    };

    let meta = text(meta_cow)
        .size(11)
        .color(secondary_color);

    let queue_btn = button(
        row![
            icon_queue_add(11.0),
            text("A la cola").size(10).color(secondary_color),
        ]
        .spacing(4)
        .align_y(Alignment::Center),
    )
    .style(secondary_button)
    .padding([2, 6])
    .on_press(on_add_to_queue(video_clone));

    let info_col = column![title_btn, chan_btn, meta, queue_btn].spacing(3).width(Length::Fill);

    let card_btn = button(row![thumb_btn, info_col].spacing(10).align_y(Alignment::Start))
        .style(video_card_button)
        .padding(4)
        .on_press(on_open_video(vid_id.clone()));

    mouse_area(card_btn)
        .on_middle_press(on_open_video_new_tab(vid_id))
        .into()
}
