use crate::schema::{
    KitManifest, KitTuning, ManualTuning, ProjectKey, TuningMode, VoiceManifest,
    MAX_DECODED_SAMPLE_BYTES,
};
use hound::SampleFormat;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SampleFrame {
    pub left: f32,
    pub right: f32,
}

#[derive(Clone, Debug)]
pub struct PreparedSample {
    pub frames: Box<[SampleFrame]>,
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedAssignment {
    pub sample_index: usize,
    pub velocity_min: u8,
    pub velocity_max: u8,
    pub round_robin: u8,
    pub gain: f32,
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedVoice {
    pub manifest: VoiceManifest,
    pub assignments: Box<[PreparedAssignment]>,
    pub tuning_cents: i16,
}

#[derive(Clone, Debug)]
pub struct PreparedKit {
    pub manifest: KitManifest,
    pub samples: Box<[PreparedSample]>,
    pub(crate) voices: Box<[PreparedVoice]>,
}

impl PreparedKit {
    pub fn decoded_sample_bytes(&self) -> usize {
        self.samples
            .iter()
            .map(|sample| sample.frames.len() * std::mem::size_of::<SampleFrame>())
            .sum()
    }
}

pub fn load_package(
    directory: &Path,
    project_key: ProjectKey,
    tuning: &KitTuning,
) -> Result<PreparedKit, String> {
    project_key.validate().map_err(|error| error.to_string())?;
    let package_root = fs::canonicalize(directory)
        .map_err(|error| format!("open package {}: {error}", directory.display()))?;
    let manifest_path = package_root.join("manifest.json");
    let manifest_size = fs::metadata(&manifest_path)
        .map_err(|error| format!("inspect {}: {error}", manifest_path.display()))?
        .len();
    if manifest_size > MAX_MANIFEST_BYTES {
        return Err("kit manifest exceeds 1 MiB".into());
    }
    let manifest_bytes = fs::read(&manifest_path)
        .map_err(|error| format!("read {}: {error}", manifest_path.display()))?;
    let manifest = KitManifest::from_json(&manifest_bytes).map_err(|error| error.to_string())?;
    tuning
        .validate_for(&manifest)
        .map_err(|error| error.to_string())?;
    let mut samples = Vec::new();
    let mut voices = Vec::with_capacity(manifest.voices.len());
    for voice in &manifest.voices {
        let mut assignments = Vec::with_capacity(voice.samples.len());
        for assignment in &voice.samples {
            let path = package_root.join(&assignment.path);
            let canonical_path = fs::canonicalize(&path)
                .map_err(|error| format!("open {}: {error}", path.display()))?;
            if !canonical_path.starts_with(&package_root) {
                return Err(format!(
                    "{} resolves outside the kit package",
                    assignment.path
                ));
            }
            if fs::metadata(&path)
                .map_err(|error| format!("inspect {}: {error}", path.display()))?
                .len()
                > MAX_DECODED_SAMPLE_BYTES
            {
                return Err(format!("{} exceeds the sample file bound", path.display()));
            }
            let bytes = fs::read(&canonical_path)
                .map_err(|error| format!("read {}: {error}", canonical_path.display()))?;
            let actual_hash = sha256_hex(&bytes);
            if !actual_hash.eq_ignore_ascii_case(&assignment.sha256) {
                return Err(format!(
                    "{} hash mismatch: expected {}, found {}",
                    assignment.path, assignment.sha256, actual_hash
                ));
            }
            let sample = decode_wav(&canonical_path, &bytes)?;
            if sample.sample_rate != assignment.metadata.sample_rate
                || sample.channels != assignment.metadata.channels
                || sample.frames.len() as u64 != assignment.metadata.frames
            {
                return Err(format!(
                    "{} decoded metadata does not match the manifest",
                    assignment.path
                ));
            }
            let sample_index = samples.len();
            samples.push(sample);
            assignments.push(PreparedAssignment {
                sample_index,
                velocity_min: assignment.velocity_min,
                velocity_max: assignment.velocity_max,
                round_robin: assignment.round_robin,
                gain: db_to_gain(assignment.gain_db),
            });
        }
        voices.push(PreparedVoice {
            manifest: voice.clone(),
            assignments: assignments.into_boxed_slice(),
            tuning_cents: tuning_for_voice(voice, project_key, tuning),
        });
    }
    Ok(PreparedKit {
        manifest,
        samples: samples.into_boxed_slice(),
        voices: voices.into_boxed_slice(),
    })
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for byte in digest {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn decode_wav(path: &Path, bytes: &[u8]) -> Result<PreparedSample, String> {
    let cursor = std::io::Cursor::new(bytes);
    let mut reader = hound::WavReader::new(cursor)
        .map_err(|error| format!("decode {}: {error}", path.display()))?;
    let specification = reader.spec();
    if !(1..=2).contains(&specification.channels) {
        return Err(format!("{} must be mono or stereo", path.display()));
    }
    let values = match (specification.sample_format, specification.bits_per_sample) {
        (SampleFormat::Float, 32) => reader
            .samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("decode {}: {error}", path.display()))?,
        (SampleFormat::Int, bits @ 8..=32) => {
            let scale = ((1_i64 << (bits - 1)) - 1) as f32;
            reader
                .samples::<i32>()
                .map(|sample| sample.map(|value| value as f32 / scale))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| format!("decode {}: {error}", path.display()))?
        }
        _ => {
            return Err(format!(
                "{} has an unsupported WAV encoding",
                path.display()
            ))
        }
    };
    let channels = usize::from(specification.channels);
    if values.len() % channels != 0 {
        return Err(format!("{} has an incomplete WAV frame", path.display()));
    }
    let frames = values
        .chunks_exact(channels)
        .map(|frame| {
            let left = frame[0];
            let right = if channels == 2 { frame[1] } else { left };
            SampleFrame {
                left: finite(left),
                right: finite(right),
            }
        })
        .collect::<Vec<_>>()
        .into_boxed_slice();
    Ok(PreparedSample {
        frames,
        sample_rate: specification.sample_rate,
        channels: specification.channels,
    })
}

fn finite(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

pub(crate) fn db_to_gain(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

fn tuning_for_voice(voice: &VoiceManifest, key: ProjectKey, tuning: &KitTuning) -> i16 {
    let requested = match tuning.mode {
        TuningMode::Off => 0,
        TuningMode::FollowKey => follow_key_cents(voice, key),
        TuningMode::Manual => tuning
            .pieces
            .get(&voice.id)
            .copied()
            .map(|manual| manual_cents(voice, manual))
            .unwrap_or(0),
    };
    requested.clamp(voice.tuning_limits.down_cents, voice.tuning_limits.up_cents)
}

fn follow_key_cents(voice: &VoiceManifest, key: ProjectKey) -> i16 {
    use crate::schema::FollowKeyRule;
    let (target, octave) = match voice.follow_key {
        FollowKeyRule::Excluded => return 0,
        FollowKeyRule::Tonic => (key.tonic.0, 0),
        FollowKeyRule::ScaleDegree { degree, octave } => (key.scale_pitch_class(degree), octave),
        FollowKeyRule::ChordDegree { degree, octave } => (key.chord_pitch_class(degree), octave),
    };
    cents_to_pitch_class(voice.base_pitch_hz, target)
        .saturating_add(i16::from(octave).saturating_mul(1_200))
}

fn manual_cents(voice: &VoiceManifest, manual: ManualTuning) -> i16 {
    let pitch = manual
        .target_pitch_class
        .map(|target| cents_to_pitch_class(voice.base_pitch_hz, target.0))
        .unwrap_or(0);
    pitch.saturating_add(manual.cents_adjustment)
}

fn cents_to_pitch_class(base_hz: Option<f32>, target: u8) -> i16 {
    let Some(base_hz) = base_hz else {
        return 0;
    };
    let midi = 69.0 + 12.0 * (base_hz / 440.0).log2();
    let current = midi.round() as i16;
    let current_pc = current.rem_euclid(12);
    let mut semitones = i16::from(target % 12) - current_pc;
    if semitones > 6 {
        semitones -= 12;
    } else if semitones < -6 {
        semitones += 12;
    }
    let detune = ((current as f32 - midi) * 100.0).round() as i16;
    semitones.saturating_mul(100).saturating_add(detune)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{
        Envelope, FollowKeyRule, ModeledParameters, MusicalMode, PitchClass, TuningLimits,
        VoiceKind,
    };
    use std::collections::BTreeMap;

    fn voice(id: &str, base: f32, follow_key: FollowKeyRule) -> VoiceManifest {
        VoiceManifest {
            id: id.into(),
            display_name: id.into(),
            trigger_note: 36,
            articulation: "hit".into(),
            family: "kick".into(),
            kind: VoiceKind::Modeled,
            choke_group: None,
            gain_db: 0.0,
            pan: 0.0,
            envelope: Envelope {
                attack_ms: 0.0,
                hold_ms: 1.0,
                decay_ms: 500.0,
                release_ms: 20.0,
            },
            samples: Vec::new(),
            base_pitch_hz: Some(base),
            tuning_limits: TuningLimits {
                down_cents: -1_200,
                up_cents: 1_200,
            },
            follow_key,
            modeled: Some(ModeledParameters {
                body_hz: base,
                body_decay_ms: 500.0,
                pitch_drop_cents: 0.0,
                noise_amount: 0.0,
                noise_decay_ms: 1.0,
                metallic_amount: 0.0,
            }),
        }
    }

    #[test]
    fn c_sharp_minor_manual_kick_and_snare_and_follow_key_are_exact() {
        let key = ProjectKey {
            tonic: PitchClass(1),
            mode: MusicalMode::NaturalMinor,
        };
        let kick = voice("kick", 32.703, FollowKeyRule::Tonic);
        let snare = voice("snare", 65.406, FollowKeyRule::Tonic);
        let follow = KitTuning {
            mode: TuningMode::FollowKey,
            pieces: BTreeMap::new(),
        };
        assert!((tuning_for_voice(&kick, key, &follow) - 100).abs() <= 1);
        assert!((tuning_for_voice(&snare, key, &follow) - 100).abs() <= 1);

        let manual = KitTuning {
            mode: TuningMode::Manual,
            pieces: BTreeMap::from([
                (
                    "kick".into(),
                    ManualTuning {
                        target_pitch_class: Some(PitchClass(1)),
                        cents_adjustment: 0,
                    },
                ),
                (
                    "snare".into(),
                    ManualTuning {
                        target_pitch_class: Some(PitchClass(1)),
                        cents_adjustment: 0,
                    },
                ),
            ]),
        };
        assert!((tuning_for_voice(&kick, key, &manual) - 100).abs() <= 1);
        assert!((tuning_for_voice(&snare, key, &manual) - 100).abs() <= 1);
    }

    #[test]
    fn excluded_broadband_voice_does_not_follow_project_key() {
        let hat = voice("hat", 9_000.0, FollowKeyRule::Excluded);
        let tuning = KitTuning {
            mode: TuningMode::FollowKey,
            pieces: BTreeMap::new(),
        };
        assert_eq!(
            tuning_for_voice(
                &hat,
                ProjectKey {
                    tonic: PitchClass(1),
                    mode: MusicalMode::NaturalMinor,
                },
                &tuning
            ),
            0
        );
    }
}
