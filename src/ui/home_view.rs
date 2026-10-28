use crate::innertube::VideoCard;
use crate::ui::icons::{icon_live_dot, view_avatar_image_or_badge};
use crate::ui::theme::*;
use iced::widget::{button, column, container, image, mouse_area, row, scrollable, stack, text, Space};
use iced::{Alignment, Element, Length};
use std::collections::HashMap;

/// Calculates responsive column count and 16:9 thumbnail height matching YouTube Desktop specs
pub fn calculate_grid_layout(window_width: f32) -> (usize, f32) {
    let padding = 48.0; // 24px left + 24px right
    let col_gap = 16.0;  // --ytd-rich-grid-item-margin: 16px
    let available_width = (window_width - padding).max(280.0);

    // YouTube desktop enforces min-width of ~320px per item
    let cols = ((available_width + col_gap) / (320.0 + col_gap)).floor() as usize;
    let chunk_size = cols.clamp(1, 6);

    let total_gaps = (chunk_size.saturating_sub(1) as f32) * col_gap;
    let card_width = (available_width - total_gaps) / (chunk_size as f32);
    // Strict 16:9 aspect ratio
    let thumb_height = ((card_width * 9.0 / 16.0).round()).clamp(130.0, 420.0);

    (chunk_size, thumb_height)
}

#[allow(clippy::too_many_arguments)]
pub fn view_video_card<'a, Message: Clone + 'static>(
    video: &'a VideoCard,
    thumb_handle: Option<&'a image::Handle>,
    blurred_thumb_handle: Option<&'a image::Handle>,
    avatar_handle: Option<&'a image::Handle>,
    blurred_avatar_handle: Option<&'a image::Handle>,
    blur_amount: f32,
    thumb_height: f32,
    on_open_video: impl Fn(String) -> Message + 'static,
    on_open_video_new_tab: impl Fn(String) -> Message + 'static,
    on_open_channel: impl Fn(String) -> Message + 'static,
) -> Element<'a, Message> {
    let vid_id = video.video_id.clone();
    let chan_id = video.channel_id.clone();
    let blur = blur_amount.clamp(0.0, 1.0);
    let text_alpha = (1.0 - blur * 0.62).clamp(0.25, 1.0);

    // YouTube-style duration or live badge
    let badge_element: Element<'a, Message> = if video.is_live {
        container(
            row![
                icon_live_dot(8.0),
                text("EN DIRECTO")
                    .size(11)
                    .color(iced::Color { a: text_alpha, ..iced::Color::WHITE })
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        )
        .padding([2, 5])
        .style(yt_live_badge_container)
        .into()
    } else if !video.duration_str.is_empty() {
        container(
            text(&video.duration_str)
                .size(12)
                .color(iced::Color { a: text_alpha, ..iced::Color::WHITE }),
        )
        .padding([2, 4])
        .style(yt_badge_container)
        .into()
    } else {
        Space::with_width(Length::Fixed(0.0)).into()
    };

    let badge_overlay = container(
        column![
            Space::with_height(Length::Fill),
            row![Space::with_width(Length::Fill), badge_element],
        ],
    )
    .width(Length::Fill)
    .height(Length::Fixed(thumb_height))
    .padding(6);

    // Swap to Gaussian-blurred thumbnail handle during blur transition and cross-fade with a frosted veil
    let active_thumb = if blur > 0.20 {
        blurred_thumb_handle.or(thumb_handle)
    } else {
        thumb_handle
    };

    let base_thumb: Element<'a, Message> = if let Some(h) = active_thumb {
        image(h.clone())
            .width(Length::Fill)
            .height(Length::Fixed(thumb_height))
            .content_fit(iced::ContentFit::Cover)
            .into()
    } else {
        container(Space::with_width(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fixed(thumb_height))
            .style(|_theme| container::Style {
                background: Some(iced::Color::from_rgb8(0x18, 0x18, 0x18).into()),
                border: iced::border::rounded(12),
                ..container::Style::default()
            })
            .into()
    };

    let thumb_element: Element<'a, Message> = if blur > 0.02 {
        let frost_alpha = (blur * 0.42).clamp(0.0, 0.55);
        let frost_layer = container(Space::with_width(Length::Fill).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fixed(thumb_height))
            .style(move |_t| container::Style {
                background: Some(iced::Color::from_rgba(0.07, 0.07, 0.09, frost_alpha).into()),
                border: iced::border::rounded(12),
                ..container::Style::default()
            });
        container(stack![base_thumb, frost_layer, badge_overlay])
            .width(Length::Fill)
            .height(Length::Fixed(thumb_height))
            .style(|_t| container::Style {
                border: iced::border::rounded(12),
                ..container::Style::default()
            })
            .into()
    } else {
        container(stack![base_thumb, badge_overlay])
            .width(Length::Fill)
            .height(Length::Fixed(thumb_height))
            .style(|_t| container::Style {
                border: iced::border::rounded(12),
                ..container::Style::default()
            })
            .into()
    };

    let thumb_btn = button(thumb_element)
        .style(ghost_button)
        .padding(0)
        .width(Length::Fill)
        .on_press(on_open_video(vid_id.clone()));

    let active_avatar = if blur > 0.20 {
        blurred_avatar_handle.or(avatar_handle)
    } else {
        avatar_handle
    };

    // YouTube Desktop avatar: 36x36 circular
    let avatar = view_avatar_image_or_badge(&video.channel_name, active_avatar, 36.0, 14.0);

    let channel_avatar_btn = button(avatar)
        .style(ghost_button)
        .padding(0)
        .on_press(on_open_channel(chan_id.clone()));

    // YouTube displays titles up to 2 full lines (around 85 chars)
    let title_cow: std::borrow::Cow<'a, str> = if video.title.chars().count() > 88 {
        let mut t: String = video.title.chars().take(85).collect();
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
            .size(14)
            .line_height(iced::widget::text::LineHeight::Relative(1.25))
            .color(title_color),
    )
    .style(ghost_button)
    .padding(0)
    .on_press(on_open_video(vid_id.clone()));

    let chan_cow: std::borrow::Cow<'a, str> = if video.channel_name.chars().count() > 45 {
        let mut s: String = video.channel_name.chars().take(42).collect();
        s.push_str("...");
        std::borrow::Cow::Owned(s)
    } else {
        std::borrow::Cow::Borrowed(&video.channel_name)
    };

    let channel_btn = button(
        text(chan_cow)
            .size(12)
            .color(secondary_color),
    )
    .style(ghost_button)
    .padding(0)
    .on_press(on_open_channel(chan_id));

    let meta_text = text(&video.meta_suffix)
        .size(12)
        .color(secondary_color);

    // YouTube Desktop layout: Channel on its own line, views and timestamp on the line below
    let info_col: Element<'a, Message> = if blur > 0.08 {
        let pill_alpha = (blur * 0.36).clamp(0.0, 0.45);
        let text_col = column![
            title_btn,
            channel_btn,
            meta_text,
        ]
        .spacing(3)
        .width(Length::Fill);

        let blur_bars = column![
            container(Space::with_width(Length::Fill))
                .width(Length::Fill)
                .height(Length::Fixed(14.0))
                .style(move |_t| container::Style {
                    background: Some(iced::Color::from_rgba(0.22, 0.22, 0.26, pill_alpha).into()),
                    border: iced::border::rounded(4),
                    ..container::Style::default()
                }),
            container(Space::with_width(Length::Fixed(140.0)))
                .width(Length::Fixed(140.0))
                .height(Length::Fixed(12.0))
                .style(move |_t| container::Style {
                    background: Some(iced::Color::from_rgba(0.18, 0.18, 0.22, pill_alpha).into()),
                    border: iced::border::rounded(4),
                    ..container::Style::default()
                }),
        ]
        .spacing(4)
        .padding([2, 0]);

        stack![text_col, blur_bars].into()
    } else {
        column![
            title_btn,
            channel_btn,
            meta_text,
        ]
        .spacing(3)
        .into()
    };

    let details_row = row![channel_avatar_btn, info_col]
        .spacing(12)
        .align_y(Alignment::Start);

    let card_content = column![thumb_btn, details_row]
        .spacing(12);

    let card_btn = button(card_content)
        .style(video_card_button)
        .padding(0)
        .width(Length::FillPortion(1))
        .on_press(on_open_video(vid_id.clone()));

    mouse_area(card_btn)
        .on_middle_press(on_open_video_new_tab(vid_id))
        .into()
}

pub fn view_skeleton_card<'a, Message: 'static>(thumb_height: f32) -> Element<'a, Message> {
    let thumb_box = container(Space::with_width(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fixed(thumb_height))
        .style(|_t| container::Style {
            background: Some(iced::Color::from_rgb8(0x1a, 0x1a, 0x1a).into()),
            border: iced::border::rounded(12),
            ..container::Style::default()
        });

    let avatar_circle = container(Space::with_width(Length::Fixed(36.0)))
        .width(Length::Fixed(36.0))
        .height(Length::Fixed(36.0))
        .style(|_t| container::Style {
            background: Some(iced::Color::from_rgb8(0x22, 0x22, 0x22).into()),
            border: iced::border::rounded(18),
            ..container::Style::default()
        });

    let line_1 = container(Space::with_width(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fixed(14.0))
        .style(|_t| container::Style {
            background: Some(iced::Color::from_rgb8(0x24, 0x24, 0x24).into()),
            border: iced::border::rounded(4),
            ..container::Style::default()
        });

    let line_2 = container(Space::with_width(Length::FillPortion(2)))
        .width(Length::FillPortion(2))
        .height(Length::Fixed(14.0))
        .style(|_t| container::Style {
            background: Some(iced::Color::from_rgb8(0x24, 0x24, 0x24).into()),
            border: iced::border::rounded(4),
            ..container::Style::default()
        });

    let line_chan = container(Space::with_width(Length::Fixed(130.0)))
        .width(Length::Fixed(130.0))
        .height(Length::Fixed(12.0))
        .style(|_t| container::Style {
            background: Some(iced::Color::from_rgb8(0x1e, 0x1e, 0x1e).into()),
            border: iced::border::rounded(4),
            ..container::Style::default()
        });

    let line_meta = container(Space::with_width(Length::Fixed(100.0)))
        .width(Length::Fixed(100.0))
        .height(Length::Fixed(11.0))
        .style(|_t| container::Style {
            background: Some(iced::Color::from_rgb8(0x1a, 0x1a, 0x1a).into()),
            border: iced::border::rounded(4),
            ..container::Style::default()
        });

    let details_row = row![
        avatar_circle,
        column![line_1, line_2, line_chan, line_meta].spacing(5).width(Length::Fill),
    ]
    .spacing(12)
    .align_y(Alignment::Start);

    column![thumb_box, details_row]
        .spacing(12)
        .width(Length::FillPortion(1))
        .into()
}

#[allow(clippy::too_many_arguments)]
pub fn view_home_grid<'a, Message: Clone + 'static>(
    videos: &'a [VideoCard],
    thumbnail_cache: &'a HashMap<String, image::Handle>,
    blurred_cache: &'a HashMap<String, image::Handle>,
    blur_amount: f32,
    window_width: f32,
    is_loading_more: bool,
    on_open_video: impl Fn(String) -> Message + Copy + 'static,
    on_open_video_new_tab: impl Fn(String) -> Message + Copy + 'static,
    on_open_channel: impl Fn(String) -> Message + Copy + 'static,
    on_scroll: impl Fn(iced::widget::scrollable::Viewport) -> Message + 'static,
) -> Element<'a, Message> {
    let (chunk_size, thumb_height) = calculate_grid_layout(window_width);

    // Spacing between rows: 40px (--ytd-rich-grid-row-margin: 40px)
    let mut grid = column![].spacing(40).width(Length::Fill);

    if videos.is_empty() {
        // Immediate skeleton grid matching YouTube layout
        for _row_idx in 0..3 {
            let mut row_cards = row![].spacing(16).width(Length::Fill);
            for _col_idx in 0..chunk_size {
                row_cards = row_cards.push(view_skeleton_card(thumb_height));
            }
            grid = grid.push(row_cards);
        }
    } else {
        for chunk in videos.chunks(chunk_size) {
            let mut row_cards = row![].spacing(16).width(Length::Fill);
            let chunk_len = chunk.len();
            for video in chunk {
                let thumb = thumbnail_cache.get(&video.thumbnail_url);
                let blurred_thumb = blurred_cache.get(&video.thumbnail_url);
                let avatar = thumbnail_cache.get(&video.channel_avatar);
                let blurred_avatar = blurred_cache.get(&video.channel_avatar);
                row_cards = row_cards.push(view_video_card(
                    video,
                    thumb,
                    blurred_thumb,
                    avatar,
                    blurred_avatar,
                    blur_amount,
                    thumb_height,
                    on_open_video,
                    on_open_video_new_tab,
                    on_open_channel,
                ));
            }
            for _ in chunk_len..chunk_size {
                row_cards = row_cards.push(Space::with_width(Length::FillPortion(1)));
            }
            grid = grid.push(row_cards);
        }
        if is_loading_more {
            let mut skeleton_row = row![].spacing(16).width(Length::Fill);
            for _ in 0..chunk_size {
                skeleton_row = skeleton_row.push(view_skeleton_card(thumb_height));
            }
            grid = grid.push(skeleton_row);
        }
    }

    container(
        scrollable(
            container(grid)
                .padding(iced::Padding {
                    top: 24.0,
                    right: 24.0,
                    bottom: 40.0,
                    left: 24.0,
                })
                .width(Length::Fill),
        )
        .id(iced::widget::scrollable::Id::new("home_scrollable"))
        .on_scroll(on_scroll)
        .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .clip(true)
    .style(main_container)
    .into()
}
