//! Speech Indicator Catalogue
//!
//! A visual catalogue of different speech indicator designs for the transcribe-rs-v2 project.
//! Inspired by ParaDict2's dot matrix visualizer.
//!
//! Run with: cargo run --bin indicator-catalogue

use eframe::egui::{self, Color32, Pos2, Rect, Rounding, Stroke, Vec2};
use std::time::Instant;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 700.0])
            .with_title("Speech Indicator Catalogue"),
        ..Default::default()
    };

    eframe::run_native(
        "Indicator Catalogue",
        options,
        Box::new(|_cc| Ok(Box::new(IndicatorCatalogue::new()))),
    )
}

/// Dictation state for demonstration
#[derive(Clone, Copy, PartialEq)]
enum DictationState {
    Idle,
    Recording,
    Transcribing,
    Enhancing,
    Error,
}

impl DictationState {
    fn color(&self) -> Color32 {
        match self {
            DictationState::Idle => Color32::from_rgb(100, 100, 100),
            DictationState::Recording => Color32::from_rgb(0, 255, 51),    // Matrix Green
            DictationState::Transcribing => Color32::from_rgb(77, 153, 255), // Matrix Blue
            DictationState::Enhancing => Color32::from_rgb(255, 204, 51),  // Matrix Yellow
            DictationState::Error => Color32::from_rgb(255, 77, 77),       // Matrix Red
        }
    }

    fn name(&self) -> &'static str {
        match self {
            DictationState::Idle => "Idle",
            DictationState::Recording => "Recording",
            DictationState::Transcribing => "Transcribing",
            DictationState::Enhancing => "Enhancing",
            DictationState::Error => "Error",
        }
    }
}

struct IndicatorCatalogue {
    start_time: Instant,
    state: DictationState,
    simulated_audio_level: f32,
}

impl IndicatorCatalogue {
    fn new() -> Self {
        Self {
            start_time: Instant::now(),
            state: DictationState::Recording,
            simulated_audio_level: 0.0,
        }
    }

    fn time(&self) -> f32 {
        self.start_time.elapsed().as_secs_f32()
    }

    /// Simulate audio level with pseudo-random variations
    fn simulate_audio(&mut self) {
        let t = self.time();
        // Combine multiple sine waves for more natural-looking audio
        self.simulated_audio_level = ((t * 3.7).sin() * 0.3
            + (t * 7.3).sin() * 0.25
            + (t * 11.1).sin() * 0.2
            + (t * 17.9).sin() * 0.15
            + (t * 23.7).sin() * 0.1)
            .abs()
            .min(1.0);
    }

    fn draw_indicator_card(
        &self,
        ui: &mut egui::Ui,
        title: &str,
        description: &str,
        draw_fn: impl FnOnce(&mut egui::Ui, Rect, f32, f32, DictationState),
    ) {
        egui::Frame::none()
            .fill(Color32::from_rgb(30, 30, 35))
            .rounding(Rounding::same(8.0))
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.heading(title);
                    ui.label(egui::RichText::new(description).color(Color32::GRAY).small());
                    ui.add_space(8.0);

                    let (response, painter) =
                        ui.allocate_painter(Vec2::new(ui.available_width(), 60.0), egui::Sense::hover());
                    let rect = response.rect;

                    // Draw dark background for indicator
                    painter.rect_filled(rect, Rounding::same(4.0), Color32::from_rgb(15, 15, 18));

                    draw_fn(ui, rect, self.time(), self.simulated_audio_level, self.state);
                });
            });
    }
}

impl eframe::App for IndicatorCatalogue {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.simulate_audio();
        ctx.request_repaint(); // Continuous animation

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Speech Indicator Catalogue");
            ui.label("Exploring various visual indicators for push-to-talk dictation");
            ui.add_space(8.0);

            // State selector
            ui.horizontal(|ui| {
                ui.label("Preview State:");
                for state in [
                    DictationState::Idle,
                    DictationState::Recording,
                    DictationState::Transcribing,
                    DictationState::Enhancing,
                    DictationState::Error,
                ] {
                    if ui
                        .selectable_label(self.state == state, state.name())
                        .clicked()
                    {
                        self.state = state;
                    }
                }
            });

            ui.add_space(16.0);

            egui::ScrollArea::vertical().show(ui, |ui| {
                let indicators: Vec<(&str, &str, fn(&mut egui::Ui, Rect, f32, f32, DictationState))> = vec![
                    ("1. Dot Matrix (ParaDict2)", "12x5 grid, audio-reactive", draw_dot_matrix),
                    ("2. Waveform Bars", "Classic equalizer vertical bars", draw_waveform_bars),
                    ("3. Pulsing Ring", "Circular ring pulses with audio", draw_pulsing_ring),
                    ("4. Breathing Orb", "Glowing orb with soft glow", draw_breathing_orb),
                    ("5. Sound Wave", "Smooth oscillating wave", draw_sound_wave),
                    ("6. Ripple Circles", "Concentric circles rippling", draw_ripple_circles),
                    ("7. Spectrum Dots", "Horizontal dot strip", draw_spectrum_dots),
                    ("8. Minimal Bar", "Clean horizontal progress", draw_minimal_bar),
                    ("9. DNA Helix", "Double helix rotating", draw_dna_helix),
                    ("10. Particle Burst", "Particles from center", draw_particle_burst),
                    ("11. Text Pulse", "Minimalist pulsing text", draw_text_pulse),
                    ("12. Circular Dots", "Dots in rotating circle", draw_circular_dots),
                ];

                for row in indicators.chunks(2) {
                    ui.columns(2, |columns| {
                        for (i, (title, desc, draw_fn)) in row.iter().enumerate() {
                            self.draw_indicator_card(&mut columns[i], title, desc, *draw_fn);
                        }
                    });
                    ui.add_space(12.0);
                }
            });
        });
    }
}

// ============================================================================
// Indicator Drawing Functions
// ============================================================================

fn draw_dot_matrix(ui: &mut egui::Ui, rect: Rect, time: f32, audio: f32, state: DictationState) {
    let painter = ui.painter();
    let color = state.color();

    const COLS: usize = 12;
    const ROWS: usize = 5;

    let cell_width = rect.width() * 0.8 / COLS as f32;
    let cell_height = rect.height() * 0.7 / ROWS as f32;
    let dot_radius = cell_width.min(cell_height) * 0.35;

    let offset_x = rect.left() + (rect.width() - cell_width * COLS as f32) / 2.0;
    let offset_y = rect.top() + (rect.height() - cell_height * ROWS as f32) / 2.0;

    for col in 0..COLS {
        for row in 0..ROWS {
            let x = offset_x + col as f32 * cell_width + cell_width / 2.0;
            let y = offset_y + row as f32 * cell_height + cell_height / 2.0;

            // Calculate activation based on state
            let is_active = match state {
                DictationState::Recording => {
                    // Audio-reactive: envelope based on column position
                    let envelope = ((col as f32 / COLS as f32) * std::f32::consts::PI).sin();
                    let active_rows = (audio * envelope * ROWS as f32) as usize;
                    let center_row = ROWS / 2;
                    let dist_from_center = (row as i32 - center_row as i32).unsigned_abs() as usize;
                    dist_from_center < active_rows
                }
                DictationState::Transcribing | DictationState::Enhancing => {
                    // Progress animation: spreading from center
                    let center_col = COLS as f32 / 2.0;
                    let center_row = ROWS as f32 / 2.0;
                    let dist = ((col as f32 - center_col).powi(2) + (row as f32 - center_row).powi(2)).sqrt();
                    let max_dist = ((COLS as f32 / 2.0).powi(2) + (ROWS as f32 / 2.0).powi(2)).sqrt();
                    let progress = ((time * 2.0).sin() * 0.5 + 0.5) * max_dist;
                    dist < progress
                }
                DictationState::Error => {
                    // Blink all
                    (time * 4.0).sin() > 0.0
                }
                DictationState::Idle => false,
            };

            let alpha = if is_active { 200 } else { 25 };
            let dot_color = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha);

            painter.circle_filled(Pos2::new(x, y), dot_radius, dot_color);
        }
    }
}

fn draw_waveform_bars(ui: &mut egui::Ui, rect: Rect, time: f32, audio: f32, state: DictationState) {
    let painter = ui.painter();
    let color = state.color();

    const NUM_BARS: usize = 16;
    let bar_width = rect.width() * 0.7 / NUM_BARS as f32;
    let gap = bar_width * 0.3;
    let max_height = rect.height() * 0.8;

    let offset_x = rect.left() + (rect.width() - (bar_width + gap) * NUM_BARS as f32) / 2.0;

    for i in 0..NUM_BARS {
        let x = offset_x + i as f32 * (bar_width + gap);

        let height = match state {
            DictationState::Recording => {
                let phase = time * 5.0 + i as f32 * 0.4;
                (phase.sin() * 0.5 + 0.5) * audio * max_height
            }
            DictationState::Transcribing | DictationState::Enhancing => {
                let phase = time * 3.0 + i as f32 * 0.3;
                (phase.sin() * 0.3 + 0.5) * max_height * 0.6
            }
            DictationState::Error => {
                max_height * 0.3
            }
            DictationState::Idle => max_height * 0.1,
        };

        let bar_rect = Rect::from_min_size(
            Pos2::new(x, rect.center().y - height / 2.0),
            Vec2::new(bar_width, height.max(4.0)),
        );

        painter.rect_filled(bar_rect, Rounding::same(2.0), color);
    }
}

fn draw_pulsing_ring(ui: &mut egui::Ui, rect: Rect, time: f32, audio: f32, state: DictationState) {
    let painter = ui.painter();
    let color = state.color();
    let center = rect.center();

    let base_radius = rect.height() * 0.35;

    match state {
        DictationState::Recording => {
            // Multiple rings based on audio
            for i in 0..3 {
                let delay = i as f32 * 0.3;
                let pulse = ((time - delay) * 4.0).sin() * 0.5 + 0.5;
                let radius = base_radius * (0.5 + pulse * 0.5 * audio);
                let alpha = (200.0 * (1.0 - i as f32 * 0.3)) as u8;
                let ring_color = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha);
                painter.circle_stroke(center, radius, Stroke::new(3.0 - i as f32, ring_color));
            }
        }
        DictationState::Transcribing | DictationState::Enhancing => {
            // Rotating partial ring
            let radius = base_radius * 0.8;
            let start_angle = time * 3.0;
            let arc_length = std::f32::consts::PI * 1.2;

            for i in 0..20 {
                let angle = start_angle + (i as f32 / 20.0) * arc_length;
                let x = center.x + angle.cos() * radius;
                let y = center.y + angle.sin() * radius;
                painter.circle_filled(Pos2::new(x, y), 3.0, color);
            }
        }
        DictationState::Error => {
            let pulse = (time * 6.0).sin() * 0.5 + 0.5;
            painter.circle_stroke(center, base_radius * (0.7 + pulse * 0.3), Stroke::new(4.0, color));
        }
        DictationState::Idle => {
            painter.circle_stroke(center, base_radius * 0.6, Stroke::new(2.0, Color32::from_rgb(60, 60, 60)));
        }
    }
}

fn draw_breathing_orb(ui: &mut egui::Ui, rect: Rect, time: f32, audio: f32, state: DictationState) {
    let painter = ui.painter();
    let color = state.color();
    let center = rect.center();

    let base_radius = rect.height() * 0.3;

    let (radius, alpha) = match state {
        DictationState::Recording => {
            let pulse = (time * 4.0).sin() * 0.3 + 0.7;
            (base_radius * (0.5 + audio * 0.5) * pulse, 180)
        }
        DictationState::Transcribing | DictationState::Enhancing => {
            let pulse = (time * 2.0).sin() * 0.2 + 0.8;
            (base_radius * pulse, 150)
        }
        DictationState::Error => {
            let pulse = (time * 5.0).sin() * 0.5 + 0.5;
            (base_radius * 0.8, (pulse * 255.0) as u8)
        }
        DictationState::Idle => (base_radius * 0.5, 50),
    };

    // Glow effect with multiple circles
    for i in (0..4).rev() {
        let glow_radius = radius + i as f32 * 6.0;
        let glow_alpha = alpha / (i + 1) as u8;
        let glow_color = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), glow_alpha);
        painter.circle_filled(center, glow_radius, glow_color);
    }
}

fn draw_sound_wave(ui: &mut egui::Ui, rect: Rect, time: f32, audio: f32, state: DictationState) {
    let painter = ui.painter();
    let color = state.color();

    let num_points = 50;
    let amplitude = rect.height() * 0.35;

    let points: Vec<Pos2> = (0..num_points)
        .map(|i| {
            let x = rect.left() + (i as f32 / num_points as f32) * rect.width();
            let phase = time * 4.0 + i as f32 * 0.2;

            let y_offset = match state {
                DictationState::Recording => phase.sin() * amplitude * audio,
                DictationState::Transcribing | DictationState::Enhancing => {
                    (phase * 0.5).sin() * amplitude * 0.3
                }
                DictationState::Error => (phase * 2.0).sin() * amplitude * 0.2,
                DictationState::Idle => 0.0,
            };

            Pos2::new(x, rect.center().y + y_offset)
        })
        .collect();

    for i in 1..points.len() {
        painter.line_segment([points[i - 1], points[i]], Stroke::new(2.5, color));
    }
}

fn draw_ripple_circles(ui: &mut egui::Ui, rect: Rect, time: f32, audio: f32, state: DictationState) {
    let painter = ui.painter();
    let color = state.color();
    let center = rect.center();

    let max_radius = rect.height() * 0.45;

    match state {
        DictationState::Recording => {
            for i in 0..4 {
                let phase = (time * 2.0 + i as f32 * 0.5) % 2.0;
                let radius = phase * max_radius * (0.5 + audio * 0.5);
                let alpha = ((1.0 - phase / 2.0) * 200.0) as u8;
                let ripple_color = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha);
                painter.circle_stroke(center, radius, Stroke::new(2.0, ripple_color));
            }
        }
        DictationState::Transcribing | DictationState::Enhancing => {
            for i in 0..3 {
                let phase = (time * 1.5 + i as f32 * 0.7) % 2.0;
                let radius = phase * max_radius * 0.8;
                let alpha = ((1.0 - phase / 2.0) * 150.0) as u8;
                let ripple_color = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha);
                painter.circle_stroke(center, radius, Stroke::new(1.5, ripple_color));
            }
        }
        DictationState::Error => {
            let pulse = (time * 4.0).sin() * 0.5 + 0.5;
            painter.circle_stroke(center, max_radius * 0.6, Stroke::new(3.0, color));
            painter.circle_stroke(center, max_radius * 0.6 * pulse, Stroke::new(2.0, color));
        }
        DictationState::Idle => {
            painter.circle_stroke(center, max_radius * 0.3, Stroke::new(1.0, Color32::from_rgb(60, 60, 60)));
        }
    }
}

fn draw_spectrum_dots(ui: &mut egui::Ui, rect: Rect, time: f32, audio: f32, state: DictationState) {
    let painter = ui.painter();
    let color = state.color();

    const NUM_DOTS: usize = 20;
    let dot_spacing = rect.width() * 0.8 / NUM_DOTS as f32;
    let offset_x = rect.left() + rect.width() * 0.1;

    for i in 0..NUM_DOTS {
        let x = offset_x + i as f32 * dot_spacing;
        let center_dist = (i as f32 - NUM_DOTS as f32 / 2.0).abs() / (NUM_DOTS as f32 / 2.0);

        let (radius, alpha) = match state {
            DictationState::Recording => {
                let phase = time * 5.0 + i as f32 * 0.3;
                let scale = (1.0 - center_dist) * audio;
                let r = 3.0 + (phase.sin() * 0.5 + 0.5) * scale * 5.0;
                (r, 180)
            }
            DictationState::Transcribing | DictationState::Enhancing => {
                let wave = ((time * 3.0 + i as f32 * 0.2).sin() * 0.5 + 0.5) * 4.0;
                (3.0 + wave, 150)
            }
            DictationState::Error => (4.0, 180),
            DictationState::Idle => (2.0, 50),
        };

        let dot_color = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha);
        painter.circle_filled(Pos2::new(x, rect.center().y), radius, dot_color);
    }
}

fn draw_minimal_bar(ui: &mut egui::Ui, rect: Rect, time: f32, audio: f32, state: DictationState) {
    let painter = ui.painter();
    let color = state.color();

    let bar_height = 6.0;
    let bar_width = rect.width() * 0.8;
    let bar_x = rect.left() + rect.width() * 0.1;
    let bar_y = rect.center().y - bar_height / 2.0;

    // Background bar
    let bg_rect = Rect::from_min_size(Pos2::new(bar_x, bar_y), Vec2::new(bar_width, bar_height));
    painter.rect_filled(bg_rect, Rounding::same(3.0), Color32::from_rgb(40, 40, 45));

    let fill_width = match state {
        DictationState::Recording => bar_width * audio,
        DictationState::Transcribing | DictationState::Enhancing => {
            let progress = (time * 0.5) % 1.0;
            bar_width * progress
        }
        DictationState::Error => bar_width,
        DictationState::Idle => 0.0,
    };

    if fill_width > 0.0 {
        let fill_rect = Rect::from_min_size(Pos2::new(bar_x, bar_y), Vec2::new(fill_width, bar_height));
        painter.rect_filled(fill_rect, Rounding::same(3.0), color);
    }
}

fn draw_dna_helix(ui: &mut egui::Ui, rect: Rect, time: f32, audio: f32, state: DictationState) {
    let painter = ui.painter();
    let color = state.color();

    const NUM_POINTS: usize = 12;
    let amplitude = rect.height() * 0.3;

    for i in 0..NUM_POINTS {
        let t = i as f32 / NUM_POINTS as f32;
        let x = rect.left() + rect.width() * 0.1 + t * rect.width() * 0.8;

        let phase = time * 3.0 + t * std::f32::consts::PI * 2.0;
        let y1 = rect.center().y + phase.sin() * amplitude;
        let y2 = rect.center().y - phase.sin() * amplitude;

        let (r1, r2, alpha) = match state {
            DictationState::Recording => (4.0 + audio * 3.0, 4.0 + audio * 3.0, 180),
            DictationState::Transcribing | DictationState::Enhancing => (4.0, 4.0, 150),
            DictationState::Error => (3.0, 3.0, 200),
            DictationState::Idle => (2.0, 2.0, 50),
        };

        let dot_color = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha);

        painter.circle_filled(Pos2::new(x, y1), r1, dot_color);
        painter.circle_filled(Pos2::new(x, y2), r2, dot_color);

        // Connecting line
        if state != DictationState::Idle {
            painter.line_segment(
                [Pos2::new(x, y1), Pos2::new(x, y2)],
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha / 3)),
            );
        }
    }
}

fn draw_particle_burst(ui: &mut egui::Ui, rect: Rect, time: f32, audio: f32, state: DictationState) {
    let painter = ui.painter();
    let color = state.color();
    let center = rect.center();

    const NUM_PARTICLES: usize = 12;
    let max_radius = rect.height() * 0.4;

    for i in 0..NUM_PARTICLES {
        let base_angle = (i as f32 / NUM_PARTICLES as f32) * std::f32::consts::PI * 2.0;

        let (radius, angle_offset, size, alpha) = match state {
            DictationState::Recording => {
                let phase = (time * 3.0 + i as f32 * 0.5) % 1.0;
                let r = max_radius * phase * (0.5 + audio * 0.5);
                (r, time * 0.5, 3.0 + audio * 2.0, ((1.0 - phase) * 200.0) as u8)
            }
            DictationState::Transcribing | DictationState::Enhancing => {
                let phase = (time * 2.0 + i as f32 * 0.3) % 1.0;
                (max_radius * 0.5 + phase * max_radius * 0.3, time * 0.3, 3.0, 150)
            }
            DictationState::Error => {
                let pulse = (time * 4.0).sin() * 0.5 + 0.5;
                (max_radius * 0.4, 0.0, 4.0, (pulse * 200.0) as u8)
            }
            DictationState::Idle => (max_radius * 0.3, 0.0, 2.0, 40),
        };

        let angle = base_angle + angle_offset;
        let x = center.x + angle.cos() * radius;
        let y = center.y + angle.sin() * radius;

        let particle_color = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha);
        painter.circle_filled(Pos2::new(x, y), size, particle_color);
    }

    // Center dot
    if state != DictationState::Idle {
        painter.circle_filled(center, 4.0, color);
    }
}

fn draw_text_pulse(ui: &mut egui::Ui, rect: Rect, time: f32, audio: f32, state: DictationState) {
    let painter = ui.painter();
    let color = state.color();

    let text = match state {
        DictationState::Idle => "IDLE",
        DictationState::Recording => "REC",
        DictationState::Transcribing => "PROCESSING",
        DictationState::Enhancing => "ENHANCING",
        DictationState::Error => "ERROR",
    };

    let alpha = match state {
        DictationState::Recording => {
            let pulse = (time * 4.0).sin() * 0.3 + 0.7;
            (pulse * 255.0 * (0.5 + audio * 0.5)) as u8
        }
        DictationState::Transcribing | DictationState::Enhancing => {
            let pulse = (time * 2.0).sin() * 0.2 + 0.8;
            (pulse * 255.0) as u8
        }
        DictationState::Error => ((time * 5.0).sin() * 127.0 + 128.0) as u8,
        DictationState::Idle => 80,
    };

    let text_color = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha);

    // Draw recording indicator dot
    if state == DictationState::Recording {
        let dot_alpha = ((time * 4.0).sin() * 127.0 + 128.0) as u8;
        let dot_color = Color32::from_rgba_unmultiplied(255, 50, 50, dot_alpha);
        painter.circle_filled(Pos2::new(rect.center().x - 40.0, rect.center().y), 5.0, dot_color);
    }

    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(18.0),
        text_color,
    );
}

fn draw_circular_dots(ui: &mut egui::Ui, rect: Rect, time: f32, audio: f32, state: DictationState) {
    let painter = ui.painter();
    let color = state.color();
    let center = rect.center();

    const NUM_DOTS: usize = 8;
    let radius = rect.height() * 0.3;

    for i in 0..NUM_DOTS {
        let base_angle = (i as f32 / NUM_DOTS as f32) * std::f32::consts::PI * 2.0;

        let (angle, dot_size, alpha) = match state {
            DictationState::Recording => {
                let a = base_angle + time * 2.0;
                let size = 4.0 + (time * 5.0 + i as f32).sin().abs() * audio * 4.0;
                (a, size, 200)
            }
            DictationState::Transcribing | DictationState::Enhancing => {
                let a = base_angle + time * 1.5;
                let active = ((time * 3.0) as usize % NUM_DOTS) == i;
                let size = if active { 6.0 } else { 3.0 };
                (a, size, if active { 255 } else { 100 })
            }
            DictationState::Error => {
                let pulse = (time * 6.0).sin() * 0.5 + 0.5;
                (base_angle, 4.0, (pulse * 255.0) as u8)
            }
            DictationState::Idle => (base_angle, 2.0, 50),
        };

        let x = center.x + angle.cos() * radius;
        let y = center.y + angle.sin() * radius;

        let dot_color = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha);
        painter.circle_filled(Pos2::new(x, y), dot_size, dot_color);
    }
}
