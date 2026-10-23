use crate::innertube::ChannelDetails;
use crate::ui::home_view::{view_skeleton_card, view_video_card};
use crate::ui::icons::{icon_check_circle, view_avatar_image_or_badge};
use crate::ui::theme::*;
use iced::widget::{button, column, container, image, row, scrollable, text, Space};
use iced::{Alignment, Element, Length};

#[allow(clippy::too_many_arguments)]
pub fn view_channel_page<'a, Message: Clone + 'static>(
    channel: &'a ChannelDetails,
    selected_tab_idx: usize,
    thumbnail_cache: &'a std::collections::HashMap<String, iced::widget::image::Handle>,
    blurred_cache: &'a std::collections::HashMap<String, iced::widget::image::Handle>,
    blur_amount: f32,
    window_width: f32,
    on_select_tab: impl Fn(usize) -> Message + Copy + 'static,
    on_toggle_subscribe: Message,
    on_open_video: impl Fn(String) -> Message + Copy + 'static,
    on_open_video_new_tab: impl Fn(String) -> Message + Copy + 'static,
    on_open_channel: impl Fn(String) -> Message + Copy + 'static,
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
    let blur = blur_amount.clamp(0.0, 1.0);

    // Top banner (real banner image when available, fallback to surface container)
    let banner_handle = channel
        .banner_url
        .as_ref()
        .and_then(|u| {
            if blur > 0.20 {
                blurred_cache.get(u).or_else(|| thumbnail_cache.get(u))
            } else {
                thumbnail_cache.get(u)
            }
        });
    let banner: Element<'a, Message> = if let Some(bh) = banner_handle {
        container(
            image(bh.clone())
                .width(Length::Fill)
                .height(Length::Fixed(160.0))
                .content_fit(iced::ContentFit::Cover),
        )
        .width(Length::Fill)
        .height(Length::Fixed(160.0))
        .into()
    } else {
        container(Space::with_width(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fixed(140.0))
            .style(|_t| container::Style {
                background: Some(COLOR_SURFACE.into()),
                ..container::Style::default()
            })
            .into()
    };

    // Channel header with real avatar or badge fallback
    let avatar_handle = if blur > 0.20 {
        blurred_cache.get(&channel.avatar_url).or_else(|| thumbnail_cache.get(&channel.avatar_url))
    } else {
        thumbnail_cache.get(&channel.avatar_url)
    };
    let avatar = view_avatar_image_or_badge(&channel.title, avatar_handle, 80.0, 28.0);

    let sub_btn = button(
        row![
            if channel.is_subscribed {
                icon_check_circle(14.0)
            } else {
                Space::with_width(Length::Fixed(0.0)).into()
            },
            text(if channel.is_subscribed { "Suscrito" } else { "Suscribirse" }).size(14),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    )
    .style(if channel.is_subscribed { secondary_button } else { primary_button })
    .padding([6, 16])
    .on_press(on_toggle_subscribe);

    let channel_info = column![
        text(if channel.title.is_empty() { "Canal" } else { &channel.title })
            .size(24)
            .color(COLOR_TEXT_PRIMARY),
        row![
            text(&channel.handle).size(13).color(COLOR_TEXT_SECONDARY),
            text(if channel.handle.is_empty() || channel.subscriber_count.is_empty() { "" } else { " | " })
                .size(13)
                .color(COLOR_TEXT_SECONDARY),
            text(&channel.subscriber_count).size(13).color(COLOR_TEXT_SECONDARY),
        ]
        .spacing(4),
        sub_btn,
    ]
    .spacing(6);

    let header_row = row![avatar, channel_info]
        .spacing(20)
        .align_y(Alignment::Center)
        .padding([16, 24]);

    // Tabs bar (Videos, En directo, Listas)
    let mut tabs_row = row![].spacing(16).padding([8, 24]);
    for (idx, tab) in channel.tabs.iter().enumerate() {
        let is_selected = idx == selected_tab_idx;
        let tab_btn = button(
            text(&tab.title)
                .size(15)
                .color(if is_selected { COLOR_TEXT_PRIMARY } else { COLOR_TEXT_SECONDARY }),
        )
        .style(if is_selected { secondary_button } else { ghost_button })
        .on_press(on_select_tab(idx));

        tabs_row = tabs_row.push(tab_btn);
    }

    // Video grid of selected tab
    let selected_videos = channel.tabs.get(selected_tab_idx).map(|t| &t.videos[..]).unwrap_or(&[]);

    let mut grid = column![].spacing(24).padding([16, 24]).width(Length::Fill);
    if selected_videos.is_empty() {
        for _ in 0..2 {
            let mut row_cards = row![].spacing(20).width(Length::Fill);
            for _ in 0..chunk_size {
                row_cards = row_cards.push(view_skeleton_card());
            }
            grid = grid.push(row_cards);
        }
    } else {
        for chunk in selected_videos.chunks(chunk_size) {
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
    }

    container(
        scrollable(
            column![banner, header_row, tabs_row, grid]
                .spacing(12)
                .width(Length::Fill),
        )
        .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(main_container)
    .into()
}
