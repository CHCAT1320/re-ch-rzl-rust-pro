#![windows_subsystem = "console"]

mod chart;
mod ease;
#[cfg(target_arch = "wasm32")]
mod web;
#[cfg(target_os = "ios")]
mod ios;

use macroquad::prelude::*;
use chart::Chart;
use sasa::{AudioManager, PlaySfxParams, Sfx, backend::cpal::{CpalBackend, CpalSettings}};
use sasa::AudioClip;
use sasa::MusicParams;
use std::cell::{Cell, RefCell};
use std::f64::consts::PI;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

use crate::chart::{Theme, tick_to_seconds_impl};

// 强制使用独立显卡（NVIDIA Optimus / AMD PowerXpress）
#[cfg(target_os = "windows")]
#[unsafe(no_mangle)]
#[allow(non_upper_case_globals)]
pub static NvOptimusEnablement: u32 = 1;

#[cfg(target_os = "windows")]
#[unsafe(no_mangle)]
#[allow(non_upper_case_globals)]
pub static AmdPowerXpressRequestHighPerformance: i32 = 1;

const FONT_DATA: &[u8] = include_bytes!("../assets/fonts/rizline-subset.ttf");

thread_local! {
    static RENDER_WIDTH: Cell<f32> = Cell::new(0.0);
    static RENDER_HEIGHT: Cell<f32> = Cell::new(0.0);
    static REVELATION_SIZE: Cell<f64> = Cell::new(1.0);
    static SPEED: Cell<f64> = Cell::new(7.0);
}

fn revelation_size() -> f64 {
    REVELATION_SIZE.with(|c| c.get())
}

pub fn set_revelation_size(value: f64) {
    if value.is_finite() && value > 0.0 {
        REVELATION_SIZE.with(|c| c.set(value));
    }
}

fn speed() -> f64 {
    SPEED.with(|c| c.get())
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn set_speed(value: f64) {
    if value.is_finite() && value > 0.0 {
        SPEED.with(|c| c.set(value));
    }
}

fn render_w() -> f32 {
    let rw = RENDER_WIDTH.with(|c| c.get());
    if rw > 0.0 {
        rw
    } else {
        screen_width()
    }
}

fn render_h() -> f32 {
    let rh = RENDER_HEIGHT.with(|c| c.get());
    if rh > 0.0 {
        rh
    } else {
        screen_height()
    }
}

fn screen_radio_w() -> f64 {
    render_w() as f64 / 720.0
}

fn screen_radio_h() -> f64 {
    render_h() as f64 / 1280.0
}

fn scale_x() -> f64 {
    revelation_size() * screen_radio_w()
}

fn scale_y() -> f64 {
    revelation_size() * screen_radio_h()
}

fn center_x() -> f64 {
    360.0 * screen_radio_w()
}

fn window_conf() -> Conf {
    Conf {
        window_title: "ch-rzl-player".to_owned(),
        window_width: 540,
        window_height: 960,
        fullscreen: false,
        window_resizable: true,
        high_dpi: false,
        sample_count: 16,
        platform: macroquad::miniquad::conf::Platform {
            // Render targets need WebGL2: miniquad's MSAA resolve uses
            // glReadBuffer and READ/DRAW_FRAMEBUFFER, which do not exist in
            // WebGL1. Calling them throws a JS exception straight back into
            // wasm, which leaks miniquad's event-handler borrow and makes
            // every later input event trap with "unreachable executed".
            webgl_version: macroquad::miniquad::conf::WebGLVersion::WebGL2,
            ..Default::default()
        },
        ..Default::default()
    }
}

fn update_fps(display_fps: &mut i32, last_update: &mut f64) {
    let now = get_time();
    if now - *last_update >= 1.0 {
        *display_fps = get_fps();
        *last_update = now;
    }
}

fn floor_y() -> f64 {
    1040.0 * screen_radio_h()
}

fn draw_scaled_range() {
    if revelation_size() >= 1.0 {
        return;
    }
    let x = 360.0 * screen_radio_w() * (1.0 - revelation_size());
    let y = 1040.0 * screen_radio_h() * (1.0 - revelation_size());
    let w = 720.0 * scale_x();
    let h = 1280.0 * scale_y();
    draw_rectangle_lines(x as f32, y as f32, w as f32, h as f32, 2.0, RED);
}

fn calculate_combo(comb: i32) -> i32 {
    if comb == 0 {
        0
    } else if comb <= 5 {
        comb
    } else if comb <= 8 {
        2 * comb - 5
    } else if comb <= 11 {
        3 * comb - 13
    } else {
        4 * comb - 24
    }
}

fn count_hits(chart: &Chart) -> i32 {
    let mut hit_count = 0;
    for line in &chart.lines {
        for note in &line.notes {
            if note.is_hited {
                hit_count += 1;
                if note.note_type == 2 {
                    hit_count += 1;
                }
            }
        }
    }
    hit_count
}

fn draw_text_outlined(text: &str, x: f32, y: f32, font_size: f32, fill: Color, outline: Color, thickness: f32) {
    for (dx, dy) in [
        (-1.0, 0.0),
        (1.0, 0.0),
        (0.0, -1.0),
        (0.0, 1.0),
        (-1.0, -1.0),
        (1.0, -1.0),
        (-1.0, 1.0),
        (1.0, 1.0),
    ] {
        macroquad::text::draw_text(text, x + dx * thickness, y + dy * thickness, font_size, outline);
    }
    macroquad::text::draw_text(text, x, y, font_size, fill);
}

// Every on-screen text is drawn through this shadow so it stays readable over
// light chart backgrounds. A fully transparent fill keeps the glyph-atlas
// warm-up calls invisible while still caching the glyphs.
fn draw_text(text: &str, x: f32, y: f32, font_size: f32, color: Color) {
    if color.a <= 0.0 {
        macroquad::text::draw_text(text, x, y, font_size, color);
        return;
    }
    draw_text_outlined(text, x, y, font_size, color, BLACK, (font_size * 0.05).max(1.0));
}

fn draw_combo(chart: &Chart) {
    let combo = calculate_combo(count_hits(chart));
    if combo == 0 {
        return;
    }
    let combo_text = combo.to_string();
    let combo_font = 60.0 * screen_radio_w();
    let label_font = 40.0 * screen_radio_w();
    let thickness = 2.0 * screen_radio_w();

    let right_edge = 670.0 * screen_radio_w();
    let bottom_y = 130.0 * screen_radio_h();

    let combo_dim = measure_text(&combo_text, None, combo_font as u16, 1.0);
    let combo_x = (right_edge - combo_dim.width as f64) as f32;
    let combo_y = (bottom_y + combo_dim.offset_y as f64 - combo_dim.height as f64) as f32;

    let label_dim = measure_text("CATPLAY", None, label_font as u16, 1.0);
    let label_x = (combo_x as f64 - label_dim.width as f64) as f32;
    let label_y = (bottom_y + label_dim.offset_y as f64 - label_dim.height as f64) as f32;

    draw_text_outlined(&combo_text, combo_x, combo_y, combo_font as f32, BLACK, WHITE, thickness as f32);
    draw_text_outlined("CATPLAY", label_x, label_y, label_font as f32, BLACK, WHITE, thickness as f32);
}

fn draw_revelation_info(chart: &Chart, time: f64) {
    if revelation_size() >= 1.0 {
        return;
    }
    let font_size = 24.0 * screen_radio_w();
    let x = 20.0 * screen_radio_w();
    let mut y = 60.0 * screen_radio_h();

    let sample = "Canvas count: 0";
    let dim = measure_text(sample, None, font_size as u16, 1.0);
    let line_height = dim.height as f64 + 10.0 * screen_radio_w();

    let move_count: usize = chart.canvas_moves.iter().map(|c| c.x_position_key_points.len()).sum();
    let speed_count: usize = chart.canvas_moves.iter().map(|c| c.speed_key_points.len()).sum();
    let point_count: usize = chart.lines.iter().map(|l| l.line_points.len()).sum();
    let note_count: usize = chart.lines.iter().map(|l| l.notes.len()).sum();
    let camera_pos = find_canmera_move(chart, time);

    let lines: [String; 13] = [
        format!("Canvas count: {}", chart.canvas_moves.len()),
        format!("Canvas move event count: {}", move_count),
        format!("Canvas speed event count: {}", speed_count),
        format!("Line count: {}", chart.lines.len()),
        format!("Point count: {}", point_count),
        format!("Note count: {}", note_count),
        format!("Camera scale: {}", camera_pos[1]),
        format!("Revelation scale: {}", revelation_size()),
        format!("Camera scale event count: {}", chart.camera_move.scale_key_points.len()),
        format!("Camera move event count: {}", chart.camera_move.x_position_key_points.len()),
        format!("Camera X: {}", camera_pos[0]),
        format!("Challange time count: {}", chart.challenge_times.len()),
        format!("Speed: {}", speed()),
    ];

    for line in &lines {
        draw_text(line, x as f32, y as f32, font_size as f32, WHITE);
        y += line_height;
    }
}

fn draw_shui_yin() {
    let (base_font, text) = if revelation_size() >= 1.0 {
        (24.0 * screen_radio_w(), "CH-RZL-RUST PLAYER VERSION 0.1.3 ALL CODE BY CHCAT1320")
    } else {
        (18.0 * screen_radio_w(), "CHART REVELATION : CH-RZL-RUST PLAYER VERSION 0.1.3 ALL CODE BY CHCAT1320")
    };
    let max_w = render_w() as f64 * 0.97;
    let base_dim = measure_text(text, None, base_font as u16, 1.0);
    let font_size = if base_dim.width as f64 > max_w {
        base_font * (max_w / base_dim.width as f64)
    } else {
        base_font
    };
    let dim = measure_text(text, None, font_size as u16, 1.0);
    let tx = (center_x() - dim.width as f64 / 2.0) as f32;
    let ty = (700.0 * screen_radio_h() + dim.offset_y as f64 - dim.height as f64 / 2.0) as f32;
    let thickness = (1.0 * screen_radio_w()) as f32;
    draw_text_outlined(text, tx, ty, font_size as f32, WHITE, BLACK, thickness);
}

fn speed_ratio() -> f64 {
    (215.0 / 32.0 + speed()) * (10.0 / 129.0)
}

thread_local! {
    static RNG_STATE: Cell<u64> = Cell::new(0x9E3779B97F4A7C15);
}

fn init_rng() {
    let seed = ((get_time() * 1e9) as u64) | 1;
    RNG_STATE.with(|s| s.set(seed));
}

fn rand_f64() -> f64 {
    RNG_STATE.with(|s| {
        let mut x = s.get();
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        s.set(x);
        (x >> 11) as f64 / (1u64 << 53) as f64
    })
}

// 加载谱面时对各类按时间排序的数据统一排序
fn sort_chart(chart: &mut Chart) {
    // 每个 canvas 的 x/speed 关键点按时间排序
    for canvas in &mut chart.canvas_moves {
        canvas.x_position_key_points.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
        canvas.speed_key_points.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
    }
    // camera 的 scale/x 关键点按时间排序
    chart.camera_move.scale_key_points.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
    chart.camera_move.x_position_key_points.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
    // bpm 变速点按时间排序
    chart.bpm_shifts.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
    // 每条线的 linePoints / notes 按时间排序
    for line in &mut chart.lines {
        line.line_points.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
        line.notes.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());
    }
    // 空 line 没有首点，放到末尾，避免不完整谱面在加载阶段崩溃
    chart.lines.sort_by(|a, b| {
        a.line_points
            .first()
            .map(|point| point.time)
            .unwrap_or(f64::INFINITY)
            .partial_cmp(
                &b.line_points
                    .first()
                    .map(|point| point.time)
                    .unwrap_or(f64::INFINITY),
            )
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

#[allow(dead_code)]
fn find_riz_time_theme(chart: &Chart, time: f64) -> Theme {
    if chart.themes.is_empty() {
        return Theme { colors_list: Vec::new() };
    }
    let default = chart.themes[0].clone();
    for (idx, ct) in chart.challenge_times.iter().enumerate() {
        if time >= chart.tick_to_seconds(ct.start) && time <= chart.tick_to_seconds(ct.end) + ct.trans_time {
            let theme_idx = (idx + 1).min(chart.themes.len() - 1);
            return chart.themes[theme_idx].clone();
        }
    }
    default
}

fn theme_color(theme: &Theme, color_index: usize, fallback: chart::Color) -> chart::Color {
    theme.colors_list.get(color_index).copied().unwrap_or(fallback)
}

fn default_note_color() -> chart::Color {
    chart::Color { r: 255, g: 255, b: 255, a: 255 }
}

// 返回参考实现中的 lastThemeIndex：只有挑战完全激活时才切换全局主题。
// 进入/退出阶段由 draw_challenge_time 的遮罩负责，不应让所有 note 瞬间变色。
fn find_active_challenge_theme_index(chart: &Chart, time: f64) -> Option<usize> {
    let mut active = None;
    for (idx, ct) in chart.challenge_times.iter().enumerate() {
        let start_secs = chart.tick_to_seconds(ct.start);
        let end_secs = chart.tick_to_seconds(ct.end);
        let trans_start_secs = start_secs + ct.trans_time;
        let is_active = if ct.trans_time <= 0.0 {
            time >= start_secs && time <= end_secs
        } else {
            time > trans_start_secs && time <= end_secs
        };
        if is_active && idx + 1 < chart.themes.len() {
            active = Some(idx + 1);
        }
    }
    active
}

// 查找当前全局 note 颜色。颜色按播放时间对应的主题获取，而不是按 note.time 获取。
fn note_color_for_theme(chart: &Chart, theme_idx: Option<usize>) -> chart::Color {
    let fallback = default_note_color();
    let Some(default_theme) = chart.themes.first() else {
        return fallback;
    };
    let theme = theme_idx
        .and_then(|idx| chart.themes.get(idx))
        .unwrap_or(default_theme);
    theme_color(theme, 1, theme_color(default_theme, 1, fallback))
}

fn effect_color_for_theme(chart: &Chart, theme_idx: Option<usize>) -> chart::Color {
    let fallback = chart::Color { r: 255, g: 255, b: 255, a: 255 };
    let Some(default_theme) = chart.themes.first() else {
        return fallback;
    };
    let theme = theme_idx
        .and_then(|idx| chart.themes.get(idx))
        .unwrap_or(default_theme);
    theme_color(theme, 2, theme_color(default_theme, 2, fallback))
}

fn draw_background(chart: &Chart, theme_idx: Option<usize>) {
    let screen_width = render_w();
    let screen_height = render_h();
    let bg = chart
        .themes
        .get(theme_idx.unwrap_or(0))
        .map(|theme| theme_color(theme, 0, chart::Color { r: 0, g: 0, b: 0, a: 255 }))
        .unwrap_or(chart::Color { r: 0, g: 0, b: 0, a: 255 });
    let color = [bg.r, bg.g, bg.b, 255];
    draw_rectangle(0.0, 0.0, screen_width as f32, screen_height as f32, color.into());
}

fn recalculate_all_fp(chart: &mut Chart) {
    let canvas_count = chart.canvas_moves.len();
    let mut speed_fps: Vec<Vec<f64>> = Vec::with_capacity(canvas_count);
    for canvas in &chart.canvas_moves {
        let mut fps = Vec::new();
        let mut prev_seconds = 0.0f64;
        for i in 0..canvas.speed_key_points.len() {
            let secs = chart.tick_to_seconds(canvas.speed_key_points[i].time);
            if i == 0 {
                fps.push(0.0);
            } else {
                let dt = secs - prev_seconds;
                fps.push(fps[i - 1] + dt * canvas.speed_key_points[i - 1].value);
            }
            prev_seconds = secs;
        }
        speed_fps.push(fps);
    }

    for (canvas_idx, canvas) in chart.canvas_moves.iter_mut().enumerate() {
        for i in 0..canvas.speed_key_points.len() {
            canvas.speed_key_points[i].floor_position = speed_fps[canvas_idx][i];
        }
    }

    let mut line_point_fps: Vec<Vec<f64>> = Vec::new();
    let mut note_data: Vec<Vec<(f64, Option<f64>)>> = Vec::new();
    for line in &chart.lines {
        let mut lpfps = Vec::new();
        for point in &line.line_points {
            let secs = chart.tick_to_seconds(point.time);
            let skp = &chart.canvas_moves[point.canvas_index as usize].speed_key_points;
            lpfps.push(speed_to_fp(secs, skp, chart));
        }
        line_point_fps.push(lpfps);

        let mut nd = Vec::new();
        for note in &line.notes {
            let canvas_index = find_line_point_canvas_index(&line.line_points, note.time);
            let secs = chart.tick_to_seconds(note.time);
            let skp = &chart.canvas_moves[canvas_index as usize].speed_key_points;
            let fp = speed_to_fp(secs, skp, chart);

            let tail_fp = if note.note_type == 2 {
                if let Some(ref infos) = note.other_informations {
                    if infos.len() >= 3 {
                        let tail_time = infos[0];
                        let tail_canvas_index = infos[1] as i32;
                        let tail_secs = chart.tick_to_seconds(tail_time);
                        let tail_skp = &chart.canvas_moves[tail_canvas_index as usize].speed_key_points;
                        Some(speed_to_fp(tail_secs, tail_skp, chart))
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };
            nd.push((fp, tail_fp));
        }
        note_data.push(nd);
    }

    for (line_idx, line) in chart.lines.iter_mut().enumerate() {
        for (point_idx, point) in line.line_points.iter_mut().enumerate() {
            point.floor_position = line_point_fps[line_idx][point_idx];
        }
        for (note_idx, note) in line.notes.iter_mut().enumerate() {
            let (fp, tail_fp) = note_data[line_idx][note_idx];
            note.floor_position = fp;
            if let Some(tail_fp) = tail_fp {
                if let Some(ref mut infos) = note.other_informations {
                    if infos.len() > 2 {
                        infos[2] = tail_fp;
                    }
                }
            }
        }
    }
}

fn find_line_point_canvas_index(line_points: &[chart::LinePoint], tick: f64) -> i32 {
    if line_points.is_empty() {
        return 0;
    }
    let mut left = 0isize;
    let mut right = line_points.len() as isize - 1;
    let mut idx = right as usize;

    while left <= right {
        let mid = (left + right) as usize / 2;
        if line_points[mid].time <= tick {
            idx = mid;
            left = mid as isize + 1;
        } else {
            right = mid as isize - 1;
        }
    }
    line_points[idx].canvas_index
}

fn find_key_point_value(seconds: f64, events: &[chart::KeyPoint], chart: &Chart) -> f64 {
    let tick = chart.seconds_to_tick(seconds);

    if events.is_empty() {
        return 0.0;
    }
    if events.len() == 1 {
        return if tick >= events[0].time { events[0].value } else { 0.0 };
    }

    let last = &events[events.len() - 1];
    if tick > last.time {
        return last.value;
    }

    let mut left = 0isize;
    let mut right = events.len() as isize - 1;
    let mut event1: Option<&chart::KeyPoint> = None;
    let mut event2: Option<&chart::KeyPoint> = None;

    while left <= right {
        let mid = ((left + right) / 2) as usize;
        let mid_event = &events[mid];

        if mid_event.time == tick {
            return mid_event.value;
        } else if mid_event.time < tick {
            event1 = Some(mid_event);
            left = mid as isize + 1;
        } else {
            event2 = Some(mid_event);
            right = mid as isize - 1;
        }
    }

    if let (Some(e1), Some(e2)) = (event1, event2) {
        let ease_value = ease::EASE_FUNCS[e1.ease_type as usize]((tick - e1.time) / (e2.time - e1.time));
        return e1.value + (e2.value - e1.value) * ease_value;
    }

    0.0
}



fn speed_to_fp(seconds: f64, speed_key_points: &[chart::KeyPoint], chart: &Chart) -> f64 {
    if speed_key_points.is_empty() {
        return 0.0;
    }

    let mut left = 0isize;
    let mut right = speed_key_points.len() as isize - 1;
    let mut target_idx = right as usize;

    while left <= right {
        let mid = (left + right) as usize / 2;
        let mid_seconds = chart.tick_to_seconds(speed_key_points[mid].time);

        if mid_seconds <= seconds {
            target_idx = mid;
            left = mid as isize + 1;
        } else {
            right = mid as isize - 1;
        }
    }

    let current = &speed_key_points[target_idx];
    let current_seconds = chart.tick_to_seconds(current.time);
    let dt = seconds - current_seconds;

    current.floor_position + dt * current.value
}

fn find_canvas_move(chart: &Chart, time: f64, index: i32) -> [f64; 2] {
    let canvas_moves = &chart.canvas_moves;
    let canvas_move = &canvas_moves[index as usize];
    let canvas_x_positions_key_points = &canvas_move.x_position_key_points;
    let canvas_speed_key_points = &canvas_move.speed_key_points;
    let x = find_key_point_value(time, &canvas_x_positions_key_points, chart);
    let fp = speed_to_fp(time, &canvas_speed_key_points, chart);
    [x, fp]
}

fn find_canmera_move(chart: &Chart, time: f64) -> [f64; 2] {
    let camera_moves = &chart.camera_move;
    let camera_x_positions_key_points = &camera_moves.x_position_key_points;
    let camera_scale_key_points = &camera_moves.scale_key_points;
    let x = -find_key_point_value(time, &camera_x_positions_key_points, chart);
    let scale = find_key_point_value(time, &camera_scale_key_points, chart);
    [x, scale]
}

// 二分查找给定秒数所在的线段，返回 (line_point_idx, next_line_point_idx)
// idx1 是时间 <= secs 的最大线点；idx2 是时间 > secs 的最小线点
// 越界时退回到首/末线点（idx1 == idx2）
fn find_line_segment(line_points: &[chart::LinePoint], secs: f64, chart: &Chart) -> (usize, usize) {
    if line_points.is_empty() {
        return (0, 0);
    }
    let mut left = 0isize;
    let mut right = line_points.len() as isize - 1;
    let mut idx1: Option<usize> = None;
    let mut idx2: Option<usize> = None;
    while left <= right {
        let mid = ((left + right) / 2) as usize;
        let mid_time = chart.tick_to_seconds(line_points[mid].time);
        if mid_time <= secs {
            idx1 = Some(mid);
            left = mid as isize + 1;
        } else {
            idx2 = Some(mid);
            right = mid as isize - 1;
        }
    }
    // 越界回落：secs 在所有线点之前 → (0, 0)；在所有线点之后 → (last, last)
    let last = line_points.len() - 1;
    (idx1.unwrap_or(0), idx2.unwrap_or(last))
}


// 画布序号字号量化成 8 的倍数：预缓存精确覆盖实际字号，录制中不会新增字形导致字体图集膨胀（黑框框）
fn quantize_font_size(x: f64) -> f64 {
    ((x / 8.0).round() * 8.0).clamp(8.0, 256.0)
}

fn update_canvases_text(chart: &Chart, time: f64) {
    if revelation_size() >= 1.0 {
        return;
    }
    let camera_pos = find_canmera_move(chart, time);
    let camera_x = camera_pos[0];
    let scale = camera_pos[1];
    // 字号按 8 的倍数量化，与预缓存集合一致
    let font_size = quantize_font_size(70.0 * scale * scale_x());
    let count = chart.canvas_moves.len();
    if count == 0 {
        return;
    }
    // 序号在整屏高度内等分，各自落在所属格子的中心
    let band = 1280.0 * screen_radio_h() / count as f64;
    for i in 0..count {
        let canvas_pos = find_canvas_move(chart, time, i as i32);
        let x = (canvas_pos[0] + camera_x) * 720.0 * scale * scale_x() + center_x();
        let text = &format!("{}", i);
        let dim = measure_text(text, None, font_size as u16, 1.0);
        let tx = x - dim.width as f64 / 2.0;
        let ty = ((count - 1 - i) as f64 + 0.5) * band + dim.offset_y as f64 - dim.height as f64 / 2.0;
        draw_text_outlined(
            text,
            tx as f32,
            ty as f32,
            font_size as f32,
            BLACK,
            WHITE,
            (font_size as f32 * 0.05).max(1.0),
        );
    }
}

fn get_current_color(now_seconds: f64, color_points: &[chart::ColorPoint], chart: &Chart) -> Option<chart::Color> {
    if color_points.is_empty() {
        return None;
    }
    let mut current = color_points[0].start_color;
    for i in 0..color_points.len() {
        let start_secs = chart.tick_to_seconds(color_points[i].time);
        let end_secs = if i + 1 < color_points.len() {
            chart.tick_to_seconds(color_points[i + 1].time)
        } else {
            f64::INFINITY
        };
        if now_seconds > end_secs { continue; }
        if now_seconds < start_secs { break; }
        let delta = (now_seconds - start_secs) / (end_secs - start_secs);
        let sc = color_points[i].start_color;
        let ec = color_points[i].end_color;
        current = chart::Color {
            r: (sc.r as f64 + (ec.r as f64 - sc.r as f64) * delta) as u8,
            g: (sc.g as f64 + (ec.g as f64 - sc.g as f64) * delta) as u8,
            b: (sc.b as f64 + (ec.b as f64 - sc.b as f64) * delta) as u8,
            a: (sc.a as f64 + (ec.a as f64 - sc.a as f64) * delta) as u8,
        };
        break;
    }
    Some(current)
}

fn mix_color(base: &chart::Color, overlay: &chart::Color) -> chart::Color {
    if overlay.a == 0 {
        return *base;
    }
    if overlay.a == 255 {
        return chart::Color { r: overlay.r, g: overlay.g, b: overlay.b, a: base.a };
    }
    let a0 = overlay.a as f64 / 255.0;
    chart::Color {
        r: (base.r as f64 + (overlay.r as f64 - base.r as f64) * a0) as u8,
        g: (base.g as f64 + (overlay.g as f64 - base.g as f64) * a0) as u8,
        b: (base.b as f64 + (overlay.b as f64 - base.b as f64) * a0) as u8,
        a: base.a,
    }
}

fn chart_color_to_macroquad(c: &chart::Color) -> macroquad::prelude::Color {
    macroquad::prelude::Color::new(c.r as f32 / 255.0, c.g as f32 / 255.0, c.b as f32 / 255.0, c.a as f32 / 255.0)
}

#[allow(dead_code)]
fn draw_test_line() {
    let screen_width = screen_width();
    draw_line(0.0, floor_y() as f32, screen_width, floor_y() as f32, 4.0, RED);
}

fn segment_intersects_screen(p0: (f64, f64), p1: (f64, f64), width: f64, height: f64) -> bool {
    let (x0, y0) = p0;
    let (x1, y1) = p1;
    let dx = x1 - x0;
    let dy = y1 - y0;
    let mut t_min = 0.0f64;
    let mut t_max = 1.0f64;
    for &(p, q) in &[
        (-dx, x0),
        (dx, width - x0),
        (-dy, y0),
        (dy, height - y0),
    ] {
        if p == 0.0 {
            if q < 0.0 { return false; }
        } else {
            let t = q / p;
            if p < 0.0 {
                if t > t_max { return false; }
                if t > t_min { t_min = t; }
            } else {
                if t < t_min { return false; }
                if t < t_max { t_max = t; }
            }
        }
    }
    true
}

fn draw_lines(chart: &Chart, time: f64) {
    let screen_width = render_w() as f64;
    let screen_height = render_h() as f64;
    let camera_pos = find_canmera_move(chart, time);
    let camear_x = camera_pos[0];
    let camera_scale = camera_pos[1];
    let rev_scale = (scale_x() * 2.0) as f32;
    for line in &chart.lines {
        let line_color = get_current_color(time, &line.line_color, chart);
        for (i, point) in line.line_points.iter().enumerate() {
            let cvs_pos = find_canvas_move(chart, time, point.canvas_index as i32);
            let x = (point.x_position + cvs_pos[0] + camear_x) * 720.0 * camera_scale * scale_x() + center_x();
            let y = (-(point.floor_position - cvs_pos[1]) * camera_scale * speed_ratio() * 1280.0) * scale_y() + floor_y();
            if revelation_size() < 1.0 {
                draw_circle(x as f32, y as f32, 4.0, BLACK);
            }
            let result_color = match line_color {
                Some(lc) => mix_color(&point.color, &lc),
                None => point.color,
            };
            if i + 1 < line.line_points.len() {
                let next_point = &line.line_points[i + 1];
                let mut next_point_cvs_pos = cvs_pos;
                if point.canvas_index != next_point.canvas_index {
                    next_point_cvs_pos = find_canvas_move(chart, time, next_point.canvas_index as i32); 
                }
                let next_x = (next_point.x_position + next_point_cvs_pos[0] + camear_x) * 720.0 * camera_scale * scale_x() + center_x();
                let next_y = (-(next_point.floor_position - next_point_cvs_pos[1]) * camera_scale * speed_ratio() * 1280.0) * scale_y() + floor_y();
                let next_point_result_color = match line_color {
                    Some(lc) => mix_color(&next_point.color, &lc),
                    None => next_point.color,
                };
                if !segment_intersects_screen((x, y), (next_x, next_y), screen_width, screen_height) {
                    continue;
                } if x == next_x && y == next_y {
                    draw_circle(x as f32, y as f32, 4.0 * rev_scale, chart_color_to_macroquad(&result_color));
                    continue;
                }
                // x 按 easeType 缓动插值，y 线性插值；颜色按屏幕投影 t 取色
                let dx = next_x - x;
                let dy = next_y - y;
                let len_sq = dx * dx + dy * dy;
                let cap_radius = 2.0 * rev_scale;
                let is_first_seg = i == 0;
                let is_last_seg = i + 1 >= line.line_points.len();
                let cap_start_color = chart_color_to_macroquad(&result_color);
                let cap_end_color = chart_color_to_macroquad(&next_point_result_color);
                for j in 0..16 {
                    let t0 = j as f64 / 16.0;
                    let t1 = (j + 1) as f64 / 16.0;
                    let ease0 = ease::EASE_FUNCS[point.ease_type as usize](t0);
                    let ease1 = ease::EASE_FUNCS[point.ease_type as usize](t1);
                    let p0_x = x + dx * ease0;
                    let p0_y = y + dy * t0;
                    let p1_x = x + dx * ease1;
                    let p1_y = y + dy * t1;
                    let color_t = if len_sq > 1e-12 {
                        let mid_x = (p0_x + p1_x) * 0.5;
                        let mid_y = (p0_y + p1_y) * 0.5;
                        (((mid_x - x) * dx + (mid_y - y) * dy) / len_sq).clamp(0.0, 1.0)
                    } else {
                        0.5
                    };
                    let r = (result_color.r as f64 + (next_point_result_color.r as f64 - result_color.r as f64) * color_t) as u8;
                    let g = (result_color.g as f64 + (next_point_result_color.g as f64 - result_color.g as f64) * color_t) as u8;
                    let b = (result_color.b as f64 + (next_point_result_color.b as f64 - result_color.b as f64) * color_t) as u8;
                    let a = (result_color.a as f64 + (next_point_result_color.a as f64 - result_color.a as f64) * color_t) as u8;
                    let color = macroquad::prelude::Color::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a as f32 / 255.0);
                    draw_line(p0_x as f32, p0_y as f32, p1_x as f32, p1_y as f32, 3.0 * rev_scale, color);
                }
                // 圆头笔尖：仅在整条 line 的首/末段端点画圆，内部段边界由相邻段覆盖，消除段间重复 cap
                if is_first_seg {
                    draw_circle(x as f32, y as f32, cap_radius, cap_start_color);
                }
                if is_last_seg {
                    draw_circle(next_x as f32, next_y as f32, cap_radius, cap_end_color);
                }
            } else if line.line_points.len() == 1 {
                draw_circle(x as f32, y as f32, 5.0 * rev_scale, chart_color_to_macroquad(&result_color));
            }
        }
    }
}

fn draw_judge_ring(chart: &Chart, time: f64) {
    let camera_pos = find_canmera_move(chart, time);
    let camear_x = camera_pos[0];
    let camera_scale = camera_pos[1];
    let cs = (camera_scale * scale_x() * 2.0) as f32;
    for line in &chart.lines {
        let judge_ring_color = &line.judge_ring_color;
        if judge_ring_color.is_empty() {
            continue;
        }
        let color = get_current_color(time, &judge_ring_color, chart);
        // 二分查找找到最近的两个线点
        let mut left = 0isize;
        let mut right = line.line_points.len() as isize - 1;
        let mut idx1 = right as usize;
        let mut idx2 = right as usize;
        while left <= right {
            let mid = (left + right) as usize / 2;
            let mid_time = chart.tick_to_seconds(line.line_points[mid].time);
            if mid_time <= time {
                idx1 = mid;
                left = mid as isize + 1;
            } else {
                idx2 = mid;
                right = mid as isize - 1;
            }
        }
        let p0 = &line.line_points[idx1];
        let p1 = &line.line_points[idx2];
        let p0_cvs_pos = find_canvas_move(chart, time, p0.canvas_index as i32);
        let p1_cvs_pos = find_canvas_move(chart, time, p1.canvas_index as i32);
        let p0_x = (p0.x_position + p0_cvs_pos[0] + camear_x) * 720.0 * camera_scale * scale_x() + center_x();
        let p1_x = (p1.x_position + p1_cvs_pos[0] + camear_x) * 720.0 * camera_scale * scale_x() + center_x();
        let p0_time = chart.tick_to_seconds(p0.time);
        let p1_time = chart.tick_to_seconds(p1.time);
        if time < p0_time || time > p1_time {
            continue;
        }
        // 按 y 坐标插值：求该段与判定线（floor_y）的交点参数 t，保证环贴合在判定线上的线
        let design_floor = 1040.0;
        let y0 = -(p0.floor_position - p0_cvs_pos[1]) * camera_scale * speed_ratio() * 1280.0 + design_floor;
        let y1 = -(p1.floor_position - p1_cvs_pos[1]) * camera_scale * speed_ratio() * 1280.0 + design_floor;
        let t = if (y1 - y0).abs() > 1e-9 {
            ((design_floor - y0) / (y1 - y0)).clamp(0.0, 1.0)
        } else {
            0.5
        };
        let ease0 = ease::EASE_FUNCS[p0.ease_type as usize](t);
        let x = p0_x + (p1_x - p0_x) * ease0;
        if let Some(ref c) = color {
            draw_circle_lines(x as f32, floor_y() as f32, 15.0 * cs, 3.0 * cs, chart_color_to_macroquad(c));
        }
    }
}

// 打击特效（对应 JS 的 hit 类）
struct HitEffect {
    x: f64,
    timer: f64,
    block_count: usize,
    blocks_r: Vec<f64>,
    block_s: Vec<f64>,
    r_b_offset: Vec<f64>,
    r_b_s: Vec<f64>,
}

impl HitEffect {
    fn new(x: f64, timer: f64, in_challenge: bool) -> Self {
        let block_count = (rand_f64() * 2.0).floor() as usize + 3;
        let mut blocks_r = Vec::with_capacity(block_count);
        let mut block_s = Vec::with_capacity(block_count);
        for _ in 0..block_count {
            blocks_r.push((rand_f64() * 361.0).floor());
            block_s.push((rand_f64() * 20.0).floor() + 10.0);
        }
        let mut r_b_offset = Vec::new();
        let mut r_b_s = Vec::new();
        if in_challenge {
            let count = (rand_f64() * 5.0).floor() as usize + 1;
            for _ in 0..count {
                r_b_offset.push(rand_f64() * 440.0);
                r_b_s.push((rand_f64() * 10.0).floor() + 10.0);
            }
        }
        Self { x, timer, block_count, blocks_r, block_s, r_b_offset, r_b_s }
    }
}

// 当前完整激活的挑战主题索引；进入/退出过渡不切换打击特效颜色。
fn find_hit_theme_index(chart: &Chart, time: f64) -> Option<usize> {
    find_active_challenge_theme_index(chart, time)
}

// 生成打击特效（在 is_hited 更新前调用，避免可变借用冲突）
fn spawn_hit_effects(chart: &Chart, time: f64, hits: &mut Vec<HitEffect>) {
    let camera_pos = find_canmera_move(chart, time);
    let camera_x = camera_pos[0];
    let camera_scale = camera_pos[1];
    let theme_idx = find_hit_theme_index(chart, time);
    for line in &chart.lines {
        for note in &line.notes {
            let note_time = chart.tick_to_seconds(note.time);
            if time >= note_time && !note.is_hited {
                let (idx1, idx2) = find_line_segment(&line.line_points, note_time, chart);
                let lp = &line.line_points[idx1];
                let next_lp = &line.line_points[idx2];
                let lp_cvs = find_canvas_move(chart, time, lp.canvas_index as i32);
                let next_lp_cvs = find_canvas_move(chart, time, next_lp.canvas_index as i32);
                let lp_time = chart.tick_to_seconds(lp.time);
                let next_lp_time = chart.tick_to_seconds(next_lp.time);
                let lp_x = (lp.x_position + lp_cvs[0] + camera_x) * 720.0 * camera_scale * scale_x() + center_x();
                let next_lp_x = (next_lp.x_position + next_lp_cvs[0] + camera_x) * 720.0 * camera_scale * scale_x() + center_x();
                let t_note = if (next_lp_time - lp_time).abs() > 1e-12 {
                    ((note_time - lp_time) / (next_lp_time - lp_time)).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let ease_note = ease::EASE_FUNCS[lp.ease_type as usize](t_note);
                let x = lp_x + (next_lp_x - lp_x) * ease_note;
                hits.push(HitEffect::new(x, time, theme_idx.is_some()));
            }
        }
    }
}

fn draw_hit_blocks(h: &HitEffect, t: f64, scale: f64, color: chart::Color) {
    let judge_y = floor_y();
    let ease11 = ease::EASE_FUNCS[11](t);
    let ease10 = ease::EASE_FUNCS[10](t);
    for i in 0..h.block_count {
        let angle = h.blocks_r[i] * PI / 180.0;
        let wh = h.block_s[i] * scale * 2.0;
        let offset = wh / 2.0;
        let block_offset = ease11 * 100.0 * scale * 2.0;
        let x1 = h.x + block_offset * angle.cos() - offset;
        let y1 = judge_y + block_offset * angle.sin() - offset;
        let radius = (wh - wh * ease10) * 0.5;
        if radius <= 0.0 {
            continue;
        }
        let alpha = (1.0 - ease10) as f32;
        let color = macroquad::prelude::Color::new(
            color.r as f32 / 255.0,
            color.g as f32 / 255.0,
            color.b as f32 / 255.0,
            alpha,
        );
        draw_circle(x1 as f32, y1 as f32, radius as f32, color);
    }
}

fn draw_riz_blocks(h: &HitEffect, t: f64, scale: f64, color: chart::Color) {
    if h.r_b_offset.is_empty() {
        return;
    }
    let judge_y = floor_y();
    let ease11 = ease::EASE_FUNCS[11](t);
    let ease10 = ease::EASE_FUNCS[10](t);
    for i in 0..h.r_b_offset.len() {
        let wh = h.r_b_s[i] * scale * 2.0;
        let offset = wh / 2.0;
        let block_offset = ease11 * h.r_b_offset[i] * scale * 2.0;
        let y1 = judge_y - block_offset + offset;
        let radius = (wh - wh * ease10) * 0.5;
        if radius <= 0.0 {
            continue;
        }
        let alpha = (1.0 - ease10) as f32;
        let color = macroquad::prelude::Color::new(
            color.r as f32 / 255.0,
            color.g as f32 / 255.0,
            color.b as f32 / 255.0,
            alpha,
        );
        draw_circle(h.x as f32, y1 as f32, radius as f32, color);
    }
}

fn draw_hits(chart: &Chart, hits: &[HitEffect], time: f64, effect_color: chart::Color) {
    let scale = find_canmera_move(chart, time)[1] * scale_x();
    let judge_y = floor_y();
    for h in hits.iter() {
        let t = ((time - h.timer) / 0.5).clamp(0.0, 1.0);
        let ease_value = ease::EASE_FUNCS[11](t);
        let size = (30.0 + 70.0 * ease_value) * 2.0;
        let lw = ((30.0 - 30.0 * ease_value) * scale * 2.0).max(0.1);
        let ring_color = chart_color_to_macroquad(&effect_color);
        draw_circle_lines(h.x as f32, judge_y as f32, (size * scale * 0.5) as f32, lw as f32, ring_color);
        draw_hit_blocks(h, t, scale, effect_color);
        draw_riz_blocks(h, t, scale, effect_color);
    }
}

fn update_note_state(chart: &mut Chart, time: f64, hits: &mut Vec<HitEffect>, mute: bool) {
    spawn_hit_effects(chart, time, hits);

    // 第一遍：标记 is_hited 并播放命中音效
    for line in &mut chart.lines {
        for note in line.notes.iter_mut() {
            let note_time = tick_to_seconds_impl(note.time, &chart.bpm_shifts, chart.bpm);
            if time >= note_time {
                if !note.is_hited {
                    if !mute {
                        play_hit_sound(note.note_type);
                    }
                }
                note.is_hited = true;
            } else {
                note.is_hited = false;
            }
        }
    }

    hits.retain(|h| time >= h.timer && time - h.timer <= 0.5);
}

fn draw_notes(chart: &Chart, time: f64, note_color: chart::Color) {
    let camera_pos = find_canmera_move(chart, time);
    let camera_x = camera_pos[0];
    let camera_scale = camera_pos[1];
    let cs = (camera_scale * scale_x() * 2.0) as f32;
    for line in &chart.lines {
        for note in &line.notes {
            if time > chart.tick_to_seconds(note.time) && note.note_type != 2 {
                continue;
            }
            let note_time = chart.tick_to_seconds(note.time);

            // note 所在线段（用于 note 的实际 x/y，对应 JS 的 j.linePoint）
            let (note_idx1, note_idx2) = find_line_segment(&line.line_points, note_time, chart);
            let note_lp = &line.line_points[note_idx1];
            let next_note_lp = &line.line_points[note_idx2];
            let note_lp_cvs = find_canvas_move(chart, time, note_lp.canvas_index as i32);
            let next_note_lp_cvs = find_canvas_move(chart, time, next_note_lp.canvas_index as i32);
            let note_lp_time = chart.tick_to_seconds(note_lp.time);
            // next_note_lp.time 是 chart 中的实际 time（始终有限），无需 INFINITY 兜底
            let next_note_lp_time = chart.tick_to_seconds(next_note_lp.time);
            let note_lp_x = (note_lp.x_position + note_lp_cvs[0] + camera_x) * 720.0 * camera_scale * scale_x() + center_x();
            let next_note_lp_x = (next_note_lp.x_position + next_note_lp_cvs[0] + camera_x) * 720.0 * camera_scale * scale_x() + center_x();
            let t_note = if (next_note_lp_time - note_lp_time).abs() > 1e-12 {
                ((note_time - note_lp_time) / (next_note_lp_time - note_lp_time)).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let ease_note = ease::EASE_FUNCS[note_lp.ease_type as usize](t_note);
            let x_at_note = note_lp_x + (next_note_lp_x - note_lp_x) * ease_note;
            // y 使用 note 所在线段的 startCanvas（与 JS 的 j.currentY 一致）
            let y_at_note = -(note.floor_position - note_lp_cvs[1]) * camera_scale * speed_ratio() * 1280.0 * scale_y() + floor_y();
            if y_at_note < 0.0 {
                continue;
            }

            if note.note_type == 0 {
                let x = x_at_note;
                let y = y_at_note;
                draw_circle(x as f32, y as f32, 10.0 * cs, chart_color_to_macroquad(&note_color));
                draw_circle_lines(x as f32, y as f32, 10.0 * cs, 6.0 * cs, BLACK);
            } else if note.note_type == 1 {
                let x = x_at_note;
                let y = y_at_note;
                draw_circle(x as f32, y as f32, 9.0 * cs, WHITE);
                draw_circle_lines(x as f32, y as f32, 9.0 * cs, 6.0 * cs, BLACK);
            } else if note.note_type == 2 {
                if let Some(ref infos) = note.other_informations {
                    if infos.len() >= 3 {
                        let hold_end_time = infos[0];
                        let hold_end_canvas_index = infos[1] as i32;
                        let hold_end_fp = infos[2];
                        let hold_end_secs = chart.tick_to_seconds(hold_end_time);
                        // 超过 hold 尾 0.25s 后不再绘制
                        if time - hold_end_secs > 0.2 {
                            continue;
                        }
                        // 当前时间所在线段（用于 type 2 时间过后的 x 滑动）
                        let (cur_idx1, cur_idx2) = find_line_segment(&line.line_points, time, chart);
                        let cur_lp = &line.line_points[cur_idx1];
                        let next_cur_lp = &line.line_points[cur_idx2];
                        let cur_lp_cvs = find_canvas_move(chart, time, cur_lp.canvas_index as i32);
                        let next_cur_lp_cvs = find_canvas_move(chart, time, next_cur_lp.canvas_index as i32);
                        let cur_lp_time = chart.tick_to_seconds(cur_lp.time);
                        let next_cur_lp_time = chart.tick_to_seconds(next_cur_lp.time);
                        let cur_lp_x = (cur_lp.x_position + cur_lp_cvs[0] + camera_x) * 720.0 * camera_scale * scale_x() + center_x();
                        let next_cur_lp_x = (next_cur_lp.x_position + next_cur_lp_cvs[0] + camera_x) * 720.0 * camera_scale * scale_x() + center_x();
                        let t_cur = if (next_cur_lp_time - cur_lp_time).abs() > 1e-12 {
                            ((time - cur_lp_time) / (next_cur_lp_time - cur_lp_time)).clamp(0.0, 1.0)
                        } else {
                            0.0
                        };
                        let ease_cur = ease::EASE_FUNCS[cur_lp.ease_type as usize](t_cur);
                        let x_at_cur = cur_lp_x + (next_cur_lp_x - cur_lp_x) * ease_cur;
                        // 时间已过 note 头：x 沿当前线段滑动，y 固定在判定线
                        let (x, y) = if time > note_time {
                            (x_at_cur, floor_y())
                        } else {
                            (x_at_note, y_at_note)
                        };
                        // 计算 hold 尾端的 y（取当前时刻 tail 所在 canvas 的位置）
                        let tail_cvs_pos = find_canvas_move(chart, time, hold_end_canvas_index);
                        let tail_y_offset = hold_end_fp - tail_cvs_pos[1];
                        let dy = -(tail_y_offset) * camera_scale * speed_ratio() * 1280.0 * scale_y() + floor_y();
                        let color_mq = chart_color_to_macroquad(&note_color);
                        // 已过 hold 尾：头部按 1 - dt^3 收缩消失
                        if time > hold_end_secs {
                            let dt = (time - hold_end_secs) / 0.2;
                            let progress = (1.0 - dt.powi(3)).max(0.0) as f32;
                            draw_circle_lines(x as f32, y as f32, 10.0 * progress * cs, 3.0 * cs, BLACK);
                            draw_circle(x as f32, y as f32, 8.0 * progress * cs, WHITE);
                            draw_circle_lines(x as f32, y as f32, 8.0 * progress * cs, 3.0 * cs, BLACK);
                            continue;
                        }
                        // 头部外圈
                        draw_circle_lines(x as f32, y as f32, 10.0 * cs, 3.0 * cs, BLACK);
                        // hold 尾渐变矩形（自 y 向 dy：noteColor 在 0~36/63，之后渐变到透明白）
                        let hold_size = 10.0 * cs;
                        let segments = 24;
                        let y_start = y as f32;
                        let y_end = dy as f32;
                        for i in 0..segments {
                            let t0 = i as f32 / segments as f32;
                            let t1 = (i + 1) as f32 / segments as f32;
                            let rect_y0 = y_start + (y_end - y_start) * t0;
                            let rect_y1 = y_start + (y_end - y_start) * t1;
                            let mid_t = (t0 + t1) * 0.5;
                            let seg_color = if mid_t <= 36.0 / 63.0 {
                                color_mq
                            } else {
                                let fade = ((mid_t - 36.0 / 63.0) / (27.0 / 63.0)).clamp(0.0, 1.0);
                                macroquad::prelude::Color::new(
                                    color_mq.r + (1.0 - color_mq.r) * fade,
                                    color_mq.g + (1.0 - color_mq.g) * fade,
                                    color_mq.b + (1.0 - color_mq.b) * fade,
                                    color_mq.a * (1.0 - fade),
                                )
                            };
                            let rect_y = rect_y0.min(rect_y1);
                            let rect_h = (rect_y1 - rect_y0).abs();
                            if rect_h > 0.0 {
                                draw_rectangle(x as f32 - hold_size, rect_y, hold_size * 2.0, rect_h, seg_color);
                            }
                        }
                        // hold 尾两侧黑边
                        draw_line(x as f32 - hold_size, y as f32, x as f32 - hold_size, dy as f32, 5.0 * cs, BLACK);
                        draw_line(x as f32 + hold_size, y as f32, x as f32 + hold_size, dy as f32, 5.0 * cs, BLACK);
                        // 头部内圈
                        draw_circle(x as f32, y as f32, 10.0 * cs, WHITE);
                        draw_circle_lines(x as f32, y as f32, 10.0 * cs, 8.0 * cs, BLACK);
                    }
                }
            }
        }
    }
}

const TAP_HIT_DATA: &[u8] = include_bytes!("../assets/audio/hit.wav");
const DRAG_HIT_DATA: &[u8] = include_bytes!("../assets/audio/drag.wav");

// Desktop audio output is usually 44.1 kHz, but mobile devices commonly run at
// 48 kHz. Let the device pick there instead of forcing a rate it may reject.
#[cfg(not(target_arch = "wasm32"))]
fn cpal_settings() -> CpalSettings {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        CpalSettings {
            preferred_sample_rate: None,
            buffer_size: None,
            ..Default::default()
        }
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        CpalSettings {
            preferred_sample_rate: Some(44100),
            buffer_size: Some(256),
            ..Default::default()
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct HitSounds {
    _manager: AudioManager,
    tap: Sfx,
    drag: Sfx,
}

#[cfg(not(target_arch = "wasm32"))]
impl HitSounds {
    fn new() -> Self {
        let backend = CpalBackend::new(cpal_settings());
        let mut manager = AudioManager::new(backend)
            .expect("failed to create hit sound AudioManager");
        let tap = manager
            .create_sfx(
                AudioClip::new(TAP_HIT_DATA.to_vec()).expect("invalid tap hit wav"),
                None,
            )
            .expect("failed to create tap sfx");
        let drag = manager
            .create_sfx(
                AudioClip::new(DRAG_HIT_DATA.to_vec()).expect("invalid drag hit wav"),
                None,
            )
            .expect("failed to create drag sfx");
        Self {
            _manager: manager,
            tap,
            drag,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
thread_local! {
    static HIT_SOUNDS: RefCell<HitSounds> = RefCell::new(HitSounds::new());
}

fn play_hit_sound(note_type: i32) {
    #[cfg(target_arch = "wasm32")]
    {
        web::play_hit_sound(note_type);
    }
    #[cfg(not(target_arch = "wasm32"))]
    HIT_SOUNDS.with(|cell| {
        let mut sounds = cell.borrow_mut();
    let sfx: &mut Sfx = match note_type {
        1 => &mut sounds.drag,
        _ => &mut sounds.tap,
    };
    if let Err(e) = sfx.play(PlaySfxParams::default()) {
        eprintln!("play_hit_sound failed: {e:?}");
    }
    });
}

const COMPOSITE_VERTEX_SHADER: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
varying lowp vec2 uv;
varying lowp vec4 color;
uniform mat4 Model;
uniform mat4 Projection;
void main() {
    gl_Position = Projection * Model * vec4(position, 1.0);
    uv = texcoord;
    color = color0 / 255.0;
}
"#;

const COMPOSITE_FRAGMENT_SHADER: &str = r#"#version 100
precision lowp float;
varying vec2 uv;
varying vec4 color;
uniform sampler2D Texture;
uniform sampler2D ChallengeTexture;
uniform vec2 MaskCenter;
uniform vec2 MaskAspect;
uniform float MaskRadius;
void main() {
    vec4 base = texture2D(Texture, uv) * color;
    vec4 challenge = texture2D(ChallengeTexture, uv) * color;
    vec2 delta = (uv - MaskCenter) * MaskAspect;
    float inside = step(length(delta), MaskRadius);
    gl_FragColor = mix(base, challenge, inside);
}
"#;

struct ChallengeComposer {
    size: (u32, u32),
    default_target: RenderTarget,
    challenge_target: RenderTarget,
    material: Material,
}

impl ChallengeComposer {
    fn new() -> Self {
        let material = load_material(
            ShaderSource::Glsl {
                vertex: COMPOSITE_VERTEX_SHADER,
                fragment: COMPOSITE_FRAGMENT_SHADER,
            },
            MaterialParams {
                uniforms: vec![
                    UniformDesc::new("MaskCenter", UniformType::Float2),
                    UniformDesc::new("MaskAspect", UniformType::Float2),
                    UniformDesc::new("MaskRadius", UniformType::Float1),
                ],
                textures: vec!["ChallengeTexture".to_owned()],
                ..Default::default()
            },
        )
        .expect("failed to create challenge compositor");
        Self {
            size: (1, 1),
            default_target: render_target(1, 1),
            challenge_target: render_target(1, 1),
            material,
        }
    }

    fn ensure_size(&mut self, width: u32, height: u32) {
        let size = (width.max(1), height.max(1));
        if self.size == size {
            return;
        }
        self.size = size;
        self.default_target = render_target(size.0, size.1);
        self.challenge_target = render_target(size.0, size.1);
    }
}

fn transition_mask(chart: &Chart, time: f64) -> Option<(usize, usize, Vec2, f32)> {
    let mut selected = None;
    for (idx, ct) in chart.challenge_times.iter().enumerate() {
        let theme_idx = idx + 1;
        if theme_idx >= chart.themes.len() || ct.trans_time <= 0.0 {
            continue;
        }
        let start = chart.tick_to_seconds(ct.start);
        let end = chart.tick_to_seconds(ct.end);
        let trans_start = start + ct.trans_time;
        let trans_end = end + ct.trans_time;
        if time > start && time <= trans_start {
            selected = Some((theme_idx, 0usize, vec2(0.5, 1.0), ((time - start) / ct.trans_time) as f32));
        } else if time > end && time <= trans_end {
            selected = Some((theme_idx, 0usize, vec2(0.5, 0.0), (1.0 - (time - end) / ct.trans_time) as f32));
        }
    }
    let (theme_idx, _, center, progress) = selected?;
    // Use the previous theme only when the previous challenge actually reaches
    // this transition. If there is a real gap, the chart has already returned
    // to the default theme and the new transition must start from themes[0].
    let base_idx = if theme_idx > 1 {
        let previous = &chart.challenge_times[theme_idx - 2];
        let current = &chart.challenge_times[theme_idx - 1];
        let previous_end = chart.tick_to_seconds(previous.end) + previous.trans_time;
        let current_start = chart.tick_to_seconds(current.start);
        if previous_end >= current_start {
            theme_idx - 1
        } else {
            0
        }
    } else {
        0
    };
    Some((
        theme_idx,
        base_idx,
        center,
        // Match sim-rzc: the circle radius is 15 screen heights at full
        // transition progress. The shader's aspect correction keeps it round.
        progress.clamp(0.0, 1.0) * 15.0,
    ))
}

fn draw_scene(chart: &Chart, hits: &[HitEffect], time: f64, show_canvases: bool, theme_idx: Option<usize>) {
    clear_background(BLACK);
    draw_background(chart, theme_idx);
    if show_canvases {
        update_canvases_text(chart, time);
    }
    draw_lines(chart, time);
    draw_notes(chart, time, note_color_for_theme(chart, theme_idx));
    draw_hits(chart, hits, time, effect_color_for_theme(chart, theme_idx));
    draw_judge_ring(chart, time);
    draw_scaled_range();
    draw_combo(chart);
    draw_shui_yin();
    draw_revelation_info(chart, time);
}

fn draw_frame(
    chart: &mut Chart,
    hits: &mut Vec<HitEffect>,
    time: f64,
    mute: bool,
    show_canvases: bool,
    composer: &mut ChallengeComposer,
    output_target: Option<RenderTarget>,
) {
    update_note_state(chart, time, hits, mute);
    let width = render_w().round().max(1.0) as u32;
    let height = render_h().round().max(1.0) as u32;
    let Some((theme_idx, base_idx, mask_center, mask_radius)) = transition_mask(chart, time) else {
        match output_target.as_ref() {
            Some(target) => set_camera(&Camera2D {
                render_target: Some(target.clone()),
                ..Camera2D::from_display_rect(Rect::new(0.0, 0.0, width as f32, height as f32))
            }),
            None => set_default_camera(),
        }
        draw_scene(chart, hits, time, show_canvases, find_active_challenge_theme_index(chart, time));
        return;
    };

    composer.ensure_size(width, height);
    let default_camera = Camera2D {
        render_target: Some(composer.default_target.clone()),
        ..Camera2D::from_display_rect(Rect::new(0.0, 0.0, width as f32, height as f32))
    };
    set_camera(&default_camera);
    draw_scene(chart, hits, time, show_canvases, (base_idx > 0).then_some(base_idx));

    let challenge_camera = Camera2D {
        render_target: Some(composer.challenge_target.clone()),
        ..Camera2D::from_display_rect(Rect::new(0.0, 0.0, width as f32, height as f32))
    };
    set_camera(&challenge_camera);
    draw_scene(chart, hits, time, show_canvases, Some(theme_idx));

    match output_target.as_ref() {
        Some(target) => set_camera(&Camera2D {
            render_target: Some(target.clone()),
            ..Camera2D::from_display_rect(Rect::new(0.0, 0.0, width as f32, height as f32))
        }),
        None => set_default_camera(),
    }
    clear_background(BLACK);
    composer
        .material
        .set_texture("ChallengeTexture", composer.challenge_target.texture.clone());
    // The RenderTarget texture has an inverted Y axis when sampled by the
    // screen quad. Keep the transition origin aligned with the displayed
    // image after flipping that quad vertically.
    composer
        .material
        .set_uniform("MaskCenter", vec2(mask_center.x, 1.0 - mask_center.y));
    composer.material.set_uniform(
        "MaskAspect",
        vec2(width as f32 / height as f32, 1.0),
    );
    composer.material.set_uniform("MaskRadius", mask_radius);
    gl_use_material(&composer.material);
    draw_texture_ex(
        &composer.default_target.texture,
        0.0,
        0.0,
        WHITE,
        DrawTextureParams {
            dest_size: Some(vec2(width as f32, height as f32)),
            flip_y: true,
            ..Default::default()
        },
    );
    gl_use_default_material();
}

fn load_hit_samples(data: &[u8], channels: usize) -> Vec<f32> {
    let cursor = std::io::Cursor::new(data);
    let Ok(mut reader) = hound::WavReader::new(cursor) else {
        return Vec::new();
    };
    let src_channels = reader.spec().channels as usize;
    let samples: Vec<i16> = reader.samples::<i16>().map(|s| s.unwrap_or(0)).collect();
    if samples.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(samples.len() / src_channels * channels);
    for frame in samples.chunks_exact(src_channels) {
        let mono = frame.iter().map(|&s| s as f32 / 32768.0).sum::<f32>() / src_channels as f32;
        for _ in 0..channels {
            out.push(mono);
        }
    }
    out
}

fn mix_audio(bgm_path: &Path, chart: &Chart, output: &str) -> Result<PathBuf, hound::Error> {
    let mut reader = hound::WavReader::open(bgm_path)?;
    let spec = reader.spec();
    let channels = spec.channels as usize;
    let sample_rate = spec.sample_rate;
    let total = reader.duration() as usize * channels;
    let mut mix = vec![0.0f32; total];
    for (i, s) in reader.samples::<i16>().enumerate() {
        if let Ok(s) = s {
            if i < total {
                mix[i] = s as f32 / 32768.0;
            }
        }
    }
    drop(reader);

    let tap = load_hit_samples(TAP_HIT_DATA, channels);
    let drag = load_hit_samples(DRAG_HIT_DATA, channels);
    let mix_gain = 0.8;

    for line in &chart.lines {
        for note in &line.notes {
            let t = tick_to_seconds_impl(note.time, &chart.bpm_shifts, chart.bpm);
            let sfx = if note.note_type == 1 { &drag } else { &tap };
            if sfx.is_empty() {
                continue;
            }
            let start = (t * sample_rate as f64) as usize * channels;
            for (k, &s) in sfx.iter().enumerate() {
                let idx = start + k;
                if idx < total {
                    mix[idx] += s * mix_gain;
                }
            }
        }
    }

    let out_path = PathBuf::from(output);
    let mut writer = hound::WavWriter::create(&out_path, spec)?;
    for &s in &mix {
        let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        writer.write_sample(v)?;
    }
    writer.finalize()?;
    Ok(out_path)
}

fn draw_render_progress(frame: u64, enc_frame: u64, total: u64, submit_speed: f64, enc_speed: f64, eta: f64, elapsed: f64) {
    let w = screen_width();
    let h = screen_height();
    draw_rectangle(0.0, 0.0, w, h, Color::from_rgba(0, 0, 0, 210));
    let progress = if total > 0 { frame as f32 / total as f32 } else { 0.0 };
    let enc_progress = if total > 0 { enc_frame as f32 / total as f32 } else { 0.0 };

    // 固定宽度进度条，居中；绿=已提交，蓝=ffmpeg已处理
    let bar_w = 320.0;
    let bar_h = 18.0;
    let bx = (w - bar_w) / 2.0;
    let by = h * 0.5;
    draw_rectangle(bx, by, bar_w, bar_h, DARKGRAY);
    draw_rectangle(bx, by, bar_w * progress, bar_h, GREEN);
    draw_rectangle(bx, by, bar_w * enc_progress, bar_h, BLUE);
    draw_rectangle_lines(bx, by, bar_w, bar_h, 2.0, WHITE);

    // 文本紧凑垂直排列，居中
    let cx = w / 2.0;
    let mut y = by - 28.0;
    let lines = [
        format!("渲染中... {:.1}% (ffmpeg {:.1}%)", progress * 100.0, enc_progress * 100.0),
        format!("提交帧: {}/{}   ffmpeg已处理: {}", frame, total, enc_frame),
        format!("提交速度: {:.1} fps   ffmpeg速度: {:.1} fps", submit_speed, enc_speed),
        format!("预计剩余: {:.0}s   已耗时: {:.1}s", eta, elapsed),
    ];
    for line in &lines {
        let dim = measure_text(line, None, 22, 1.0);
        draw_text(line, cx - dim.width / 2.0, y, 22.0, WHITE);
        y -= 26.0;
    }
}

fn ffmpeg_names() -> &'static [&'static str] {
    if cfg!(windows) {
        &["ffmpeg.exe", "ffmpeg"]
    } else {
        &["ffmpeg", "ffmpeg.exe"]
    }
}

fn resolve_ffmpeg() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for name in ffmpeg_names() {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return candidate;
                }
            }
        }
    }
    PathBuf::from(ffmpeg_names()[0])
}

fn detect_hw_encoder(ffmpeg: &Path) -> Option<&'static str> {
    let out = Command::new(ffmpeg).arg("-encoders").output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout);
    for enc in ["h264_nvenc", "h264_qsv", "h264_amf", "h264_videotoolbox"] {
        if s.contains(enc) {
            return Some(enc);
        }
    }
    None
}

async fn render_video(
    chart: &mut Chart,
    hits: &mut Vec<HitEffect>,
    composer: &mut ChallengeComposer,
    duration: f64,
    bgm_path: &Path,
    out_w: u32,
    out_h: u32,
    fps: u32,
    hwaccel: Option<bool>,
) {
    let ffmpeg = resolve_ffmpeg();
    eprintln!("ffmpeg: {}", ffmpeg.display());

    let mixed_path = match mix_audio(bgm_path, chart, "mixed_audio.wav") {
        Ok(p) => p,
        Err(e) => {
            eprintln!("音频混合失败: {e}");
            return;
        }
    };

    let encoder = match hwaccel {
        Some(true) => detect_hw_encoder(&ffmpeg).unwrap_or("libx264"),
        Some(false) => "libx264",
        None => detect_hw_encoder(&ffmpeg).unwrap_or("libx264"),
    };
    eprintln!("视频编码器: {encoder}");

    // 渲染分辨率固定为输出尺寸（画面清晰一致），窗口缩小为小窗口只显示进度条（与游戏画面分离）
    RENDER_WIDTH.with(|c| c.set(out_w as f32));
    RENDER_HEIGHT.with(|c| c.set(out_h as f32));
    macroquad::miniquad::window::set_window_size(640, 360);
    next_frame().await;

    // 预热字体图集（render target 里新字形上传会丢失，故先全部缓存）
    // 1) 进度条文字（中文+数字，22 号）
    {
        let progress_text = "0123456789.-:/% 渲染中... (ffmpeg )提交帧:  ffmpeg已处理: 提交速度: fps ffmpeg速度: 预计剩余: s 已耗时: s";
        let dim = measure_text(progress_text, None, 22, 1.0);
        draw_text(progress_text, -dim.width - 10.0, -dim.height - 10.0, 22.0, Color::from_rgba(0, 0, 0, 0));
    }
    // 2) cvs 序号数字：按 camera scale 范围量化字号（8 的倍数），只预缓存实际会用到的字号
    {
        let mut min_scale = f64::INFINITY;
        let mut max_scale = f64::NEG_INFINITY;
        for kp in &chart.camera_move.scale_key_points {
            min_scale = min_scale.min(kp.value);
            max_scale = max_scale.max(kp.value);
        }
        let min_font = if min_scale.is_finite() { quantize_font_size(70.0 * min_scale * scale_x()) as i64 } else { 8 };
        let max_font = if max_scale.is_finite() { quantize_font_size(70.0 * max_scale * scale_x()) as i64 } else { 8 };
        let y_dummy = -1000.0;
        for size in (min_font..=max_font).step_by(8).chain([27, 36, 60, 90].into_iter()) {
            let size = size as f64;
            let dim = measure_text("0123456789", None, size as u16, 1.0);
            draw_text("0123456789", -dim.width - 10.0, y_dummy, size as f32, Color::from_rgba(0, 0, 0, 0));
        }
        // 3) 水印/信息文本的 ASCII 在固定字号缓存
        let ascii: String = (32..127).map(|c| c as u8 as char).collect();
        for size in [27, 36, 60, 90] {
            let dim = measure_text(&ascii, None, size as u16, 1.0);
            draw_text(&ascii, -dim.width - 10.0, y_dummy, size as f32, Color::from_rgba(0, 0, 0, 0));
        }
    }
    next_frame().await;

    let w = out_w;
    let h = out_h;
    let output = "output.mp4";

    // 根据编码器追加质量/速度参数，色彩丰富的画面不易糊
    let mut enc_args: Vec<String> = Vec::new();
    match encoder {
        "h264_nvenc" => {
            enc_args.push("-preset".to_string());
            enc_args.push("p5".to_string());
            enc_args.push("-cq".to_string());
            enc_args.push("18".to_string());
        }
        "h264_qsv" => {
            enc_args.push("-preset".to_string());
            enc_args.push("medium".to_string());
            enc_args.push("-global_quality".to_string());
            enc_args.push("18".to_string());
        }
        "h264_amf" => {
            enc_args.push("-quality".to_string());
            enc_args.push("quality".to_string());
            enc_args.push("-rc".to_string());
            enc_args.push("cqp".to_string());
            enc_args.push("-qp_i".to_string());
            enc_args.push("18".to_string());
            enc_args.push("-qp_p".to_string());
            enc_args.push("18".to_string());
        }
        "h264_videotoolbox" => {
            enc_args.push("-q:v".to_string());
            enc_args.push("65".to_string());
            enc_args.push("-allow_sw".to_string());
            enc_args.push("1".to_string());
        }
        _ => {
            enc_args.push("-preset".to_string());
            enc_args.push("medium".to_string());
            enc_args.push("-crf".to_string());
            enc_args.push("16".to_string());
        }
    }

    let mut child = match Command::new(&ffmpeg)
        .args(["-y", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", &format!("{w}x{h}"), "-r", &fps.to_string(), "-i", "-"])
        .args(["-i", mixed_path.to_str().unwrap()])
        .args(["-c:v", encoder, "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest"])
        .args(&enc_args)
        .args(["-stats_period", "0.2", "-progress", "pipe:2"])
        .arg(output)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("ffmpeg 启动失败: {e}");
            eprintln!("找不到 ffmpeg：请把 ffmpeg.exe 放到程序同目录，或安装后加入 PATH（例如 winget install Gyan.FFmpeg）");
            return;
        }
    };
    let mut stdin = child.stdin.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    // 后台线程读取 ffmpeg 进度和日志：进度用于进度条，日志输出到控制台（不再写 txt）
    let enc_state: Arc<Mutex<(u64, f64)>> = Arc::new(Mutex::new((0, 0.0)));
    let state = enc_state.clone();
    thread::spawn(move || {
        use std::io::BufRead;
        let reader = std::io::BufReader::new(stderr);
        for line in reader.lines() {
            let Ok(line) = line else { break };
            if let Some(v) = line.strip_prefix("frame=") {
                if let Ok(f) = v.trim().parse::<u64>() {
                    let mut progress = state.lock().unwrap();
                    progress.0 = progress.0.max(f);
                }
            } else if let Some(v) = line.strip_prefix("fps=") {
                if let Ok(f) = v.trim().parse::<f64>() {
                    state.lock().unwrap().1 = f;
                }
            } else if let Some(v) = line.strip_prefix("out_time_us=") {
                if let Ok(us) = v.trim().parse::<u64>() {
                    let encoded_frames = (us as f64 * fps as f64 / 1_000_000.0).round() as u64;
                    let mut progress = state.lock().unwrap();
                    progress.0 = progress.0.max(encoded_frames);
                }
            } else if line == "progress=continue" || line == "progress=end" {
                let progress = state.lock().unwrap();
                eprintln!("ffmpeg 进度: 已处理 {} 帧，{:.1} fps", progress.0, progress.1);
            } else {
                // 过滤 -progress 输出的单值 key=value 行，其余 ffmpeg 日志打到控制台
                let mut it = line.splitn(2, '=');
                let key = it.next().unwrap_or("");
                let val = it.next();
                let is_progress_line = val
                    .map(|v| !v.trim().contains(' ') && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
                    .unwrap_or(false);
                if !is_progress_line {
                    eprintln!("{line}");
                }
            }
        }
    });

    let target = render_target_msaa(w, h);
    let total_frames = (duration * fps as f64).ceil().max(1.0) as u64;
    eprintln!("开始渲染: {w}x{h}@{fps}fps，共 {total_frames} 帧，编码器 {encoder}");
    let start = Instant::now();
    let dt = 1.0 / fps as f64;
    let mut frame: u64 = 0;
    let mut time = 0.0;
    const BATCH: u64 = 10;
    let row_bytes = w as usize * 4;

    while frame < total_frames {
        draw_frame(chart, hits, time, true, true, composer, Some(target.clone()));
        set_default_camera();

        let img = target.texture.get_texture_data();
        // OpenGL 纹理行序是反的，翻转行序修正视频方向
        let mut rgb = Vec::with_capacity(img.bytes.len() / 4 * 3);
        for row in (0..img.height as usize).rev() {
            let src = &img.bytes[row * row_bytes..(row + 1) * row_bytes];
            for chunk in src.chunks_exact(4) {
                rgb.extend_from_slice(&chunk[..3]);
            }
        }
        if let Err(e) = stdin.write_all(&rgb) {
            eprintln!("写帧到 ffmpeg 失败: {e}");
            break;
        }

        frame += 1;
        time += dt;

        // 每 BATCH 帧更新一次窗口进度，避免每帧等 vsync
        if frame % BATCH == 0 || frame >= total_frames {
            let (enc_frame, enc_speed) = {
                let s = enc_state.lock().unwrap();
                (s.0, s.1)
            };
            let elapsed = start.elapsed().as_secs_f64();
            let submit_speed = frame as f64 / elapsed.max(1e-9);
            let eta = if submit_speed > 0.0 {
                (total_frames - frame) as f64 / submit_speed
            } else {
                0.0
            };
            draw_render_progress(frame, enc_frame, total_frames, submit_speed, enc_speed, eta, elapsed);
            next_frame().await;
        }
    }

    drop(stdin);

    // 等待 ffmpeg 编码完成，期间保持窗口响应
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                clear_background(BLACK);
                let done_text = "正在合成视频... 完成后请点击窗口关闭按钮退出";
                let dim = measure_text(done_text, None, 24, 1.0);
                draw_text(done_text, (screen_width() - dim.width) / 2.0, screen_height() / 2.0, 24.0, WHITE);
                next_frame().await;
                if is_quit_requested() {
                    let _ = child.kill();
                    let _ = child.wait();
                    return;
                }
            }
            Err(_) => break std::process::ExitStatus::default(),
        }
    };
    eprintln!("渲染完成: {frame} 帧，耗时 {:.1}s ({status})", start.elapsed().as_secs_f64());
    let _ = std::fs::remove_file(&mixed_path);
    RENDER_WIDTH.with(|c| c.set(0.0));
    RENDER_HEIGHT.with(|c| c.set(0.0));
    hits.clear();

    // 帧已全部交给 ffmpeg：保持窗口，直到用户点击窗口关闭按钮才退出
    loop {
        clear_background(BLACK);
        let done_text = "渲染完成，点击窗口关闭按钮退出";
        let dim = measure_text(done_text, None, 28, 1.0);
        draw_text(done_text, (screen_width() - dim.width) / 2.0, screen_height() / 2.0, 28.0, WHITE);
        next_frame().await;
        if is_quit_requested() {
            break;
        }
    }
}

// macOS forbids running a modal panel from inside the window's drawRect
// transaction: the blocking rfd API calls `runModal` and aborts the process.
// The async API uses `beginSheetModalForWindow` instead, and macroquad polls
// the main future every frame, so awaiting it is safe. The wasm build never
// calls this - it reads the chart and music from the page's file inputs.
#[cfg(not(any(
    target_arch = "wasm32",
    target_os = "android",
    target_os = "ios"
)))]
async fn pick_file(title: &str, ext: &str) -> Option<PathBuf> {
    // Windows/Linux: rfd's async dialog resolves its future by waking it from a
    // helper thread, and macroquad's executor panics on any wake call. The
    // blocking dialog runs its own modal loop, so use that there instead.
    #[cfg(not(target_os = "macos"))]
    {
        rfd::FileDialog::new()
            .add_filter(ext, &[ext])
            .set_title(title)
            .pick_file()
    }

    // macOS: a blocking runModal inside the window's drawRect transaction
    // aborts the process, so the sheet-based async API is required.
    #[cfg(target_os = "macos")]
    {
        rfd::AsyncFileDialog::new()
            .add_filter(ext, &[ext])
            .set_title(title)
            .pick_file()
            .await
            .map(|handle| handle.path().to_path_buf())
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    #[cfg(target_arch = "wasm32")]
    return run_web().await;

    #[cfg(any(target_os = "android", target_os = "ios"))]
    return run_mobile().await;

    #[cfg(not(any(
        target_arch = "wasm32",
        target_os = "android",
        target_os = "ios"
    )))]
    {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut recorder_mode = false;
    let mut width = 1080u32;
    let mut height = 1920u32;
    let mut fps = 60u32;
    let mut hwaccel: Option<bool> = None;
    let mut wav_arg: Option<String> = None;
    let mut json_arg: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--recorder" => recorder_mode = true,
            "--width" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    width = v.parse().unwrap_or(width);
                }
            }
            "--height" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    height = v.parse().unwrap_or(height);
                }
            }
            "--fps" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    fps = v.parse().unwrap_or(fps);
                }
            }
            "--hwaccel" => hwaccel = Some(true),
            "--no-hwaccel" => hwaccel = Some(false),
            "--revelation" | "--revelation-size" => {
                i += 1;
                if let Some(v) = args.get(i) {
                    if let Ok(value) = v.parse::<f64>() {
                        set_revelation_size(value);
                    }
                }
            }
            _ => {
                if args[i].to_lowercase().ends_with(".wav") {
                    wav_arg = Some(args[i].clone());
                } else if args[i].to_lowercase().ends_with(".json") {
                    json_arg = Some(args[i].clone());
                }
            }
        }
        i += 1;
    }

    let audio_path = match wav_arg {
        Some(p) => PathBuf::from(p),
        None => match pick_file("选择背景音频 (wav)", "wav").await {
            Some(p) => p,
            None => return,
        },
    };
    let json_path = match json_arg {
        Some(p) => PathBuf::from(p),
        None => match pick_file("选择谱面 (json)", "json").await {
            Some(p) => p,
            None => return,
        },
    };

    let backend = CpalBackend::new(cpal_settings());
    let mut manager = AudioManager::new(backend).unwrap();
    let data = std::fs::read(&audio_path).unwrap();
    let clip = AudioClip::new(data).unwrap();
    let music_duration = clip.length();
    let params = MusicParams {
        loop_mix_time: -1.0,
        amplifier: 1.0,
        playback_rate: 1.0,
        ..Default::default()
    };

    let mut music = manager.create_music(clip, params).unwrap();
    let font = load_ttf_font_from_bytes(FONT_DATA).expect("invalid embedded font");
    set_default_font(font);
    let mut json_data = match std::fs::read(&json_path) {
        Ok(data) => data,
        Err(error) => {
            eprintln!("无法读取谱面 {}: {error}", json_path.display());
            return;
        }
    };
    let mut chart: Chart = match simd_json::serde::from_slice(&mut json_data) {
        Ok(chart) => chart,
        Err(error) => {
            eprintln!("谱面格式错误 {}: {error}", json_path.display());
            return;
        }
    };
    sort_chart(&mut chart);
    recalculate_all_fp(&mut chart);

    init_rng();

    let mut hits: Vec<HitEffect> = Vec::new();
    let mut composer = ChallengeComposer::new();

    if recorder_mode {
        // --recorder：直接视频渲染
        let _ = music.pause();
        render_video(
            &mut chart,
            &mut hits,
            &mut composer,
            music_duration as f64,
            &audio_path,
            width,
            height,
            fps,
            hwaccel,
        )
        .await;
        return;
    }

    // 普通播放
    music.play().unwrap();
    let mut display_fps = 0;
    let mut last_fps_update = get_time();
        loop {
            let position = music.position() as f64;
            draw_frame(&mut chart, &mut hits, position, false, true, &mut composer, None);
        update_fps(&mut display_fps, &mut last_fps_update);
        draw_text(&format!("second:{:.2}  fps:{}", position, display_fps), 20.0, 25.0, 30.0, WHITE);
        next_frame().await;
    }
    }
}

#[cfg(target_arch = "wasm32")]
async fn run_web() {
    init_rng();
    let font = load_ttf_font_from_bytes(FONT_DATA).expect("invalid embedded font");
    set_default_font(font);
    web::init_hit_sounds(TAP_HIT_DATA, DRAG_HIT_DATA);

    let mut chart: Chart = loop {
        if let Some(mut bytes) = web::take_chart() {
            match simd_json::serde::from_slice(&mut bytes) {
                Ok(chart) => break chart,
                Err(error) => eprintln!("谱面格式错误: {error}"),
            }
        }
        clear_background(BLACK);
        draw_text("请在页面上选择谱面 JSON", 40.0, 80.0, 30.0, WHITE);
        next_frame().await;
    };
    sort_chart(&mut chart);
    recalculate_all_fp(&mut chart);

    while !web::music_ready() {
        clear_background(BLACK);
        draw_text("请选择音乐文件", 40.0, 80.0, 30.0, WHITE);
        next_frame().await;
    }

    web::music_play();
    let mut hits: Vec<HitEffect> = Vec::new();
    let mut composer = ChallengeComposer::new();
    let mut display_fps = 0;
    let mut last_fps_update = get_time();
    loop {
        let position = web::music_position();
        draw_frame(&mut chart, &mut hits, position, false, true, &mut composer, None);
        update_fps(&mut display_fps, &mut last_fps_update);
        draw_text(&format!("second:{:.2}  fps:{}", position, display_fps), 20.0, 25.0, 30.0, WHITE);
        next_frame().await;
    }
}

// Mobile has no command line and no rfd backend, so the chart and the music are
// read from the app's Documents folder. On iOS that folder is exposed through
// Finder / the Files app (UIFileSharingEnabled), so dropping files in is the
// "upload" step; no native file picker is required.
#[cfg(target_os = "android")]
fn mobile_document_dir() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("HOME") {
        let documents = PathBuf::from(home).join("Documents");
        if documents.is_dir() {
            return Some(documents);
        }
    }
    std::env::current_dir().ok()
}

#[cfg(target_os = "android")]
fn find_first_file(dir: &std::path::Path, extensions: &[&str]) -> Option<PathBuf> {
    let mut matches: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_file())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| {
                    let lower = name.to_lowercase();
                    extensions.iter().any(|ext| lower.ends_with(ext))
                })
                .unwrap_or(false)
        })
        .collect();
    matches.sort();
    matches.into_iter().next()
}

#[cfg(any(target_os = "android", target_os = "ios"))]
async fn run_mobile() {
    init_rng();
    let font = load_ttf_font_from_bytes(FONT_DATA).expect("invalid embedded font");
    set_default_font(font);

    let json_path = choose_chart().await;
    let audio_path = choose_music().await;

    let mut json_data = match std::fs::read(&json_path) {
        Ok(data) => data,
        Err(error) => {
            eprintln!("cannot read chart {}: {error}", json_path.display());
            return;
        }
    };
    let mut chart: Chart = match simd_json::serde::from_slice(&mut json_data) {
        Ok(chart) => chart,
        Err(error) => {
            eprintln!("invalid chart {}: {error}", json_path.display());
            return;
        }
    };
    sort_chart(&mut chart);
    recalculate_all_fp(&mut chart);

    let backend = CpalBackend::new(cpal_settings());
    let mut manager = AudioManager::new(backend).expect("failed to create AudioManager");
    let data = std::fs::read(&audio_path).expect("cannot read music file");
    let clip = AudioClip::new(data).expect("invalid music file");
    let params = MusicParams {
        loop_mix_time: -1.0,
        amplifier: 1.0,
        playback_rate: 1.0,
        ..Default::default()
    };
    let mut music = manager
        .create_music(clip, params)
        .expect("failed to create music");
    music.play().expect("failed to start music");

    let mut hits: Vec<HitEffect> = Vec::new();
    let mut composer = ChallengeComposer::new();
    let mut display_fps = 0;
    let mut last_fps_update = get_time();
    loop {
        let position = music.position() as f64;
        draw_frame(&mut chart, &mut hits, position, false, true, &mut composer, None);
        update_fps(&mut display_fps, &mut last_fps_update);
        draw_text(&format!("second:{:.2}  fps:{}", position, display_fps), 20.0, 25.0, 30.0, WHITE);
        next_frame().await;
    }
}

#[cfg(target_os = "android")]
async fn choose_chart() -> PathBuf {
    let documents = mobile_document_dir();
    loop {
        if let Some(path) = documents
            .as_deref()
            .and_then(|dir| find_first_file(dir, &[".json"]))
        {
            return path;
        }
        clear_background(BLACK);
        draw_text("Put a chart .json into the app Documents folder", 30.0, 80.0, 26.0, WHITE);
        next_frame().await;
    }
}

#[cfg(target_os = "android")]
async fn choose_music() -> PathBuf {
    let documents = mobile_document_dir();
    loop {
        if let Some(path) = documents
            .as_deref()
            .and_then(|dir| find_first_file(dir, &[".wav", ".ogg", ".mp3", ".flac"]))
        {
            return path;
        }
        clear_background(BLACK);
        draw_text("Put a music file into the app Documents folder", 30.0, 120.0, 26.0, WHITE);
        next_frame().await;
    }
}

#[cfg(target_os = "ios")]
async fn choose_chart() -> PathBuf {
    choose_with_picker("public.json", "Select the chart .json").await
}

#[cfg(target_os = "ios")]
async fn choose_music() -> PathBuf {
    choose_with_picker("public.audio", "Select the music file").await
}

#[cfg(target_os = "ios")]
async fn choose_with_picker(uti: &'static str, prompt: &'static str) -> PathBuf {
    loop {
        if let Some(path) = ios::take_picked() {
            return path;
        }
        if !ios::is_open() {
            ios::open_picker(uti);
        }
        clear_background(BLACK);
        draw_text(prompt, 30.0, 80.0, 26.0, WHITE);
        next_frame().await;
    }
}
