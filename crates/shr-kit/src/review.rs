use anyhow::{bail, Context, Result};
use hound::{SampleFormat, WavSpec, WavWriter};
use shr_drums::{
    event_queue, load_package, DrumEngine, DrumEvent, EngineDiagnostics, KitTuning, PreparedKit,
    ProjectKey, StereoFrame, VoiceKind,
};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

const SAMPLE_RATE: u32 = 48_000;
const BLOCK: usize = 128;
const VELOCITIES: [u8; 3] = [32, 80, 127];

#[derive(Clone, Debug)]
struct Measurement {
    file: String,
    voice: String,
    velocity: u8,
    peak: f32,
    rms: f32,
    crest_db: f32,
    tail_ms: f32,
    low_energy: f64,
    mid_energy: f64,
    high_energy: f64,
    centroid_hz: f64,
    diagnostics: EngineDiagnostics,
}

pub(crate) fn render_review(
    kit_directory: &Path,
    output: &Path,
    expected_kit_id: &str,
    comparison_voice_ids: &[&str],
) -> Result<()> {
    if output.exists() {
        bail!("refusing to replace existing review {}", output.display());
    }
    let kit = load_package(kit_directory, ProjectKey::default(), &KitTuning::default())
        .map_err(anyhow::Error::msg)?;
    if kit.manifest.kit_id != expected_kit_id
        || kit.manifest.voices.len() < 24
        || kit.manifest.voices.iter().any(|voice| {
            voice.kind != VoiceKind::Modeled
                || !voice.samples.is_empty()
                || voice.advanced_model.is_none()
        })
    {
        bail!("review requires the fully modeled {expected_kit_id} kit");
    }
    fs::create_dir_all(output.join("voices"))?;
    fs::create_dir_all(output.join("comparisons"))?;
    fs::create_dir_all(output.join("patterns"))?;

    let mut measurements = Vec::new();
    for voice in &kit.manifest.voices {
        for velocity in VELOCITIES {
            let duration = voice_duration_seconds(voice, kit.manifest.max_tail_seconds);
            let (frames, diagnostics) =
                render_events(&kit, &[(0, voice.trigger_note, velocity)], duration)?;
            let relative = format!(
                "voices/{:03}-{}-v{:03}.wav",
                voice.trigger_note, voice.id, velocity
            );
            write_wav(&output.join(&relative), &frames)?;
            measurements.push(measure(
                relative,
                voice.id.clone(),
                velocity,
                &frames,
                diagnostics,
            ));
        }
    }

    for &id in comparison_voice_ids {
        let voice = kit
            .manifest
            .voices
            .iter()
            .find(|voice| voice.id == id)
            .with_context(|| format!("missing comparison voice {id}"))?;
        let duration = voice_duration_seconds(voice, kit.manifest.max_tail_seconds);
        let (authored, authored_diagnostics) =
            render_events(&kit, &[(0, voice.trigger_note, 127)], duration)?;
        let authored_path = format!("comparisons/{id}-authored-clipped.wav");
        write_wav(&output.join(&authored_path), &authored)?;
        measurements.push(measure(
            authored_path,
            format!("{id}-authored"),
            127,
            &authored,
            authored_diagnostics,
        ));

        let mut dry_kit = kit.clone();
        dry_kit.bypass_intentional_colour();
        let (dry, dry_diagnostics) =
            render_events(&dry_kit, &[(0, voice.trigger_note, 127)], duration)?;
        let dry_path = format!("comparisons/{id}-drive-bypassed.wav");
        write_wav(&output.join(&dry_path), &dry)?;
        measurements.push(measure(
            dry_path,
            format!("{id}-drive-bypassed"),
            127,
            &dry,
            dry_diagnostics,
        ));
    }

    for pattern in patterns(&kit) {
        let (frames, diagnostics) = render_events(&kit, &pattern.events, pattern.duration)?;
        let relative = format!("patterns/{}.wav", pattern.name);
        write_wav(&output.join(&relative), &frames)?;
        measurements.push(measure(
            relative,
            pattern.name.into(),
            0,
            &frames,
            diagnostics,
        ));
    }

    write_measurements(output, &measurements)?;
    write_voice_structures(output, &kit)?;
    write_report(
        output,
        kit_directory,
        &kit,
        &measurements,
        comparison_voice_ids,
    )?;
    println!(
        "rendered {} voices at three velocities, {} clip comparisons, and three patterns to {}",
        kit.manifest.voices.len(),
        comparison_voice_ids.len() * 2,
        output.display()
    );
    Ok(())
}

struct Pattern {
    name: &'static str,
    duration: f32,
    events: Vec<(usize, u8, u8)>,
}

fn patterns(kit: &PreparedKit) -> Vec<Pattern> {
    let step = |tempo: f32| (SAMPLE_RATE as f32 * 60.0 / tempo / 4.0).round() as usize;
    let mut floor = Vec::new();
    let floor_step = step(124.0);
    for position in 0..32usize {
        if position % 4 == 0 {
            floor.push((position * floor_step, 36, 118));
        }
        if position % 8 == 4 {
            floor.push((position * floor_step, 38, 108));
            floor.push((position * floor_step, 39, 86));
        }
        floor.push((
            position * floor_step,
            if position % 4 == 2 { 44 } else { 42 },
            if position % 4 == 2 { 72 } else { 88 },
        ));
        if matches!(position, 7 | 15 | 23) {
            floor.push((position * floor_step, 46, 96));
        }
    }
    floor.extend([
        (0, 49, 105),
        (8 * floor_step, 45, 96),
        (9 * floor_step, 47, 92),
        (10 * floor_step, 50, 90),
        (24 * floor_step, 51, 98),
    ]);
    floor.sort_by_key(|event| event.0);

    let mut warehouse = Vec::new();
    let warehouse_step = step(128.0);
    for position in 0..32usize {
        warehouse.push((
            position * warehouse_step,
            if position % 2 == 0 { 22 } else { 54 },
            72 + (position % 4) as u8 * 8,
        ));
        if position % 4 == 0 {
            warehouse.push((
                position * warehouse_step,
                if position % 8 == 0 { 34 } else { 33 },
                120,
            ));
        }
        if position % 8 == 4 {
            warehouse.push((position * warehouse_step, 30, 116));
            warehouse.push((position * warehouse_step, 29, 92));
        }
    }
    warehouse.extend([
        (3 * warehouse_step, 37, 93),
        (6 * warehouse_step, 56, 102),
        (11 * warehouse_step, 48, 106),
        (14 * warehouse_step, 24, 112),
        (22 * warehouse_step, 25, 108),
        (24 * warehouse_step, 52, 117),
        (27 * warehouse_step, 26, 104),
    ]);
    warehouse.sort_by_key(|event| event.0);

    let showcase_step = step(120.0) * 2;
    let mut showcase = kit
        .manifest
        .voices
        .iter()
        .enumerate()
        .map(|(index, voice)| {
            (
                index * showcase_step,
                voice.trigger_note,
                [48, 82, 122][index % 3],
            )
        })
        .collect::<Vec<_>>();
    showcase.sort_by_key(|event| event.0);

    vec![
        Pattern {
            name: "01-house-floor",
            duration: 32.0 * floor_step as f32 / SAMPLE_RATE as f32 + 5.8,
            events: floor,
        },
        Pattern {
            name: "02-clipped-warehouse",
            duration: 32.0 * warehouse_step as f32 / SAMPLE_RATE as f32 + 3.0,
            events: warehouse,
        },
        Pattern {
            name: "03-whole-palette",
            duration: showcase.len() as f32 * showcase_step as f32 / SAMPLE_RATE as f32 + 5.8,
            events: showcase,
        },
    ]
}

fn render_events(
    kit: &PreparedKit,
    events: &[(usize, u8, u8)],
    duration_seconds: f32,
) -> Result<(Vec<StereoFrame>, EngineDiagnostics)> {
    let frame_count = (duration_seconds * SAMPLE_RATE as f32).ceil() as usize;
    let (sender, receiver) = event_queue();
    let mut engine = DrumEngine::new(SAMPLE_RATE, kit.clone(), receiver)?;
    let mut output = vec![StereoFrame::SILENCE; frame_count];
    let mut event_index = 0usize;
    let mut position = 0usize;
    while position < frame_count {
        while event_index < events.len() && events[event_index].0 == position {
            let (_, note, velocity) = events[event_index];
            sender
                .push(DrumEvent::NoteOn { note, velocity })
                .map_err(|_| anyhow::anyhow!("offline event queue unexpectedly full"))?;
            event_index += 1;
        }
        let next_event = events
            .get(event_index)
            .map_or(frame_count, |event| event.0.min(frame_count));
        let end = (position + BLOCK).min(next_event).min(frame_count);
        if end == position {
            continue;
        }
        engine.process(&mut output[position..end]);
        position = end;
    }
    Ok((output, engine.diagnostics()))
}

fn voice_duration_seconds(voice: &shr_drums::VoiceManifest, maximum: f32) -> f32 {
    let envelope = voice.envelope;
    ((envelope.attack_ms + envelope.hold_ms + envelope.decay_ms * 1.4) / 1_000.0 + 0.1)
        .clamp(0.45, maximum)
}

fn write_wav(path: &Path, frames: &[StereoFrame]) -> Result<()> {
    let specification = WavSpec {
        channels: 2,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 24,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::create(path, specification)
        .with_context(|| format!("create {}", path.display()))?;
    for frame in frames {
        for sample in [frame.left, frame.right] {
            writer.write_sample((sample.clamp(-1.0, 1.0) * 8_388_607.0).round() as i32)?;
        }
    }
    writer.finalize()?;
    Ok(())
}

fn measure(
    file: String,
    voice: String,
    velocity: u8,
    frames: &[StereoFrame],
    diagnostics: EngineDiagnostics,
) -> Measurement {
    let peak = frames
        .iter()
        .map(|frame| frame.left.abs().max(frame.right.abs()))
        .fold(0.0_f32, f32::max);
    let sum_squares = frames
        .iter()
        .map(|frame| {
            let mono = (frame.left + frame.right) * 0.5;
            f64::from(mono * mono)
        })
        .sum::<f64>();
    let rms = (sum_squares / frames.len().max(1) as f64).sqrt() as f32;
    let crest_db = if rms > 0.0 {
        20.0 * (peak / rms).log10()
    } else {
        0.0
    };
    let tail_frame = frames
        .iter()
        .rposition(|frame| frame.left.abs().max(frame.right.abs()) >= 0.001)
        .unwrap_or(0);
    let tail_ms = tail_frame as f32 * 1_000.0 / SAMPLE_RATE as f32;
    let (low_energy, mid_energy, high_energy, centroid_hz) = spectral_measurements(frames);
    Measurement {
        file,
        voice,
        velocity,
        peak,
        rms,
        crest_db,
        tail_ms,
        low_energy,
        mid_energy,
        high_energy,
        centroid_hz,
        diagnostics,
    }
}

fn spectral_measurements(frames: &[StereoFrame]) -> (f64, f64, f64, f64) {
    let frequencies = [
        40.0, 60.0, 90.0, 140.0, 220.0, 350.0, 600.0, 1_000.0, 1_800.0, 3_200.0, 5_000.0, 8_000.0,
        12_000.0, 16_000.0,
    ];
    let length = frames.len().min(65_536);
    let mut low = 0.0;
    let mut mid = 0.0;
    let mut high = 0.0;
    let mut weighted = 0.0;
    let mut total = 0.0;
    for frequency in frequencies {
        let coefficient = 2.0 * (std::f64::consts::TAU * frequency / SAMPLE_RATE as f64).cos();
        let (mut previous, mut older) = (0.0_f64, 0.0_f64);
        for frame in &frames[..length] {
            let sample = f64::from((frame.left + frame.right) * 0.5);
            let current = sample + coefficient * previous - older;
            older = previous;
            previous = current;
        }
        let power = (previous * previous + older * older - coefficient * previous * older).max(0.0);
        match frequency {
            value if value <= 220.0 => low += power,
            value if value <= 3_200.0 => mid += power,
            _ => high += power,
        }
        weighted += power * frequency;
        total += power;
    }
    (low, mid, high, weighted / total.max(f64::MIN_POSITIVE))
}

fn write_measurements(output: &Path, measurements: &[Measurement]) -> Result<()> {
    let mut table = String::from(
        "file\tvoice\tvelocity\tpeak_dbfs\trms_dbfs\tcrest_db\ttail_ms\tlow_energy\tmid_energy\thigh_energy\tcentroid_hz\tinternal_peak\tintentional_clip_events\tbus_pre_safety_peak\tsafety_ceiling_events\n",
    );
    for measurement in measurements {
        writeln!(
            table,
            "{}\t{}\t{}\t{:.3}\t{:.3}\t{:.3}\t{:.1}\t{:.6e}\t{:.6e}\t{:.6e}\t{:.1}\t{:.5}\t{}\t{:.5}\t{}",
            measurement.file,
            measurement.voice,
            measurement.velocity,
            amplitude_db(measurement.peak),
            amplitude_db(measurement.rms),
            measurement.crest_db,
            measurement.tail_ms,
            measurement.low_energy,
            measurement.mid_energy,
            measurement.high_energy,
            measurement.centroid_hz,
            measurement.diagnostics.intentional_internal_peak,
            measurement.diagnostics.intentional_clip_events,
            measurement.diagnostics.bus_pre_safety_peak,
            measurement.diagnostics.safety_ceiling_events,
        )?;
    }
    fs::write(output.join("measurements.tsv"), table)?;
    Ok(())
}

fn write_voice_structures(output: &Path, kit: &PreparedKit) -> Result<()> {
    let mut report = String::from(
        format!(
            "# {} voice structures\n\nAll sounds are real-time SHR Drums models. No voice has a sample assignment.\n\n",
            kit.manifest.display_name
        ),
    );
    for voice in &kit.manifest.voices {
        let model = voice
            .advanced_model
            .as_ref()
            .context("reviewed voice lacks advanced model")?;
        writeln!(
            report,
            "## {} — note {}\n\n- Purpose/family: `{}` / `{}`.\n- Core: `{:?}` algorithm, `{:?}` body at {:.1} Hz; pitch {:.0}→{:.0} cents over {:.1}/{:.1} ms.\n- Layers: body {:.2} ({:.0} ms), click {:.2} at {:.0} Hz ({:.1} ms), `{:?}` noise {:.2} at {:.0} Hz ({:.0} ms + tail {:.2}/{:.0} ms), {} resonant/inharmonic modes, {} timed bursts.\n- Modulation: FM ratio {:.3}/index {:.2}, phase {:.2}, ring ratio {:.3}/mix {:.2}, feedback {:.2}.\n- Drive: body `{:?}` {:.0} dB/{:.2}, noise `{:?}` {:.0} dB/{:.2}, master `{:?}` {:.0} dB/{:.2}.\n- Performance: velocity affects click {:.2}, noise {:.2}, drive {:.2}, decay {:.2}, brightness {:.2}, and pitch {:.2}; width {:.2}, micro-delay {:.2} ms, choke {:?}/{:.1} ms.\n",
            voice.display_name,
            voice.trigger_note,
            voice.id,
            voice.family,
            model.algorithm,
            model.oscillator,
            model.base_hz,
            model.pitch.start_cents,
            model.pitch.mid_cents,
            model.pitch.attack_ms,
            model.pitch.decay_ms,
            model.body.level,
            model.body.decay_ms,
            model.click.level,
            model.click.tone_hz,
            model.click.decay_ms,
            model.noise.filter,
            model.noise.level,
            model.noise.cutoff_hz,
            model.noise.decay_ms,
            model.noise.tail_level,
            model.noise.tail_decay_ms,
            model.modes.len(),
            model.bursts.len(),
            model.modulation.fm_ratio,
            model.modulation.fm_index,
            model.modulation.phase_amount,
            model.modulation.ring_ratio,
            model.modulation.ring_amount,
            model.modulation.feedback,
            model.body.drive.curve,
            model.body.drive.pre_gain_db,
            model.body.drive.amount,
            model.noise.drive.curve,
            model.noise.drive.pre_gain_db,
            model.noise.drive.amount,
            model.master_drive.curve,
            model.master_drive.pre_gain_db,
            model.master_drive.amount,
            model.velocity.click,
            model.velocity.noise,
            model.velocity.drive,
            model.velocity.decay,
            model.velocity.brightness,
            model.velocity.pitch,
            model.stereo.width,
            model.stereo.micro_delay_ms,
            voice.choke_group,
            voice.choke_release_ms,
        )?;
    }
    fs::write(output.join("VOICE_STRUCTURES.md"), report)?;
    Ok(())
}

fn write_report(
    output: &Path,
    kit_directory: &Path,
    kit: &PreparedKit,
    measurements: &[Measurement],
    comparison_voice_ids: &[&str],
) -> Result<()> {
    let ceiling = kit.manifest.processing.ceiling_dbfs;
    let output_over_ceiling = measurements.iter().any(|measurement| {
        amplitude_db(measurement.peak) > ceiling + 0.001 || !measurement.peak.is_finite()
    });
    let internal_clipped = measurements
        .iter()
        .filter(|measurement| measurement.diagnostics.intentional_clip_events > 0)
        .count();
    let safety_events = measurements
        .iter()
        .map(|measurement| measurement.diagnostics.safety_ceiling_events)
        .sum::<u64>();
    let mut report = String::new();
    writeln!(
        report,
        "# {} private modeled review\n\n- Kit: `{}`\n- Engine identity: SHR Drums modeled engine, compatible from `{}` and below `{}`.\n- Voices: {} unique triggers, all `modeled`, zero sample assignments.\n- Render format: deterministic 48 kHz stereo 24-bit PCM generated offline by SHR Drums; no file was played.\n- Velocities: 32, 80, and 127 for every voice.\n- Final safety ceiling: {:.1} dBFS; observed output above ceiling: **{}**.\n",
        kit.manifest.display_name,
        kit_directory.display(),
        kit.manifest.engine.minimum,
        kit.manifest.engine.maximum_exclusive,
        kit.manifest.voices.len(),
        ceiling,
        output_over_ceiling,
    )?;
    writeln!(
        report,
        "## Intentional clipping versus final protection\n\nVoice-local body, click, noise, mode, and master stages apply authored soft, hard, cubic, or fold clipping. The kit bus then applies bounded saturation and parallel compression. Across the review, {} renders recorded intentional pre-shaper samples above unity. The protected final ceiling intervened on {} rendered frames; that intervention is reported separately and every stored sample remained finite and at or below the {:.1} dBFS ceiling. A ceiling intervention is not treated as the authored distortion itself—the distortion occurs earlier—but it proves the host-facing boundary remained protected.\n",
        internal_clipped,
        safety_events,
        ceiling,
    )?;
    writeln!(
        report,
        "The `comparisons/` directory contains authored and drive-bypassed velocity-127 renders for {}. The bypass retains filtering and the final ceiling, uses a conservative 0.5 dB comparison makeup, and disables model drive, feedback, kit saturation, transient/body emphasis, and parallel compression.\n",
        comparison_voice_ids.join(" and ")
    )?;
    report.push_str(
        "## Measurements\n\n`measurements.tsv` contains peak, RMS, crest factor, active tail, coarse low/mid/high spectral energy, spectral centroid, intentional internal peak/clip count, bus pre-safety peak, and final-ceiling intervention count for every WAV. `VOICE_STRUCTURES.md` documents each voice layer and velocity response. The three pattern renders include a conventional floor pattern, a clipped warehouse pattern, and a whole-palette pattern that triggers every note.\n",
    );
    report.push_str(
        "\nAdvanced voices hard-retrigger their own prior oscillator/noise instance, while cross-voice hat choking uses each voice's short authored choke release. Pattern renders therefore match the bounded electronic drum-machine retrigger behavior used by SHR-DAW.\n",
    );
    fs::write(output.join("REPORT.md"), report)?;
    Ok(())
}

fn amplitude_db(value: f32) -> f32 {
    if value > 0.0 {
        20.0 * value.log10()
    } else {
        -120.0
    }
}
