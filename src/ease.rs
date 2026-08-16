use std::f64::consts::PI;

fn linear(x: f64) -> f64 { x }
fn ease_in_quad(x: f64) -> f64 { x * x }
fn ease_out_quad(x: f64) -> f64 { 1.0 - (1.0 - x) * (1.0 - x) }
fn ease_in_out_quad(x: f64) -> f64 { if x < 0.5 { 2.0 * x * x } else { 1.0 - (-2.0 * x + 2.0_f64).powi(2) / 2.0 } }
fn ease_in_cubic(x: f64) -> f64 { x * x * x }
fn ease_out_cubic(x: f64) -> f64 { 1.0 - (1.0 - x).powi(3) }
fn ease_in_out_cubic(x: f64) -> f64 { if x < 0.5 { 4.0 * x * x * x } else { 1.0 - (-2.0 * x + 2.0_f64).powi(3) / 2.0 } }
fn ease_in_quart(x: f64) -> f64 { x * x * x * x }
fn ease_out_quart(x: f64) -> f64 { 1.0 - (1.0 - x).powi(4) }
fn ease_in_out_quart(x: f64) -> f64 { if x < 0.5 { 8.0 * x * x * x * x } else { 1.0 - (-2.0 * x + 2.0_f64).powi(4) / 2.0 } }
fn ease_in_quint(x: f64) -> f64 { x * x * x * x * x }
fn ease_out_quint(x: f64) -> f64 { 1.0 - (1.0 - x).powi(5) }
fn ease_in_out_quint(x: f64) -> f64 { if x < 0.5 { 16.0 * x * x * x * x * x } else { 1.0 - (-2.0 * x + 2.0_f64).powi(5) / 2.0 } }
fn ease_zero(_x: f64) -> f64 { 0.0 }
fn ease_one(_x: f64) -> f64 { 1.0 }
fn ease_in_circ(x: f64) -> f64 { 1.0 - (1.0 - x * x).sqrt() }
fn ease_out_circ(x: f64) -> f64 { (1.0 - (x - 1.0).powi(2)).sqrt() }
fn ease_out_sine(x: f64) -> f64 { (x * PI / 2.0).sin() }
fn ease_in_sine(x: f64) -> f64 { 1.0 - (x * PI / 2.0).cos() }

pub const EASE_FUNCS: [fn(f64) -> f64; 19] = [
    linear,          // 0
    ease_in_quad,    // 1
    ease_out_quad,   // 2
    ease_in_out_quad,// 3
    ease_in_cubic,   // 4
    ease_out_cubic,  // 5
    ease_in_out_cubic,// 6
    ease_in_quart,   // 7
    ease_out_quart,  // 8
    ease_in_out_quart,// 9
    ease_in_quint,   // 10
    ease_out_quint,  // 11
    ease_in_out_quint,// 12
    ease_zero,       // 13
    ease_one,        // 14
    ease_in_circ,    // 15
    ease_out_circ,   // 16
    ease_out_sine,   // 17
    ease_in_sine,    // 18
];

#[allow(dead_code)]
pub fn lerp(start: f64, end: f64, t: f64) -> f64 {
    start + (end - start) * t
}

#[allow(dead_code)]
pub fn tween_execute(now_time: f64, start_time: f64, end_time: f64, start: f64, end: f64, ease_type: i32) -> f64 {
    let duration = end_time - start_time;
    let rdt = if duration <= 0.0 { 0.0 } else { (now_time - start_time) / duration };
    let t = EASE_FUNCS[ease_type as usize](rdt);
    lerp(start, end, t)
}