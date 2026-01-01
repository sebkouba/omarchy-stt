//! Indicator Daemon
//!
//! Displays a pill-shaped overlay indicator for recording/transcription states.
//! Replaces the eww-based indicator with animated dot matrix and helix visuals.
//!
//! State is read from /tmp/ptt_indicator_state:
//!   - "recording" -> green dot matrix (audio-reactive)
//!   - "transcribing" -> blue helix animation
//!   - "enhancing" -> yellow helix animation
//!   - "error" -> red helix animation
//!   - "idle" or missing -> window hidden
//!
//! Audio level read from /tmp/ptt_audio_level (0-100) for recording visualization.

use eframe::egui::{self, Color32, Pos2, Rect, Rounding, Stroke, Vec2};
use std::fs;
use std::time::Instant;

const STATE_FILE: &str = "/tmp/ptt_indicator_state";
const AUDIO_LEVEL_FILE: &str = "/tmp/ptt_audio_level";

const INDICATOR_WIDTH: f32 = 200.0;
const INDICATOR_HEIGHT: f32 = 40.0;

fn main() -> eframe::Result<()> {
    // Position at bottom center of screen
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([INDICATOR_WIDTH, INDICATOR_HEIGHT])
            .with_position([660.0, 1030.0]) // Approximate bottom center for 1920x1080
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_mouse_passthrough(true)
            .with_resizable(false),
        ..Default::default()
    };

    eframe::run_native(
        "Indicator",
        options,
        Box::new(|_cc| Ok(Box::new(IndicatorApp::new()))),
    )
}

#[derive(Clone, Copy, PartialEq)]
enum State {
    Idle,
    Recording,
    Transcribing,
    Enhancing,
    Error,
}

impl State {
    fn from_str(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "recording" => State::Recording,
            "transcribing" => State::Transcribing,
            "enhancing" => State::Enhancing,
            "error" => State::Error,
            _ => State::Idle,
        }
    }

    fn color(&self) -> Color32 {
        match self {
            State::Idle => Color32::from_rgb(60, 60, 60),
            State::Recording => Color32::from_rgb(0, 255, 51), // Matrix Green
            State::Transcribing => Color32::from_rgb(77, 153, 255), // Matrix Blue
            State::Enhancing => Color32::from_rgb(255, 204, 51), // Matrix Yellow
            State::Error => Color32::from_rgb(255, 77, 77),    // Matrix Red
        }
    }

    fn is_visible(&self) -> bool {
        !matches!(self, State::Idle)
    }
}

struct IndicatorApp {
    start_time: Instant,
    state: State,
    audio_level: f32,
}

impl IndicatorApp {
    fn new() -> Self {
        Self {
            start_time: Instant::now(),
            state: State::Idle,
            audio_level: 0.0,
        }
    }

    fn time(&self) -> f32 {
        self.start_time.elapsed().as_secs_f32()
    }

    fn read_state(&mut self) {
        self.state = fs::read_to_string(STATE_FILE)
            .map(|s| State::from_str(&s))
            .unwrap_or(State::Idle);
    }

    fn read_audio_level(&mut self) {
        self.audio_level = fs::read_to_string(AUDIO_LEVEL_FILE)
            .ok()
            .and_then(|s| s.trim().parse::<f32>().ok())
            .map(|v| (v / 100.0).clamp(0.0, 1.0))
            .unwrap_or(0.0);
    }
}

impl eframe::App for IndicatorApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0] // Transparent background
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.read_state();
        self.read_audio_level();

        // Request continuous repaint for animations
        ctx.request_repaint();

        // Actually hide/show window based on state
        let should_be_visible = self.state.is_visible();
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(should_be_visible));

        if !should_be_visible {
            return;
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                let rect =
                    Rect::from_min_size(Pos2::ZERO, Vec2::new(INDICATOR_WIDTH, INDICATOR_HEIGHT));

                let painter = ui.painter();

                // Pill background
                let pill_rounding = rect.height() / 2.0;
                painter.rect_filled(
                    rect,
                    Rounding::same(pill_rounding),
                    Color32::from_rgba_unmultiplied(30, 30, 46, 230),
                );

                let color = self.state.color();
                let time = self.time();

                match self.state {
                    State::Recording => {
                        draw_dot_matrix(painter, rect, self.audio_level, color);
                    }
                    State::Transcribing | State::Enhancing => {
                        draw_helix(painter, rect, time, color);
                    }
                    State::Error => {
                        draw_helix(painter, rect, time * 2.0, color);
                    }
                    State::Idle => {}
                }
            });
    }
}

fn draw_dot_matrix(painter: &egui::Painter, rect: Rect, audio: f32, color: Color32) {
    const ROWS: usize = 5;

    let pill_radius = rect.height() / 2.0;
    let padding = 4.0;
    let inner_radius = pill_radius - padding;

    let dot_radius = 2.8;
    let dot_spacing = 7.5;

    let center_y = rect.center().y;
    let left_circle_x = rect.left() + pill_radius;
    let right_circle_x = rect.right() - pill_radius;

    let total_rows_height = (ROWS - 1) as f32 * dot_spacing;
    let first_row_y = center_y - total_rows_height / 2.0;

    for row in 0..ROWS {
        let y = first_row_y + row as f32 * dot_spacing;
        let dist_from_center_y = (y - center_y).abs();

        let max_x_in_circle = if dist_from_center_y < inner_radius {
            (inner_radius * inner_radius - dist_from_center_y * dist_from_center_y).sqrt()
        } else {
            0.0
        };

        let left_bound = left_circle_x - max_x_in_circle;
        let right_bound = right_circle_x + max_x_in_circle;

        let row_width = right_bound - left_bound;
        let num_dots = ((row_width - dot_radius * 2.0) / dot_spacing).floor() as usize + 1;
        let actual_width = (num_dots - 1) as f32 * dot_spacing;
        let start_x = left_bound + (row_width - actual_width) / 2.0;

        for col in 0..num_dots {
            let x = start_x + col as f32 * dot_spacing;

            let t = col as f32 / (num_dots - 1).max(1) as f32;
            let envelope = (t * std::f32::consts::PI).sin();
            let active_rows = (audio * envelope * (ROWS as f32 + 1.0)) as usize;
            let center_row = ROWS / 2;
            let dist_from_center = (row as i32 - center_row as i32).unsigned_abs() as usize;
            let is_active = dist_from_center < active_rows;

            let alpha = if is_active { 220 } else { 25 };
            let dot_color = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha);

            painter.circle_filled(Pos2::new(x, y), dot_radius, dot_color);
        }
    }
}

fn draw_helix(painter: &egui::Painter, rect: Rect, time: f32, color: Color32) {
    const NUM_POINTS: usize = 20;
    let dot_radius = 2.8;

    let pill_radius = rect.height() / 2.0;
    let padding = 4.0;
    let inner_radius = pill_radius - padding;

    let center_y = rect.center().y;
    let left_circle_x = rect.left() + pill_radius;
    let right_circle_x = rect.right() - pill_radius;

    let left_bound = rect.left() + padding + dot_radius;
    let right_bound = rect.right() - padding - dot_radius;
    let width = right_bound - left_bound;

    for i in 0..NUM_POINTS {
        let t = i as f32 / (NUM_POINTS - 1) as f32;
        let x = left_bound + t * width;
        let phase = time * 2.5 + t * std::f32::consts::PI * 2.5;

        // Calculate max amplitude at this x position (respecting pill shape)
        let max_amplitude = if x < left_circle_x {
            let dx = left_circle_x - x;
            if dx < inner_radius {
                (inner_radius * inner_radius - dx * dx).sqrt()
            } else {
                0.0
            }
        } else if x > right_circle_x {
            let dx = x - right_circle_x;
            if dx < inner_radius {
                (inner_radius * inner_radius - dx * dx).sqrt()
            } else {
                0.0
            }
        } else {
            inner_radius
        };

        let amplitude = max_amplitude * 0.9;
        let y1 = center_y + phase.sin() * amplitude;
        let y2 = center_y - phase.sin() * amplitude;

        // Connecting line
        let line_alpha = (phase.sin().abs() * 40.0 + 20.0) as u8;
        painter.line_segment(
            [Pos2::new(x, y1), Pos2::new(x, y2)],
            Stroke::new(
                1.0,
                Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), line_alpha),
            ),
        );

        let alpha = 180 + (phase.cos() * 75.0) as i32;
        let dot_color = Color32::from_rgba_unmultiplied(
            color.r(),
            color.g(),
            color.b(),
            alpha.clamp(100, 255) as u8,
        );

        painter.circle_filled(Pos2::new(x, y1), dot_radius, dot_color);
        painter.circle_filled(Pos2::new(x, y2), dot_radius, dot_color);
    }
}
