use crate::schema::{
    AdvancedModel, DriveCurve, DriveStage, FilterMode, ModelAlgorithm, OscillatorShape,
    MAX_MODEL_MODES,
};
use std::f32::consts::TAU;

const MAX_MICRO_DELAY_FRAMES: usize = 768;

#[derive(Clone, Debug)]
pub(crate) struct ModelState {
    body_phase: f32,
    overtone_phase: f32,
    click_phase: f32,
    mod_phase: f32,
    mode_phases: [f32; MAX_MODEL_MODES],
    noise_state: u32,
    noise_low: f32,
    noise_band: f32,
    noise_colour: f32,
    click_low: f32,
    feedback: f32,
    delay: [f32; MAX_MICRO_DELAY_FRAMES],
    delay_position: usize,
    pitch_variation_cents: f32,
    timing_variation_ms: f32,
    level_variation: f32,
    stereo_variation: f32,
}

impl Default for ModelState {
    fn default() -> Self {
        Self {
            body_phase: 0.0,
            overtone_phase: 0.0,
            click_phase: 0.0,
            mod_phase: 0.0,
            mode_phases: [0.0; MAX_MODEL_MODES],
            noise_state: 1,
            noise_low: 0.0,
            noise_band: 0.0,
            noise_colour: 0.0,
            click_low: 0.0,
            feedback: 0.0,
            delay: [0.0; MAX_MICRO_DELAY_FRAMES],
            delay_position: 0,
            pitch_variation_cents: 0.0,
            timing_variation_ms: 0.0,
            level_variation: 1.0,
            stereo_variation: 0.0,
        }
    }
}

impl ModelState {
    pub(crate) fn reset(&mut self, seed: u32, model: &AdvancedModel) {
        *self = Self::default();
        self.noise_state = seed | 1;
        self.pitch_variation_cents = bipolar(&mut self.noise_state) * model.variation.pitch_cents;
        self.timing_variation_ms = bipolar(&mut self.noise_state) * model.variation.timing_ms;
        self.level_variation = 1.0 + bipolar(&mut self.noise_state) * model.variation.level;
        self.stereo_variation = bipolar(&mut self.noise_state) * model.variation.stereo;
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ModelRender {
    pub left: f32,
    pub right: f32,
    pub internal_peak: f32,
    pub intentional_clip_events: u32,
}

pub(crate) fn render(
    model: &AdvancedModel,
    state: &mut ModelState,
    age: u64,
    sample_rate: u32,
    velocity: f32,
    tuning_cents: i16,
) -> ModelRender {
    let rate = sample_rate as f32;
    let time = age as f32 / rate;
    let time_ms = time * 1_000.0;
    let character = velocity.clamp(0.0, 1.0);
    let decay_scale = (1.0 + model.velocity.decay * (character - 0.5) * 0.8).clamp(0.35, 2.0);
    let pitch_velocity = model.velocity.pitch * (character - 0.5) * 240.0;
    let pitch_cents = pitch_value(model, time_ms)
        + f32::from(tuning_cents)
        + state.pitch_variation_cents
        + pitch_velocity;
    let frequency = (model.base_hz * 2.0_f32.powf(pitch_cents / 1_200.0)).clamp(5.0, rate * 0.45);

    state.mod_phase =
        wrap_phase(state.mod_phase + TAU * frequency * model.modulation.fm_ratio / rate);
    let modulator = state.mod_phase.sin();
    let instantaneous =
        (frequency * (1.0 + modulator * model.modulation.fm_index * 0.1)).clamp(5.0, rate * 0.45);
    state.body_phase = wrap_phase(state.body_phase + TAU * instantaneous / rate);
    state.overtone_phase =
        wrap_phase(state.overtone_phase + TAU * instantaneous * model.body.overtone_ratio / rate);
    let phase = state.body_phase
        + modulator * model.modulation.phase_amount
        + state.feedback * model.modulation.feedback;
    let body_decay = exp_decay(time_ms, model.body.decay_ms * decay_scale);
    let mut body = oscillator(
        model.oscillator,
        phase,
        model.body.pulse_width,
        model.body.shape,
    );
    body += oscillator(
        OscillatorShape::Sine,
        state.overtone_phase,
        0.5,
        model.body.shape,
    ) * model.body.overtone_level;
    if model.modulation.ring_amount > 0.0 {
        let ring = (state.body_phase * model.modulation.ring_ratio).sin();
        body *= 1.0 + (ring - 1.0) * model.modulation.ring_amount;
    }
    body *= model.body.level * body_decay;

    let click_decay = exp_decay(time_ms, model.click.decay_ms);
    state.click_phase =
        wrap_phase(state.click_phase + TAU * model.click.tone_hz.min(rate * 0.45) / rate);
    let raw_noise = bipolar(&mut state.noise_state);
    let click_source =
        state.click_phase.sin() * (1.0 - model.click.noise_mix) + raw_noise * model.click.noise_mix;
    let click_alpha = one_pole_alpha(model.click.high_pass_hz, rate);
    state.click_low += click_alpha * (click_source - state.click_low);
    let click_character = (1.0 + model.velocity.click * (character - 0.5)).clamp(0.15, 2.5);
    let mut click =
        (click_source - state.click_low) * model.click.level * click_decay * click_character;

    let brightness = (1.0 + model.velocity.brightness * (character - 0.5)).clamp(0.35, 2.0);
    let cutoff = (model.noise.cutoff_hz * brightness).clamp(20.0, rate * 0.45);
    let noise = coloured_noise(state, raw_noise, model.noise.colour);
    let filtered_noise = filter_noise(
        state,
        noise,
        model.noise.filter,
        cutoff,
        model.noise.resonance,
        rate,
    );
    let noise_attack = if model.noise.attack_ms <= 0.0 {
        1.0
    } else {
        (time_ms / model.noise.attack_ms).clamp(0.0, 1.0)
    };
    let noise_character = (1.0 + model.velocity.noise * (character - 0.5)).clamp(0.1, 2.5);
    let mut noise_envelope =
        model.noise.level * noise_attack * exp_decay(time_ms, model.noise.decay_ms * decay_scale);
    noise_envelope +=
        model.noise.tail_level * exp_decay(time_ms, model.noise.tail_decay_ms * decay_scale);
    if !model.bursts.is_empty() {
        noise_envelope += clap_bursts(model, state, time_ms);
    }
    let mut noise_layer = filtered_noise * noise_envelope * noise_character;

    let drive_velocity = (1.0 + model.velocity.drive * character * 0.75).clamp(0.5, 2.5);
    let mut rendered = ModelRender::default();
    body = driven(body, model.body.drive, drive_velocity, &mut rendered);
    click = driven(click, model.click.drive, drive_velocity, &mut rendered);
    noise_layer = driven(
        noise_layer,
        model.noise.drive,
        drive_velocity,
        &mut rendered,
    );

    let width = (model.stereo.width + state.stereo_variation * 0.25).clamp(0.0, 1.0);
    let burst_pan = burst_pan(model, state, time_ms) * width;
    let mut left = body + click + noise_layer * (1.0 - burst_pan.max(0.0));
    let mut right = body + click + noise_layer * (1.0 + burst_pan.min(0.0));
    for (index, mode) in model.modes.iter().enumerate() {
        let mode_frequency = (frequency * mode.ratio).clamp(5.0, rate * 0.45);
        state.mode_phases[index] =
            wrap_phase(state.mode_phases[index] + TAU * mode_frequency / rate);
        let mode_time = (time_ms - state.timing_variation_ms * index as f32 * 0.15).max(0.0);
        let value = oscillator(
            mode.shape,
            state.mode_phases[index] + modulator * model.modulation.phase_amount * 0.35,
            model.body.pulse_width,
            model.body.shape,
        ) * mode.level
            * exp_decay(mode_time, mode.decay_ms * decay_scale);
        let value = driven(value, mode.drive, drive_velocity, &mut rendered);
        let pan = (mode.pan * width).clamp(-1.0, 1.0);
        left += value * (1.0 - pan.max(0.0));
        right += value * (1.0 + pan.min(0.0));
    }

    let algorithm_gain = match model.algorithm {
        ModelAlgorithm::Kick => 1.0,
        ModelAlgorithm::Snare => 0.9,
        ModelAlgorithm::Clap => 0.82,
        ModelAlgorithm::Hat => 0.72,
        ModelAlgorithm::Tom => 0.95,
        ModelAlgorithm::Cymbal => 0.68,
        ModelAlgorithm::Percussion => 0.85,
    };
    left *= algorithm_gain * state.level_variation;
    right *= algorithm_gain * state.level_variation;
    left = driven(left, model.master_drive, drive_velocity, &mut rendered);
    right = driven(right, model.master_drive, drive_velocity, &mut rendered);
    state.feedback = finite((left + right) * 0.5).clamp(-4.0, 4.0);
    right = micro_delay(state, right, model.stereo.micro_delay_ms, sample_rate);
    rendered.left = finite(left).clamp(-8.0, 8.0);
    rendered.right = finite(right).clamp(-8.0, 8.0);
    rendered
}

fn pitch_value(model: &AdvancedModel, time_ms: f32) -> f32 {
    if model.pitch.attack_ms > 0.0 && time_ms < model.pitch.attack_ms {
        let position = time_ms / model.pitch.attack_ms;
        model.pitch.start_cents + (model.pitch.mid_cents - model.pitch.start_cents) * position
    } else {
        let elapsed = (time_ms - model.pitch.attack_ms).max(0.0);
        model.pitch.mid_cents * exp_decay(elapsed, model.pitch.decay_ms)
    }
}

fn clap_bursts(model: &AdvancedModel, state: &ModelState, time_ms: f32) -> f32 {
    model
        .bursts
        .iter()
        .enumerate()
        .map(|(index, burst)| {
            let shifted = burst.time_ms + state.timing_variation_ms * (index as f32 * 0.4 - 0.6);
            if time_ms < shifted {
                0.0
            } else {
                burst.level * exp_decay(time_ms - shifted, burst.decay_ms)
            }
        })
        .sum()
}

fn burst_pan(model: &AdvancedModel, state: &ModelState, time_ms: f32) -> f32 {
    let mut weighted_pan = 0.0;
    let mut total = 0.0;
    for (index, burst) in model.bursts.iter().enumerate() {
        let shifted = burst.time_ms + state.timing_variation_ms * (index as f32 * 0.4 - 0.6);
        if time_ms >= shifted {
            let weight = burst.level * exp_decay(time_ms - shifted, burst.decay_ms);
            weighted_pan += burst.pan * weight;
            total += weight;
        }
    }
    if total > 0.000_001 {
        (weighted_pan / total).clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

fn oscillator(shape: OscillatorShape, phase: f32, pulse_width: f32, amount: f32) -> f32 {
    let wrapped = phase.rem_euclid(TAU) / TAU;
    match shape {
        OscillatorShape::Sine => phase.sin(),
        OscillatorShape::Triangle => 1.0 - 4.0 * (wrapped - 0.5).abs(),
        OscillatorShape::Pulse => {
            if wrapped < pulse_width {
                1.0
            } else {
                -1.0
            }
        }
        OscillatorShape::Shaped => {
            let sine = phase.sin();
            let cubic = sine - sine * sine * sine * 0.28;
            sine + (cubic - sine) * amount
        }
    }
}

fn coloured_noise(state: &mut ModelState, white: f32, colour: f32) -> f32 {
    state.noise_colour += 0.08 * (white - state.noise_colour);
    if colour >= 0.0 {
        white + (state.noise_colour - white) * colour
    } else {
        let high = white - state.noise_colour;
        white + (high - white) * -colour
    }
}

fn filter_noise(
    state: &mut ModelState,
    input: f32,
    mode: FilterMode,
    cutoff: f32,
    resonance: f32,
    sample_rate: f32,
) -> f32 {
    let coefficient = (2.0 * (std::f32::consts::PI * cutoff / sample_rate).sin()).min(0.99);
    let damping = (1.0 - resonance).clamp(0.05, 1.0);
    let high = input - state.noise_low - damping * state.noise_band;
    state.noise_band = finite(state.noise_band + coefficient * high).clamp(-8.0, 8.0);
    state.noise_low = finite(state.noise_low + coefficient * state.noise_band).clamp(-8.0, 8.0);
    finite(match mode {
        FilterMode::LowPass => state.noise_low,
        FilterMode::HighPass => high,
        FilterMode::BandPass => state.noise_band,
    })
    .clamp(-8.0, 8.0)
}

fn driven(
    input: f32,
    stage: DriveStage,
    velocity_drive: f32,
    diagnostics: &mut ModelRender,
) -> f32 {
    if stage.amount <= 0.0 {
        return finite(input);
    }
    let pre = finite(input) * db_to_gain(stage.pre_gain_db) * velocity_drive;
    diagnostics.internal_peak = diagnostics.internal_peak.max(pre.abs());
    if pre.abs() > 1.0 {
        diagnostics.intentional_clip_events = diagnostics.intentional_clip_events.saturating_add(1);
    }
    let shaped = match stage.curve {
        DriveCurve::SoftClip => pre.tanh(),
        DriveCurve::HardClip => pre.clamp(-1.0, 1.0),
        DriveCurve::Cubic => {
            let bounded = pre.clamp(-1.5, 1.5);
            (bounded - bounded * bounded * bounded / 3.0).clamp(-1.0, 1.0)
        }
        DriveCurve::Fold => {
            let folded = (pre + 1.0).rem_euclid(4.0);
            if folded <= 2.0 {
                folded - 1.0
            } else {
                3.0 - folded
            }
        }
    } * db_to_gain(stage.post_gain_db);
    finite(input + (shaped - input) * stage.amount)
}

fn micro_delay(state: &mut ModelState, input: f32, delay_ms: f32, sample_rate: u32) -> f32 {
    let frames = ((delay_ms * sample_rate as f32 / 1_000.0).round() as usize)
        .min(MAX_MICRO_DELAY_FRAMES - 1);
    if frames == 0 {
        return input;
    }
    state.delay[state.delay_position] = input;
    let read = (state.delay_position + MAX_MICRO_DELAY_FRAMES - frames) % MAX_MICRO_DELAY_FRAMES;
    let output = state.delay[read];
    state.delay_position = (state.delay_position + 1) % MAX_MICRO_DELAY_FRAMES;
    output
}

fn exp_decay(time_ms: f32, decay_ms: f32) -> f32 {
    (-6.907_755 * time_ms / decay_ms.max(0.1)).exp()
}

fn one_pole_alpha(cutoff: f32, sample_rate: f32) -> f32 {
    1.0 - (-TAU * cutoff.clamp(5.0, sample_rate * 0.45) / sample_rate).exp()
}

fn bipolar(state: &mut u32) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    *state as f32 / u32::MAX as f32 * 2.0 - 1.0
}

fn db_to_gain(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

fn wrap_phase(value: f32) -> f32 {
    value.rem_euclid(TAU)
}

fn finite(value: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        0.0
    }
}
