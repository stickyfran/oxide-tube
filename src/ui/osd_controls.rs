use crate::config::{PreferredCodec, PreferredResolution};
use crate::innertube::{CaptionTrack, StreamQuality, VideoChapter};
use crate::ui::icons::*;
use crate::ui::theme::*;
use iced::widget::{button, column, container, mouse_area, row, slider, text, Space};
use iced::{Alignment, Element, Length};
use std::time::Instant;

use iced::mouse::Cursor;
use iced::widget::canvas::{self, Geometry, Path};
use iced::{Color, Point, Rectangle, Size};

#[derive(Debug, Clone)]
pub struct ProgressBarProgram {
    pub progress_ratio: f32,
    pub buffered_ratio: f32,
    pub hover_ratio: Option<f32>,
    pub chapter_starts: Vec<u64>,
    pub duration_secs: u64,
}

impl<Message> canvas::Program<Message> for ProgressBarProgram {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let is_hovering = self.hover_ratio.is_some();
        let bw = bounds.width * self.buffered_ratio.clamp(0.0, 1.0);
        let hw = self.hover_ratio.map(|hr| bounds.width * hr.clamp(0.0, 1.0)).unwrap_or(0.0);
        let pw = bounds.width * self.progress_ratio.clamp(0.0, 1.0);

        let mut starts: Vec<u64> = self
            .chapter_starts
            .iter()
            .copied()
            .filter(|&s| s < self.duration_secs)
            .collect();
        if !starts.is_empty() && starts[0] > 0 {
            starts.insert(0, 0);
        }

        if starts.len() >= 2 && self.duration_secs > 0 {
            let num_segs = starts.len();
            let dur_f = self.duration_secs as f32;
            let gap = 3.0_f32;

            for i in 0..num_segs {
                let r0 = (starts[i] as f32 / dur_f).clamp(0.0, 1.0);
                let r1 = starts
                    .get(i + 1)
                    .map(|&s| (s as f32 / dur_f).clamp(0.0, 1.0))
                    .unwrap_or(1.0);

                let x0 = if i == 0 {
                    0.0
                } else {
                    (bounds.width * r0 + gap * 0.5).min(bounds.width)
                };
                let x1 = if i + 1 == num_segs {
                    bounds.width
                } else {
                    (bounds.width * r1 - gap * 0.5).max(x0)
                };
                let seg_w = x1 - x0;
                if seg_w <= 0.5 {
                    continue;
                }

                let is_hovered_seg = self
                    .hover_ratio
                    .map(|hr| hr >= r0 && (hr < r1 || i + 1 == num_segs))
                    .unwrap_or(false);

                let h = if is_hovered_seg {
                    8.0
                } else if is_hovering {
                    5.2
                } else {
                    4.0
                };
                let y = (bounds.height - h) / 2.0;
                let rad = (h / 2.0).min(2.5);

                // 1. Segment background rail
                let bg_alpha = if is_hovered_seg { 0.35 } else { 0.22 };
                let bg_path = Path::rounded_rectangle(Point::new(x0, y), Size::new(seg_w, h), rad.into());
                frame.fill(&bg_path, Color::from_rgba(1.0, 1.0, 1.0, bg_alpha));

                // 2. Segment buffered rail
                if bw > x0 {
                    let seg_bw = (bw.min(x1) - x0).max(0.0);
                    if seg_bw > 0.0 {
                        let buf_path = Path::rounded_rectangle(Point::new(x0, y), Size::new(seg_bw, h), rad.into());
                        frame.fill(&buf_path, Color::from_rgba(1.0, 1.0, 1.0, 0.50));
                    }
                }

                // 3. Segment hover rail
                if hw > x0 {
                    let seg_hw = (hw.min(x1) - x0).max(0.0);
                    if seg_hw > 0.0 {
                        let hov_path = Path::rounded_rectangle(Point::new(x0, y), Size::new(seg_hw, h), rad.into());
                        frame.fill(&hov_path, Color::from_rgba(1.0, 1.0, 1.0, 0.38));
                    }
                }

                // 4. Segment played progress rail
                if pw > x0 {
                    let seg_pw = (pw.min(x1) - x0).max(0.0);
                    if seg_pw > 0.0 {
                        let prog_path = Path::rounded_rectangle(Point::new(x0, y), Size::new(seg_pw, h), rad.into());
                        frame.fill(&prog_path, Color::from_rgb8(0xff, 0x00, 0x00));
                    }
                }
            }
        } else {
            let h = if is_hovering { 6.0 } else { 4.0 };
            let y = (bounds.height - h) / 2.0;

            // 1. Background rail (Translucent white)
            let bg_path = Path::rounded_rectangle(Point::new(0.0, y), Size::new(bounds.width, h), 2.0.into());
            frame.fill(&bg_path, Color::from_rgba(1.0, 1.0, 1.0, 0.22));

            // 2. Buffered rail
            if bw > 0.0 {
                let buf_path = Path::rounded_rectangle(Point::new(0.0, y), Size::new(bw, h), 2.0.into());
                frame.fill(&buf_path, Color::from_rgba(1.0, 1.0, 1.0, 0.50));
            }

            // 3. Hover rail
            if hw > 0.0 {
                let hover_path = Path::rounded_rectangle(Point::new(0.0, y), Size::new(hw, h), 2.0.into());
                frame.fill(&hover_path, Color::from_rgba(1.0, 1.0, 1.0, 0.35));
            }

            // 4. Progress rail (YouTube Vibrant Red #ff0000)
            if pw > 0.0 {
                let prog_path = Path::rounded_rectangle(Point::new(0.0, y), Size::new(pw, h), 2.0.into());
                frame.fill(&prog_path, Color::from_rgb8(0xff, 0x00, 0x00));
            }
        }

        // 5. Handle (Red circle indicator)
        let radius = if is_hovering { 7.0 } else { 5.5 };
        let circle_center = Point::new(pw.clamp(radius, (bounds.width - radius).max(radius)), bounds.height / 2.0);
        let circle_path = Path::circle(circle_center, radius);
        frame.fill(&circle_path, Color::from_rgb8(0xff, 0x00, 0x00));

        vec![frame.into_geometry()]
    }
}

#[derive(Debug, Clone)]
pub struct SeekHoverTooltipProgram {
    pub hover_ratio: f32,
    pub time_label: String,
    pub chapter_title: Option<String>,
}

impl<Message> canvas::Program<Message> for SeekHoverTooltipProgram {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry> {
        if bounds.width < 10.0 {
            return Vec::new();
        }
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let cursor_x = (bounds.width * self.hover_ratio.clamp(0.0, 1.0))
            .clamp(6.0, (bounds.width - 6.0).max(6.0));

        if let Some(ref raw_title) = self.chapter_title {
            let title_str = if raw_title.chars().count() > 46 {
                let mut s: String = raw_title.chars().take(43).collect();
                s.push_str("...");
                s
            } else {
                raw_title.clone()
            };

            let char_len = title_str.chars().count().max(self.time_label.len() + 4);
            let pill_w = ((char_len as f32) * 7.2 + 28.0)
                .clamp(92.0, (bounds.width - 8.0).max(92.0));
            let pill_h = 38.0_f32;
            let half_w = pill_w / 2.0;
            let center_x = cursor_x.clamp(
                half_w + 4.0,
                (bounds.width - half_w - 4.0).max(half_w + 4.0),
            );
            let box_x = center_x - half_w;
            let box_y = (bounds.height - pill_h - 5.0).max(0.0);

            let pill_path = Path::rounded_rectangle(
                Point::new(box_x, box_y),
                Size::new(pill_w, pill_h),
                7.0.into(),
            );
            frame.fill(&pill_path, Color::from_rgba(0.05, 0.05, 0.07, 0.94));
            frame.stroke(
                &pill_path,
                canvas::Stroke::default()
                    .with_color(Color::from_rgba(1.0, 1.0, 1.0, 0.20))
                    .with_width(1.0),
            );

            // Small downward pointer triangle at cursor_x
            let tip_x = cursor_x.clamp(box_x + 10.0, box_x + pill_w - 10.0);
            let mut ptr = canvas::path::Builder::new();
            ptr.move_to(Point::new(tip_x - 5.0, box_y + pill_h));
            ptr.line_to(Point::new(tip_x + 5.0, box_y + pill_h));
            ptr.line_to(Point::new(tip_x, box_y + pill_h + 5.0));
            ptr.close();
            frame.fill(&ptr.build(), Color::from_rgba(0.05, 0.05, 0.07, 0.94));

            // Chapter title (top line)
            frame.fill_text(canvas::Text {
                content: title_str,
                position: Point::new(center_x, box_y + 12.0),
                color: Color::from_rgb8(0xf8, 0xf8, 0xf8),
                size: 12.5.into(),
                horizontal_alignment: iced::alignment::Horizontal::Center,
                vertical_alignment: iced::alignment::Vertical::Center,
                font: iced::Font::DEFAULT,
                ..Default::default()
            });

            // Timestamp (bottom line)
            frame.fill_text(canvas::Text {
                content: self.time_label.clone(),
                position: Point::new(center_x, box_y + 28.0),
                color: Color::from_rgb8(0xff, 0x4d, 0x4d),
                size: 11.5.into(),
                horizontal_alignment: iced::alignment::Horizontal::Center,
                vertical_alignment: iced::alignment::Vertical::Center,
                font: iced::Font::DEFAULT,
                ..Default::default()
            });
        } else {
            let pill_w = ((self.time_label.len() as f32) * 7.5 + 20.0).max(54.0);
            let pill_h = 22.0_f32;
            let half_w = pill_w / 2.0;
            let center_x = cursor_x.clamp(
                half_w + 4.0,
                (bounds.width - half_w - 4.0).max(half_w + 4.0),
            );
            let box_x = center_x - half_w;
            let box_y = (bounds.height - pill_h - 5.0).max(0.0);

            let pill_path = Path::rounded_rectangle(
                Point::new(box_x, box_y),
                Size::new(pill_w, pill_h),
                5.0.into(),
            );
            frame.fill(&pill_path, Color::from_rgba(0.05, 0.05, 0.07, 0.94));
            frame.stroke(
                &pill_path,
                canvas::Stroke::default()
                    .with_color(Color::from_rgba(1.0, 1.0, 1.0, 0.20))
                    .with_width(1.0),
            );

            let tip_x = cursor_x.clamp(box_x + 8.0, box_x + pill_w - 8.0);
            let mut ptr = canvas::path::Builder::new();
            ptr.move_to(Point::new(tip_x - 4.5, box_y + pill_h));
            ptr.line_to(Point::new(tip_x + 4.5, box_y + pill_h));
            ptr.line_to(Point::new(tip_x, box_y + pill_h + 4.5));
            ptr.close();
            frame.fill(&ptr.build(), Color::from_rgba(0.05, 0.05, 0.07, 0.94));

            frame.fill_text(canvas::Text {
                content: self.time_label.clone(),
                position: Point::new(center_x, box_y + 11.0),
                color: Color::from_rgb8(0xf8, 0xf8, 0xf8),
                size: 12.0.into(),
                horizontal_alignment: iced::alignment::Horizontal::Center,
                vertical_alignment: iced::alignment::Vertical::Center,
                font: iced::Font::DEFAULT,
                ..Default::default()
            });
        }

        vec![frame.into_geometry()]
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PlayPauseRippleProgram {
    pub start_time: Instant,
    pub is_playing: bool,
}

impl<Message> canvas::Program<Message> for PlayPauseRippleProgram {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry> {
        let elapsed = self.start_time.elapsed().as_secs_f32();
        let duration = 0.50_f32;
        if elapsed >= duration {
            return Vec::new();
        }

        let progress = (elapsed / duration).clamp(0.0, 1.0);
        // Ease-out scale from 1.0 to 1.38
        let scale = 1.0 + 0.38 * (1.0 - (1.0 - progress).powi(2));
        // Opacity stays strong for first 35%, then fades out smoothly to 0
        let alpha = if progress < 0.35 {
            0.92
        } else {
            0.92 * (1.0 - (progress - 0.35) / 0.65).powi(2)
        };

        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let center = Point::new(bounds.width / 2.0, bounds.height / 2.0);

        // 1. YouTube-style dark translucent circular background badge
        let radius = 36.0 * scale;
        let circle = Path::circle(center, radius);
        frame.fill(&circle, Color::from_rgba(0.0, 0.0, 0.0, 0.62 * alpha));

        // 2. Play triangle or Pause bars (pure white with matching alpha)
        let icon_color = Color::from_rgba(1.0, 1.0, 1.0, alpha);
        if self.is_playing {
            // Play triangle pointing right, centered at `center`
            let hw = 12.0 * scale;
            let hh = 15.0 * scale;
            let p1 = Point::new(center.x - hw * 0.70, center.y - hh);
            let p2 = Point::new(center.x + hw * 1.30, center.y);
            let p3 = Point::new(center.x - hw * 0.70, center.y + hh);
            let mut builder = canvas::path::Builder::new();
            builder.move_to(p1);
            builder.line_to(p2);
            builder.line_to(p3);
            builder.close();
            frame.fill(&builder.build(), icon_color);
        } else {
            // Pause: two vertical rounded bars
            let bar_w = 6.0 * scale;
            let bar_h = 24.0 * scale;
            let spacing = 7.0 * scale;
            let left_x = center.x - spacing / 2.0 - bar_w;
            let right_x = center.x + spacing / 2.0;
            let top_y = center.y - bar_h / 2.0;

            let left_bar = Path::rounded_rectangle(
                Point::new(left_x, top_y),
                Size::new(bar_w, bar_h),
                2.0.into(),
            );
            let right_bar = Path::rounded_rectangle(
                Point::new(right_x, top_y),
                Size::new(bar_w, bar_h),
                2.0.into(),
            );
            frame.fill(&left_bar, icon_color);
            frame.fill(&right_bar, icon_color);
        }

        vec![frame.into_geometry()]
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SeekFeedbackAnim {
    #[allow(dead_code)]
    pub start_time: Instant,
    pub last_tap_time: Instant,
    pub total_seconds: i64,
    #[allow(dead_code)]
    pub tap_count: u32,
    pub is_forward: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct SeekFeedbackProgram {
    pub anim: SeekFeedbackAnim,
}

impl<Message> canvas::Program<Message> for SeekFeedbackProgram {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry> {
        let elapsed_tap = self.anim.last_tap_time.elapsed().as_secs_f32();
        let duration = 0.65_f32;
        if elapsed_tap >= duration {
            return Vec::new();
        }

        // Opacity remains full for first 350ms, then fades smoothly to 0
        let alpha = if elapsed_tap < 0.35 {
            0.92
        } else {
            0.92 * (1.0 - (elapsed_tap - 0.35) / 0.30).powi(2)
        };

        // Scale bounce on each tap (1.18 -> 1.0)
        let tap_t = (elapsed_tap / 0.18).clamp(0.0, 1.0);
        let scale = 1.0 + 0.18 * (1.0 - tap_t);

        let mut frame = canvas::Frame::new(renderer, bounds.size());

        let center_x = if self.anim.is_forward {
            bounds.width * 0.75
        } else {
            bounds.width * 0.25
        };
        let center = Point::new(center_x, bounds.height * 0.50);

        // 1. YouTube-style translucent rounded capsule badge
        let badge_w = 110.0 * scale;
        let badge_h = 76.0 * scale;
        let bg_rect = Path::rounded_rectangle(
            Point::new(center.x - badge_w / 2.0, center.y - badge_h / 2.0),
            Size::new(badge_w, badge_h),
            38.0.into(),
        );
        frame.fill(&bg_rect, Color::from_rgba(0.0, 0.0, 0.0, 0.65 * alpha));

        // 2. Three animated chevrons rippling sequentially in direction of travel
        let anim_cycle = (elapsed_tap * 4.0).fract();
        let chevron_y = center.y - 10.0 * scale;
        let spacing = 14.0 * scale;

        for i in 0..3 {
            let wave_idx = if self.anim.is_forward { i } else { 2 - i };
            let wave_phase = (anim_cycle - (wave_idx as f32) * 0.22).fract();
            let wave_bright = if wave_phase >= 0.0 && wave_phase <= 0.45 {
                1.0 - (wave_phase / 0.45) * 0.55
            } else {
                0.35
            };
            let c_color = Color::from_rgba(1.0, 1.0, 1.0, alpha * wave_bright);

            let offset_x = ((i as f32) - 1.0) * spacing;
            let cx = center.x + offset_x;

            let mut builder = canvas::path::Builder::new();
            if self.anim.is_forward {
                builder.move_to(Point::new(cx - 4.5 * scale, chevron_y - 7.5 * scale));
                builder.line_to(Point::new(cx + 4.5 * scale, chevron_y));
                builder.line_to(Point::new(cx - 4.5 * scale, chevron_y + 7.5 * scale));
            } else {
                builder.move_to(Point::new(cx + 4.5 * scale, chevron_y - 7.5 * scale));
                builder.line_to(Point::new(cx - 4.5 * scale, chevron_y));
                builder.line_to(Point::new(cx + 4.5 * scale, chevron_y + 7.5 * scale));
            }
            frame.stroke(
                &builder.build(),
                canvas::Stroke::default()
                    .with_color(c_color)
                    .with_width(2.6 * scale)
                    .with_line_cap(canvas::LineCap::Round)
                    .with_line_join(canvas::LineJoin::Round),
            );
        }

        // 3. Seconds indicator label: e.g. "+10 segundos" or "-15 segundos"
        let sign = if self.anim.is_forward { "+" } else { "-" };
        let text_content = format!("{sign}{} segundos", self.anim.total_seconds);
        frame.fill_text(canvas::Text {
            content: text_content,
            position: Point::new(center.x, center.y + 16.0 * scale),
            color: Color::from_rgba(1.0, 1.0, 1.0, alpha * 0.95),
            size: (12.0 * scale).into(),
            horizontal_alignment: iced::alignment::Horizontal::Center,
            vertical_alignment: iced::alignment::Vertical::Center,
            font: iced::Font::DEFAULT,
            ..Default::default()
        });

        vec![frame.into_geometry()]
    }
}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct BufferSpinnerProgram {
    pub buffering_percent: u8,
    pub is_buffering: bool,
}

impl<Message> canvas::Program<Message> for BufferSpinnerProgram {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: Cursor,
    ) -> Vec<Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let center = Point::new(bounds.width / 2.0, bounds.height / 2.0);
        let radius = (bounds.width.min(bounds.height) / 2.0 - 8.0).max(12.0);

        static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        let elapsed = START.get_or_init(std::time::Instant::now).elapsed().as_secs_f32();
        let rotation = (elapsed * 5.2) % (std::f32::consts::TAU);

        // Circular background pill with slight translucency
        let bg = Path::circle(center, radius + 8.0);
        frame.fill(&bg, Color::from_rgba(0.0, 0.0, 0.0, 0.65));

        // Background track (subtle light circle)
        let track = Path::circle(center, radius);
        frame.stroke(
            &track,
            canvas::Stroke::default()
                .with_width(3.2)
                .with_color(Color::from_rgba(1.0, 1.0, 1.0, 0.15)),
        );

        // Rotating red arc (YouTube Red)
        let arc_len = std::f32::consts::PI * 0.75; // ~135 degrees arc
        let start_angle = rotation;
        let end_angle = start_angle + arc_len;

        let arc_path = Path::new(|builder| {
            builder.arc(canvas::path::Arc {
                center,
                radius,
                start_angle: iced::Radians(start_angle),
                end_angle: iced::Radians(end_angle),
            });
        });

        frame.stroke(
            &arc_path,
            canvas::Stroke::default()
                .with_width(3.2)
                .with_color(COLOR_ACCENT_RED)
                .with_line_cap(canvas::LineCap::Round),
        );

        vec![frame.into_geometry()]
    }
}

pub fn format_duration(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes}:{secs:02}")
    }
}

#[derive(Debug, Clone)]
pub struct OsdState {
    pub last_mouse_movement: Instant,
    pub last_click_time: Option<Instant>,
    pub is_visible: bool,
    pub show_qualities: bool,
    pub show_audio_menu: bool,
    pub show_subtitles_menu: bool,
    pub show_speed_menu: bool,
    pub current_subtitle_text: String,
    pub current_pos_secs: u64,
    pub buffered_pos_secs: u64,
    pub duration_secs: u64,
    pub is_playing: bool,
    pub volume: f32,
    pub selected_quality_itag: u32,
    pub selected_quality_label: String,
    pub selected_audio_itag: u32,
    pub selected_audio_track_id: String,
    pub selected_audio_label: String,
    pub selected_caption_lang: Option<String>,
    pub playback_rate: f64,
    pub first_frame_rendered: bool,
    pub is_buffering: bool,
    pub buffering_percent: u8,
    pub hover_seek_ratio: Option<f32>,
    pub play_pause_anim: Option<(Instant, bool)>,
    pub seek_anim: Option<SeekFeedbackAnim>,
    pub preferred_codec: PreferredCodec,
    pub preferred_resolution: PreferredResolution,
    pub set_as_default_quality: bool,
    pub has_playback_error: bool,
}

impl Default for OsdState {
    fn default() -> Self {
        Self {
            last_mouse_movement: Instant::now(),
            last_click_time: None,
            is_visible: true,
            show_qualities: false,
            show_audio_menu: false,
            show_subtitles_menu: false,
            show_speed_menu: false,
            current_subtitle_text: String::new(),
            current_pos_secs: 0,
            buffered_pos_secs: 0,
            duration_secs: 0,
            is_playing: true,
            volume: 1.0,
            selected_quality_itag: 0,
            selected_quality_label: "1080p".to_string(),
            selected_audio_itag: 251,
            selected_audio_track_id: "default".to_string(),
            selected_audio_label: "Original (Opus)".to_string(),
            selected_caption_lang: None,
            playback_rate: 1.0,
            first_frame_rendered: false,
            is_buffering: false,
            buffering_percent: 100,
            hover_seek_ratio: None,
            play_pause_anim: None,
            seek_anim: None,
            preferred_codec: PreferredCodec::Av1,
            preferred_resolution: PreferredResolution::Res1080p,
            set_as_default_quality: true,
            has_playback_error: false,
        }
    }
}

impl OsdState {
    pub fn trigger_seek_feedback(&mut self, delta_secs: i64) {
        let is_forward = delta_secs > 0;
        let delta_abs = delta_secs.abs();
        let now = Instant::now();
        if let Some(ref mut anim) = self.seek_anim {
            if anim.last_tap_time.elapsed().as_millis() <= 750 && anim.is_forward == is_forward {
                anim.last_tap_time = now;
                anim.total_seconds += delta_abs;
                anim.tap_count += 1;
                return;
            }
        }
        self.seek_anim = Some(SeekFeedbackAnim {
            start_time: now,
            last_tap_time: now,
            total_seconds: delta_abs,
            tap_count: 1,
            is_forward,
        });
    }

    pub fn on_mouse_move(&mut self) {
        self.last_mouse_movement = Instant::now();
        self.is_visible = true;
    }

    pub fn has_open_menu(&self) -> bool {
        self.show_qualities || self.show_audio_menu || self.show_subtitles_menu || self.show_speed_menu
    }

    pub fn close_all_menus(&mut self) -> bool {
        let was_open = self.has_open_menu();
        self.show_qualities = false;
        self.show_audio_menu = false;
        self.show_subtitles_menu = false;
        self.show_speed_menu = false;
        was_open
    }

    #[allow(dead_code)]
    pub fn fullscreen_control_bar_height(&self) -> f32 {
        if !self.is_visible {
            return 0.0;
        }
        let mut h = 72.0;
        if !self.current_subtitle_text.is_empty() {
            h += 38.0;
        }
        if self.has_open_menu() {
            h += 248.0;
        }
        h
    }

    pub fn tick_visibility(&mut self) {
        if !self.is_playing || self.has_open_menu() {
            self.is_visible = true;
            return;
        }
        if self.last_mouse_movement.elapsed().as_millis() > 2500 {
            self.is_visible = false;
        }
    }
}

fn friendly_codec_name(codecs: &str) -> &'static str {
    if codecs.starts_with("av01") {
        "AV1"
    } else if codecs.starts_with("vp9") || codecs.starts_with("vp09") {
        "VP9"
    } else if codecs.starts_with("avc1") {
        "H264"
    } else if codecs.contains("opus") {
        "Opus"
    } else if codecs.starts_with("mp4a") {
        "AAC"
    } else {
        "Auto"
    }
}

pub fn view_video_hitbox_and_popups<'a, Message: Clone + 'static>(
    on_play_pause: Message,
    hide_cursor: bool,
) -> Element<'a, Message> {
    mouse_area(
        container(Space::with_width(Length::Fill).height(Length::Fill))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(transparent_player_container),
    )
    .on_press(on_play_pause)
    .interaction(if hide_cursor {
        iced::mouse::Interaction::None
    } else {
        iced::mouse::Interaction::Idle
    })
    .into()
}

pub fn view_floating_dropdown_menu<'a, Message: Clone + 'static>(
    state: &'a OsdState,
    video_qualities: &'a [StreamQuality],
    audio_tracks: &'a [StreamQuality],
    caption_tracks: &'a [CaptionTrack],
    on_close_menus: Message,
    on_select_quality_itag: impl Fn(u32) -> Message + 'static,
    on_toggle_save_default_quality: Message,
    on_set_preferred_codec: impl Fn(PreferredCodec) -> Message + 'static,
    on_select_audio_track: impl Fn(usize) -> Message + 'static,
    on_select_speed: impl Fn(f64) -> Message + 'static,
    on_select_caption_track: impl Fn(Option<usize>) -> Message + 'static,
) -> Option<Element<'a, Message>> {
    if state.show_qualities {
        let codecs = [
            (PreferredCodec::Av1, "AV1"),
            (PreferredCodec::Vp9, "VP9"),
            (PreferredCodec::H264, "H264"),
        ];
        let mut codec_row = row![
            text("Códec def:")
                .size(11.5)
                .color(COLOR_TEXT_SECONDARY)
        ]
        .spacing(4)
        .align_y(Alignment::Center);

        for (c_enum, c_lbl) in codecs {
            let is_c = state.preferred_codec == c_enum;
            let c_btn = button(
                row![
                    if is_c {
                        icon_check_circle(10.0)
                    } else {
                        Space::with_width(Length::Fixed(0.0)).into()
                    },
                    text(c_lbl)
                        .size(11)
                        .color(if is_c { COLOR_ACCENT_RED } else { COLOR_TEXT_PRIMARY }),
                ]
                .spacing(4)
                .align_y(Alignment::Center),
            )
            .style(if is_c { secondary_button } else { ghost_button })
            .padding([3, 7])
            .on_press(on_set_preferred_codec(c_enum));
            codec_row = codec_row.push(c_btn);
        }

        let mut list = column![
            text("Calidad y Códec (Hardware Xe)")
                .size(13)
                .color(COLOR_TEXT_SECONDARY),
            codec_row,
            container(Space::with_height(Length::Fixed(1.0)))
                .width(Length::Fill)
                .style(|_t| container::Style {
                    background: Some(COLOR_BORDER.into()),
                    ..container::Style::default()
                }),
        ]
        .spacing(5)
        .width(Length::Fixed(270.0));

        for q in video_qualities.iter().filter(|q| !q.quality_label.is_empty()) {
            let codec_tag = friendly_codec_name(&q.codecs);
            let is_selected = if state.selected_quality_itag != 0 {
                q.itag == state.selected_quality_itag
            } else {
                q.quality_label == state.selected_quality_label
            };
            let is_default_res = q.height == state.preferred_resolution.max_height();

            // Radio circle button to toggle/set this resolution as default
            let radio_circle: Element<'a, Message> = if is_default_res {
                container(
                    container(Space::new(Length::Fixed(6.0), Length::Fixed(6.0)))
                        .style(|_t| container::Style {
                            background: Some(COLOR_ACCENT_RED.into()),
                            border: iced::border::rounded(3),
                            ..container::Style::default()
                        }),
                )
                .width(Length::Fixed(14.0))
                .height(Length::Fixed(14.0))
                .align_x(iced::alignment::Horizontal::Center)
                .align_y(iced::alignment::Vertical::Center)
                .style(|_t| container::Style {
                    border: iced::border::rounded(7).color(COLOR_ACCENT_RED).width(1.5),
                    ..container::Style::default()
                })
                .into()
            } else {
                container(Space::new(Length::Fixed(14.0), Length::Fixed(14.0)))
                    .style(|_t| container::Style {
                        border: iced::border::rounded(7).color(COLOR_TEXT_SECONDARY).width(1.2),
                        ..container::Style::default()
                    })
                    .into()
            };

            let default_radio_btn = button(radio_circle)
                .style(ghost_button)
                .padding([2, 4])
                .on_press(on_toggle_save_default_quality.clone());

            let mut item_row = row![
                default_radio_btn,
                if is_selected {
                    icon_check_circle(12.0)
                } else {
                    Space::with_width(Length::Fixed(12.0)).into()
                },
                text(format!("{}  ·  {}", &q.quality_label, codec_tag))
                    .size(13)
                    .color(if is_selected { COLOR_ACCENT_RED } else { COLOR_TEXT_PRIMARY }),
                Space::with_width(Length::Fill),
            ]
            .spacing(6)
            .align_y(Alignment::Center);

            if is_default_res {
                item_row = item_row.push(
                    text("Default")
                        .size(10.5)
                        .color(COLOR_TEXT_SECONDARY),
                );
            }

            let btn = button(item_row)
                .style(if is_selected { secondary_button } else { ghost_button })
                .padding([6, 8])
                .width(Length::Fill)
                .on_press(on_select_quality_itag(q.itag));
            list = list.push(btn);
        }

        let card = mouse_area(
            container(
                iced::widget::scrollable(list).height(Length::Shrink),
            )
            .max_height(265.0)
            .padding([10, 12])
            .style(osd_container),
        );

        return Some(
            row![
                mouse_area(Space::with_width(Length::Fill))
                    .on_press(on_close_menus.clone()),
                card,
            ]
            .width(Length::Fill)
            .padding([4, 8])
            .into(),
        );
    }

    if state.show_audio_menu {
        let mut list = column![
            text("Pista de Audio / Idioma")
                .size(13)
                .color(COLOR_TEXT_SECONDARY)
        ]
        .spacing(4)
        .width(Length::Fixed(240.0));

        if audio_tracks.is_empty() {
            list = list.push(
                text("Pista única (Original)")
                    .size(13)
                    .color(COLOR_TEXT_PRIMARY),
            );
        } else {
            for (idx, a) in audio_tracks.iter().enumerate() {
                let is_selected =
                    a.itag == state.selected_audio_itag && a.audio_track_id == state.selected_audio_track_id;
                let btn = button(
                    row![
                        if is_selected {
                            icon_check_circle(12.0)
                        } else {
                            Space::with_width(Length::Fixed(12.0)).into()
                        },
                        text(&a.audio_track_name)
                            .size(13)
                            .color(if is_selected { COLOR_ACCENT_RED } else { COLOR_TEXT_PRIMARY }),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .style(if is_selected { secondary_button } else { ghost_button })
                .padding([6, 10])
                .width(Length::Fill)
                .on_press(on_select_audio_track(idx));
                list = list.push(btn);
            }
        }

        let card = mouse_area(
            container(
                iced::widget::scrollable(list).height(Length::Shrink),
            )
            .max_height(225.0)
            .padding([10, 12])
            .style(osd_container),
        );

        return Some(
            row![
                mouse_area(Space::with_width(Length::Fill))
                    .on_press(on_close_menus.clone()),
                card,
            ]
            .width(Length::Fill)
            .padding([4, 8])
            .into(),
        );
    }

    if state.show_speed_menu {
        let speeds: &[(f64, &str)] = &[
            (0.25, "0.25x"),
            (0.5, "0.5x"),
            (0.75, "0.75x"),
            (1.0, "Normal (1x)"),
            (1.25, "1.25x"),
            (1.5, "1.5x"),
            (1.75, "1.75x"),
            (2.0, "2.0x"),
        ];
        let mut list = column![
            text("Velocidad de reproducción")
                .size(13)
                .color(COLOR_TEXT_SECONDARY)
        ]
        .spacing(4)
        .width(Length::Fixed(210.0));

        for &(r, lbl) in speeds {
            let is_selected = (state.playback_rate - r).abs() < 0.05;
            let btn = button(
                row![
                    if is_selected {
                        icon_check_circle(12.0)
                    } else {
                        Space::with_width(Length::Fixed(12.0)).into()
                    },
                    text(lbl)
                        .size(13)
                        .color(if is_selected { COLOR_ACCENT_RED } else { COLOR_TEXT_PRIMARY }),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .style(if is_selected { secondary_button } else { ghost_button })
            .padding([6, 10])
            .width(Length::Fill)
            .on_press(on_select_speed(r));
            list = list.push(btn);
        }

        let card = mouse_area(
            container(
                iced::widget::scrollable(list).height(Length::Shrink),
            )
            .max_height(225.0)
            .padding([10, 12])
            .style(osd_container),
        );

        return Some(
            row![
                mouse_area(Space::with_width(Length::Fill))
                    .on_press(on_close_menus.clone()),
                card,
            ]
            .width(Length::Fill)
            .padding([4, 8])
            .into(),
        );
    }

    if state.show_subtitles_menu {
        let mut list = column![
            text("Subtítulos / CC")
                .size(13)
                .color(COLOR_TEXT_SECONDARY)
        ]
        .spacing(4)
        .width(Length::Fixed(250.0));

        let off_selected = state.selected_caption_lang.is_none();
        let off_btn = button(
            row![
                if off_selected {
                    icon_check_circle(12.0)
                } else {
                    Space::with_width(Length::Fixed(12.0)).into()
                },
                text("Desactivados")
                    .size(13)
                    .color(if off_selected { COLOR_ACCENT_RED } else { COLOR_TEXT_PRIMARY }),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        )
        .style(if off_selected { secondary_button } else { ghost_button })
        .padding([6, 10])
        .width(Length::Fill)
        .on_press(on_select_caption_track(None));
        list = list.push(off_btn);

        if caption_tracks.is_empty() {
            list = list.push(
                text("No disponibles en este video")
                    .size(12)
                    .color(COLOR_TEXT_SECONDARY),
            );
        } else {
            let mut ordered_indices: Vec<usize> = (0..caption_tracks.len()).collect();
            ordered_indices.sort_by_key(|&i| {
                let code = caption_tracks[i].language_code.as_str();
                if code.starts_with("es") {
                    0
                } else if code.starts_with("en") {
                    1
                } else {
                    2
                }
            });

            for idx in ordered_indices {
                let track = &caption_tracks[idx];
                let is_selected = state.selected_caption_lang.as_deref() == Some(track.name.as_str());
                let btn = button(
                    row![
                        if is_selected {
                            icon_check_circle(12.0)
                        } else {
                            Space::with_width(Length::Fixed(12.0)).into()
                        },
                        text(&track.name)
                            .size(13)
                            .color(if is_selected { COLOR_ACCENT_RED } else { COLOR_TEXT_PRIMARY }),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .style(if is_selected { secondary_button } else { ghost_button })
                .padding([6, 10])
                .width(Length::Fill)
                .on_press(on_select_caption_track(Some(idx)));
                list = list.push(btn);
            }
        }

        let card = mouse_area(
            container(
                iced::widget::scrollable(list).height(Length::Shrink),
            )
            .max_height(225.0)
            .padding([10, 12])
            .style(osd_container),
        );

        return Some(
            row![
                mouse_area(Space::with_width(Length::Fill))
                    .on_press(on_close_menus.clone()),
                card,
            ]
            .width(Length::Fill)
            .padding([4, 8])
            .into(),
        );
    }

    None
}

#[allow(clippy::too_many_arguments)]
pub fn view_control_bar<'a, Message: Clone + 'static>(
    state: &'a OsdState,
    video_qualities: &'a [StreamQuality],
    audio_tracks: &'a [StreamQuality],
    caption_tracks: &'a [CaptionTrack],
    chapters: &'a [VideoChapter],
    _theater_mode: bool,
    is_fullscreen: bool,
    on_close_menus: Message,
    on_play_pause: Message,
    on_seek: impl Fn(u64) -> Message + 'static,
    on_volume: impl Fn(f32) -> Message + 'static,
    on_toggle_mute: Message,
    on_toggle_quality_menu: Message,
    on_select_quality_itag: impl Fn(u32) -> Message + 'static,
    on_toggle_save_default_quality: Message,
    on_set_preferred_codec: impl Fn(PreferredCodec) -> Message + 'static,
    on_toggle_audio_menu: Message,
    on_select_audio_track: impl Fn(usize) -> Message + 'static,
    on_toggle_speed_menu: Message,
    on_select_speed: impl Fn(f64) -> Message + 'static,
    on_toggle_captions: Message,
    on_select_caption_track: impl Fn(Option<usize>) -> Message + 'static,
    on_toggle_pip: Message,
    on_toggle_theater: Message,
    on_toggle_fullscreen: Message,
) -> Element<'a, Message> {
    let effective_dur = if state.duration_secs > 0 {
        state.duration_secs
    } else {
        video_qualities
            .first()
            .map(|s| s.approx_duration_ms / 1000)
            .unwrap_or(0)
    };

    let time_str = if let Some(ratio) = state.hover_seek_ratio {
        let hover_secs = ((ratio.clamp(0.0, 1.0)) * (effective_dur.max(1) as f32)) as u64;
        format!(
            "{} ({}) / {}",
            format_duration(state.current_pos_secs),
            format_duration(hover_secs),
            format_duration(effective_dur)
        )
    } else {
        format!(
            "{} / {}",
            format_duration(state.current_pos_secs),
            format_duration(effective_dur)
        )
    };

    let progress_ratio = if effective_dur > 0 {
        (state.current_pos_secs as f32 / effective_dur as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let buffered_ratio = if effective_dur > 0 {
        (state.buffered_pos_secs as f32 / effective_dur as f32).clamp(progress_ratio, 1.0)
    } else {
        0.0
    };

    let target_secs = if let Some(ratio) = state.hover_seek_ratio {
        ((ratio.clamp(0.0, 1.0)) * (effective_dur.max(1) as f32)) as u64
    } else {
        state.current_pos_secs
    };

    let chapter_starts: Vec<u64> = chapters.iter().map(|c| c.start_secs).collect();

    let pb_canvas = iced::widget::canvas(ProgressBarProgram {
        progress_ratio,
        buffered_ratio,
        hover_ratio: state.hover_seek_ratio,
        chapter_starts,
        duration_secs: effective_dur,
    })
    .width(Length::Fill)
    .height(Length::Fixed(16.0));

    let interactive_seek_bar: Element<'a, Message> = mouse_area(
        container(pb_canvas)
            .width(Length::Fill)
            .padding([2, 0]),
    )
    .interaction(iced::mouse::Interaction::Pointer)
    .on_press(on_seek(target_secs))
    .into();

    let play_icon_el = if state.is_playing {
        icon_pause(18.0)
    } else {
        icon_play(18.0)
    };

    let play_btn = button(play_icon_el)
        .style(osd_control_button)
        .padding([6, 10])
        .on_press(on_play_pause);

    let vol_icon_el = if state.volume < 0.001 {
        icon_volume_mute(18.0)
    } else if state.volume < 0.5 {
        icon_volume_low(18.0)
    } else {
        icon_volume(18.0)
    };

    let mute_btn = button(vol_icon_el)
        .style(osd_control_button)
        .padding([6, 8])
        .on_press(on_toggle_mute);

    let vol_slider = slider(0.0_f32..=1.0_f32, state.volume, on_volume)
        .step(0.05_f32)
        .width(72);

    let speed_label = if (state.playback_rate - 1.0).abs() < 0.05 {
        "1x".to_string()
    } else {
        format!("{}x", state.playback_rate)
    };

    let speed_btn = button(
        text(speed_label)
            .size(13)
            .color(if state.show_speed_menu { COLOR_ACCENT_RED } else { COLOR_TEXT_PRIMARY }),
    )
    .style(osd_control_button)
    .padding([6, 8])
    .on_press(on_toggle_speed_menu);

    let audio_btn = button(icon_audio_track(18.0))
        .style(osd_control_button)
        .padding([6, 8])
        .on_press(on_toggle_audio_menu);

    let gear_btn = button(icon_settings(18.0))
        .style(osd_control_button)
        .padding([6, 8])
        .on_press(on_toggle_quality_menu);

    let cc_btn = button(icon_subtitles(18.0))
        .style(osd_control_button)
        .padding([6, 8])
        .on_press(on_toggle_captions);

    let pip_btn = button(icon_pip(18.0))
        .style(osd_control_button)
        .padding([6, 8])
        .on_press(on_toggle_pip);

    let theater_btn = button(icon_theater(18.0))
        .style(osd_control_button)
        .padding([6, 8])
        .on_press(on_toggle_theater);

    let fs_btn = button(if is_fullscreen {
        icon_exit_fullscreen(18.0)
    } else {
        icon_fullscreen(18.0)
    })
    .style(osd_control_button)
    .padding([6, 8])
    .on_press(on_toggle_fullscreen);

    let mut time_items = row![
        text(time_str).size(13).color(COLOR_TEXT_PRIMARY),
    ]
    .spacing(6)
    .align_y(Alignment::Center);

    if state.is_buffering {
        time_items = time_items.push(
            text(format!("(Cargando buffer {}%)", state.buffering_percent))
                .size(11)
                .color(COLOR_ACCENT_RED),
        );
    } else if state.buffered_pos_secs > state.current_pos_secs {
        let ahead = state.buffered_pos_secs.saturating_sub(state.current_pos_secs);
        time_items = time_items.push(
            text(format!("(+{ahead}s buffer)"))
                .size(11)
                .color(COLOR_TEXT_SECONDARY),
        );
    }

    if !chapters.is_empty() {
        if let Some(active_ch) = chapters
            .iter()
            .rev()
            .find(|c| c.start_secs <= target_secs)
            .or_else(|| chapters.first())
        {
            let ch_display = if active_ch.title.chars().count() > 38 {
                let mut s: String = active_ch.title.chars().take(35).collect();
                s.push_str("...");
                s
            } else {
                active_ch.title.clone()
            };
            time_items = time_items.push(
                text(format!("•  {ch_display}"))
                    .size(12)
                    .color(if state.hover_seek_ratio.is_some() {
                        COLOR_TEXT_PRIMARY
                    } else {
                        COLOR_TEXT_SECONDARY
                    }),
            );
        }
    }

    let empty_center_spacer: Element<'a, Message> = if state.has_open_menu() {
        mouse_area(Space::new(Length::Fill, Length::Fixed(34.0)))
            .on_press(on_close_menus.clone())
            .into()
    } else {
        Space::new(Length::Fill, Length::Fixed(34.0)).into()
    };

    let mut controls_row = row![
        play_btn,
        mute_btn,
        vol_slider,
        time_items,
        empty_center_spacer,
        cc_btn,
        audio_btn,
        speed_btn,
        gear_btn,
        pip_btn,
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    if !is_fullscreen {
        controls_row = controls_row.push(theater_btn);
    }
    controls_row = controls_row.push(fs_btn);

    let mut outer_col = column![].spacing(4);

    // 1. Floating dropdown menu right ABOVE the seek bar
    if let Some(menu_el) = view_floating_dropdown_menu(
        state,
        video_qualities,
        audio_tracks,
        caption_tracks,
        on_close_menus.clone(),
        on_select_quality_itag,
        on_toggle_save_default_quality,
        on_set_preferred_codec,
        on_select_audio_track,
        on_select_speed,
        on_select_caption_track,
    ) {
        outer_col = outer_col.push(menu_el);
    }

    // 3. Floating Hover Tooltip (Chapter title + timestamp right above cursor on seek bar)
    if let Some(hr) = state.hover_seek_ratio {
        let hover_secs = ((hr.clamp(0.0, 1.0)) * (effective_dur.max(1) as f32)) as u64;
        let hover_time_label = format_duration(hover_secs);
        let hovered_chapter_title = if !chapters.is_empty() {
            chapters
                .iter()
                .rev()
                .find(|c| c.start_secs <= hover_secs)
                .or_else(|| chapters.first())
                .map(|c| c.title.clone())
        } else {
            None
        };
        let tooltip_h = if hovered_chapter_title.is_some() { 45.0 } else { 29.0 };
        let tooltip_canvas = iced::widget::canvas(SeekHoverTooltipProgram {
            hover_ratio: hr,
            time_label: hover_time_label,
            chapter_title: hovered_chapter_title,
        })
        .width(Length::Fill)
        .height(Length::Fixed(tooltip_h));

        outer_col = outer_col.push(
            container(tooltip_canvas)
                .width(Length::Fill)
                .padding([0, 12]),
        );
    }

    let bottom_bar_box = container(
        column![interactive_seek_bar, controls_row].spacing(6),
    )
    .style(youtube_player_control_bar)
    .padding([8, 12])
    .width(Length::Fill);

    let bottom_bar_inner: Element<'a, Message> = if state.has_open_menu() {
        iced::widget::mouse_area(bottom_bar_box)
            .on_press(on_close_menus.clone())
            .into()
    } else {
        iced::widget::mouse_area(bottom_bar_box)
            .into()
    };

    outer_col = outer_col.push(bottom_bar_inner);

    let outer_container = container(outer_col)
        .style(transparent_player_container)
        .width(Length::Fill);

    outer_container.into()
}

pub fn view_subtitle_overlay<'a, Message: 'static>(
    subtitle_text: &'a str,
    controls_visible: bool,
) -> Element<'a, Message> {
    let sub_banner = container(
        text(subtitle_text)
            .size(16)
            .color(COLOR_TEXT_PRIMARY),
    )
    .style(|_t| container::Style {
        background: Some(iced::Color::from_rgba(0.05, 0.05, 0.05, 0.92).into()),
        border: iced::border::rounded(6).color(COLOR_BORDER).width(1.0),
        ..container::Style::default()
    })
    .padding([6, 14]);

    let bottom_spacing = if controls_visible { 72.0 } else { 20.0 };

    container(
        column![
            Space::with_height(Length::Fill),
            row![Space::with_width(Length::Fill), sub_banner, Space::with_width(Length::Fill)]
                .width(Length::Fill),
            Space::with_height(Length::Fixed(bottom_spacing)),
        ]
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(transparent_player_container)
    .into()
}

