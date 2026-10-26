use crate::innertube::VideoCard;
use crate::ui::icons::{icon_live_dot, view_avatar_image_or_badge};
use crate::ui::theme::*;
use iced::widget::{button, column, container, image, mouse_area, row, scrollable, stack, text, Space};
use iced::{Alignment, Element, Length};
use std::collections::HashMap;

#[allow(clippy::too_many_arguments)]
pub fn view_video_card<'a, Message: Clone + 'static>(
    video: &'a VideoCard,
    thumb_handle: Option<&'a image::Handle>,
    blurred_thumb_handle: Option<&'a image::Handle>,
    avatar_handle: Option<&'a image::Handle>,
    blurred_avatar_handle: Option<&'a image::Handle>,
    blur_amount: f32,
    on_open_video: impl Fn(String) -> Message + 'static,
    on_open_video_new_tab: impl Fn(String) -> Message + 'static,
    on_open_channel: impl Fn(String) -> Message + 'static,
) -> Element<'a, Message> {
    let vid_id = video.video_id.clone();
    let chan_id = video.channel_id.clone();
    let blur = blur_amount.clamp(0.0, 1.0);
    let text_alpha = (1.0 - blur * 0.62).clamp(0.25, 1.0);

    let badge_content: Element<'a, Message> = if video.is_live {
        row![
            icon_live_dot(8.0),
            text("EN VIVO")
                .size(11)
                .color(iced::Color { a: text_alpha, ..COLOR_TEXT_PRIMARY })
        ]
        .spacing(4)
        .align_y(Alignment::Center)
        .into()
    } else {
        text(&video.duration_str)
            .size(11)
            .color(iced::Color { a: text_alpha, ..COLOR_TEXT_PRIMARY })
            .into()
    };

    let thumb_badge = container(badge_content)
        .padding([2, 6])
        .style(osd_container);

    let badge_overlay = container(
        column![
            Space::with_height(Length::Fill),
            row![Space::with_width(Length::Fill), thumb_badge],
        ],
    )
    .width(Length::Fill)
    .height(Length::Fixed(190.0))
    .padding(8);

    // Swap to Gaussian-blurred thumbnail handle during blur transition and cross-fade with a frosted veil
    let active_thumb = if blur > 0.20 {
        blurred_thumb_handle.or(thumb_handle)
    } else {
        thumb_handle
    };

    let base_thumb: Element<'a, Message> = if let Some(h) = active_thumb {
        image(h.clone())
            .width(Length::Fill)
            .height(Length::Fixed(190.0))
            .content_fit(iced::ContentFit::Cover)
            .into()
    } else {
        container(Space::with_width(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fixed(190.0))
            .style(|_theme| container::Style {
                background: Some(iced::Color::from_rgb8(0x18, 0x18, 0x18).into()),
                border: iced::border::rounded(12).color(COLOR_BORDER).width(1.0),
                ..container::Style::default()
            })
            .into()
    };

    let thumb_element: Element<'a, Message> = if blur > 0.02 {
        let frost_alpha = (blur * 0.42).clamp(0.0, 0.55);
        let frost_layer = container(Space::with_width(Length::Fill).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fixed(190.0))
            .style(move |_t| container::Style {
                background: Some(iced::Color::from_rgba(0.07, 0.07, 0.09, frost_alpha).into()),
                border: iced::border::rounded(12),
                ..container::Style::default()
            });
        container(stack![base_thumb, frost_layer, badge_overlay])
            .width(Length::Fill)
            .height(Length::Fixed(190.0))
            .style(|_t| container::Style {
                border: iced::border::rounded(12),
                ..container::Style::default()
            })
            .into()
    } else {
        container(stack![base_thumb, badge_overlay])
            .width(Length::Fill)
            .height(Length::Fixed(190.0))
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

    // Channel avatar with real image or initial fallback
    let avatar = view_avatar_image_or_badge(&video.channel_name, active_avatar, 36.0, 14.0);

    let channel_avatar_btn = button(avatar)
        .style(ghost_button)
        .padding(0)
        .on_press(on_open_channel(chan_id.clone()));

    let title_cow: std::borrow::Cow<'a, str> = if video.title.chars().count() > 38 {
        let mut t: String = video.title.chars().take(35).collect();
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
            .line_height(iced::widget::text::LineHeight::Relative(1.15))
            .color(title_color),
    )
    .style(ghost_button)
    .padding(0)
    .on_press(on_open_video(vid_id.clone()));

    let chan_cow: std::borrow::Cow<'a, str> = if video.channel_name.chars().count() > 22 {
        let mut s: String = video.channel_name.chars().take(20).collect();
        s.push_str("..");
        std::borrow::Cow::Owned(s)
    } else {
        std::borrow::Cow::Borrowed(&video.channel_name)
    };

    let channel_btn = button(
        text(chan_cow)
            .size(11)
            .color(secondary_color),
    )
    .style(ghost_button)
    .padding(0)
    .on_press(on_open_channel(chan_id));

    let meta_row = row![
        channel_btn,
        text(&video.meta_suffix)
            .size(11)
            .color(secondary_color),
    ]
    .spacing(2)
    .align_y(Alignment::Center);

    let info_col: Element<'a, Message> = if blur > 0.08 {
        let pill_alpha = (blur * 0.36).clamp(0.0, 0.45);
        let text_col = column![
            title_btn,
            meta_row,
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
                .height(Length::Fixed(11.0))
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
            meta_row,
        ]
        .spacing(3)
        .into()
    };

    let details_row = row![channel_avatar_btn, info_col]
        .spacing(10)
        .align_y(Alignment::Start);

    let card_content = column![thumb_btn, details_row]
        .spacing(8);

    let card_btn = button(card_content)
        .style(video_card_button)
        .padding(6)
        .width(Length::FillPortion(1))
        .on_press(on_open_video(vid_id.clone()));

    mouse_area(card_btn)
        .on_middle_press(on_open_video_new_tab(vid_id))
        .into()
}

pub fn view_skeleton_card<'a, Message: 'static>() -> Element<'a, Message> {
    let thumb_box = container(Space::with_width(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fixed(190.0))
        .style(|_t| container::Style {
            background: Some(iced::Color::from_rgb8(0x1a, 0x1a, 0x1a).into()),
            border: iced::border::rounded(12).color(COLOR_BORDER).width(1.0),
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

    let line_2 = container(Space::with_width(Length::Fixed(150.0)))
        .width(Length::Fixed(150.0))
        .height(Length::Fixed(11.0))
        .style(|_t| container::Style {
            background: Some(iced::Color::from_rgb8(0x1e, 0x1e, 0x1e).into()),
            border: iced::border::rounded(4),
            ..container::Style::default()
        });

    let details_row = row![
        avatar_circle,
        column![line_1, line_2].spacing(5).width(Length::Fill),
    ]
    .spacing(10)
    .align_y(Alignment::Start);

    column![thumb_box, details_row]
        .spacing(10)
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
    let chunk_size = if window_width < 700.0 {
        1
    } else if window_width < 1100.0 {
        2
    } else if window_width < 1650.0 {
        3
    } else if window_width < 2200.0 {
        4
    } else {
        5
    };

    let mut grid = column![].spacing(24).width(Length::Fill);

    if videos.is_empty() {
        // Brutally immediate skeleton grid while feed is fetching
        for _row_idx in 0..3 {
            let mut row_cards = row![].spacing(20).width(Length::Fill);
            for _col_idx in 0..chunk_size {
                row_cards = row_cards.push(view_skeleton_card());
            }
            grid = grid.push(row_cards);
        }
    } else {
        for chunk in videos.chunks(chunk_size) {
            let mut row_cards = row![].spacing(20).width(Length::Fill);
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
            let mut skeleton_row = row![].spacing(20).width(Length::Fill);
            for _ in 0..chunk_size {
                skeleton_row = skeleton_row.push(view_skeleton_card());
            }
            grid = grid.push(skeleton_row);
        }
    }

    container(
        scrollable(
            container(grid)
                .padding([20, 24])
                .width(Length::Fill),
        )
        .on_scroll(on_scroll)
        .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .clip(true)
    .style(main_container)
    .into()
}
