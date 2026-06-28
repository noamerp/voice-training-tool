#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

include!(concat!(env!("OUT_DIR"), "/harvard_sentences.rs"));

use eframe::egui::{self, Color32, RichText, Vec2};
use rand::RngExt;
use rodio::{
    ChannelCount, DeviceSinkBuilder, MixerDeviceSink, Player, SampleRate, Source, source::Zero,
};
use std::f32::consts::PI;
use std::time::{Duration, Instant};

const E2_MIDI: u8 = 40;
const OCTAVE: u8 = 12;
const ROWS: u8 = 3; // E2-D#3, E3-D#4, E4-D#5

static NOTE_NAMES: &[&str] = &[
    "E2", "F2", "F#2", "G2", "G#2", "A2", "A#2", "B2", "C3", "C#3", "D3", "D#3", "E3", "F3", "F#3",
    "G3", "G#3", "A3", "A#3", "B3", "C4", "C#4", "D4", "D#4", "E4", "F4", "F#4", "G4", "G#4", "A4",
    "A#4", "B4", "C5", "C#5", "D5", "D#5",
];

fn midi_to_freq(midi: u8) -> f32 {
    440.0 * 2.0_f32.powf((midi as f32 - 69.0) / 12.0)
}

fn note_name(midi: u8) -> &'static str {
    NOTE_NAMES[(midi - E2_MIDI) as usize]
}

fn note_colors(midi: u8) -> (Color32, Color32) {
    if (41..=49).contains(&midi) {
        (Color32::from_rgb(60, 110, 210), Color32::WHITE)
    } else if (54..=61).contains(&midi) {
        (Color32::from_rgb(200, 80, 140), Color32::WHITE)
    } else {
        (
            Color32::from_rgb(55, 55, 55),
            Color32::from_rgb(220, 220, 220),
        )
    }
}

struct SquareWave {
    freq: f32,
    sample_rate: u32,
    num_sample: u64,
}

impl SquareWave {
    fn new(freq: f32) -> Self {
        SquareWave {
            freq,
            sample_rate: 44100,
            num_sample: 0,
        }
    }
}

impl Iterator for SquareWave {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let t = self.num_sample as f32 / self.sample_rate as f32;
        self.num_sample += 1;
        Some(if (t * self.freq).fract() < 0.5 {
            1.0
        } else {
            -1.0
        })
    }
}

impl Source for SquareWave {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> ChannelCount {
        ChannelCount::new(1).unwrap()
    }
    fn sample_rate(&self) -> SampleRate {
        SampleRate::new(self.sample_rate).unwrap()
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

struct SineWave {
    freq: f32,
    sample_rate: u32,
    num_sample: u64,
}

impl SineWave {
    fn new(freq: f32) -> Self {
        SineWave {
            freq,
            sample_rate: 44100,
            num_sample: 0,
        }
    }
}

impl Iterator for SineWave {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        let t = self.num_sample as f32 / self.sample_rate as f32;
        self.num_sample += 1;
        Some((2.0 * PI * self.freq * t).sin())
    }
}

impl Source for SineWave {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> ChannelCount {
        ChannelCount::new(1).unwrap()
    }
    fn sample_rate(&self) -> SampleRate {
        SampleRate::new(self.sample_rate).unwrap()
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

struct App {
    timer_end: Option<Instant>,
    timer_paused_remaining: Option<Duration>,
    timer_rung: bool,
    volume: f32,
    _stream: MixerDeviceSink,
    tone_sink: Player,
    ring_sink: Player,
    current_note: Option<u8>,
    current_list: usize,
    prev_list: usize,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx
            .memory_mut(|m| m.options.input_options.max_click_duration = f64::INFINITY);

        let stream = DeviceSinkBuilder::open_default_sink().expect("failed to open audio output");
        let tone_sink = Player::connect_new(&stream.mixer());
        let ring_sink = Player::connect_new(&stream.mixer());
        tone_sink.pause();

        let n = HARVARD_LISTS.len();
        let current_list = rand::rng().random_range(0..n);
        let prev_list = (current_list + 1) % n;

        Self {
            timer_end: None,
            timer_paused_remaining: None,
            timer_rung: false,
            volume: 0.3,
            _stream: stream,
            tone_sink,
            ring_sink,
            current_note: None,
            current_list,
            prev_list,
        }
    }

    fn refresh_list(&mut self) {
        let n = HARVARD_LISTS.len();
        let mut rng = rand::rng();
        self.prev_list = self.current_list;
        self.current_list = loop {
            let candidate = rng.random_range(0..n);
            if candidate != self.prev_list {
                break candidate;
            }
        };
    }

    fn start_timer(&mut self, secs: u64) {
        self.timer_end = Some(Instant::now() + Duration::from_secs(secs));
        self.timer_paused_remaining = None;
        self.timer_rung = false;
    }

    fn stop_timer(&mut self) {
        self.timer_end = None;
        self.timer_paused_remaining = None;
        self.timer_rung = false;
    }

    fn pause_timer(&mut self) {
        if let Some(rem) = self.remaining() {
            self.timer_paused_remaining = Some(rem);
            self.timer_end = None;
        }
    }

    fn resume_timer(&mut self) {
        if let Some(rem) = self.timer_paused_remaining.take() {
            if rem > Duration::ZERO {
                self.timer_end = Some(Instant::now() + rem);
            }
        }
    }

    fn remaining(&self) -> Option<Duration> {
        if let Some(rem) = self.timer_paused_remaining {
            return Some(rem);
        }
        self.timer_end.map(|end| {
            let now = Instant::now();
            if now >= end {
                Duration::ZERO
            } else {
                end - now
            }
        })
    }

    fn play_note(&mut self, midi: u8) {
        if self.current_note == Some(midi) {
            return;
        }
        self.tone_sink.clear();
        self.tone_sink.append(SquareWave::new(midi_to_freq(midi)));
        self.tone_sink.play();
        self.current_note = Some(midi);
    }

    fn stop_note(&mut self) {
        if self.current_note.take().is_some() {
            self.tone_sink.clear();
        }
    }

    fn play_ring(&mut self) {
        let ch = ChannelCount::new(1).unwrap();
        let sr = SampleRate::new(44100).unwrap();
        self.ring_sink.clear();
        for _ in 0..3 {
            self.ring_sink
                .append(SineWave::new(880.0).take_duration(Duration::from_millis(220)));
            self.ring_sink
                .append(Zero::new(ch, sr).take_duration(Duration::from_millis(130)));
        }
        self.ring_sink.play();
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.tone_sink.set_volume(self.volume);
        self.ring_sink.set_volume(self.volume);

        if self.timer_end.is_some() {
            if let Some(rem) = self.remaining() {
                if rem == Duration::ZERO && !self.timer_rung {
                    self.timer_rung = true;
                    self.play_ring();
                }
            }
        }

        let ctx = ui.ctx().clone();

        let mut frame = egui::Frame::new();
        frame.inner_margin.left += 16;
        frame.inner_margin.right += 16;
        frame.inner_margin.top += 12;
        frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Voice Training Tool");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(
                        egui::Slider::new(&mut self.volume, 0.0..=1.0)
                            .show_value(false)
                            .trailing_fill(true),
                    );
                    ui.label("Volume:");
                });
            });
            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);

            // --- Timer ---
            ui.label(RichText::new("Timer").strong().size(20.0));
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                for (label, secs) in [
                    ("30s", 30u64),
                    ("1m", 60),
                    ("2m", 120),
                    ("5m", 300),
                    ("10m", 600),
                    ("15m", 900),
                    ("30m", 1800),
                ] {
                    if ui
                        .add_sized([52.0, 32.0], egui::Button::new(label))
                        .clicked()
                    {
                        self.start_timer(secs);
                    }
                }
                ui.add_space(12.0);
                let is_running = self.timer_end.is_some();
                let has_timer = is_running || self.timer_paused_remaining.is_some();
                ui.add_enabled_ui(has_timer, |ui| {
                    let symbol = if is_running { "⏸" } else { "▶" };
                    if ui
                        .add_sized([36.0, 32.0], egui::Button::new(symbol))
                        .clicked()
                    {
                        if is_running {
                            self.pause_timer();
                        } else {
                            self.resume_timer();
                        }
                    }
                });
                ui.add_enabled_ui(has_timer, |ui| {
                    if ui.add_sized([36.0, 32.0], egui::Button::new("⏹")).clicked() {
                        self.stop_timer();
                    }
                });
            });

            ui.add_space(4.0);

            let display = match self.remaining() {
                Some(rem) => {
                    let s = rem.as_secs();
                    format!("{:02}:{:02}", s / 60, s % 60)
                }
                None => "--:--".to_string(),
            };
            ui.label(RichText::new(format!("Time remaining: {display}")).size(22.0));

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(8.0);

            // --- Notes ---
            ui.horizontal(|ui| {
                ui.label(RichText::new("Notes").strong().size(20.0));
                if let Some(midi) = self.current_note {
                    let freq = midi_to_freq(midi);
                    ui.label(
                        RichText::new(format!("{} Hz", freq.round() as u32))
                            .size(16.0)
                            .color(Color32::from_rgb(160, 160, 160)),
                    );
                }
            });
            ui.add_space(4.0);

            let mut note_pressed: Option<u8> = None;

            for row in 0..ROWS {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 3.0;
                    for col in 0..OCTAVE {
                        let midi = E2_MIDI + row * OCTAVE + col;
                        let (bg, fg) = note_colors(midi);
                        let fill = if self.current_note == Some(midi) {
                            Color32::from_rgb(
                                (bg.r() as u16 + 70).min(255) as u8,
                                (bg.g() as u16 + 70).min(255) as u8,
                                (bg.b() as u16 + 70).min(255) as u8,
                            )
                        } else {
                            bg
                        };
                        let btn =
                            egui::Button::new(RichText::new(note_name(midi)).color(fg).size(11.0))
                                .fill(fill);
                        let resp = ui.add_sized(Vec2::new(46.0, 54.0), btn);
                        if resp.is_pointer_button_down_on() {
                            note_pressed = Some(midi);
                        }
                    }
                });
                ui.add_space(3.0);
            }

            match note_pressed {
                Some(midi) => self.play_note(midi),
                None => self.stop_note(),
            }

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(8.0);

            // --- Sentences ---
            ui.horizontal(|ui| {
                ui.label(RichText::new("Sentences").strong().size(20.0));
                ui.add_space(8.0);
                ui.scope(|ui| {
                    ui.spacing_mut().button_padding = Vec2::new(14.0, 4.0);
                    if ui.button(RichText::new("Refresh").size(15.0)).clicked() {
                        self.refresh_list();
                    }
                });
            });
            ui.add_space(6.0);

            ui.indent("sentences", |ui| {
                for (i, sentence) in HARVARD_LISTS[self.current_list].iter().enumerate() {
                    ui.label(
                        RichText::new(format!("{}. {}", i + 1, sentence))
                            .size(15.0)
                            .color(Color32::WHITE),
                    );
                    ui.add_space(4.0);
                }
            });
        });

        ctx.request_repaint_after(Duration::from_millis(50));
    }
}

fn load_icon() -> egui::IconData {
    let svg_data = include_bytes!("../resources/app_icon.svg");
    let tree = resvg::usvg::Tree::from_data(svg_data, &resvg::usvg::Options::default())
        .expect("failed to parse icon SVG");
    let size = 256u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size).expect("failed to create pixmap");
    let sx = size as f32 / tree.size().width();
    let sy = size as f32 / tree.size().height();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(sx, sy),
        &mut pixmap.as_mut(),
    );
    egui::IconData {
        rgba: pixmap.take(),
        width: size,
        height: size,
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([640.0, 700.0])
            .with_resizable(false)
            .with_title("Voice Training Tool")
            .with_icon(std::sync::Arc::new(load_icon())),
        ..Default::default()
    };
    eframe::run_native(
        "Voice Training Tool",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}
