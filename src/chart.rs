use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chart {
    pub file_version: i32,
    // songs_name: String, 可能没有
    pub songs_name: Option<String>,
    pub themes: Vec<Theme>,
    pub challenge_times: Vec<ChallengeTime>,
    #[serde(rename = "bPM")]
    pub bpm: f64,
    pub bpm_shifts: Vec<BpmShift>,
    pub offset: Option<f64>,
    pub lines: Vec<Line>,
    pub canvas_moves: Vec<CanvasMove>,
    pub camera_move: CameraMove,
}

impl Chart {
    pub fn seconds_to_tick(&self, seconds: f64) -> f64 {
        if self.bpm_shifts.is_empty() {
            return seconds / (60.0 / self.bpm);
        }

        let first = &self.bpm_shifts[0];
        if seconds <= first.floor_position {
            return seconds / (60.0 / (self.bpm * first.value));
        }

        let mut prev = first;
        for i in 1..self.bpm_shifts.len() {
            let curr = &self.bpm_shifts[i];
            if seconds <= curr.floor_position {
                let ratio =
                    (seconds - prev.floor_position) / (curr.floor_position - prev.floor_position);
                return prev.time + ratio * (curr.time - prev.time);
            }
            prev = curr;
        }

        let last = self.bpm_shifts.last().unwrap();
        let extra_seconds = seconds - last.floor_position;
        let extra_ticks = extra_seconds / (60.0 / (self.bpm * last.value));
        last.time + extra_ticks
    }

    pub fn tick_to_seconds(&self, tick: f64) -> f64 {
        tick_to_seconds_impl(tick, &self.bpm_shifts, self.bpm)
    }
}

pub fn tick_to_seconds_impl(tick: f64, bpm_shifts: &[BpmShift], base_bpm: f64) -> f64 {
    if bpm_shifts.is_empty() {
        return tick * (60.0 / base_bpm);
    }

    let first = &bpm_shifts[0];
    if tick <= first.time {
        return tick * (60.0 / (base_bpm * first.value));
    }

    for i in 1..bpm_shifts.len() {
        let curr = &bpm_shifts[i];
        if tick <= curr.time {
            let prev = &bpm_shifts[i - 1];
            let ratio = (tick - prev.time) / (curr.time - prev.time);
            return prev.floor_position + ratio * (curr.floor_position - prev.floor_position);
        }
    }

    let last = bpm_shifts.last().unwrap();
    let extra_ticks = tick - last.time;
    let extra_seconds = extra_ticks * (60.0 / (base_bpm * last.value));
    last.floor_position + extra_seconds
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Theme {
    pub colors_list: Vec<Color>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Color {
    #[serde(default, deserialize_with = "deserialize_color_channel")]
    pub r: u8,
    #[serde(default, deserialize_with = "deserialize_color_channel")]
    pub g: u8,
    #[serde(default, deserialize_with = "deserialize_color_channel")]
    pub b: u8,
    #[serde(default = "opaque", deserialize_with = "deserialize_alpha_channel")]
    pub a: u8,
}

fn opaque() -> u8 {
    255
}

fn deserialize_color_channel<'de, D>(deserializer: D) -> Result<u8, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_channel(deserializer, 0)
}

fn deserialize_alpha_channel<'de, D>(deserializer: D) -> Result<u8, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_channel(deserializer, 255)
}

fn deserialize_channel<'de, D>(deserializer: D, default: u8) -> Result<u8, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    let number = match value {
        Some(serde_json::Value::Number(value)) => value.as_f64(),
        Some(serde_json::Value::String(value)) => value.trim().parse::<f64>().ok(),
        _ => None,
    };

    Ok(number
        .filter(|value| value.is_finite())
        .map(|value| value.round().clamp(0.0, 255.0) as u8)
        .unwrap_or(default))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChallengeTime {
    pub check_point: f64,
    pub start: f64,
    pub end: f64,
    pub trans_time: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BpmShift {
    pub time: f64,
    pub value: f64,
    pub floor_position: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Line {
    pub line_points: Vec<LinePoint>,
    pub notes: Vec<Note>,
    pub judge_ring_color: Vec<ColorPoint>,
    pub line_color: Vec<ColorPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinePoint {
    pub time: f64,
    pub x_position: f64,
    pub color: Color,
    pub ease_type: i32,
    pub canvas_index: i32,
    pub floor_position: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    #[serde(rename = "type")]
    pub note_type: i32,
    pub time: f64,
    pub floor_position: f64,
    pub other_informations: Option<Vec<f64>>,
    #[serde(default)]
    pub is_hited: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorPoint {
    pub start_color: Color,
    pub end_color: Color,
    pub time: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasMove {
    // Some exported charts omit this metadata; rendering uses the array position.
    #[serde(default)]
    pub index: i32,
    pub x_position_key_points: Vec<KeyPoint>,
    pub speed_key_points: Vec<KeyPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraMove {
    pub scale_key_points: Vec<KeyPoint>,
    pub x_position_key_points: Vec<KeyPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyPoint {
    pub time: f64,
    pub value: f64,
    pub ease_type: i32,
    pub floor_position: f64,
}

#[cfg(test)]
mod tests {
    use super::{CanvasMove, Color};

    #[test]
    fn color_channels_accept_and_normalize_loose_values() {
        let color: Color =
            serde_json::from_str(r#"{"r":-10,"g":300,"b":127.6,"a":"128"}"#).unwrap();

        assert_eq!((color.r, color.g, color.b, color.a), (0, 255, 128, 128));
    }

    #[test]
    fn color_channels_use_safe_defaults() {
        let color: Color = serde_json::from_str(r#"{"r":null,"g":"bad"}"#).unwrap();

        assert_eq!((color.r, color.g, color.b, color.a), (0, 0, 0, 255));
    }

    #[test]
    fn canvas_index_defaults_when_exporter_omits_it() {
        let canvas: CanvasMove =
            serde_json::from_str(r#"{"xPositionKeyPoints":[],"speedKeyPoints":[]}"#).unwrap();

        assert_eq!(canvas.index, 0);
    }
}
