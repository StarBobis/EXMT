//! AnimationClip decoding: Unity's flat curve space -> per-bone TRS tracks.
//!
//! Ported from `core/exmt_core/anim.py`. A built clip stores its runtime data in
//! three containers (streamed cubic keys, dense frames, constants) and one flat
//! curve index space described by `m_ClipBindingConstant.genericBindings`.
//! Sampling at 30 Hz is what the workspace export needs; the Blender add-on does
//! not re-implement Unity's cubic evaluation.

use std::collections::HashMap;

use crate::error::{Error, Result};
use crate::typetree::Value;

pub const FORMAT: &str = "exmt-anim-1";
const TRANSFORM: i32 = 4;

#[derive(Debug, Clone)]
pub struct Binding {
    pub path: u32,
    pub attribute: i32,
    pub type_id: i32,
    pub base: u32,
    pub count: u32,
}

#[derive(Debug, Clone)]
pub struct Event {
    pub time: f32,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct Clip {
    pub name: String,
    pub sample_rate: f32,
    pub start: f32,
    pub stop: f32,
    pub looped: bool,
    pub events: Vec<Event>,
    streamed_count: usize,
    frames: Vec<(f32, Vec<(i32, [f32; 4])>)>,
    dense_frames: usize,
    dense_count: usize,
    dense_rate: f32,
    dense_begin: f32,
    dense: Vec<f32>,
    constant: Vec<f32>,
    pub bindings: Vec<Binding>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub path: String,
    pub hash: u32,
    pub position: Option<Vec<Vec<f64>>>,
    pub rotation: Option<Vec<Vec<f64>>>,
    pub scale: Option<Vec<Vec<f64>>>,
    pub euler: Option<Vec<Vec<f64>>>,
}

#[derive(Debug, Clone)]
pub struct Tracks {
    pub rows: Vec<Track>,
    pub times: Vec<f64>,
    pub rate: f32,
}

impl Clip {
    pub fn length(&self) -> f32 {
        (self.stop - self.start).max(0.0)
    }

    pub fn value(&self, index: i32, t: f64) -> f64 {
        if index < 0 {
            return 0.0;
        }
        let index = index as usize;
        if index < self.streamed_count {
            return self.streamed_value(index, t);
        }
        if index < self.streamed_count + self.dense_count {
            if self.dense_frames == 0 {
                return 0.0;
            }
            return self.dense_value(index - self.streamed_count, t);
        }
        let j = index - self.streamed_count - self.dense_count;
        self.constant.get(j).map(|v| *v as f64).unwrap_or(0.0)
    }

    fn streamed_value(&self, index: usize, t: f64) -> f64 {
        let mut row: Vec<(f32, [f32; 4])> = Vec::new();
        for (time, keys) in &self.frames {
            for (key, coeff) in keys {
                if *key as usize == index {
                    row.push((*time, *coeff));
                }
            }
        }
        if row.is_empty() {
            return 0.0;
        }
        row.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        if t <= row[0].0 as f64 {
            return row[0].1[3] as f64;
        }
        if t >= row[row.len() - 1].0 as f64 {
            return row[row.len() - 1].1[3] as f64;
        }
        let mut lo = 0usize;
        let mut hi = row.len() - 1;
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if row[mid].0 as f64 <= t {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let (key_t, [a, b, c, d]) = row[lo];
        let dt = t - key_t as f64;
        ((a as f64 * dt + b as f64) * dt + c as f64) * dt + d as f64
    }

    fn dense_value(&self, index: usize, t: f64) -> f64 {
        let x = (t - self.dense_begin as f64) * self.dense_rate as f64;
        if x <= 0.0 {
            return self.dense.get(index).map(|v| *v as f64).unwrap_or(0.0);
        }
        let frame = x as usize;
        if frame >= self.dense_frames - 1 {
            return self
                .dense
                .get((self.dense_frames - 1) * self.dense_count + index)
                .map(|v| *v as f64)
                .unwrap_or(0.0);
        }
        let frac = x - frame as f64;
        let a = self
            .dense
            .get(frame * self.dense_count + index)
            .map(|v| *v as f64)
            .unwrap_or(0.0);
        let b = self
            .dense
            .get((frame + 1) * self.dense_count + index)
            .map(|v| *v as f64)
            .unwrap_or(0.0);
        a + (b - a) * frac
    }

    /// Sample per-bone tracks at `rate` (default min(sample rate, 30)).
    pub fn tracks(&self, rate: Option<f32>, paths: &HashMap<u32, String>) -> Tracks {
        let rate = rate.unwrap_or(self.sample_rate.min(30.0)).max(0.001);
        let steps = ((self.length() * rate).floor() as usize + 1).max(2);
        let times: Vec<f64> = (0..steps)
            .map(|i| self.start as f64 + i as f64 / rate as f64)
            .collect();

        let mut by_path: HashMap<u32, Track> = HashMap::new();
        for binding in &self.bindings {
            if binding.type_id != TRANSFORM {
                continue;
            }
            let row = by_path.entry(binding.path).or_insert_with(|| Track {
                path: paths.get(&binding.path).cloned().unwrap_or_default(),
                hash: binding.path,
                position: None,
                rotation: None,
                scale: None,
                euler: None,
            });
            let Some(name) = attribute_name(binding.attribute) else {
                continue;
            };
            let axes: Vec<Vec<f64>> = (0..binding.count)
                .map(|c| {
                    times
                        .iter()
                        .map(|t| self.value(binding.base as i32 + c as i32, *t))
                        .collect()
                })
                .collect();
            match name {
                "position" => row.position = Some(axes),
                "rotation" => row.rotation = Some(axes),
                "scale" => row.scale = Some(axes),
                "euler" => row.euler = Some(axes),
                _ => {}
            }
        }
        let mut rows: Vec<Track> = by_path.into_values().collect();
        rows.sort_by(|a, b| {
            let key = |row: &Track| {
                if row.path.is_empty() {
                    format!("~{}", row.hash)
                } else {
                    row.path.clone()
                }
            };
            key(a).cmp(&key(b))
        });
        Tracks { rows, times, rate }
    }
}

fn attribute_name(attribute: i32) -> Option<&'static str> {
    match attribute {
        1 => Some("position"),
        2 => Some("rotation"),
        3 => Some("scale"),
        4 => Some("euler"),
        _ => None,
    }
}

/// Decode a clip from its typetree.
pub fn from_value(tree: &Value) -> Result<Clip> {
    let name = tree
        .get("m_Name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let sample_rate = tree
        .get("m_SampleRate")
        .and_then(|v| v.as_f32())
        .unwrap_or(30.0);
    let muscle = tree
        .get("m_MuscleClip")
        .ok_or_else(|| invalid("missing m_MuscleClip"))?;
    let start = muscle
        .get("m_StartTime")
        .and_then(|v| v.as_f32())
        .unwrap_or(0.0);
    let stop = muscle
        .get("m_StopTime")
        .and_then(|v| v.as_f32())
        .unwrap_or(0.0);
    let looped = muscle
        .get("m_LoopTime")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let events = tree
        .get("m_Events")
        .and_then(|v| v.as_array())
        .map(|rows| {
            rows.iter()
                .map(|row| Event {
                    time: row.get("time").and_then(|v| v.as_f32()).unwrap_or(0.0),
                    name: row
                        .get("functionName")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                })
                .collect()
        })
        .unwrap_or_default();

    let data = muscle
        .get("m_Clip")
        .and_then(|v| v.get("data"))
        .ok_or_else(|| invalid("missing m_Clip.data"))?;
    let streamed = data.get("m_StreamedClip");
    let streamed_count = streamed
        .and_then(|v| v.get("curveCount"))
        .and_then(|v| v.as_i32())
        .unwrap_or(0)
        .max(0) as usize;
    let frames = streamed
        .and_then(|v| v.get("data"))
        .map(streamed_frames)
        .transpose()?
        .unwrap_or_default();

    let dense = data.get("m_DenseClip");
    let dense_frames = dense
        .and_then(|v| v.get("m_FrameCount"))
        .and_then(|v| v.as_i32())
        .unwrap_or(0)
        .max(0) as usize;
    let dense_count = dense
        .and_then(|v| v.get("m_CurveCount"))
        .and_then(|v| v.as_i32())
        .unwrap_or(0)
        .max(0) as usize;
    let dense_rate = dense
        .and_then(|v| v.get("m_SampleRate"))
        .and_then(|v| v.as_f32())
        .unwrap_or(sample_rate);
    let dense_begin = dense
        .and_then(|v| v.get("m_BeginTime"))
        .and_then(|v| v.as_f32())
        .unwrap_or(0.0);
    let dense_values = dense
        .and_then(|v| v.get("m_SampleArray"))
        .map(float_array)
        .transpose()?
        .unwrap_or_default();

    let constant = data
        .get("m_ConstantClip")
        .and_then(|v| v.get("data"))
        .map(float_array)
        .transpose()?
        .unwrap_or_default();

    let mut bindings = Vec::new();
    let mut base = 0u32;
    if let Some(rows) = tree
        .get("m_ClipBindingConstant")
        .and_then(|v| v.get("genericBindings"))
        .and_then(|v| v.as_array())
    {
        for row in rows {
            let type_id = row.get("typeID").and_then(|v| v.as_i32()).unwrap_or(0);
            let attribute = row.get("attribute").and_then(|v| v.as_i32()).unwrap_or(0);
            let count = if type_id == TRANSFORM {
                match attribute {
                    1 | 3 | 4 => 3,
                    2 => 4,
                    _ => 1,
                }
            } else {
                1
            };
            bindings.push(Binding {
                path: row.get("path").and_then(|v| v.as_u32()).unwrap_or(0),
                attribute,
                type_id,
                base,
                count,
            });
            base += count;
        }
    }

    Ok(Clip {
        name,
        sample_rate,
        start,
        stop,
        looped,
        events,
        streamed_count,
        frames,
        dense_frames,
        dense_count,
        dense_rate,
        dense_begin,
        dense: dense_values,
        constant,
        bindings,
    })
}

fn streamed_frames(value: &Value) -> Result<Vec<(f32, Vec<(i32, [f32; 4])>)>> {
    let words: Vec<u32> = match value {
        Value::Array(items) => items
            .iter()
            .map(|v| v.as_u32().ok_or_else(|| invalid("StreamedClip.data entry")))
            .collect::<Result<Vec<_>>>()?,
        Value::Bytes(bytes) => bytes
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect(),
        _ => return Err(invalid("StreamedClip.data is not a uint32 array")),
    };
    let mut frames = Vec::new();
    let mut pos = 0usize;
    while pos + 2 <= words.len() {
        let time = f32::from_bits(words[pos]);
        pos += 1;
        let count = words[pos] as i32;
        pos += 1;
        if count < 0 || pos + count as usize * 5 > words.len() {
            break;
        }
        let mut keys = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let index = words[pos] as i32;
            let coeff = [
                f32::from_bits(words[pos + 1]),
                f32::from_bits(words[pos + 2]),
                f32::from_bits(words[pos + 3]),
                f32::from_bits(words[pos + 4]),
            ];
            keys.push((index, coeff));
            pos += 5;
        }
        frames.push((time, keys));
    }
    Ok(frames)
}

fn float_array(value: &Value) -> Result<Vec<f32>> {
    match value {
        Value::Array(items) => items
            .iter()
            .map(|v| v.as_f32().ok_or_else(|| invalid("expected float array")))
            .collect(),
        Value::Bytes(bytes) => Ok(bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()),
        _ => Err(invalid("expected float array")),
    }
}

fn invalid(detail: &str) -> Error {
    Error::Invalid {
        what: "animation clip",
        detail: detail.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stream_words(out: &mut Vec<u32>, time: f32, keys: &[(i32, [f32; 4])]) {
        out.push(time.to_bits());
        out.push(keys.len() as u32);
        for (index, coeff) in keys {
            out.push(*index as u32);
            for value in coeff {
                out.push(value.to_bits());
            }
        }
    }

    fn clip_tree() -> Value {
        let mut words = Vec::new();
        stream_words(
            &mut words,
            0.0,
            &[(0, [0.0, 0.0, 2.0, 1.0]), (1, [0.0, 0.0, 0.0, 2.0])],
        );
        stream_words(
            &mut words,
            1.0,
            &[(0, [0.0, 0.0, 0.0, 3.0]), (1, [0.0, 0.0, 0.0, 4.0])],
        );
        let frames = Value::Array(words.into_iter().map(Value::U32).collect());
        Value::Struct(vec![
            ("m_Name".into(), Value::Str("clip".into())),
            ("m_SampleRate".into(), Value::F32(30.0)),
            (
                "m_MuscleClip".into(),
                Value::Struct(vec![
                    ("m_StartTime".into(), Value::F32(0.0)),
                    ("m_StopTime".into(), Value::F32(1.0)),
                    ("m_LoopTime".into(), Value::Bool(true)),
                    (
                        "m_Clip".into(),
                        Value::Struct(vec![(
                            "data".into(),
                            Value::Struct(vec![
                                (
                                    "m_StreamedClip".into(),
                                    Value::Struct(vec![
                                        ("curveCount".into(), Value::I32(2)),
                                        ("data".into(), frames),
                                    ]),
                                ),
                                ("m_DenseClip".into(), Value::Struct(vec![])),
                                ("m_ConstantClip".into(), Value::Struct(vec![])),
                            ]),
                        )]),
                    ),
                ]),
            ),
            (
                "m_ClipBindingConstant".into(),
                Value::Struct(vec![(
                    "genericBindings".into(),
                    Value::Array(vec![Value::Struct(vec![
                        ("path".into(), Value::U32(42)),
                        ("attribute".into(), Value::I32(1)),
                        ("typeID".into(), Value::I32(4)),
                    ])]),
                )]),
            ),
        ])
    }

    #[test]
    fn decodes_streamed_curves_and_samples_them() {
        let clip = from_value(&clip_tree()).unwrap();
        assert_eq!(clip.name, "clip");
        assert_eq!(clip.bindings.len(), 1);
        assert_eq!(clip.bindings[0].count, 3);
        assert_eq!(clip.value(0, 0.0), 1.0);
        assert_eq!(clip.value(0, 1.0), 3.0);
        assert!((clip.value(0, 0.5) - 2.0).abs() < 1e-5);
        let tracks = clip.tracks(Some(2.0), &HashMap::new());
        assert_eq!(tracks.times, vec![0.0, 0.5, 1.0]);
        assert_eq!(tracks.rows.len(), 1);
        assert_eq!(
            tracks.rows[0].position.as_ref().unwrap()[0],
            vec![1.0, 2.0, 3.0]
        );
    }

    #[test]
    fn a_constant_curve_is_used_when_streamed_and_dense_are_empty() {
        let mut tree = clip_tree();
        if let Value::Struct(fields) = &mut tree {
            if let Some(Value::Struct(muscle)) = fields
                .iter_mut()
                .find(|(k, _)| k == "m_MuscleClip")
                .map(|(_, v)| v)
            {
                if let Some(Value::Struct(clip)) = muscle
                    .iter_mut()
                    .find(|(k, _)| k == "m_Clip")
                    .map(|(_, v)| v)
                {
                    if let Some(Value::Struct(data)) =
                        clip.iter_mut().find(|(k, _)| k == "data").map(|(_, v)| v)
                    {
                        for (key, value) in data.iter_mut() {
                            if key == "m_StreamedClip" {
                                *value = Value::Struct(vec![
                                    ("curveCount".into(), Value::I32(0)),
                                    ("data".into(), Value::Array(vec![])),
                                ]);
                            }
                        }
                        if let Some((_, constant)) =
                            data.iter_mut().find(|(k, _)| k == "m_ConstantClip")
                        {
                            *constant = Value::Struct(vec![(
                                "data".into(),
                                Value::Array(vec![Value::F32(7.0)]),
                            )]);
                        }
                    }
                }
            }
        }
        let clip = from_value(&tree).unwrap();
        assert_eq!(clip.value(0, 0.5), 7.0);
    }
}
