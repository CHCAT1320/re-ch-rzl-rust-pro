use macroquad::prelude::*;

use crate::chart::Chart;
use crate::ease;
use crate::{
    calculate_combo, center_x, find_active_challenge_theme_index, find_canvas_move,
    find_canmera_move, find_hit_theme_index, find_line_segment, play_hit_sound, scale_x,
    HitEffect,
};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use crate::{render_h, render_w};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Difficulty {
    Ez,
    Hd,
    In,
}

impl Difficulty {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "ez" | "easy" | "0" => Some(Self::Ez),
            "hd" | "hard" | "1" => Some(Self::Hd),
            "in" | "at" | "insane" | "2" => Some(Self::In),
            _ => None,
        }
    }

    pub fn windows(self) -> JudgeWindows {
        match self {
            Self::Ez => JudgeWindows {
                judge: 0.14,
                normal_perfect: 0.18,
                max_perfect: 0.07,
                bad: 0.20,
            },
            Self::Hd => JudgeWindows {
                judge: 0.115,
                normal_perfect: 0.145,
                max_perfect: 0.06,
                bad: 0.17,
            },
            Self::In => JudgeWindows {
                judge: 0.09,
                normal_perfect: 0.11,
                max_perfect: 0.05,
                bad: 0.125,
            },
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct JudgeWindows {
    pub judge: f64,
    pub normal_perfect: f64,
    pub max_perfect: f64,
    pub bad: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JudgeKind {
    Bad,
    Miss,
    Perfect,
    Early,
    Late,
    MaxPerfect,
    ChallengeMiss,
}

impl JudgeKind {
    fn combo_breaks(self) -> bool {
        matches!(self, Self::Bad | Self::Miss | Self::ChallengeMiss)
    }

    fn is_hit(self) -> bool {
        matches!(
            self,
            Self::Perfect | Self::Early | Self::Late | Self::MaxPerfect
        )
    }

    fn label(self) -> &'static str {
        match self {
            Self::Bad => "Bad",
            Self::Miss => "Miss",
            Self::Perfect => "Hit",
            Self::Early => "Early",
            Self::Late => "Late",
            Self::MaxPerfect => "MaxPerfect",
            Self::ChallengeMiss => "RizMiss",
        }
    }
}

#[derive(Clone, Copy)]
struct PlayNote {
    line: usize,
    index: usize,
    note_type: i32,
    time: f64,
    hold_end: Option<f64>,
    in_challenge: bool,
    consumed: bool,
    drag_armed: bool,
    hold_active: bool,
    hold_head_hit: bool,
    hold_early: bool,
    hold_locked: bool,
}

#[derive(Clone, Copy)]
struct Pointer {
    id: i64,
    x: f64,
    y: f64,
    down: bool,
    pressed: bool,
}

pub struct PlaySession {
    windows: JudgeWindows,
    notes: Vec<PlayNote>,
    pending: Vec<usize>,
    tap_queue: Vec<usize>,
    drag_queue: Vec<usize>,
    // Rizline's NoteInformation.fingerIds: the inputs bound to a hold when its
    // head is judged. Entries are dropped as they are released; the list is
    // never extended afterwards, so unrelated later presses cannot take over.
    hold_bindings: Vec<Vec<i64>>,
    pointers: Vec<Pointer>,
    combo: i32,
    max_combo: i32,
    combo_score: i32,
    max_combo_score: i32,
    hit: i32,
    bad: i32,
    miss: i32,
    early: i32,
    late: i32,
    max_perfect: i32,
    challenge_hit: i32,
    challenge_notes: i32,
    all_notes: i32,
    finished: bool,
    result_printed: bool,
}

// Keyboards only exist on desktop and in the browser. Phones and tablets are
// touch only, so the key lanes are not even compiled there.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
const KEY_CODES: [KeyCode; 34] = [
    KeyCode::A,
    KeyCode::B,
    KeyCode::C,
    KeyCode::D,
    KeyCode::E,
    KeyCode::F,
    KeyCode::G,
    KeyCode::H,
    KeyCode::I,
    KeyCode::J,
    KeyCode::K,
    KeyCode::L,
    KeyCode::M,
    KeyCode::N,
    KeyCode::O,
    KeyCode::P,
    KeyCode::Q,
    KeyCode::R,
    KeyCode::S,
    KeyCode::T,
    KeyCode::U,
    KeyCode::V,
    KeyCode::W,
    KeyCode::X,
    KeyCode::Y,
    KeyCode::Z,
    KeyCode::Space,
    KeyCode::LeftShift,
    KeyCode::LeftControl,
    KeyCode::LeftAlt,
    KeyCode::Enter,
    KeyCode::Tab,
    KeyCode::Comma,
    KeyCode::Period,
];

impl PlaySession {
    pub fn new(chart: &Chart, difficulty: Difficulty) -> Self {
        let windows = difficulty.windows();
        let mut notes = Vec::new();
        let mut challenge_notes = 0i32;
        let mut all_notes = 0i32;
        for (line_idx, line) in chart.lines.iter().enumerate() {
            for (note_idx, note) in line.notes.iter().enumerate() {
                let time = chart.tick_to_seconds(note.time);
                let hold_end = if note.note_type == 2 {
                    note.other_informations
                        .as_ref()
                        .and_then(|infos| infos.first().copied())
                        .map(|tick| chart.tick_to_seconds(tick))
                } else {
                    None
                };
                let in_challenge = note_in_challenge(chart, time);
                let weight = if note.note_type == 2 { 2 } else { 1 };
                all_notes += weight;
                if in_challenge {
                    challenge_notes += weight;
                }
                notes.push(PlayNote {
                    line: line_idx,
                    index: note_idx,
                    note_type: note.note_type,
                    time,
                    hold_end,
                    in_challenge,
                    consumed: false,
                    drag_armed: false,
                    hold_active: false,
                    hold_head_hit: false,
                    hold_early: false,
                    hold_locked: false,
                });
            }
        }
        notes.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));
        let pending: Vec<usize> = (0..notes.len()).collect();
        let note_count = notes.len();
        let max_combo_score = (1..=all_notes).map(calculate_combo).sum();
        Self {
            windows,
            notes,
            pending,
            tap_queue: Vec::new(),
            drag_queue: Vec::new(),
            hold_bindings: vec![Vec::new(); note_count],
            pointers: Vec::new(),
            combo: 0,
            max_combo: 0,
            combo_score: 0,
            max_combo_score,
            hit: 0,
            bad: 0,
            miss: 0,
            early: 0,
            late: 0,
            max_perfect: 0,
            challenge_hit: 0,
            challenge_notes,
            all_notes,
            finished: false,
            result_printed: false,
        }
    }

    pub fn displayed_combo(&self) -> i32 {
        calculate_combo(self.combo)
    }

    pub fn update(
        &mut self,
        chart: &mut Chart,
        time: f64,
        hits: &mut Vec<HitEffect>,
        mute: bool,
        music_ended: bool,
    ) {
        self.poll_pointers();
        let challenge_on = find_active_challenge_theme_index(chart, time).is_some();
        self.enqueue_waiting(time);
        self.miss_late_notes(chart, time, hits, challenge_on);
        self.judge_taps(chart, time, hits, mute, challenge_on);
        self.arm_and_hit_drags(chart, time, hits, mute, challenge_on);
        self.update_holds(chart, time, hits, mute, challenge_on);
        hits.retain(|h| time >= h.timer && time - h.timer <= h.duration);
        if music_ended && !self.finished {
            self.finish_remaining(chart, time, hits, challenge_on);
            self.finished = true;
        }
        if self.finished {
            self.print_result();
        }
    }

    fn poll_pointers(&mut self) {
        self.pointers.clear();
        poll_keys(&mut self.pointers);
        let touch_list = touches();
        if touch_list.is_empty() {
            poll_mouse(&mut self.pointers);
            return;
        }
        for touch in touch_list {
            let pressed = touch.phase == TouchPhase::Started;
            let down = matches!(
                touch.phase,
                TouchPhase::Started | TouchPhase::Stationary | TouchPhase::Moved
            );
            if !pressed && !down {
                continue;
            }
            if pressed {
                eprintln!("touch: {:.0},{:.0}", touch.position.x, touch.position.y);
            }
            self.pointers.push(Pointer {
                id: touch.id as i64,
                x: touch.position.x as f64,
                y: touch.position.y as f64,
                down,
                pressed,
            });
        }
    }

    fn any_input(&self) -> bool {
        self.pointers.iter().any(|pointer| pointer.down || pointer.pressed)
    }

    fn enqueue_waiting(&mut self, time: f64) {
        let mut i = 0;
        while i < self.pending.len() {
            let idx = self.pending[i];
            let note = self.notes[idx];
            if time + self.windows.bad < note.time {
                break;
            }
            if note.note_type == 1 && time + self.windows.normal_perfect < note.time {
                i += 1;
                continue;
            }
            self.pending.remove(i);
            if note.note_type == 1 {
                self.drag_queue.push(idx);
            } else {
                self.tap_queue.push(idx);
            }
        }
    }

    fn miss_late_notes(&mut self, chart: &mut Chart, time: f64, hits: &mut Vec<HitEffect>, challenge_on: bool) {
        let mut events = Vec::new();
        let mut i = 0;
        while i < self.tap_queue.len() {
            let idx = self.tap_queue[i];
            let note = self.notes[idx];
            if note.consumed || note.hold_active || note.hold_head_hit {
                i += 1;
                continue;
            }
            let late_limit = if note.in_challenge && challenge_on {
                self.windows.normal_perfect
            } else {
                self.windows.judge
            };
            if time > note.time + late_limit {
                self.tap_queue.remove(i);
                events.push((idx, note.in_challenge && challenge_on));
            } else {
                i += 1;
            }
        }
        i = 0;
        while i < self.drag_queue.len() {
            let idx = self.drag_queue[i];
            let note = self.notes[idx];
            if note.consumed {
                i += 1;
                continue;
            }
            if time > note.time + self.windows.normal_perfect {
                self.drag_queue.remove(i);
                events.push((idx, note.in_challenge && challenge_on));
            } else {
                i += 1;
            }
        }
        for (idx, challenge) in events {
            let kind = if challenge {
                JudgeKind::ChallengeMiss
            } else {
                JudgeKind::Miss
            };
            self.apply_judge(chart, idx, kind, time, false, Some(hits), true, None);
        }
    }

    fn judge_taps(
        &mut self,
        chart: &mut Chart,
        time: f64,
        hits: &mut Vec<HitEffect>,
        mute: bool,
        challenge_on: bool,
    ) {
        // 原作 InputInfo：新按下的键 phase=1 才参与 tap/hold/drag 判定；
        // 按住的键 phase=5，只用于持续判定（drag 起效 / hold 维持）。
        let mut used = vec![false; self.notes.len()];
        let pressed: Vec<(i64, f64, f64)> = self
            .pointers
            .iter()
            .filter(|pointer| pointer.pressed)
            .map(|pointer| (pointer.id, pointer.x, pointer.y))
            .collect();
        // Original Update only processes touches whose `began` flag is set and
        // binds those same touches to the hold it judges. Collect the inputs
        // that actually hit a note this frame, then bind every hold judged this
        // frame to that whole set. This gives simultaneous holds/taps their
        // shared binding without letting an already-down or later finger take
        // over a hold.
        let mut frame_hit_ids: Vec<i64> = Vec::new();
        let mut holds_judged: Vec<usize> = Vec::new();
        for (pointer_id, input_x, input_y) in pressed {
            let Some(note_idx) = self.closest_tap(chart, time, input_x) else {
                continue;
            };
            if used[note_idx] {
                continue;
            }
            let note = self.notes[note_idx];
            let dt = time - note.time;
            let abs_dt = dt.abs();
            let in_riz = note.in_challenge && challenge_on;
            let hit_window = if in_riz {
                self.windows.normal_perfect
            } else {
                self.windows.judge
            };
            if abs_dt <= hit_window {
                let kind = if in_riz {
                    if abs_dt <= self.windows.max_perfect {
                        JudgeKind::MaxPerfect
                    } else if dt < 0.0 {
                        JudgeKind::Early
                    } else {
                        JudgeKind::Late
                    }
                } else {
                    JudgeKind::Perfect
                };
                let is_hold = self.notes[note_idx].note_type == 2;
                if is_hold {
                    self.notes[note_idx].hold_head_hit = true;
                }
                used[note_idx] = true;
                frame_hit_ids.push(pointer_id);
                if is_hold {
                    holds_judged.push(note_idx);
                }
                self.apply_judge(chart, note_idx, kind, time, mute, Some(hits), !is_hold, None);
                if is_hold {
                    self.notes[note_idx].hold_active = true;
                    self.notes[note_idx].consumed = false;
                    self.remove_from_queues(note_idx);
                    set_note_hit(chart, self.notes[note_idx].line, self.notes[note_idx].index, true);
                }
            } else if dt < 0.0 && abs_dt <= self.windows.bad {
                self.apply_judge(
                    chart,
                    note_idx,
                    JudgeKind::Bad,
                    time,
                    true,
                    Some(hits),
                    false,
                    Some((input_x, input_y)),
                );
            }
        }

        // All holds whose head was hit this frame share the inputs that hit
        // notes this frame. Later frames never extend the list.
        for idx in holds_judged {
            self.hold_bindings[idx] = frame_hit_ids.clone();
        }
    }

    fn closest_tap(&self, chart: &Chart, time: f64, input_x: f64) -> Option<usize> {
        let start = self.tap_queue.iter().position(|&idx| {
            let note = self.notes[idx];
            !note.consumed && !note.hold_head_hit && note.note_type != 1
        })?;
        let first = self.tap_queue[start];
        let note0 = self.notes[first];
        let t0 = note0.time;
        let mut best = first;
        let mut best_dist = (note_screen_x(chart, note0.line, note0.time, time) - input_x).abs();
        for &idx in self.tap_queue.iter().skip(start + 1) {
            let note = self.notes[idx];
            if (note.time - t0).abs() >= 1e-6 {
                break;
            }
            if note.consumed || note.hold_head_hit || note.note_type == 1 {
                continue;
            }
            let dist = (note_screen_x(chart, note.line, note.time, time) - input_x).abs();
            if dist < best_dist {
                best_dist = dist;
                best = idx;
            }
        }
        Some(best)
    }

    fn remove_from_queues(&mut self, note_idx: usize) {
        self.pending.retain(|&idx| idx != note_idx);
        self.tap_queue.retain(|&idx| idx != note_idx);
        self.drag_queue.retain(|&idx| idx != note_idx);
    }

    fn arm_and_hit_drags(
        &mut self,
        chart: &mut Chart,
        time: f64,
        hits: &mut Vec<HitEffect>,
        mute: bool,
        challenge_on: bool,
    ) {
        let any = self.any_input();
        if any {
            for &idx in &self.drag_queue {
                if !self.notes[idx].consumed {
                    self.notes[idx].drag_armed = true;
                }
            }
        }
        let mut hits_idx = Vec::new();
        let mut i = 0;
        while i < self.drag_queue.len() {
            let idx = self.drag_queue[i];
            let note = self.notes[idx];
            if note.consumed {
                i += 1;
                continue;
            }
            if note.drag_armed && time >= note.time {
                self.drag_queue.remove(i);
                hits_idx.push(idx);
            } else {
                i += 1;
            }
        }
        for idx in hits_idx {
            let in_riz = self.notes[idx].in_challenge && challenge_on;
            let kind = if in_riz {
                JudgeKind::MaxPerfect
            } else {
                JudgeKind::Perfect
            };
            self.apply_judge(chart, idx, kind, time, mute, Some(hits), true, None);
        }
    }

    fn update_holds(
        &mut self,
        chart: &mut Chart,
        time: f64,
        hits: &mut Vec<HitEffect>,
        _mute: bool,
        challenge_on: bool,
    ) {
        let mut finished = Vec::new();
        let mut missed = Vec::new();
        let mut downgraded = Vec::new();
        let mut lock = Vec::new();
        // Drop released fingers from every active hold, mirroring CheckHold.
        for idx in 0..self.notes.len() {
            if !self.notes[idx].hold_active {
                continue;
            }
            let pointers = &self.pointers;
            self.hold_bindings[idx]
                .retain(|id| pointers.iter().any(|pointer| pointer.id == *id && pointer.down));
        }
        for (idx, note) in self.notes.iter().enumerate() {
            if !note.hold_active {
                continue;
            }
            let end = note.hold_end.unwrap_or(note.time);
            let held = !self.hold_bindings[idx].is_empty();
            // 原作 CheckHold 的尾判只看音符自身的 inChallengeTime，不看尾判
            // 发生时挑战是否仍然激活。
            let in_riz = note.in_challenge;
            // 原作 CheckHold：松手容差 = 1.6 × (inChallengeTime ? normalPerfect : judgeTime)。
            let tol = if in_riz {
                self.windows.normal_perfect * 1.6
            } else {
                self.windows.judge * 1.6
            };
            // 进入松手容差带即锁定为成功，之后无论输入如何都不再判 Miss/Bad。
            if end - time <= tol {
                lock.push(idx);
            }
            if time >= end {
                finished.push(idx);
            } else if !held {
                if !note.hold_locked && end - time > tol {
                    missed.push(idx);
                } else if end - time > self.windows.max_perfect * 1.6 {
                    // 提前松手超过 1.6×maxPerfect：尾判降为 Perfect（不作 MaxPerfect）
                    downgraded.push(idx);
                }
            }
        }
        for idx in lock {
            self.notes[idx].hold_locked = true;
        }
        for idx in downgraded {
            self.notes[idx].hold_early = true;
        }
        for idx in missed {
            let challenge = self.notes[idx].in_challenge;
            let kind = if challenge {
                JudgeKind::ChallengeMiss
            } else {
                JudgeKind::Miss
            };
            self.notes[idx].hold_active = false;
            self.apply_judge(chart, idx, kind, time, true, Some(hits), true, None);
        }
        for idx in finished {
            let in_riz = self.notes[idx].in_challenge && challenge_on && !self.notes[idx].hold_early;
            let kind = if in_riz {
                JudgeKind::MaxPerfect
            } else {
                JudgeKind::Perfect
            };
            self.notes[idx].hold_active = false;
            self.apply_judge(chart, idx, kind, time, true, Some(hits), true, None);
        }
    }

    fn finish_remaining(&mut self, chart: &mut Chart, time: f64, hits: &mut Vec<HitEffect>, challenge_on: bool) {
        // 曲末收尾：仍按住的长条按成功结算；只有确实已过晚 Miss 窗口的音符才判 Miss。
        // 这样处于“允许提前松手”区间内、或被正常按住的音符不会被误判。
        let mut judged = Vec::new();
        for (idx, note) in self.notes.iter().enumerate() {
            if note.consumed {
                continue;
            }
            if note.hold_active {
                judged.push((idx, true));
                continue;
            }
            let late_limit = if note.note_type == 1 {
                self.windows.normal_perfect
            } else if note.in_challenge && challenge_on {
                self.windows.normal_perfect
            } else {
                self.windows.judge
            };
            if time > note.time + late_limit {
                judged.push((idx, false));
            }
        }
        for (idx, hold_ok) in judged {
            let note = self.notes[idx];
            if hold_ok {
                let in_riz = note.in_challenge && challenge_on && !note.hold_early;
                let kind = if in_riz {
                    JudgeKind::MaxPerfect
                } else {
                    JudgeKind::Perfect
                };
                self.notes[idx].hold_active = false;
                self.apply_judge(chart, idx, kind, time, true, Some(hits), true, None);
            } else {
                let kind = if note.in_challenge && challenge_on {
                    JudgeKind::ChallengeMiss
                } else {
                    JudgeKind::Miss
                };
                self.apply_judge(chart, idx, kind, time, true, Some(hits), true, None);
            }
        }
    }

    fn apply_judge(
        &mut self,
        chart: &mut Chart,
        note_idx: usize,
        kind: JudgeKind,
        time: f64,
        mute: bool,
        hits: Option<&mut Vec<HitEffect>>,
        consume: bool,
        fx_pos: Option<(f64, f64)>,
    ) {
        let note = self.notes[note_idx];
        if consume && self.notes[note_idx].consumed {
            return;
        }
        if consume {
            self.remove_from_queues(note_idx);
        }
        eprintln!(
            "judge: {}  type:{}  dt:{:+.0}ms  combo:{}",
            kind.label(),
            note.note_type,
            (time - note.time) * 1000.0,
            if kind.combo_breaks() { 0 } else { self.combo + if kind.is_hit() { 1 } else { 0 } }
        );
        match kind {
            JudgeKind::Bad => self.bad += 1,
            JudgeKind::Miss | JudgeKind::ChallengeMiss => self.miss += 1,
            JudgeKind::Perfect => self.hit += 1,
            JudgeKind::Early => {
                self.hit += 1;
                self.early += 1;
            }
            JudgeKind::Late => {
                self.hit += 1;
                self.late += 1;
            }
            JudgeKind::MaxPerfect => {
                self.hit += 1;
                self.max_perfect += 1;
            }
        }
        if kind.combo_breaks() {
            self.combo = 0;
            if let Some(hits) = hits {
                spawn_fail_effect(chart, note.line, note.time, time, kind, fx_pos, hits);
            }
        } else if kind.is_hit() {
            self.combo += 1;
            self.max_combo = self.max_combo.max(self.combo);
            self.combo_score += calculate_combo(self.combo);
            if note.in_challenge {
                self.challenge_hit += 1;
            }
            if let Some(hits) = hits {
                spawn_hit_effect(chart, note.line, note.time, time, hits);
            }
            if !mute {
                play_hit_sound(note.note_type);
            }
        }
        if consume {
            self.notes[note_idx].consumed = true;
            self.notes[note_idx].hold_active = false;
            let hide = if note.note_type == 2 {
                kind.is_hit()
            } else {
                kind.is_hit() || kind.combo_breaks()
            };
            set_note_hit(chart, note.line, note.index, hide);
            if matches!(kind, JudgeKind::Miss | JudgeKind::ChallengeMiss) {
                mark_note_missed(chart, note.line, note.index, time);
                if note.note_type == 2 && !note.hold_head_hit {
                    self.miss += 1;
                }
            }
        }
    }

    fn print_result(&mut self) {
        if self.result_printed {
            return;
        }
        self.result_printed = true;
        let combo_rate = if self.max_combo_score > 0 {
            self.combo_score as f64 / self.max_combo_score as f64
        } else {
            1.0
        };
        let riz_rate = if self.challenge_notes > 0 {
            self.challenge_hit as f64 / self.challenge_notes as f64
        } else {
            1.0
        };
        let complete = combo_rate * 70.0 + riz_rate * 20.0;
        let all_perfect = self.miss == 0;
        eprintln!("=== PLAY RESULT ===");
        eprintln!("Notes: {}", self.all_notes);
        eprintln!("Hit: {}", self.hit);
        eprintln!("MaxPerfect: {}", self.max_perfect);
        eprintln!("Early: {}", self.early);
        eprintln!("Late: {}", self.late);
        eprintln!("Bad: {}", self.bad);
        eprintln!("Miss: {}", self.miss);
        eprintln!("MaxCombo: {}", self.max_combo);
        eprintln!("ComboScore: {} / {}", self.combo_score, self.max_combo_score);
        eprintln!("CompleteRate: {:.4}%", complete + 0.0);
        eprintln!("AllPerfect: {}", all_perfect);
        #[cfg(target_os = "android")]
        crate::android::show_result(&format!(
            "Notes: {}\nHit: {}\nMaxPerfect: {}\nEarly: {}\nLate: {}\nBad: {}\nMiss: {}\nMaxCombo: {}\nComboScore: {} / {}\nCompleteRate: {:.2}%\nAllPerfect: {}",
            self.all_notes,
            self.hit,
            self.max_perfect,
            self.early,
            self.late,
            self.bad,
            self.miss,
            self.max_combo,
            self.combo_score,
            self.max_combo_score,
            complete,
            all_perfect
        ));
    }
}

fn note_in_challenge(chart: &Chart, time: f64) -> bool {
    chart.challenge_times.iter().any(|ct| {
        let start = chart.tick_to_seconds(ct.start);
        let end = chart.tick_to_seconds(ct.end);
        time >= start && time <= end
    })
}

fn set_note_hit(chart: &mut Chart, line: usize, index: usize, hit: bool) {
    if let Some(note) = chart.lines.get_mut(line).and_then(|line| line.notes.get_mut(index)) {
        note.is_hited = hit;
    }
}

fn mark_note_missed(chart: &mut Chart, line: usize, index: usize, time: f64) {
    if let Some(note) = chart.lines.get_mut(line).and_then(|line| line.notes.get_mut(index)) {
        if note.missed_at <= 0.0 {
            note.missed_at = time;
        }
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn poll_keys(pointers: &mut Vec<Pointer>) {
    disable_ime();
    for (i, &code) in KEY_CODES.iter().enumerate() {
        let down = is_key_down(code);
        let pressed = is_key_pressed(code);
        if pressed {
            eprintln!("key: {}", key_name(code));
        }
        if down || pressed {
            pointers.push(Pointer {
                id: -(i as i64 + 1),
                x: key_lane_x(i, KEY_CODES.len()),
                y: key_lane_y(),
                down,
                pressed,
            });
        }
    }
}

// Touch only: no keyboard lanes on phones/tablets.
#[cfg(any(target_os = "android", target_os = "ios"))]
fn poll_keys(_pointers: &mut Vec<Pointer>) {}

// A mouse stands in for a finger on desktop and in the browser. It is only
// consulted when no real touch is active, so a mouse never fights a touchscreen.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn poll_mouse(pointers: &mut Vec<Pointer>) {
    let pressed = is_mouse_button_pressed(MouseButton::Left);
    let down = is_mouse_button_down(MouseButton::Left);
    if !pressed && !down {
        return;
    }
    let pos = mouse_position();
    if pressed {
        eprintln!("touch: {:.0},{:.0}", pos.0, pos.1);
    }
    pointers.push(Pointer {
        id: -100,
        x: pos.0 as f64,
        y: pos.1 as f64,
        down,
        pressed,
    });
}

#[cfg(any(target_os = "android", target_os = "ios"))]
fn poll_mouse(_pointers: &mut Vec<Pointer>) {}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn key_lane_x(index: usize, count: usize) -> f64 {
    let t = if index == 0 {
        0.5
    } else {
        (index + 2) as f64 / (count as f64 + 1.0) * 0.7 + 0.15
    };
    t * render_w() as f64
}

fn note_screen_x(chart: &Chart, line_idx: usize, note_time: f64, time: f64) -> f64 {
    let line = &chart.lines[line_idx];
    if line.line_points.is_empty() {
        return center_x();
    }
    let camera_pos = find_canmera_move(chart, time);
    let camera_x = camera_pos[0];
    let camera_scale = camera_pos[1];
    let (idx1, idx2) = find_line_segment(&line.line_points, note_time, chart);
    let lp = &line.line_points[idx1];
    let next_lp = &line.line_points[idx2];
    let lp_cvs = find_canvas_move(chart, time, lp.canvas_index as i32);
    let next_lp_cvs = find_canvas_move(chart, time, next_lp.canvas_index as i32);
    let lp_time = chart.tick_to_seconds(lp.time);
    let next_lp_time = chart.tick_to_seconds(next_lp.time);
    let lp_x = (lp.x_position + lp_cvs[0] + camera_x) * 720.0 * camera_scale * scale_x() + center_x();
    let next_lp_x =
        (next_lp.x_position + next_lp_cvs[0] + camera_x) * 720.0 * camera_scale * scale_x() + center_x();
    let t_note = if (next_lp_time - lp_time).abs() > 1e-12 {
        ((note_time - lp_time) / (next_lp_time - lp_time)).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let ease_note = ease::EASE_FUNCS[lp.ease_type as usize](t_note);
    lp_x + (next_lp_x - lp_x) * ease_note
}

// 音符的实际屏幕位置：音符时间之前 x 由自身时间所在的线段决定；
// 时间已过则和 draw_notes 一样改用“当前时间”所在的线段（音符沿判定线滑动）。
// 打击特效必须用当前位置，否则线的位置一变（长条尾判、晚 Miss）就会画偏。
fn note_effect_x(chart: &Chart, line_idx: usize, note_time: f64, time: f64) -> f64 {
    let pos_time = if time > note_time { time } else { note_time };
    note_screen_x(chart, line_idx, pos_time, time)
}

fn spawn_hit_effect(
    chart: &Chart,
    line_idx: usize,
    note_time: f64,
    time: f64,
    hits: &mut Vec<HitEffect>,
) {
    let x = note_effect_x(chart, line_idx, note_time, time);
    let theme_idx = find_hit_theme_index(chart, time);
    hits.push(HitEffect::new(x, time, theme_idx.is_some()));
}

fn spawn_fail_effect(
    chart: &mut Chart,
    line_idx: usize,
    note_time: f64,
    time: f64,
    kind: JudgeKind,
    fx_pos: Option<(f64, f64)>,
    hits: &mut Vec<HitEffect>,
) {
    let x = fx_pos
        .map(|(x, _)| x)
        .unwrap_or_else(|| note_effect_x(chart, line_idx, note_time, time));
    if kind == JudgeKind::Bad {
        let y = fx_pos.map(|(_, y)| y).unwrap_or_else(|| crate::floor_y());
        hits.push(HitEffect::bad(x, y, time));
    } else {
        hits.push(HitEffect::miss(x, time));
        if let Some(line) = chart.lines.get_mut(line_idx) {
            line.miss_flash_until = time + 0.35;
        }
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn key_lane_y() -> f64 {
    render_h() as f64 * (1.0 - 0.13379312)
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn disable_ime() {
    #[cfg(target_os = "windows")]
    {
        use std::sync::atomic::{AtomicBool, Ordering};
        static DONE: AtomicBool = AtomicBool::new(false);

        #[link(name = "imm32")]
        unsafe extern "system" {
            fn ImmDisableIME(id_thread: u32) -> i32;
            fn ImmAssociateContext(hwnd: *mut core::ffi::c_void, himc: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
        }
        #[link(name = "user32")]
        unsafe extern "system" {
            fn GetActiveWindow() -> *mut core::ffi::c_void;
            fn GetForegroundWindow() -> *mut core::ffi::c_void;
        }

        if !DONE.load(Ordering::Relaxed) {
            unsafe {
                ImmDisableIME(u32::MAX);
            }
        }
        unsafe {
            let hwnd = GetActiveWindow();
            let hwnd = if hwnd.is_null() { GetForegroundWindow() } else { hwnd };
            if !hwnd.is_null() {
                ImmAssociateContext(hwnd, core::ptr::null_mut());
                DONE.store(true, Ordering::Relaxed);
            }
        }
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn key_name(code: KeyCode) -> &'static str {
    match code {
        KeyCode::A => "A",
        KeyCode::B => "B",
        KeyCode::C => "C",
        KeyCode::D => "D",
        KeyCode::E => "E",
        KeyCode::F => "F",
        KeyCode::G => "G",
        KeyCode::H => "H",
        KeyCode::I => "I",
        KeyCode::J => "J",
        KeyCode::K => "K",
        KeyCode::L => "L",
        KeyCode::M => "M",
        KeyCode::N => "N",
        KeyCode::O => "O",
        KeyCode::P => "P",
        KeyCode::Q => "Q",
        KeyCode::R => "R",
        KeyCode::S => "S",
        KeyCode::T => "T",
        KeyCode::U => "U",
        KeyCode::V => "V",
        KeyCode::W => "W",
        KeyCode::X => "X",
        KeyCode::Y => "Y",
        KeyCode::Z => "Z",
        KeyCode::Space => "Space",
        KeyCode::LeftShift => "LeftShift",
        KeyCode::LeftControl => "LeftControl",
        KeyCode::LeftAlt => "LeftAlt",
        KeyCode::Enter => "Enter",
        KeyCode::Tab => "Tab",
        KeyCode::Comma => ",",
        KeyCode::Period => ".",
        _ => "Unknown",
    }
}
