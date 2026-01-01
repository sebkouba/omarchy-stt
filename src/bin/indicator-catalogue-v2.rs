//! Speech Indicator Catalogue V2
//!
//! Focused exploration: Tight dot matrix (recording) + DNA helix (processing)
//! Based on ParaDict2 color scheme.

use eframe::egui::{self, Color32, Pos2, Rect, Rounding, Stroke, Vec2};
use std::time::Instant;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([950.0, 800.0])
            .with_title("Indicator Catalogue V2 - Matrix + Helix"),
        ..Default::default()
    };

    eframe::run_native(
        "Indicator Catalogue V2",
        options,
        Box::new(|_cc| Ok(Box::new(IndicatorCatalogue::new()))),
    )
}

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
            DictationState::Idle => Color32::from_rgb(60, 60, 60),
            DictationState::Recording => Color32::from_rgb(0, 255, 51), // Matrix Green
            DictationState::Transcribing => Color32::from_rgb(77, 153, 255), // Matrix Blue
            DictationState::Enhancing => Color32::from_rgb(255, 204, 51), // Matrix Yellow
            DictationState::Error => Color32::from_rgb(255, 77, 77),    // Matrix Red
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
    audio_level: f32,
}

impl IndicatorCatalogue {
    fn new() -> Self {
        Self {
            start_time: Instant::now(),
            state: DictationState::Recording,
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

    fn draw_card(
        &self,
        ui: &mut egui::Ui,
        title: &str,
        description: &str,
        draw_fn: fn(&egui::Painter, Rect, f32, f32, DictationState),
    ) {
        egui::Frame::none()
            .fill(Color32::from_rgb(25, 25, 30))
            .rounding(Rounding::same(8.0))
            .inner_margin(16.0)
            .show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.heading(title);
                    ui.label(
                        egui::RichText::new(description)
                            .color(Color32::GRAY)
                            .small(),
                    );
                    ui.add_space(12.0);

                    let (response, painter) = ui.allocate_painter(
                        Vec2::new(ui.available_width(), 70.0),
                        egui::Sense::hover(),
                    );
                    let rect = response.rect;

                    painter.rect_filled(rect, Rounding::same(6.0), Color32::from_rgb(10, 10, 12));

                    draw_fn(&painter, rect, self.time(), self.audio_level, self.state);
                });
            });
    }
}

impl eframe::App for IndicatorCatalogue {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.simulate_audio();
        ctx.request_repaint();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Indicator Catalogue V2");
            ui.label("Tight dot matrix (recording) + DNA helix (processing)");
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.label("State:");
                for state in [
                    DictationState::Idle,
                    DictationState::Recording,
                    DictationState::Transcribing,
                    DictationState::Enhancing,
                    DictationState::Error,
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

            egui::ScrollArea::vertical().show(ui, |ui| {
                let indicators: Vec<(
                    &str,
                    &str,
                    fn(&egui::Painter, Rect, f32, f32, DictationState),
                )> = vec![
                    (
                        "1. Classic Helix",
                        "Standard DNA helix for processing",
                        draw_variant_1,
                    ),
                    (
                        "2. Smooth Helix",
                        "Helix with more points, fluid motion",
                        draw_variant_2,
                    ),
                    ("3. Double Helix", "Two intertwined helixes", draw_variant_3),
                    (
                        "4. Wave Helix",
                        "Helix with amplitude modulation",
                        draw_variant_4,
                    ),
                    (
                        "5. Trailing Helix",
                        "Helix with fading trail",
                        draw_variant_5,
                    ),
                    (
                        "6. Compact Helix",
                        "Tighter helix, faster rotation",
                        draw_variant_6,
                    ),
                ];

                for row in indicators.chunks(2) {
                    ui.columns(2, |columns| {
                        for (i, (title, desc, draw_fn)) in row.iter().enumerate() {
                            self.draw_card(&mut columns[i], title, desc, *draw_fn);
                        }
                    });
                    ui.add_space(12.0);
                }
            });
        });
    }
}

// ============================================================================
// Shared: Tight Dot Matrix (Recording)
// ============================================================================

fn draw_tight_matrix(painter: &egui::Painter, rect: Rect, _time: f32, audio: f32, color: Color32) {
    const COLS: usize = 24;
    const ROWS: usize = 5;

    let total_width = rect.width() * 0.85;
    let total_height = rect.height() * 0.7;

    let dot_spacing_x = total_width / COLS as f32;
    let dot_spacing_y = total_height / ROWS as f32;
    let dot_radius = dot_spacing_x.min(dot_spacing_y) * 0.4;

    let offset_x = rect.left() + (rect.width() - total_width) / 2.0 + dot_spacing_x / 2.0;
    let offset_y = rect.top() + (rect.height() - total_height) / 2.0 + dot_spacing_y / 2.0;

    for col in 0..COLS {
        for row in 0..ROWS {
            let x = offset_x + col as f32 * dot_spacing_x;
            let y = offset_y + row as f32 * dot_spacing_y;

            // Audio-reactive: envelope based on column position
            let envelope = ((col as f32 / COLS as f32) * std::f32::consts::PI).sin();
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

// ============================================================================
// Variant 1: Classic Helix
// ============================================================================

fn draw_variant_1(
    painter: &egui::Painter,
    rect: Rect,
    time: f32,
    audio: f32,
    state: DictationState,
) {
    let color = state.color();

    match state {
        DictationState::Recording => {
            draw_tight_matrix(painter, rect, time, audio, color);
        }
        DictationState::Transcribing | DictationState::Enhancing => {
            draw_helix_classic(painter, rect, time, color);
        }
        DictationState::Error => {
            draw_helix_classic(painter, rect, time * 2.0, color);
        }
        DictationState::Idle => {
            draw_idle_dots(painter, rect, color);
        }
    }
}

fn draw_helix_classic(painter: &egui::Painter, rect: Rect, time: f32, color: Color32) {
    const NUM_POINTS: usize = 16;
    let amplitude = rect.height() * 0.3;
    let dot_radius = 4.0;

    let width = rect.width() * 0.85;
    let offset_x = rect.left() + (rect.width() - width) / 2.0;

    for i in 0..NUM_POINTS {
        let t = i as f32 / NUM_POINTS as f32;
        let x = offset_x + t * width;
        let phase = time * 3.0 + t * std::f32::consts::PI * 2.0;

        let y1 = rect.center().y + phase.sin() * amplitude;
        let y2 = rect.center().y - phase.sin() * amplitude;

        // Connecting line
        painter.line_segment(
            [Pos2::new(x, y1), Pos2::new(x, y2)],
            Stroke::new(
                1.0,
                Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 60),
            ),
        );

        painter.circle_filled(Pos2::new(x, y1), dot_radius, color);
        painter.circle_filled(Pos2::new(x, y2), dot_radius, color);
    }
}

// ============================================================================
// Variant 2: Smooth Helix (more points, fluid)
// ============================================================================

fn draw_variant_2(
    painter: &egui::Painter,
    rect: Rect,
    time: f32,
    audio: f32,
    state: DictationState,
) {
    let color = state.color();

    match state {
        DictationState::Recording => {
            draw_tight_matrix(painter, rect, time, audio, color);
        }
        DictationState::Transcribing | DictationState::Enhancing => {
            draw_helix_smooth(painter, rect, time, color);
        }
        DictationState::Error => {
            draw_helix_smooth(painter, rect, time * 2.5, color);
        }
        DictationState::Idle => {
            draw_idle_dots(painter, rect, color);
        }
    }
}

fn draw_helix_smooth(painter: &egui::Painter, rect: Rect, time: f32, color: Color32) {
    const NUM_POINTS: usize = 28;
    let amplitude = rect.height() * 0.28;
    let dot_radius = 3.5;

    let width = rect.width() * 0.88;
    let offset_x = rect.left() + (rect.width() - width) / 2.0;

    for i in 0..NUM_POINTS {
        let t = i as f32 / NUM_POINTS as f32;
        let x = offset_x + t * width;
        let phase = time * 2.5 + t * std::f32::consts::PI * 2.5;

        let y1 = rect.center().y + phase.sin() * amplitude;
        let y2 = rect.center().y - phase.sin() * amplitude;

        // Softer connecting line
        let line_alpha = (((phase.sin().abs()) * 40.0) + 20.0) as u8;
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

// ============================================================================
// Variant 3: Double Helix (two intertwined)
// ============================================================================

fn draw_variant_3(
    painter: &egui::Painter,
    rect: Rect,
    time: f32,
    audio: f32,
    state: DictationState,
) {
    let color = state.color();

    match state {
        DictationState::Recording => {
            draw_tight_matrix(painter, rect, time, audio, color);
        }
        DictationState::Transcribing | DictationState::Enhancing => {
            draw_helix_double(painter, rect, time, color);
        }
        DictationState::Error => {
            draw_helix_double(painter, rect, time * 2.0, color);
        }
        DictationState::Idle => {
            draw_idle_dots(painter, rect, color);
        }
    }
}

fn draw_helix_double(painter: &egui::Painter, rect: Rect, time: f32, color: Color32) {
    const NUM_POINTS: usize = 20;
    let amplitude = rect.height() * 0.25;
    let dot_radius = 3.5;

    let width = rect.width() * 0.85;
    let offset_x = rect.left() + (rect.width() - width) / 2.0;

    // Two helixes offset by 90 degrees
    for helix in 0..2 {
        let phase_offset = helix as f32 * std::f32::consts::PI * 0.5;
        let alpha_mult = if helix == 0 { 1.0 } else { 0.6 };

        for i in 0..NUM_POINTS {
            let t = i as f32 / NUM_POINTS as f32;
            let x = offset_x + t * width;
            let phase = time * 2.8 + t * std::f32::consts::PI * 2.0 + phase_offset;

            let y1 = rect.center().y + phase.sin() * amplitude;
            let y2 = rect.center().y - phase.sin() * amplitude;

            let alpha = (200.0 * alpha_mult) as u8;
            let dot_color = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha);

            painter.circle_filled(Pos2::new(x, y1), dot_radius, dot_color);
            painter.circle_filled(Pos2::new(x, y2), dot_radius, dot_color);
        }
    }
}

// ============================================================================
// Variant 4: Wave Helix (amplitude modulation)
// ============================================================================

fn draw_variant_4(
    painter: &egui::Painter,
    rect: Rect,
    time: f32,
    audio: f32,
    state: DictationState,
) {
    let color = state.color();

    match state {
        DictationState::Recording => {
            draw_tight_matrix(painter, rect, time, audio, color);
        }
        DictationState::Transcribing | DictationState::Enhancing => {
            draw_helix_wave(painter, rect, time, color);
        }
        DictationState::Error => {
            draw_helix_wave(painter, rect, time * 2.0, color);
        }
        DictationState::Idle => {
            draw_idle_dots(painter, rect, color);
        }
    }
}

fn draw_helix_wave(painter: &egui::Painter, rect: Rect, time: f32, color: Color32) {
    const NUM_POINTS: usize = 22;
    let base_amplitude = rect.height() * 0.28;
    let dot_radius = 4.0;

    let width = rect.width() * 0.85;
    let offset_x = rect.left() + (rect.width() - width) / 2.0;

    for i in 0..NUM_POINTS {
        let t = i as f32 / NUM_POINTS as f32;
        let x = offset_x + t * width;

        // Amplitude modulation - wave travels along helix
        let amp_mod = (time * 1.5 + t * std::f32::consts::PI * 2.0).sin() * 0.4 + 0.6;
        let amplitude = base_amplitude * amp_mod;

        let phase = time * 3.0 + t * std::f32::consts::PI * 2.0;

        let y1 = rect.center().y + phase.sin() * amplitude;
        let y2 = rect.center().y - phase.sin() * amplitude;

        // Dot size also modulates
        let size = dot_radius * (0.7 + amp_mod * 0.5);

        painter.circle_filled(Pos2::new(x, y1), size, color);
        painter.circle_filled(Pos2::new(x, y2), size, color);

        // Connect with varying opacity
        let line_alpha = (amp_mod * 80.0) as u8;
        painter.line_segment(
            [Pos2::new(x, y1), Pos2::new(x, y2)],
            Stroke::new(
                1.0,
                Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), line_alpha),
            ),
        );
    }
}

// ============================================================================
// Variant 5: Trailing Helix (fading trail)
// ============================================================================

fn draw_variant_5(
    painter: &egui::Painter,
    rect: Rect,
    time: f32,
    audio: f32,
    state: DictationState,
) {
    let color = state.color();

    match state {
        DictationState::Recording => {
            draw_tight_matrix(painter, rect, time, audio, color);
        }
        DictationState::Transcribing | DictationState::Enhancing => {
            draw_helix_trailing(painter, rect, time, color);
        }
        DictationState::Error => {
            draw_helix_trailing(painter, rect, time * 2.0, color);
        }
        DictationState::Idle => {
            draw_idle_dots(painter, rect, color);
        }
    }
}

fn draw_helix_trailing(painter: &egui::Painter, rect: Rect, time: f32, color: Color32) {
    const NUM_POINTS: usize = 20;
    let amplitude = rect.height() * 0.28;
    let dot_radius = 4.0;

    let width = rect.width() * 0.85;
    let offset_x = rect.left() + (rect.width() - width) / 2.0;

    // Calculate "head" position that moves along
    let head_pos = (time * 0.4) % 1.0;

    for i in 0..NUM_POINTS {
        let t = i as f32 / NUM_POINTS as f32;
        let x = offset_x + t * width;
        let phase = time * 3.0 + t * std::f32::consts::PI * 2.0;

        let y1 = rect.center().y + phase.sin() * amplitude;
        let y2 = rect.center().y - phase.sin() * amplitude;

        // Trail fades based on distance from head
        let dist_from_head = (t - head_pos)
            .abs()
            .min((t - head_pos + 1.0).abs())
            .min((t - head_pos - 1.0).abs());
        let trail_factor = (1.0 - dist_from_head * 2.5).max(0.2);

        let alpha = (220.0 * trail_factor) as u8;
        let size = dot_radius * (0.6 + trail_factor * 0.6);
        let dot_color = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha);

        painter.circle_filled(Pos2::new(x, y1), size, dot_color);
        painter.circle_filled(Pos2::new(x, y2), size, dot_color);
    }
}

// ============================================================================
// Variant 6: Compact Helix (tighter, faster)
// ============================================================================

fn draw_variant_6(
    painter: &egui::Painter,
    rect: Rect,
    time: f32,
    audio: f32,
    state: DictationState,
) {
    let color = state.color();

    match state {
        DictationState::Recording => {
            draw_tight_matrix(painter, rect, time, audio, color);
        }
        DictationState::Transcribing | DictationState::Enhancing => {
            draw_helix_compact(painter, rect, time, color);
        }
        DictationState::Error => {
            draw_helix_compact(painter, rect, time * 2.5, color);
        }
        DictationState::Idle => {
            draw_idle_dots(painter, rect, color);
        }
    }
}

fn draw_helix_compact(painter: &egui::Painter, rect: Rect, time: f32, color: Color32) {
    const NUM_POINTS: usize = 32;
    let amplitude = rect.height() * 0.25;
    let dot_radius = 3.0;

    let width = rect.width() * 0.9;
    let offset_x = rect.left() + (rect.width() - width) / 2.0;

    for i in 0..NUM_POINTS {
        let t = i as f32 / NUM_POINTS as f32;
        let x = offset_x + t * width;
        // More rotations (3.5 full turns)
        let phase = time * 4.0 + t * std::f32::consts::PI * 7.0;

        let y1 = rect.center().y + phase.sin() * amplitude;
        let y2 = rect.center().y - phase.sin() * amplitude;

        painter.circle_filled(Pos2::new(x, y1), dot_radius, color);
        painter.circle_filled(Pos2::new(x, y2), dot_radius, color);
    }
}

// ============================================================================
// Idle state - simple faint dots
// ============================================================================

fn draw_idle_dots(painter: &egui::Painter, rect: Rect, color: Color32) {
    const NUM_DOTS: usize = 5;
    let dot_radius = 3.0;
    let spacing = rect.width() * 0.6 / NUM_DOTS as f32;
    let offset_x = rect.center().x - (NUM_DOTS as f32 - 1.0) * spacing / 2.0;

    let faint_color = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 40);

    for i in 0..NUM_DOTS {
        let x = offset_x + i as f32 * spacing;
        painter.circle_filled(Pos2::new(x, rect.center().y), dot_radius, faint_color);
    }
}
