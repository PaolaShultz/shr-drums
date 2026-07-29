use crate::package::{db_to_gain, PreparedKit, SampleFrame};
use crate::queue::{DrumEvent, EventReceiver};
use crate::schema::VoiceKind;
use std::f32::consts::TAU;
use std::fmt;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StereoFrame {
    pub left: f32,
    pub right: f32,
}

impl StereoFrame {
    pub const SILENCE: Self = Self {
        left: 0.0,
        right: 0.0,
    };
}

#[derive(Clone, Copy, Debug, Default)]
struct ActiveVoice {
    active: bool,
    definition: usize,
    sample: Option<usize>,
    sample_position: f32,
    sample_increment: f32,
    age: u64,
    released: bool,
    envelope: f32,
    phase: f32,
    noise_state: u32,
    velocity_gain: f32,
}

#[derive(Clone, Copy, Debug, Default)]
struct FilterState {
    low_left: f32,
    low_right: f32,
    high_input_left: f32,
    high_input_right: f32,
    high_left: f32,
    high_right: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EngineError(&'static str);

impl fmt::Display for EngineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for EngineError {}

pub struct DrumEngine {
    sample_rate: u32,
    kit: PreparedKit,
    events: EventReceiver,
    voices: Box<[ActiveVoice]>,
    round_robin: [u32; 128],
    maximum_age: u64,
    filter: FilterState,
    detector: f32,
    slow_detector: f32,
    compressor: f32,
    room_left: Box<[f32]>,
    room_right: Box<[f32]>,
    room_index: usize,
}

impl DrumEngine {
    pub fn new(
        sample_rate: u32,
        kit: PreparedKit,
        events: EventReceiver,
    ) -> Result<Self, EngineError> {
        if !(8_000..=384_000).contains(&sample_rate) {
            return Err(EngineError("sample rate must be 8000..=384000"));
        }
        let room_frames = (sample_rate as usize / 3).max(1);
        let maximum_age = (kit.manifest.max_tail_seconds * sample_rate as f32)
            .round()
            .max(1.0) as u64;
        Ok(Self {
            sample_rate,
            voices: vec![ActiveVoice::default(); kit.manifest.max_polyphony].into_boxed_slice(),
            kit,
            events,
            round_robin: [0; 128],
            maximum_age,
            filter: FilterState::default(),
            detector: 0.0,
            slow_detector: 0.0,
            compressor: 0.0,
            room_left: vec![0.0; room_frames].into_boxed_slice(),
            room_right: vec![0.0; room_frames].into_boxed_slice(),
            room_index: 0,
        })
    }

    pub fn kit_id(&self) -> &str {
        &self.kit.manifest.kit_id
    }

    pub fn active_voice_count(&self) -> usize {
        self.voices.iter().filter(|voice| voice.active).count()
    }

    pub fn all_notes_off(&mut self) {
        for voice in &mut self.voices {
            *voice = ActiveVoice::default();
        }
        self.room_left.fill(0.0);
        self.room_right.fill(0.0);
        self.room_index = 0;
        self.filter = FilterState::default();
        self.detector = 0.0;
        self.slow_detector = 0.0;
        self.compressor = 0.0;
    }

    pub fn drain(&mut self) {
        for voice in &mut self.voices {
            if voice.active {
                voice.released = true;
            }
        }
    }

    /// Render a block without allocation, locking, parsing, file access, or
    /// work beyond fixed event, voice, and frame bounds.
    pub fn process(&mut self, output: &mut [StereoFrame]) {
        self.consume_events();
        for frame in output {
            let mut mixed = StereoFrame::SILENCE;
            for slot in 0..self.voices.len() {
                let contribution = self.render_voice(slot);
                mixed.left += contribution.left;
                mixed.right += contribution.right;
            }
            *frame = self.process_bus(mixed);
        }
    }

    fn consume_events(&mut self) {
        // At most the complete bounded queue can be consumed per callback.
        for _ in 0..512 {
            let Some(event) = self.events.pop() else {
                break;
            };
            match event {
                DrumEvent::NoteOn { note, velocity } if velocity > 0 => {
                    self.trigger(note, velocity)
                }
                DrumEvent::NoteOn { note, .. } | DrumEvent::NoteOff { note } => {
                    self.release_note(note)
                }
                DrumEvent::Choke { group } => self.choke(group),
                DrumEvent::AllNotesOff => self.all_notes_off(),
                DrumEvent::Drain => self.drain(),
            }
        }
    }

    fn trigger(&mut self, note: u8, velocity: u8) {
        let sequence = self.round_robin[usize::from(note)];
        self.round_robin[usize::from(note)] = sequence.wrapping_add(1);
        for definition in 0..self.kit.voices.len() {
            if self.kit.voices[definition].manifest.trigger_note != note {
                continue;
            }
            if let Some(group) = self.kit.voices[definition].manifest.choke_group {
                self.choke(group);
            }
            let prepared = &self.kit.voices[definition];
            let matching_count = prepared
                .assignments
                .iter()
                .filter(|assignment| {
                    (assignment.velocity_min..=assignment.velocity_max).contains(&velocity)
                })
                .count();
            let selection = if matching_count == 0 {
                None
            } else {
                let wanted = (sequence as usize % matching_count + 1) as u8;
                prepared
                    .assignments
                    .iter()
                    .filter(|assignment| {
                        (assignment.velocity_min..=assignment.velocity_max).contains(&velocity)
                    })
                    .find(|assignment| assignment.round_robin == wanted)
            };
            let sample = selection.map(|assignment| assignment.sample_index);
            let sample_gain = selection.map_or(1.0, |assignment| assignment.gain);
            let tuning_ratio = 2.0_f32.powf(prepared.tuning_cents as f32 / 1_200.0);
            let sample_increment = sample.map_or(1.0, |index| {
                self.kit.samples[index].sample_rate as f32 / self.sample_rate as f32 * tuning_ratio
            });
            let slot = self
                .voices
                .iter()
                .position(|voice| !voice.active)
                .unwrap_or_else(|| {
                    self.voices
                        .iter()
                        .enumerate()
                        .max_by_key(|(_, voice)| voice.age)
                        .map_or(0, |(index, _)| index)
                });
            self.voices[slot] = ActiveVoice {
                active: true,
                definition,
                sample,
                sample_position: 0.0,
                sample_increment,
                age: 0,
                released: false,
                envelope: 0.0,
                phase: 0.0,
                noise_state: 0x9e37_79b9 ^ u32::from(note) ^ sequence.rotate_left(13),
                velocity_gain: (f32::from(velocity) / 127.0).powf(1.35)
                    * sample_gain
                    * db_to_gain(prepared.manifest.gain_db),
            };
        }
    }

    fn release_note(&mut self, note: u8) {
        for voice in &mut self.voices {
            if voice.active && self.kit.voices[voice.definition].manifest.trigger_note == note {
                voice.released = true;
            }
        }
    }

    fn choke(&mut self, group: u8) {
        for voice in &mut self.voices {
            if voice.active && self.kit.voices[voice.definition].manifest.choke_group == Some(group)
            {
                *voice = ActiveVoice::default();
            }
        }
    }

    fn render_voice(&mut self, slot: usize) -> StereoFrame {
        let voice = &mut self.voices[slot];
        if !voice.active {
            return StereoFrame::SILENCE;
        }
        let prepared = &self.kit.voices[voice.definition];
        let manifest = &prepared.manifest;
        let envelope = envelope_value(voice, manifest.envelope, self.sample_rate, self.maximum_age);
        if !voice.active {
            return StereoFrame::SILENCE;
        }
        let mut left = 0.0;
        let mut right = 0.0;
        if matches!(manifest.kind, VoiceKind::Sampled | VoiceKind::Hybrid) {
            if let Some(index) = voice.sample {
                let sample = read_sample(&self.kit.samples[index].frames, voice.sample_position);
                left += sample.left;
                right += sample.right;
                voice.sample_position += voice.sample_increment;
            }
        }
        if matches!(manifest.kind, VoiceKind::Modeled | VoiceKind::Hybrid) {
            if let Some(model) = manifest.modeled {
                let tuned = model.body_hz * 2.0_f32.powf(prepared.tuning_cents as f32 / 1_200.0);
                let pitch_drop = model.pitch_drop_cents
                    * (-6.0 * voice.age as f32 / self.sample_rate as f32).exp();
                let frequency = tuned * 2.0_f32.powf(pitch_drop / 1_200.0);
                voice.phase = (voice.phase + TAU * frequency / self.sample_rate as f32) % TAU;
                let body_decay = (-6.907_755 * voice.age as f32
                    / (model.body_decay_ms * self.sample_rate as f32 / 1_000.0).max(1.0))
                .exp();
                let noise_decay = (-6.907_755 * voice.age as f32
                    / (model.noise_decay_ms * self.sample_rate as f32 / 1_000.0).max(1.0))
                .exp();
                voice.noise_state ^= voice.noise_state << 13;
                voice.noise_state ^= voice.noise_state >> 17;
                voice.noise_state ^= voice.noise_state << 5;
                let noise = (voice.noise_state as f32 / u32::MAX as f32) * 2.0 - 1.0;
                let metallic = (voice.phase * 2.73).sin() * (voice.phase * 4.11).sin();
                let modeled = voice.phase.sin() * body_decay
                    + noise * model.noise_amount * noise_decay
                    + metallic * model.metallic_amount * body_decay;
                left += modeled;
                right += modeled;
            }
        }
        voice.age = voice.age.saturating_add(1);
        let pan = manifest.pan;
        left *= envelope * voice.velocity_gain * (1.0 - pan.max(0.0));
        right *= envelope * voice.velocity_gain * (1.0 + pan.min(0.0));
        StereoFrame {
            left: finite(left),
            right: finite(right),
        }
    }

    fn process_bus(&mut self, input: StereoFrame) -> StereoFrame {
        let settings = self.kit.manifest.processing;
        let dt = 1.0 / self.sample_rate as f32;
        let low_alpha = 1.0 - (-TAU * settings.low_pass_hz * dt).exp();
        self.filter.low_left += low_alpha * (input.left - self.filter.low_left);
        self.filter.low_right += low_alpha * (input.right - self.filter.low_right);
        let high_alpha = (-TAU * settings.high_pass_hz * dt).exp();
        self.filter.high_left = high_alpha
            * (self.filter.high_left + self.filter.low_left - self.filter.high_input_left);
        self.filter.high_right = high_alpha
            * (self.filter.high_right + self.filter.low_right - self.filter.high_input_right);
        self.filter.high_input_left = self.filter.low_left;
        self.filter.high_input_right = self.filter.low_right;
        let mut left = self.filter.high_left;
        let mut right = self.filter.high_right;

        let peak = left.abs().max(right.abs());
        self.detector += (peak - self.detector) * 0.15;
        self.slow_detector += (peak - self.slow_detector) * 0.002;
        let transient = (self.detector - self.slow_detector).max(0.0);
        let shape = 1.0
            + settings.transient * transient.min(1.0)
            + settings.body * self.slow_detector.min(1.0) * 0.5;
        left *= shape;
        right *= shape;

        let threshold = 0.28;
        let target_reduction = if peak > threshold {
            (threshold / peak).sqrt()
        } else {
            1.0
        };
        let coefficient = if target_reduction < self.compressor {
            0.08
        } else {
            0.002
        };
        self.compressor += (target_reduction - self.compressor) * coefficient;
        let crushed_left = soft_clip(left * 5.0) * self.compressor;
        let crushed_right = soft_clip(right * 5.0) * self.compressor;
        left += crushed_left * settings.parallel_compression * 0.35;
        right += crushed_right * settings.parallel_compression * 0.35;

        let room_left = self.room_left[self.room_index];
        let room_right = self.room_right[self.room_index];
        let feedback = 0.15 + settings.room_decay * 0.72;
        self.room_left[self.room_index] = finite(right * 0.25 + room_right * feedback);
        self.room_right[self.room_index] = finite(left * 0.25 + room_left * feedback);
        self.room_index += 1;
        if self.room_index == self.room_left.len() {
            self.room_index = 0;
        }
        left += room_left * settings.room_amount;
        right += room_right * settings.room_amount;

        let drive = 1.0 + settings.saturation * 6.0;
        let makeup = db_to_gain(settings.output_gain_db);
        let ceiling = db_to_gain(settings.ceiling_dbfs);
        StereoFrame {
            left: protect(soft_clip(left * drive) / soft_clip(drive) * makeup, ceiling),
            right: protect(
                soft_clip(right * drive) / soft_clip(drive) * makeup,
                ceiling,
            ),
        }
    }
}

fn envelope_value(
    voice: &mut ActiveVoice,
    envelope: crate::schema::Envelope,
    sample_rate: u32,
    maximum_age: u64,
) -> f32 {
    if voice.age >= maximum_age {
        *voice = ActiveVoice::default();
        return 0.0;
    }
    let frames_per_ms = sample_rate as f32 / 1_000.0;
    let attack = (envelope.attack_ms * frames_per_ms).max(1.0);
    let hold_end = attack + envelope.hold_ms * frames_per_ms;
    let decay = (envelope.decay_ms * frames_per_ms).max(1.0);
    let in_attack = (voice.age as f32) < attack;
    if voice.released {
        let release = (envelope.release_ms * frames_per_ms).max(1.0);
        voice.envelope *= (-6.907_755 / release).exp();
    } else if in_attack {
        voice.envelope = (voice.age as f32 + 1.0) / attack;
    } else if (voice.age as f32) < hold_end {
        voice.envelope = 1.0;
    } else {
        voice.envelope *= (-6.907_755 / decay).exp();
    }
    if voice.envelope < 0.000_1 && (voice.released || !in_attack) {
        *voice = ActiveVoice::default();
        0.0
    } else {
        voice.envelope
    }
}

fn read_sample(frames: &[SampleFrame], position: f32) -> SampleFrame {
    let index = position as usize;
    let Some(first) = frames.get(index) else {
        return SampleFrame::default();
    };
    let second = frames.get(index + 1).unwrap_or(first);
    let fraction = position - index as f32;
    SampleFrame {
        left: first.left + (second.left - first.left) * fraction,
        right: first.right + (second.right - first.right) * fraction,
    }
}

fn soft_clip(value: f32) -> f32 {
    value / (1.0 + value.abs())
}

fn protect(value: f32, ceiling: f32) -> f32 {
    finite(value).clamp(-ceiling, ceiling)
}

fn finite(value: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package::{PreparedAssignment, PreparedVoice};
    use crate::queue::event_queue;
    use crate::schema::*;
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;
    use std::collections::BTreeMap;

    struct CountingAllocator;

    thread_local! {
        static COUNT_ALLOCATIONS: Cell<bool> = const { Cell::new(false) };
        static ALLOCATION_COUNT: Cell<usize> = const { Cell::new(0) };
    }

    unsafe impl GlobalAlloc for CountingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            COUNT_ALLOCATIONS.with(|enabled| {
                if enabled.get() {
                    ALLOCATION_COUNT.with(|count| count.set(count.get() + 1));
                }
            });
            // SAFETY: forwarded unchanged to the system allocator.
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
            // SAFETY: the pointer/layout pair came from the system allocator.
            unsafe { System.dealloc(pointer, layout) }
        }
    }

    #[global_allocator]
    static ALLOCATOR: CountingAllocator = CountingAllocator;

    fn assert_no_allocations(action: impl FnOnce()) {
        ALLOCATION_COUNT.with(|count| count.set(0));
        COUNT_ALLOCATIONS.with(|enabled| enabled.set(true));
        action();
        COUNT_ALLOCATIONS.with(|enabled| enabled.set(false));
        assert_eq!(ALLOCATION_COUNT.with(Cell::get), 0);
    }

    fn modeled_kit() -> PreparedKit {
        let voice = VoiceManifest {
            id: "kick".into(),
            display_name: "Kick".into(),
            trigger_note: 36,
            articulation: "hit".into(),
            family: "kick".into(),
            kind: VoiceKind::Modeled,
            choke_group: None,
            gain_db: -8.0,
            pan: 0.0,
            envelope: Envelope {
                attack_ms: 0.1,
                hold_ms: 2.0,
                decay_ms: 400.0,
                release_ms: 20.0,
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
                body_decay_ms: 400.0,
                pitch_drop_cents: 700.0,
                noise_amount: 0.08,
                noise_decay_ms: 15.0,
                metallic_amount: 0.0,
            }),
        };
        PreparedKit {
            manifest: KitManifest {
                format_version: KIT_FORMAT_VERSION,
                kit_id: "test".into(),
                display_name: "Test".into(),
                metadata: KitMetadata {
                    author: "Test".into(),
                    source: "Generated".into(),
                    licence: "CC0-1.0".into(),
                    attribution: "None required".into(),
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
                voices: vec![voice.clone()],
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
            },
            samples: Vec::new().into_boxed_slice(),
            voices: vec![PreparedVoice {
                manifest: voice,
                assignments: Vec::<PreparedAssignment>::new().into_boxed_slice(),
                tuning_cents: 0,
            }]
            .into_boxed_slice(),
        }
    }

    fn render(chunk: usize) -> Vec<StereoFrame> {
        let (sender, receiver) = event_queue();
        let mut engine = DrumEngine::new(48_000, modeled_kit(), receiver).unwrap();
        sender
            .push(DrumEvent::NoteOn {
                note: 36,
                velocity: 100,
            })
            .unwrap();
        let mut output = vec![StereoFrame::SILENCE; 4_096];
        for block in output.chunks_mut(chunk) {
            engine.process(block);
        }
        output
    }

    #[test]
    fn modeled_render_is_deterministic_finite_and_chunk_invariant() {
        let small = render(64);
        let odd = render(127);
        assert_eq!(small, odd);
        assert!(small.iter().any(|frame| frame.left.abs() > 0.0001));
        assert!(small.iter().all(|frame| {
            frame.left.is_finite()
                && frame.right.is_finite()
                && frame.left.abs() <= 10.0_f32.powf(-1.0 / 20.0)
                && frame.right.abs() <= 10.0_f32.powf(-1.0 / 20.0)
        }));
    }

    #[test]
    fn all_notes_off_is_immediate_and_drain_releases() {
        let (sender, receiver) = event_queue();
        let mut engine = DrumEngine::new(48_000, modeled_kit(), receiver).unwrap();
        sender
            .push(DrumEvent::NoteOn {
                note: 36,
                velocity: 127,
            })
            .unwrap();
        engine.process(&mut [StereoFrame::SILENCE; 32]);
        assert_eq!(engine.active_voice_count(), 1);
        sender.all_notes_off().unwrap();
        let mut stopped = [StereoFrame::SILENCE; 32];
        engine.process(&mut stopped);
        assert_eq!(engine.active_voice_count(), 0);
        assert!(stopped.iter().all(|frame| *frame == StereoFrame::SILENCE));
    }

    #[test]
    fn render_callback_path_does_not_allocate() {
        let (sender, receiver) = event_queue();
        let mut engine = DrumEngine::new(48_000, modeled_kit(), receiver).unwrap();
        let mut output = [StereoFrame::SILENCE; 256];
        sender
            .push(DrumEvent::NoteOn {
                note: 36,
                velocity: 96,
            })
            .unwrap();
        assert_no_allocations(|| engine.process(&mut output));
        assert!(output.iter().any(|frame| frame.left != 0.0));
    }
}
