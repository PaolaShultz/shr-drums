use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

pub const KIT_FORMAT_VERSION: u32 = 1;
pub const MAX_VOICES: usize = 128;
pub const MAX_SAMPLES: usize = 512;
pub const MAX_POLYPHONY: usize = 64;
pub const MAX_TAIL_SECONDS: f32 = 16.0;
pub const MAX_PITCH_SHIFT_CENTS: i16 = 2_400;
pub const MAX_DECODED_SAMPLE_BYTES: u64 = 384 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KitManifest {
    pub format_version: u32,
    pub kit_id: String,
    pub display_name: String,
    pub metadata: KitMetadata,
    pub engine: EngineCompatibility,
    pub max_polyphony: usize,
    pub max_tail_seconds: f32,
    #[serde(default)]
    pub articulations: BTreeMap<String, u8>,
    pub voices: Vec<VoiceManifest>,
    pub processing: KitProcessing,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KitMetadata {
    pub author: String,
    pub source: String,
    pub licence: String,
    pub attribution: String,
    pub modification_notes: String,
    #[serde(default)]
    pub source_hashes: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EngineCompatibility {
    pub minimum: String,
    pub maximum_exclusive: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum VoiceKind {
    Sampled,
    Modeled,
    Hybrid,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VoiceManifest {
    pub id: String,
    pub display_name: String,
    pub trigger_note: u8,
    pub articulation: String,
    pub family: String,
    pub kind: VoiceKind,
    pub choke_group: Option<u8>,
    pub gain_db: f32,
    pub pan: f32,
    pub envelope: Envelope,
    #[serde(default)]
    pub samples: Vec<SampleAssignment>,
    pub base_pitch_hz: Option<f32>,
    pub tuning_limits: TuningLimits,
    pub follow_key: FollowKeyRule,
    pub modeled: Option<ModeledParameters>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub attack_ms: f32,
    pub hold_ms: f32,
    pub decay_ms: f32,
    pub release_ms: f32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SampleAssignment {
    pub path: String,
    pub velocity_min: u8,
    pub velocity_max: u8,
    pub round_robin: u8,
    pub articulation: String,
    pub gain_db: f32,
    pub metadata: SampleMetadata,
    pub sha256: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SampleMetadata {
    pub sample_rate: u32,
    pub channels: u16,
    pub frames: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TuningLimits {
    pub down_cents: i16,
    pub up_cents: i16,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum FollowKeyRule {
    Excluded,
    Tonic,
    ScaleDegree { degree: u8, octave: i8 },
    ChordDegree { degree: u8, octave: i8 },
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ModeledParameters {
    pub body_hz: f32,
    pub body_decay_ms: f32,
    pub pitch_drop_cents: f32,
    pub noise_amount: f32,
    pub noise_decay_ms: f32,
    pub metallic_amount: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KitProcessing {
    pub high_pass_hz: f32,
    pub low_pass_hz: f32,
    pub saturation: f32,
    pub transient: f32,
    pub body: f32,
    pub parallel_compression: f32,
    pub room_amount: f32,
    pub room_decay: f32,
    pub output_gain_db: f32,
    pub ceiling_dbfs: f32,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MusicalMode {
    #[default]
    Major,
    NaturalMinor,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct PitchClass(pub u8);

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectKey {
    pub tonic: PitchClass,
    pub mode: MusicalMode,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TuningMode {
    #[default]
    Off,
    FollowKey,
    Manual,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManualTuning {
    pub target_pitch_class: Option<PitchClass>,
    pub cents_adjustment: i16,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct KitTuning {
    pub mode: TuningMode,
    #[serde(default)]
    pub pieces: BTreeMap<String, ManualTuning>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationError(String);

impl ValidationError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ValidationError {}

impl KitManifest {
    pub fn from_json(bytes: &[u8]) -> Result<Self, ValidationError> {
        let manifest: Self = serde_json::from_slice(bytes)
            .map_err(|error| ValidationError::new(format!("invalid kit manifest: {error}")))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.format_version != KIT_FORMAT_VERSION {
            return Err(ValidationError::new(format!(
                "unsupported kit format {}; expected {KIT_FORMAT_VERSION}",
                self.format_version
            )));
        }
        validate_id(&self.kit_id, "kit ID")?;
        validate_text(&self.display_name, "display name", 64)?;
        for (label, value) in [
            ("author", self.metadata.author.as_str()),
            ("source", self.metadata.source.as_str()),
            ("licence", self.metadata.licence.as_str()),
            ("attribution", self.metadata.attribution.as_str()),
            (
                "modification notes",
                self.metadata.modification_notes.as_str(),
            ),
            ("minimum engine", self.engine.minimum.as_str()),
            ("maximum engine", self.engine.maximum_exclusive.as_str()),
        ] {
            validate_text(value, label, 1_024)?;
        }
        if !(1..=MAX_POLYPHONY).contains(&self.max_polyphony) {
            return Err(ValidationError::new("max_polyphony must be 1..=64"));
        }
        if !self.max_tail_seconds.is_finite()
            || !(0.05..=MAX_TAIL_SECONDS).contains(&self.max_tail_seconds)
        {
            return Err(ValidationError::new(
                "max_tail_seconds must be finite and 0.05..=16",
            ));
        }
        if self.voices.is_empty() || self.voices.len() > MAX_VOICES {
            return Err(ValidationError::new("kit must contain 1..=128 voices"));
        }
        validate_processing(self.processing)?;
        let mut ids = BTreeSet::new();
        let mut assignments = 0usize;
        let mut decoded_sample_bytes = 0u64;
        for voice in &self.voices {
            validate_id(&voice.id, "voice ID")?;
            validate_text(&voice.display_name, "voice display name", 64)?;
            validate_id(&voice.articulation, "articulation")?;
            validate_id(&voice.family, "voice family")?;
            if !ids.insert(&voice.id) {
                return Err(ValidationError::new(format!(
                    "duplicate voice ID {}",
                    voice.id
                )));
            }
            if voice.trigger_note > 127 || voice.choke_group == Some(0) {
                return Err(ValidationError::new(format!(
                    "{} has an invalid trigger or choke group",
                    voice.id
                )));
            }
            if !voice.gain_db.is_finite()
                || !(-60.0..=18.0).contains(&voice.gain_db)
                || !voice.pan.is_finite()
                || !(-1.0..=1.0).contains(&voice.pan)
            {
                return Err(ValidationError::new(format!(
                    "{} gain or pan is out of range",
                    voice.id
                )));
            }
            validate_envelope(voice.envelope, &voice.id)?;
            validate_tuning(voice)?;
            match voice.kind {
                VoiceKind::Sampled if voice.samples.is_empty() => {
                    return Err(ValidationError::new(format!(
                        "{} sampled voice has no samples",
                        voice.id
                    )))
                }
                VoiceKind::Modeled if voice.modeled.is_none() => {
                    return Err(ValidationError::new(format!(
                        "{} modeled voice has no model",
                        voice.id
                    )))
                }
                VoiceKind::Hybrid if voice.samples.is_empty() || voice.modeled.is_none() => {
                    return Err(ValidationError::new(format!(
                        "{} hybrid voice needs samples and a model",
                        voice.id
                    )))
                }
                _ => {}
            }
            let mut rr = BTreeSet::new();
            let mut velocity_bands: BTreeMap<(u8, u8), BTreeSet<u8>> = BTreeMap::new();
            for sample in &voice.samples {
                assignments += 1;
                if sample.velocity_min == 0
                    || sample.velocity_min > sample.velocity_max
                    || sample.velocity_max > 127
                    || sample.round_robin == 0
                {
                    return Err(ValidationError::new(format!(
                        "{} has an invalid velocity/round-robin assignment",
                        voice.id
                    )));
                }
                if sample.articulation != voice.articulation {
                    return Err(ValidationError::new(format!(
                        "{} sample articulation does not match its voice",
                        voice.id
                    )));
                }
                if !rr.insert((sample.velocity_min, sample.velocity_max, sample.round_robin)) {
                    return Err(ValidationError::new(format!(
                        "{} has a duplicate velocity/round-robin assignment",
                        voice.id
                    )));
                }
                velocity_bands
                    .entry((sample.velocity_min, sample.velocity_max))
                    .or_default()
                    .insert(sample.round_robin);
                validate_relative_path(&sample.path)?;
                if sample.metadata.sample_rate < 8_000
                    || sample.metadata.sample_rate > 384_000
                    || !(1..=2).contains(&sample.metadata.channels)
                    || sample.metadata.frames == 0
                    || !valid_hash(&sample.sha256)
                {
                    return Err(ValidationError::new(format!(
                        "{} has invalid sample metadata or hash",
                        voice.id
                    )));
                }
                decoded_sample_bytes = decoded_sample_bytes
                    .checked_add(sample.metadata.frames.saturating_mul(8))
                    .ok_or_else(|| ValidationError::new("decoded sample size overflow"))?;
            }
            if !voice.samples.is_empty() {
                let mut expected_velocity = 1u16;
                for (&(minimum, maximum), round_robins) in &velocity_bands {
                    if u16::from(minimum) != expected_velocity
                        || !round_robins
                            .iter()
                            .copied()
                            .eq(1..=round_robins.len() as u8)
                    {
                        return Err(ValidationError::new(format!(
                            "{} velocity layers or round robins are not contiguous",
                            voice.id
                        )));
                    }
                    expected_velocity = u16::from(maximum) + 1;
                }
                if expected_velocity != 128 {
                    return Err(ValidationError::new(format!(
                        "{} velocity layers do not cover 1..=127",
                        voice.id
                    )));
                }
            }
        }
        if assignments > MAX_SAMPLES {
            return Err(ValidationError::new("kit exceeds 512 sample assignments"));
        }
        if decoded_sample_bytes > MAX_DECODED_SAMPLE_BYTES {
            return Err(ValidationError::new(
                "kit exceeds 384 MiB of decoded stereo sample memory",
            ));
        }
        for (articulation, &note) in &self.articulations {
            if note > 127
                || !self
                    .voices
                    .iter()
                    .any(|voice| &voice.articulation == articulation)
            {
                return Err(ValidationError::new(
                    "articulation map refers to an unknown articulation",
                ));
            }
        }
        Ok(())
    }
}

fn validate_id(value: &str, label: &str) -> Result<(), ValidationError> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(ValidationError::new(format!(
            "{label} must use 1..=64 lowercase ASCII letters, digits, or hyphens"
        )));
    }
    Ok(())
}

fn validate_text(value: &str, label: &str, maximum: usize) -> Result<(), ValidationError> {
    if value.trim().is_empty()
        || value.len() > maximum
        || value.chars().any(|character| character.is_control())
    {
        return Err(ValidationError::new(format!(
            "{label} is missing or invalid"
        )));
    }
    Ok(())
}

fn validate_relative_path(value: &str) -> Result<(), ValidationError> {
    let path = Path::new(value);
    if value.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err(ValidationError::new(
            "sample paths must remain inside the kit package",
        ));
    }
    Ok(())
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_envelope(envelope: Envelope, id: &str) -> Result<(), ValidationError> {
    for (label, value, maximum) in [
        ("attack", envelope.attack_ms, 1_000.0),
        ("hold", envelope.hold_ms, 10_000.0),
        ("decay", envelope.decay_ms, 30_000.0),
        ("release", envelope.release_ms, 30_000.0),
    ] {
        if !value.is_finite() || !(0.0..=maximum).contains(&value) {
            return Err(ValidationError::new(format!(
                "{id} {label} envelope is out of range"
            )));
        }
    }
    Ok(())
}

fn validate_tuning(voice: &VoiceManifest) -> Result<(), ValidationError> {
    if voice.tuning_limits.down_cents > 0
        || voice.tuning_limits.up_cents < 0
        || voice.tuning_limits.down_cents < -MAX_PITCH_SHIFT_CENTS
        || voice.tuning_limits.up_cents > MAX_PITCH_SHIFT_CENTS
    {
        return Err(ValidationError::new(format!(
            "{} has invalid tuning limits",
            voice.id
        )));
    }
    if let Some(pitch) = voice.base_pitch_hz {
        if !pitch.is_finite() || !(15.0..=20_000.0).contains(&pitch) {
            return Err(ValidationError::new(format!(
                "{} has invalid reviewed base pitch",
                voice.id
            )));
        }
    }
    if !matches!(voice.follow_key, FollowKeyRule::Excluded) && voice.base_pitch_hz.is_none() {
        return Err(ValidationError::new(format!(
            "{} follows key but has no reviewed base pitch",
            voice.id
        )));
    }
    if let FollowKeyRule::ScaleDegree { degree, octave }
    | FollowKeyRule::ChordDegree { degree, octave } = voice.follow_key
    {
        if !(1..=7).contains(&degree) || !(-2..=2).contains(&octave) {
            return Err(ValidationError::new(format!(
                "{} follow-key degree/octave is out of range",
                voice.id
            )));
        }
    }
    if let Some(model) = voice.modeled {
        if !model.body_hz.is_finite()
            || !(15.0..=8_000.0).contains(&model.body_hz)
            || !model.body_decay_ms.is_finite()
            || !(5.0..=30_000.0).contains(&model.body_decay_ms)
            || !model.pitch_drop_cents.is_finite()
            || !(-2_400.0..=2_400.0).contains(&model.pitch_drop_cents)
            || !model.noise_amount.is_finite()
            || !(0.0..=1.0).contains(&model.noise_amount)
            || !model.noise_decay_ms.is_finite()
            || !(1.0..=30_000.0).contains(&model.noise_decay_ms)
            || !model.metallic_amount.is_finite()
            || !(0.0..=1.0).contains(&model.metallic_amount)
        {
            return Err(ValidationError::new(format!(
                "{} modeled parameters are out of range",
                voice.id
            )));
        }
    }
    Ok(())
}

fn validate_processing(processing: KitProcessing) -> Result<(), ValidationError> {
    let valid = processing.high_pass_hz.is_finite()
        && (5.0..=500.0).contains(&processing.high_pass_hz)
        && processing.low_pass_hz.is_finite()
        && (1_000.0..=24_000.0).contains(&processing.low_pass_hz)
        && processing.low_pass_hz > processing.high_pass_hz
        && [
            processing.saturation,
            processing.parallel_compression,
            processing.room_amount,
            processing.room_decay,
        ]
        .iter()
        .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
        && [processing.transient, processing.body]
            .iter()
            .all(|value| value.is_finite() && (-1.0..=1.0).contains(value))
        && processing.output_gain_db.is_finite()
        && (-24.0..=12.0).contains(&processing.output_gain_db)
        && processing.ceiling_dbfs.is_finite()
        && (-12.0..=-0.5).contains(&processing.ceiling_dbfs);
    if valid {
        Ok(())
    } else {
        Err(ValidationError::new("kit processing defaults are invalid"))
    }
}

impl ProjectKey {
    pub fn validate(self) -> Result<(), ValidationError> {
        if self.tonic.0 < 12 {
            Ok(())
        } else {
            Err(ValidationError::new("Project tonic must be 0..=11"))
        }
    }

    pub fn scale_pitch_class(self, degree: u8) -> u8 {
        let intervals = match self.mode {
            MusicalMode::Major => [0, 2, 4, 5, 7, 9, 11],
            MusicalMode::NaturalMinor => [0, 2, 3, 5, 7, 8, 10],
        };
        (self.tonic.0 + intervals[usize::from(degree.saturating_sub(1).min(6))]) % 12
    }

    pub fn chord_pitch_class(self, degree: u8) -> u8 {
        let chord_index = usize::from(degree.saturating_sub(1).min(6));
        self.scale_pitch_class([1, 3, 5, 1, 3, 5, 1][chord_index])
    }
}

impl KitTuning {
    pub fn validate_for(&self, manifest: &KitManifest) -> Result<(), ValidationError> {
        if self.pieces.len() > MAX_VOICES {
            return Err(ValidationError::new("drum tuning exceeds 128 pieces"));
        }
        for (piece, tuning) in &self.pieces {
            validate_id(piece, "tuning piece ID")?;
            if !manifest.voices.iter().any(|voice| &voice.id == piece) {
                return Err(ValidationError::new(format!(
                    "tuning refers to unknown piece {piece}"
                )));
            }
            if tuning.target_pitch_class.is_some_and(|pitch| pitch.0 > 11)
                || !(-MAX_PITCH_SHIFT_CENTS..=MAX_PITCH_SHIFT_CENTS)
                    .contains(&tuning.cents_adjustment)
            {
                return Err(ValidationError::new(format!(
                    "{piece} manual tuning is out of range"
                )));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_manifest() -> KitManifest {
        KitManifest {
            format_version: KIT_FORMAT_VERSION,
            kit_id: "test-kit".into(),
            display_name: "Test Kit".into(),
            metadata: KitMetadata {
                author: "Test".into(),
                source: "Generated fixture".into(),
                licence: "CC0-1.0".into(),
                attribution: "None".into(),
                modification_notes: "Generated".into(),
                source_hashes: BTreeMap::new(),
            },
            engine: EngineCompatibility {
                minimum: "0.1.0".into(),
                maximum_exclusive: "1.0.0".into(),
            },
            max_polyphony: 8,
            max_tail_seconds: 2.0,
            articulations: BTreeMap::new(),
            voices: vec![VoiceManifest {
                id: "kick".into(),
                display_name: "Kick".into(),
                trigger_note: 36,
                articulation: "hit".into(),
                family: "kick".into(),
                kind: VoiceKind::Modeled,
                choke_group: None,
                gain_db: -6.0,
                pan: 0.0,
                envelope: Envelope {
                    attack_ms: 0.1,
                    hold_ms: 2.0,
                    decay_ms: 500.0,
                    release_ms: 30.0,
                },
                samples: Vec::new(),
                base_pitch_hz: Some(32.703),
                tuning_limits: TuningLimits {
                    down_cents: -1_200,
                    up_cents: 1_200,
                },
                follow_key: FollowKeyRule::Tonic,
                modeled: Some(ModeledParameters {
                    body_hz: 32.703,
                    body_decay_ms: 500.0,
                    pitch_drop_cents: 600.0,
                    noise_amount: 0.05,
                    noise_decay_ms: 15.0,
                    metallic_amount: 0.0,
                }),
            }],
            processing: KitProcessing {
                high_pass_hz: 10.0,
                low_pass_hz: 20_000.0,
                saturation: 0.1,
                transient: 0.0,
                body: 0.0,
                parallel_compression: 0.0,
                room_amount: 0.0,
                room_decay: 0.2,
                output_gain_db: -6.0,
                ceiling_dbfs: -1.0,
            },
        }
    }

    #[test]
    fn strict_manifest_rejects_missing_and_unknown_behavior() {
        let mut value = serde_json::to_value(valid_manifest()).unwrap();
        value.as_object_mut().unwrap().remove("processing");
        assert!(KitManifest::from_json(&serde_json::to_vec(&value).unwrap()).is_err());

        let mut value = serde_json::to_value(valid_manifest()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("future_behavior".into(), serde_json::Value::Bool(true));
        assert!(KitManifest::from_json(&serde_json::to_vec(&value).unwrap()).is_err());
    }

    #[test]
    fn tuning_rejects_unknown_pieces_before_preparation() {
        let tuning = KitTuning {
            mode: TuningMode::Manual,
            pieces: BTreeMap::from([(
                "not-in-kit".into(),
                ManualTuning {
                    target_pitch_class: Some(PitchClass(1)),
                    cents_adjustment: 0,
                },
            )]),
        };
        assert!(tuning.validate_for(&valid_manifest()).is_err());
    }
}
