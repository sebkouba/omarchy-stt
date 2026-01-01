//! Single Indicator Preview
//!
//! Tight dot matrix (recording) + Smooth helix (processing)
//! Uses realistic dimensions matching the actual indicator widget (200x40)

use eframe::egui::{self, Color32, Pos2, Rect, Rounding, Stroke, Vec2};
use std::time::Instant;

// Actual indicator dimensions (matching eww widget)
const INDICATOR_WIDTH: f32 = 200.0;
const INDICATOR_HEIGHT: f32 = 40.0;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([500.0, 200.0])
            .with_title("Indicator Preview"),
        ..Default::default()
    };

    eframe::run_native(
        "Indicator Preview",
        options,
        Box::new(|_cc| Ok(Box::new(App::new()))),
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
    fn color(&self) -> Color32 {
        match self {
            State::Idle => Color32::from_rgb(60, 60, 60),
            State::Recording => Color32::from_rgb(0, 255, 51), // Matrix Green
            State::Transcribing => Color32::from_rgb(77, 153, 255), // Matrix Blue
            State::Enhancing => Color32::from_rgb(255, 204, 51), // Matrix Yellow
            State::Error => Color32::from_rgb(255, 77, 77),    // Matrix Red
        }
    }

    fn name(&self) -> &'static str {
        match self {
            State::Idle => "Idle",
            State::Recording => "Recording",
            State::Transcribing => "Transcribing",
            State::Enhancing => "Enhancing",
            State::Error => "Error",
        }
    }
}

struct App {
    start_time: Instant,
    state: State,
    audio_level: f32,
}

impl App {
    fn new() -> Self {
        Self {
            start_time: Instant::now(),
            state: State::Recording,
            audio_level: 0.0,
        }
    }

    fn time(&self) -> f32 {
        self.start_time.elapsed().as_secs_f32()
    }

    fn simulate_audio(&mut self) {
        let t = self.time();
        self.audio_level = ((t * 3.7).sin() * 0.3
            + (t * 7.3).sin() * 0.25
            + (t * 11.1).sin() * 0.2
            + (t * 17.9).sin() * 0.15
            + (t * 23.7).sin() * 0.1)
            .abs()
            .min(1.0);
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.simulate_audio();
        ctx.request_repaint();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("State:");
                for state in [
                    State::Idle,
                    State::Recording,
                    State::Transcribing,
                    State::Enhancing,
                    State::Error,
                ] {
                    let color = state.color();
                    let selected = self.state == state;
                    let text = egui::RichText::new(state.name()).color(if selected {
                        color
                    } else {
                        Color32::GRAY
                    });
                    if ui.selectable_label(selected, text).clicked() {
                        self.state = state;
                    }
                }
            });

            ui.add_space(16.0);

            // Center the indicator at actual size
            ui.vertical_centered(|ui| {
                let (response, painter) = ui.allocate_painter(
                    Vec2::new(INDICATOR_WIDTH, INDICATOR_HEIGHT),
                    egui::Sense::hover(),
                );
                let rect = response.rect;

                // Pill/capsule shape - semicircular ends
                let pill_rounding = rect.height() / 2.0;
                painter.rect_filled(
                    rect,
                    Rounding::same(pill_rounding),
                    Color32::from_rgb(30, 30, 46),
                );

                let color = self.state.color();
                let time = self.time();
                let audio = self.audio_level;

                match self.state {
                    State::Recording => {
                        draw_tight_matrix(&painter, rect, audio, color);
                    }
                    State::Transcribing | State::Enhancing => {
                        draw_smooth_helix(&painter, rect, time, color);
                    }
                    State::Error => {
                        draw_smooth_helix(&painter, rect, time * 2.0, color);
                    }
                    State::Idle => {
                        draw_idle(&painter, rect, color);
                    }
                }
            });

            ui.add_space(20.0);
            ui.label(format!(
                "Indicator size: {}x{} px",
                INDICATOR_WIDTH, INDICATOR_HEIGHT
            ));
        });
    }
}

fn draw_tight_matrix(painter: &egui::Painter, rect: Rect, audio: f32, color: Color32) {
    // Pill-shaped matrix: dots fill the capsule shape
    // More dots in center row, fewer at edges
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

fn draw_smooth_helix(painter: &egui::Painter, rect: Rect, time: f32, color: Color32) {
    // Helix constrained to pill shape
    const NUM_POINTS: usize = 20;
    let dot_radius = 2.8;

    let pill_radius = rect.height() / 2.0;
    let padding = 4.0;
    let inner_radius = pill_radius - padding;

    let center_y = rect.center().y;
    let left_circle_x = rect.left() + pill_radius;
    let right_circle_x = rect.right() - pill_radius;

    // Full width of the pill
    let left_bound = rect.left() + padding + dot_radius;
    let right_bound = rect.right() - padding - dot_radius;
    let width = right_bound - left_bound;

    for i in 0..NUM_POINTS {
        let t = i as f32 / (NUM_POINTS - 1) as f32;
        let x = left_bound + t * width;
        let phase = time * 2.5 + t * std::f32::consts::PI * 2.5;

        // Calculate max amplitude at this x position (respecting pill shape)
        let max_amplitude = if x < left_circle_x {
            // In left semicircle
            let dx = left_circle_x - x;
            if dx < inner_radius {
                (inner_radius * inner_radius - dx * dx).sqrt()
            } else {
                0.0
            }
        } else if x > right_circle_x {
            // In right semicircle
            let dx = x - right_circle_x;
            if dx < inner_radius {
                (inner_radius * inner_radius - dx * dx).sqrt()
            } else {
                0.0
            }
        } else {
            // In center rectangle
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

fn draw_idle(painter: &egui::Painter, rect: Rect, color: Color32) {
    const NUM_DOTS: usize = 5;
    let dot_radius = 3.0;
    let spacing = 20.0;
    let offset_x = rect.center().x - (NUM_DOTS as f32 - 1.0) * spacing / 2.0;

    let faint = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 40);

    for i in 0..NUM_DOTS {
        let x = offset_x + i as f32 * spacing;
        painter.circle_filled(Pos2::new(x, rect.center().y), dot_radius, faint);
    }
}
