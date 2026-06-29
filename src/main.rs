#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

include!(concat!(env!("OUT_DIR"), "/harvard_sentences.rs"));
include!(concat!(env!("OUT_DIR"), "/common_voice_sentences.rs"));

use directories::ProjectDirs;
use eframe::egui::{self, Color32, RichText, Vec2};
use rand::RngExt;
use rand::seq::SliceRandom;
use rodio::{
    ChannelCount, DeviceSinkBuilder, MixerDeviceSink, Player, SampleRate, Source, source::Zero,
};
use serde::{Deserialize, Serialize};
use std::f32::consts::PI;
use std::path::PathBuf;
use std::sync::Arc;
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

#[derive(Serialize, Deserialize, Clone, PartialEq)]
enum Dataset {
    Harvard,
    CommonVoice,
}

#[derive(Serialize, Deserialize)]
struct Settings {
    volume: f32,
    dataset: Dataset,
    cv_language: String,
    cv_sentence_count: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            volume: 0.3,
            dataset: Dataset::Harvard,
            cv_language: "en".to_string(),
            cv_sentence_count: 10,
        }
    }
}

fn settings_path() -> Option<PathBuf> {
    ProjectDirs::from("", "", "VoiceTrainingTool")
        .map(|dirs| dirs.config_dir().join("settings.toml"))
}

fn load_settings() -> Settings {
    settings_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_settings(settings: &Settings) {
    if let Some(path) = settings_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(s) = toml::to_string(settings) {
            let _ = std::fs::write(path, s);
        }
    }
}

fn pick_cv_sentences(lang: &str, count: usize) -> Vec<&'static str> {
    let sentences = COMMON_VOICE_SENTENCES
        .iter()
        .find(|(l, _)| *l == lang)
        .map(|(_, s)| *s)
        .unwrap_or(&[]);
    if sentences.is_empty() {
        return vec![];
    }
    let count = count.min(sentences.len());
    let mut indices: Vec<usize> = (0..sentences.len()).collect();
    indices.shuffle(&mut rand::rng());
    indices[..count].iter().map(|&i| sentences[i]).collect()
}

fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // Candidate system font paths, tried in order and added as fallbacks after the default
    // Latin/Cyrillic/Greek fonts. Any path that doesn't exist is silently skipped.
    let candidates: &[&str] = &[
        // ── macOS ─────────────────────────────────────────────────────────────────
        // CJK
        "/System/Library/Fonts/STHeiti Light.ttc",
        "/System/Library/Fonts/STHeiti Medium.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        // Arabic (ar, ckb, fa, ps, skr, ug, ur)
        "/System/Library/Fonts/SFArabic.ttf",
        "/System/Library/Fonts/GeezaPro.ttc",
        // Hebrew (he, yi)
        "/System/Library/Fonts/SFHebrew.ttf",
        // Georgian (ka)
        "/System/Library/Fonts/SFGeorgian.ttf",
        // Devanagari (hi, mr, ne-NP)
        "/System/Library/Fonts/Kohinoor.ttc",
        "/System/Library/Fonts/Supplemental/ITFDevanagari.ttc",
        // Bengali/Assamese (as)
        "/System/Library/Fonts/KohinoorBangla.ttc",
        // Telugu (te)
        "/System/Library/Fonts/KohinoorTelugu.ttc",
        "/System/Library/Fonts/Supplemental/Telugu MN.ttc",
        // Kannada (kn)
        "/System/Library/Fonts/NotoSansKannada.ttc",
        // Odia (or)
        "/System/Library/Fonts/NotoSansOriya.ttc",
        "/System/Library/Fonts/Supplemental/Oriya MN.ttc",
        // Myanmar (my)
        "/System/Library/Fonts/NotoSansMyanmar.ttc",
        "/System/Library/Fonts/Supplemental/Myanmar MN.ttc",
        // Thai (th)
        "/System/Library/Fonts/ThonburiUI.ttc",
        // Ethiopic (am, ti, tig)
        "/System/Library/Fonts/Supplemental/KefaIII.ttf",
        // Tamil (ta)
        "/System/Library/Fonts/Supplemental/Tamil MN.ttc",
        // Malayalam (ml)
        "/System/Library/Fonts/Supplemental/Malayalam MN.ttc",
        // Gurmukhi/Punjabi (pa-IN)
        "/System/Library/Fonts/Supplemental/Gurmukhi MN.ttc",
        // Khmer (km)
        "/System/Library/Fonts/Supplemental/Khmer MN.ttc",
        // Lao (lo)
        "/System/Library/Fonts/Supplemental/Lao MN.ttc",
        // Sinhala (si)
        "/System/Library/Fonts/Supplemental/Sinhala MN.ttc",
        // Thaana (dv)
        "/System/Library/Fonts/Supplemental/NotoSansThaana-Regular.ttf",
        // Tifinagh (zgh)
        "/System/Library/Fonts/Supplemental/NotoSansTifinagh-Regular.otf",
        // Ol Chiki (sat)
        "/System/Library/Fonts/Supplemental/NotoSansOlChiki-Regular.ttf",
        // Broad Unicode fallback
        "/Library/Fonts/Arial Unicode.ttf",
        // ── Linux ─────────────────────────────────────────────────────────────────
        // CJK (multiple common distro paths)
        "/usr/share/fonts/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        // Arabic
        "/usr/share/fonts/truetype/noto/NotoNaskhArabic-Regular.ttf",
        "/usr/share/fonts/noto/NotoNaskhArabic-Regular.ttf",
        // Hebrew
        "/usr/share/fonts/truetype/noto/NotoSansHebrew-Regular.ttf",
        "/usr/share/fonts/noto/NotoSansHebrew-Regular.ttf",
        // Georgian
        "/usr/share/fonts/truetype/noto/NotoSansGeorgian-Regular.ttf",
        // Devanagari
        "/usr/share/fonts/truetype/noto/NotoSansDevanagari-Regular.ttf",
        "/usr/share/fonts/noto/NotoSansDevanagari-Regular.ttf",
        // Bengali
        "/usr/share/fonts/truetype/noto/NotoSansBengali-Regular.ttf",
        "/usr/share/fonts/noto/NotoSansBengali-Regular.ttf",
        // Telugu
        "/usr/share/fonts/truetype/noto/NotoSansTelugu-Regular.ttf",
        // Kannada
        "/usr/share/fonts/truetype/noto/NotoSansKannada-Regular.ttf",
        // Odia
        "/usr/share/fonts/truetype/noto/NotoSansOriya-Regular.ttf",
        // Myanmar
        "/usr/share/fonts/truetype/noto/NotoSansMyanmar-Regular.ttf",
        // Thai
        "/usr/share/fonts/truetype/noto/NotoSansThai-Regular.ttf",
        "/usr/share/fonts/noto/NotoSansThai-Regular.ttf",
        // Ethiopic
        "/usr/share/fonts/truetype/noto/NotoSansEthiopic-Regular.ttf",
        "/usr/share/fonts/noto/NotoSansEthiopic-Regular.ttf",
        // Tamil
        "/usr/share/fonts/truetype/noto/NotoSansTamil-Regular.ttf",
        // Malayalam
        "/usr/share/fonts/truetype/noto/NotoSansMalayalam-Regular.ttf",
        // Gurmukhi
        "/usr/share/fonts/truetype/noto/NotoSansGurmukhi-Regular.ttf",
        // Khmer
        "/usr/share/fonts/truetype/noto/NotoSansKhmer-Regular.ttf",
        // Lao
        "/usr/share/fonts/truetype/noto/NotoSansLao-Regular.ttf",
        // Sinhala
        "/usr/share/fonts/truetype/noto/NotoSansSinhala-Regular.ttf",
        // Thaana
        "/usr/share/fonts/truetype/noto/NotoSansThaana-Regular.ttf",
        // Tifinagh
        "/usr/share/fonts/truetype/noto/NotoSansTifinagh-Regular.ttf",
        // Ol Chiki
        "/usr/share/fonts/truetype/noto/NotoSansOlChiki-Regular.ttf",
        // ── Windows ───────────────────────────────────────────────────────────────
        // CJK
        "C:\\Windows\\Fonts\\msyh.ttc",
        "C:\\Windows\\Fonts\\simsun.ttc",
        "C:\\Windows\\Fonts\\simhei.ttf",
        // Arabic + Hebrew + Thai (Tahoma covers all three)
        "C:\\Windows\\Fonts\\tahoma.ttf",
        // Georgian
        "C:\\Windows\\Fonts\\sylfaen.ttf",
        // Devanagari
        "C:\\Windows\\Fonts\\mangal.ttf",
        // Bengali
        "C:\\Windows\\Fonts\\vrinda.ttf",
        // Telugu
        "C:\\Windows\\Fonts\\gautami.ttf",
        // Kannada
        "C:\\Windows\\Fonts\\tunga.ttf",
        // Odia
        "C:\\Windows\\Fonts\\kalinga.ttf",
        // Myanmar
        "C:\\Windows\\Fonts\\mmrtext.ttf",
        // Thai (also in tahoma.ttf above)
        "C:\\Windows\\Fonts\\leelawad.ttf",
        // Ethiopic
        "C:\\Windows\\Fonts\\ebrima.ttf",
        // Tamil
        "C:\\Windows\\Fonts\\latha.ttf",
        // Malayalam
        "C:\\Windows\\Fonts\\kartika.ttf",
        // Gurmukhi
        "C:\\Windows\\Fonts\\raavi.ttf",
        // Khmer
        "C:\\Windows\\Fonts\\khmeruib.ttf",
        // Lao
        "C:\\Windows\\Fonts\\laoui.ttf",
        // Sinhala
        "C:\\Windows\\Fonts\\iskpota.ttf",
        // Broad Unicode fallback
        "C:\\Windows\\Fonts\\ARIALUNI.TTF",
    ];

    for (i, path) in candidates.iter().enumerate() {
        if let Ok(data) = std::fs::read(path) {
            let name = format!("unicode_fallback_{i}");
            fonts
                .font_data
                .insert(name.clone(), Arc::new(egui::FontData::from_owned(data)));
            for family in [
                &egui::FontFamily::Proportional,
                &egui::FontFamily::Monospace,
            ] {
                fonts
                    .families
                    .entry(family.clone())
                    .or_default()
                    .push(name.clone());
            }
        }
    }

    ctx.set_fonts(fonts);
}

struct App {
    timer_end: Option<Instant>,
    timer_paused_remaining: Option<Duration>,
    timer_rung: bool,
    settings: Settings,
    settings_open: bool,
    _stream: MixerDeviceSink,
    tone_sink: Player,
    ring_sink: Player,
    current_note: Option<u8>,
    current_list: usize,
    prev_list: usize,
    cv_current_sentences: Vec<&'static str>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        cc.egui_ctx
            .memory_mut(|m| m.options.input_options.max_click_duration = f64::INFINITY);
        setup_fonts(&cc.egui_ctx);

        let stream = DeviceSinkBuilder::open_default_sink().expect("failed to open audio output");
        let tone_sink = Player::connect_new(&stream.mixer());
        let ring_sink = Player::connect_new(&stream.mixer());
        tone_sink.pause();

        let settings = load_settings();

        let n = HARVARD_LISTS.len();
        let current_list = rand::rng().random_range(0..n);
        let prev_list = (current_list + 1) % n;

        let cv_current_sentences = if settings.dataset == Dataset::CommonVoice {
            pick_cv_sentences(&settings.cv_language, settings.cv_sentence_count)
        } else {
            vec![]
        };

        Self {
            timer_end: None,
            timer_paused_remaining: None,
            timer_rung: false,
            settings,
            settings_open: false,
            _stream: stream,
            tone_sink,
            ring_sink,
            current_note: None,
            current_list,
            prev_list,
            cv_current_sentences,
        }
    }

    fn refresh_sentences(&mut self) {
        match self.settings.dataset {
            Dataset::Harvard => {
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
            Dataset::CommonVoice => {
                self.cv_current_sentences =
                    pick_cv_sentences(&self.settings.cv_language, self.settings.cv_sentence_count);
            }
        }
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
        let ctx = ui.ctx().clone();

        self.tone_sink.set_volume(self.settings.volume);
        self.ring_sink.set_volume(self.settings.volume);

        if self.timer_end.is_some() {
            if let Some(rem) = self.remaining() {
                if rem == Duration::ZERO && !self.timer_rung {
                    self.timer_rung = true;
                    self.play_ring();
                }
            }
        }

        let mut frame = egui::Frame::new();
        frame.inner_margin.left += 16;
        frame.inner_margin.right += 16;
        frame.inner_margin.top += 12;
        let frame_resp = frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Voice Training Tool");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(RichText::new("⚙").size(20.0))
                        .on_hover_text("Settings")
                        .clicked()
                    {
                        self.settings_open = !self.settings_open;
                    }
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

            ui.add_space(10.0);

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
                        self.refresh_sentences();
                    }
                });
            });
            ui.add_space(6.0);

            ui.indent("sentences", |ui| {
                let sentences: &[&str] = match self.settings.dataset {
                    Dataset::Harvard => HARVARD_LISTS[self.current_list],
                    Dataset::CommonVoice => self.cv_current_sentences.as_slice(),
                };
                for (i, sentence) in sentences.iter().enumerate() {
                    ui.label(
                        RichText::new(format!("{}. {}", i + 1, sentence))
                            .size(15.0)
                            .color(Color32::WHITE),
                    );
                    ui.add_space(4.0);
                }
            });
            ui.add_space(12.0);
        });

        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(
            frame_resp.response.rect.size(),
        ));

        // --- Settings window ---
        let mut settings_open = self.settings_open;
        egui::Window::new("Settings")
            .open(&mut settings_open)
            .collapsible(false)
            .resizable(false)
            .default_pos(ctx.input(|i| i.viewport_rect()).center() - egui::Vec2::new(140.0, 100.0))
            .default_width(280.0)
            .show(&ctx, |ui| {
                ui.label(RichText::new("Volume").strong());
                let resp = ui.add(
                    egui::Slider::new(&mut self.settings.volume, 0.0..=1.0)
                        .show_value(false)
                        .trailing_fill(true),
                );
                if resp.changed() {
                    save_settings(&self.settings);
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                ui.label(RichText::new("Sentence dataset").strong());
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let is_harvard = self.settings.dataset == Dataset::Harvard;
                    if ui.selectable_label(is_harvard, "Harvard").clicked() && !is_harvard {
                        self.settings.dataset = Dataset::Harvard;
                        save_settings(&self.settings);
                    }
                    let is_cv = self.settings.dataset == Dataset::CommonVoice;
                    if ui.selectable_label(is_cv, "Common Voice").clicked() && !is_cv {
                        self.settings.dataset = Dataset::CommonVoice;
                        if self.cv_current_sentences.is_empty() {
                            self.cv_current_sentences = pick_cv_sentences(
                                &self.settings.cv_language,
                                self.settings.cv_sentence_count,
                            );
                        }
                        save_settings(&self.settings);
                    }
                });

                if self.settings.dataset == Dataset::CommonVoice {
                    ui.add_space(8.0);
                    ui.label(RichText::new("Language").strong());
                    let old_lang = self.settings.cv_language.clone();
                    egui::ComboBox::from_label("")
                        .selected_text(self.settings.cv_language.clone())
                        .show_ui(ui, |ui| {
                            for &lang in COMMON_VOICE_LANGUAGES {
                                ui.selectable_value(
                                    &mut self.settings.cv_language,
                                    lang.to_string(),
                                    lang,
                                );
                            }
                        });
                    if self.settings.cv_language != old_lang {
                        self.cv_current_sentences = pick_cv_sentences(
                            &self.settings.cv_language,
                            self.settings.cv_sentence_count,
                        );
                        save_settings(&self.settings);
                    }

                    ui.add_space(8.0);
                    ui.label(RichText::new("Sentences per refresh").strong());
                    let mut count = self.settings.cv_sentence_count;
                    if ui.add(egui::Slider::new(&mut count, 1..=20usize)).changed() {
                        self.settings.cv_sentence_count = count;
                        self.cv_current_sentences = pick_cv_sentences(
                            &self.settings.cv_language,
                            self.settings.cv_sentence_count,
                        );
                        save_settings(&self.settings);
                    }
                }
            });
        self.settings_open = settings_open;

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
            .with_inner_size([620.0, 700.0])
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
