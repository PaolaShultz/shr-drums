use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use shr_drums::{
    load_package, EngineCompatibility, Envelope, FollowKeyRule, KitManifest, KitMetadata,
    KitProcessing, KitTuning, ModeledParameters, ProjectKey, SampleAssignment, SampleMetadata,
    TuningLimits, VoiceKind, VoiceManifest, KIT_FORMAT_VERSION,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

fn main() -> Result<()> {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    match arguments.as_slice() {
        [command, path] if command == "validate" => validate(Path::new(path)),
        [command, manifest, output] if command == "compile" => {
            compile(Path::new(manifest), Path::new(output))
        }
        [command, output] if command == "factory" => factory(Path::new(output)),
        [command, source, output] if command == "import-muldjord" => {
            import_muldjord(Path::new(source), Path::new(output))
        }
        [command, wav] if command == "analyze-pitch" => analyze_pitch(Path::new(wav)),
        _ => {
            eprintln!(
                "usage:\n  shr-kit validate <manifest.json|kit.shrkit>\n  shr-kit compile <manifest.json> <output.shrkit>\n  shr-kit factory <output-directory>\n  shr-kit import-muldjord <extracted-source> <output-directory>\n  shr-kit analyze-pitch <sample.wav>"
            );
            std::process::exit(2);
        }
    }
}

fn validate(path: &Path) -> Result<()> {
    if path.is_dir() {
        let kit = load_package(path, ProjectKey::default(), &KitTuning::default())
            .map_err(anyhow::Error::msg)?;
        println!(
            "{} ({}) · {} voices · {} decoded samples · {} decoded bytes",
            kit.manifest.display_name,
            kit.manifest.kit_id,
            kit.manifest.voices.len(),
            kit.samples.len(),
            kit.decoded_sample_bytes()
        );
        return Ok(());
    }
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let manifest = KitManifest::from_json(&bytes).map_err(anyhow::Error::msg)?;
    println!(
        "{} ({}) · manifest valid",
        manifest.display_name, manifest.kit_id
    );
    Ok(())
}

fn compile(manifest_path: &Path, output: &Path) -> Result<()> {
    if output.exists() {
        bail!("refusing to replace existing package {}", output.display());
    }
    if output.extension().and_then(|value| value.to_str()) != Some("shrkit") {
        bail!("compiled package directory must end in .shrkit");
    }
    let bytes = fs::read(manifest_path)
        .with_context(|| format!("read source manifest {}", manifest_path.display()))?;
    let manifest = KitManifest::from_json(&bytes).map_err(anyhow::Error::msg)?;
    let source_root = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(output).with_context(|| format!("create package {}", output.display()))?;
    let result = (|| -> Result<()> {
        for voice in &manifest.voices {
            for sample in &voice.samples {
                let source = source_root.join(&sample.path);
                let destination = output.join(&sample.path);
                let parent = destination
                    .parent()
                    .context("sample destination has no parent")?;
                fs::create_dir_all(parent)?;
                fs::copy(&source, &destination).with_context(|| {
                    format!(
                        "copy cleared sample {} to {}",
                        source.display(),
                        destination.display()
                    )
                })?;
            }
        }
        fs::write(output.join("manifest.json"), pretty_manifest(&manifest)?)?;
        load_package(output, ProjectKey::default(), &KitTuning::default())
            .map_err(anyhow::Error::msg)?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_dir_all(output);
        return Err(error);
    }
    println!("compiled {}", output.display());
    Ok(())
}

fn factory(output: &Path) -> Result<()> {
    fs::create_dir_all(output)?;
    for family in [
        Family::BigRock,
        Family::IndustrialMetal,
        Family::ElectronicHouse,
    ] {
        build_factory_kit(output, family)?;
    }
    Ok(())
}

fn import_muldjord(source: &Path, output: &Path) -> Result<()> {
    let required = [
        source.join("LICENSE.txt"),
        source.join("README.txt"),
        source.join("MuldjordKit 20201018.sfz"),
        source.join("samples"),
    ];
    if required.iter().any(|path| !path.exists()) {
        bail!(
            "{} is not the extracted FreePats MuldjordKit SFZ+WAV 2020-10-18 archive",
            source.display()
        );
    }
    fs::create_dir_all(output)?;
    for family in [Family::BigRock, Family::IndustrialMetal] {
        build_muldjord_kit(source, output, family)?;
    }
    Ok(())
}

fn build_muldjord_kit(source: &Path, output: &Path, family: Family) -> Result<()> {
    let suffix = match family {
        Family::BigRock => "big-rock-muldjord",
        Family::IndustrialMetal => "industrial-metal-muldjord",
        Family::ElectronicHouse => unreachable!("Muldjord import is acoustic"),
    };
    let directory = output.join(format!("{suffix}.shrkit"));
    if directory.exists() {
        bail!(
            "refusing to replace existing package {}",
            directory.display()
        );
    }
    fs::create_dir_all(directory.join("samples"))?;
    let result = (|| -> Result<()> {
        fs::copy(
            source.join("LICENSE.txt"),
            directory.join("LICENSE-CC-BY-4.0.txt"),
        )?;
        fs::copy(
            source.join("README.txt"),
            directory.join("SOURCE-README.txt"),
        )?;
        let pieces: [(&str, &str, u8, f32, &str, Option<u8>, bool, [&str; 6]); 8] = [
            (
                "kick",
                "Kick",
                36,
                36.0,
                "kick",
                None,
                false,
                [
                    "KdrumL/4-KdrumL.wav",
                    "KdrumL/6-KdrumL.wav",
                    "KdrumL/15-KdrumL.wav",
                    "KdrumL/16-KdrumL.wav",
                    "KdrumL/23-KdrumL.wav",
                    "KdrumL/24-KdrumL.wav",
                ],
            ),
            (
                "snare",
                "Snare",
                38,
                298.0,
                "snare",
                None,
                false,
                [
                    "Snare1/4-Snare.wav",
                    "Snare1/5-Snare.wav",
                    "Snare1/25-Snare.wav",
                    "Snare1/26-Snare.wav",
                    "Snare1/45-Snare.wav",
                    "Snare1/46-Snare.wav",
                ],
            ),
            (
                "low-tom",
                "Low Tom",
                45,
                66.0,
                "tom",
                None,
                false,
                [
                    "Tom4/2-Tom4.wav",
                    "Tom4/3-Tom4.wav",
                    "Tom4/10-Tom4.wav",
                    "Tom4/11-Tom4.wav",
                    "Tom4/19-Tom4.wav",
                    "Tom4/20-Tom4.wav",
                ],
            ),
            (
                "mid-tom",
                "Mid Tom",
                47,
                87.0,
                "tom",
                None,
                false,
                [
                    "Tom3/2-Tom3.wav",
                    "Tom3/3-Tom3.wav",
                    "Tom3/7-Tom3.wav",
                    "Tom3/8-Tom3.wav",
                    "Tom3/14-Tom3.wav",
                    "Tom3/15-Tom3.wav",
                ],
            ),
            (
                "high-tom",
                "High Tom",
                50,
                118.0,
                "tom",
                None,
                false,
                [
                    "Tom1/2-Tom1.wav",
                    "Tom1/3-Tom1.wav",
                    "Tom1/6-Tom1.wav",
                    "Tom1/7-Tom1.wav",
                    "Tom1/10-Tom1.wav",
                    "Tom1/11-Tom1.wav",
                ],
            ),
            (
                "closed-hat",
                "Closed Hat",
                42,
                8_000.0,
                "cymbal",
                Some(1),
                true,
                [
                    "HihatClosed/2-HihatClosed.wav",
                    "HihatClosed/3-HihatClosed.wav",
                    "HihatClosed/14-HihatClosed.wav",
                    "HihatClosed/15-HihatClosed.wav",
                    "HihatClosed/28-HihatClosed.wav",
                    "HihatClosed/29-HihatClosed.wav",
                ],
            ),
            (
                "open-hat",
                "Open Hat",
                46,
                7_000.0,
                "cymbal",
                Some(1),
                true,
                [
                    "HihatOpen/2-HihatOpen.wav",
                    "HihatOpen/3-HihatOpen.wav",
                    "HihatOpen/14-HihatOpen.wav",
                    "HihatOpen/15-HihatOpen.wav",
                    "HihatOpen/28-HihatOpen.wav",
                    "HihatOpen/29-HihatOpen.wav",
                ],
            ),
            (
                "crash",
                "Crash",
                49,
                4_500.0,
                "cymbal",
                None,
                true,
                [
                    "CrashL/1-CrashL.wav",
                    "CrashL/2-CrashL.wav",
                    "CrashL/4-CrashL.wav",
                    "CrashL/5-CrashL.wav",
                    "CrashL/8-CrashL.wav",
                    "CrashL/9-CrashL.wav",
                ],
            ),
        ];
        let mut voices = Vec::new();
        for (id, name, note, pitch, voice_family, choke, broadband, files) in pieces {
            let articulation = match id {
                "closed-hat" => "closed",
                "open-hat" => "open",
                "crash" => "crash",
                _ => "hit",
            };
            let mut samples = Vec::new();
            for (index, file) in files.into_iter().enumerate() {
                let source_path = source.join("samples").join(file);
                let file_name = Path::new(file)
                    .file_name()
                    .context("Muldjord sample has no filename")?
                    .to_string_lossy();
                let relative = format!("samples/{id}-{file_name}");
                let destination = directory.join(&relative);
                fs::copy(&source_path, &destination).with_context(|| {
                    format!("copy cleared Muldjord sample {}", source_path.display())
                })?;
                let bytes = fs::read(&destination)?;
                let metadata = wav_metadata(&destination)?;
                let layer = index / 2;
                samples.push(SampleAssignment {
                    path: relative,
                    velocity_min: (layer as u8) * 42 + 1,
                    velocity_max: if layer == 2 {
                        127
                    } else {
                        (layer as u8 + 1) * 42
                    },
                    round_robin: (index % 2 + 1) as u8,
                    articulation: articulation.into(),
                    gain_db: 0.0,
                    metadata,
                    sha256: sha256_hex(&bytes),
                });
            }
            voices.push(VoiceManifest {
                id: id.into(),
                display_name: name.into(),
                trigger_note: note,
                articulation: articulation.into(),
                family: voice_family.into(),
                kind: VoiceKind::Hybrid,
                choke_group: choke,
                gain_db: if broadband { -9.0 } else { -6.0 },
                pan: match id {
                    "low-tom" => -0.25,
                    "high-tom" => 0.25,
                    _ => 0.0,
                },
                envelope: Envelope {
                    attack_ms: 0.1,
                    hold_ms: 4.0,
                    decay_ms: match id {
                        "open-hat" => 2_000.0,
                        "crash" => 7_000.0,
                        _ => 900.0,
                    },
                    release_ms: 30.0,
                },
                samples,
                base_pitch_hz: (!broadband).then_some(pitch),
                tuning_limits: if broadband {
                    TuningLimits {
                        down_cents: 0,
                        up_cents: 0,
                    }
                } else {
                    TuningLimits {
                        down_cents: -400,
                        up_cents: 400,
                    }
                },
                follow_key: if broadband {
                    FollowKeyRule::Excluded
                } else if voice_family == "tom" {
                    FollowKeyRule::ScaleDegree {
                        degree: match id {
                            "low-tom" => 1,
                            "mid-tom" => 3,
                            _ => 5,
                        },
                        octave: 0,
                    }
                } else {
                    FollowKeyRule::Tonic
                },
                modeled: Some(ModeledParameters {
                    body_hz: pitch,
                    body_decay_ms: if broadband { 180.0 } else { 700.0 },
                    pitch_drop_cents: if id == "kick" { 500.0 } else { 0.0 },
                    noise_amount: if id == "snare" || broadband {
                        0.5
                    } else {
                        0.03
                    },
                    noise_decay_ms: if broadband { 900.0 } else { 120.0 },
                    metallic_amount: if matches!(family, Family::IndustrialMetal) {
                        0.5
                    } else {
                        0.0
                    },
                }),
            });
        }
        let manifest = KitManifest {
            format_version: KIT_FORMAT_VERSION,
            kit_id: suffix.into(),
            display_name: format!("{} (Muldjord)", family.name()),
            metadata: KitMetadata {
                author: "Lars Muldjord; FreePats stereo assembly by roberto@zenvoid.org; SHR package conversion by SHR Drums contributors".into(),
                source: "https://freepats.zenvoid.org/Percussion/acoustic-drum-kit.html · MuldjordKit SFZ+WAV 2020-10-18".into(),
                licence: "Creative Commons Attribution 4.0 International (CC BY 4.0)".into(),
                attribution: "Drum samples provided by DrumGizmo.org; original recordings by Lars Muldjord; FreePats stereo assembly by roberto@zenvoid.org".into(),
                modification_notes: "Selected three velocity bands with two deterministic round robins per piece; remapped triggers to GM percussion notes; preserved stereo attacks and tails; stored reviewed offline body/ring candidates from representative mid-layer tail scans; added independently tunable modeled bodies and bounded bus processing. Audible owner review remains pending.".into(),
                source_hashes: BTreeMap::from([
                    ("MuldjordKit-SFZ+WAV-20201018.7z".into(), "b18dd10d8eab2b812a6624f25d4e05d5ecbf9d44114557b77ba62323472bddfe".into()),
                    ("MuldjordKit 20201018.sfz".into(), "410e17d984b52ab9323b103324ea99e8021cc7631469d81b772ff6d1c4c0f93b".into()),
                    ("README.txt".into(), "d3d69df4db7bd93b568af9cbc901a9ac00bf17a8f98b9229787bd6023cc4d665".into()),
                    ("LICENSE.txt".into(), "9ba9550ad48438d0836ddab3da480b3b69ffa0aac7b7878b5a0039e7ab429411".into()),
                ]),
            },
            engine: EngineCompatibility {
                minimum: "0.1.0".into(),
                maximum_exclusive: "1.0.0".into(),
            },
            max_polyphony: 32,
            max_tail_seconds: 12.0,
            articulations: BTreeMap::from([
                ("closed".into(), 42),
                ("open".into(), 46),
                ("crash".into(), 49),
            ]),
            voices,
            processing: processing(family),
        };
        manifest.validate().map_err(anyhow::Error::msg)?;
        fs::write(directory.join("manifest.json"), pretty_manifest(&manifest)?)?;
        load_package(&directory, ProjectKey::default(), &KitTuning::default())
            .map_err(anyhow::Error::msg)?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&directory);
        return Err(error);
    }
    println!("imported {}", directory.display());
    Ok(())
}

fn wav_metadata(path: &Path) -> Result<SampleMetadata> {
    let reader = hound::WavReader::open(path)?;
    let specification = reader.spec();
    Ok(SampleMetadata {
        sample_rate: specification.sample_rate,
        channels: specification.channels,
        frames: u64::from(reader.duration()),
    })
}

#[derive(Clone, Copy)]
enum Family {
    BigRock,
    IndustrialMetal,
    ElectronicHouse,
}

impl Family {
    fn id(self) -> &'static str {
        match self {
            Self::BigRock => "big-rock",
            Self::IndustrialMetal => "industrial-metal",
            Self::ElectronicHouse => "electronic-house",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::BigRock => "Big Rock",
            Self::IndustrialMetal => "Industrial Metal",
            Self::ElectronicHouse => "Electronic House",
        }
    }
}

fn build_factory_kit(root: &Path, family: Family) -> Result<()> {
    let directory = root.join(format!("{}.shrkit", family.id()));
    if directory.exists() {
        bail!(
            "refusing to replace existing package {}",
            directory.display()
        );
    }
    fs::create_dir_all(directory.join("samples"))?;
    let result = (|| -> Result<()> {
        let sampled = !matches!(family, Family::ElectronicHouse);
        let pieces = [
            ("kick", "Kick", 36, 32.703, "kick", None, false),
            ("snare", "Snare", 38, 65.406, "snare", None, false),
            ("low-tom", "Low Tom", 45, 73.416, "tom", None, false),
            ("mid-tom", "Mid Tom", 47, 97.999, "tom", None, false),
            ("high-tom", "High Tom", 50, 130.813, "tom", None, false),
            (
                "closed-hat",
                "Closed Hat",
                42,
                8_000.0,
                "cymbal",
                Some(1),
                true,
            ),
            ("open-hat", "Open Hat", 46, 7_000.0, "cymbal", Some(1), true),
            ("crash", "Crash", 49, 4_500.0, "cymbal", None, true),
        ];
        let mut voices = Vec::new();
        for (piece_index, (id, name, note, pitch, family_name, choke, broadband)) in
            pieces.into_iter().enumerate()
        {
            let articulation = match id {
                "closed-hat" => "closed",
                "open-hat" => "open",
                "crash" => "crash",
                _ => "hit",
            };
            let mut samples = Vec::new();
            if sampled {
                for layer in 0..3u8 {
                    for round_robin in 1..=2u8 {
                        let relative = format!("samples/{id}-v{}-rr{round_robin}.wav", layer + 1);
                        let destination = directory.join(&relative);
                        write_generated_attack(
                            &destination,
                            pitch,
                            piece_index as u32 * 17 + u32::from(layer) * 5 + u32::from(round_robin),
                            broadband,
                        )?;
                        let bytes = fs::read(&destination)?;
                        samples.push(SampleAssignment {
                            path: relative,
                            velocity_min: layer * 42 + 1,
                            velocity_max: if layer == 2 { 127 } else { (layer + 1) * 42 },
                            round_robin,
                            articulation: articulation.into(),
                            gain_db: -3.0,
                            metadata: SampleMetadata {
                                sample_rate: 48_000,
                                channels: 2,
                                frames: 5_760,
                            },
                            sha256: sha256_hex(&bytes),
                        });
                    }
                }
            }
            let electronic = matches!(family, Family::ElectronicHouse);
            let metallic = matches!(family, Family::IndustrialMetal);
            let kind = if sampled {
                VoiceKind::Hybrid
            } else {
                VoiceKind::Modeled
            };
            voices.push(VoiceManifest {
                id: id.into(),
                display_name: name.into(),
                trigger_note: note,
                articulation: articulation.into(),
                family: family_name.into(),
                kind,
                choke_group: choke,
                gain_db: if broadband { -15.0 } else { -9.0 },
                pan: match id {
                    "low-tom" => -0.25,
                    "high-tom" => 0.25,
                    _ => 0.0,
                },
                envelope: Envelope {
                    attack_ms: 0.1,
                    hold_ms: if broadband { 5.0 } else { 2.0 },
                    decay_ms: match id {
                        "open-hat" => 1_600.0,
                        "crash" => 5_500.0,
                        _ => 650.0,
                    },
                    release_ms: 25.0,
                },
                samples,
                base_pitch_hz: (!broadband).then_some(pitch),
                tuning_limits: if broadband {
                    TuningLimits {
                        down_cents: 0,
                        up_cents: 0,
                    }
                } else if sampled {
                    TuningLimits {
                        down_cents: -400,
                        up_cents: 400,
                    }
                } else {
                    TuningLimits {
                        down_cents: -1_200,
                        up_cents: 1_200,
                    }
                },
                follow_key: if broadband {
                    FollowKeyRule::Excluded
                } else if family_name == "tom" {
                    FollowKeyRule::ScaleDegree {
                        degree: match id {
                            "low-tom" => 1,
                            "mid-tom" => 3,
                            _ => 5,
                        },
                        octave: 0,
                    }
                } else {
                    FollowKeyRule::Tonic
                },
                modeled: Some(ModeledParameters {
                    body_hz: pitch,
                    body_decay_ms: if broadband { 180.0 } else { 600.0 },
                    pitch_drop_cents: if id == "kick" {
                        if electronic {
                            1_200.0
                        } else {
                            650.0
                        }
                    } else {
                        0.0
                    },
                    noise_amount: if id == "snare" || broadband {
                        0.75
                    } else {
                        0.05
                    },
                    noise_decay_ms: if broadband { 900.0 } else { 130.0 },
                    metallic_amount: if metallic {
                        0.5
                    } else if electronic && id == "snare" {
                        0.2
                    } else {
                        0.0
                    },
                }),
            });
        }
        let manifest = KitManifest {
            format_version: KIT_FORMAT_VERSION,
            kit_id: family.id().into(),
            display_name: family.name().into(),
            metadata: KitMetadata {
                author: "SHR Drums contributors".into(),
                source: if sampled {
                    "Locally generated review attacks; replaceable by the documented cleared-source importer"
                        .into()
                } else {
                    "Deterministic modeled synthesis".into()
                },
                licence: "CC0-1.0".into(),
                attribution: "No attribution required for generated review material".into(),
                modification_notes:
                    "Generated at 48 kHz; modeled body remains independently tunable".into(),
                source_hashes: BTreeMap::new(),
            },
            engine: EngineCompatibility {
                minimum: "0.1.0".into(),
                maximum_exclusive: "1.0.0".into(),
            },
            max_polyphony: if sampled { 32 } else { 24 },
            max_tail_seconds: if sampled { 12.0 } else { 8.0 },
            articulations: BTreeMap::from([
                ("closed".into(), 42),
                ("open".into(), 46),
                ("crash".into(), 49),
            ]),
            voices,
            processing: processing(family),
        };
        manifest.validate().map_err(anyhow::Error::msg)?;
        fs::write(directory.join("manifest.json"), pretty_manifest(&manifest)?)?;
        load_package(&directory, ProjectKey::default(), &KitTuning::default())
            .map_err(anyhow::Error::msg)?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&directory);
        return Err(error);
    }
    println!("generated {}", directory.display());
    Ok(())
}

fn processing(family: Family) -> KitProcessing {
    match family {
        Family::BigRock => KitProcessing {
            high_pass_hz: 18.0,
            low_pass_hz: 19_000.0,
            saturation: 0.24,
            transient: 0.35,
            body: 0.28,
            parallel_compression: 0.38,
            room_amount: 0.24,
            room_decay: 0.48,
            output_gain_db: -7.0,
            ceiling_dbfs: -1.5,
        },
        Family::IndustrialMetal => KitProcessing {
            high_pass_hz: 22.0,
            low_pass_hz: 16_000.0,
            saturation: 0.52,
            transient: 0.5,
            body: 0.2,
            parallel_compression: 0.62,
            room_amount: 0.36,
            room_decay: 0.62,
            output_gain_db: -9.0,
            ceiling_dbfs: -1.5,
        },
        Family::ElectronicHouse => KitProcessing {
            high_pass_hz: 15.0,
            low_pass_hz: 18_000.0,
            saturation: 0.2,
            transient: 0.4,
            body: 0.3,
            parallel_compression: 0.3,
            room_amount: 0.12,
            room_decay: 0.28,
            output_gain_db: -8.0,
            ceiling_dbfs: -1.5,
        },
    }
}

fn write_generated_attack(path: &Path, pitch: f32, seed: u32, broadband: bool) -> Result<()> {
    let specification = hound::WavSpec {
        channels: 2,
        sample_rate: 48_000,
        bits_per_sample: 24,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, specification)?;
    let mut random = seed | 1;
    for frame in 0..5_760 {
        random ^= random << 13;
        random ^= random >> 17;
        random ^= random << 5;
        let noise = random as f32 / u32::MAX as f32 * 2.0 - 1.0;
        let time = frame as f32 / 48_000.0;
        let decay = (-time * if broadband { 34.0 } else { 22.0 }).exp();
        let tone = (std::f32::consts::TAU * pitch * time).sin();
        let value = (noise * if broadband { 0.5 } else { 0.14 } + tone * 0.3) * decay;
        let integer = (value.clamp(-1.0, 1.0) * 8_388_607.0).round() as i32;
        writer.write_sample(integer)?;
        writer.write_sample(integer)?;
    }
    writer.finalize()?;
    Ok(())
}

fn analyze_pitch(path: &Path) -> Result<()> {
    let mut reader = hound::WavReader::open(path)?;
    let specification = reader.spec();
    let channels = usize::from(specification.channels);
    if channels == 0 || channels > 2 {
        bail!("pitch analysis accepts mono or stereo WAV");
    }
    let samples = reader
        .samples::<i32>()
        .take(specification.sample_rate as usize * channels * 4)
        .collect::<Result<Vec<_>, _>>()?;
    let mono = samples
        .chunks_exact(channels)
        .map(|frame| frame.iter().map(|value| *value as f64).sum::<f64>() / channels as f64)
        .collect::<Vec<_>>();
    // Skip the broadband stick/beater transient; kit metadata describes the
    // tunable body/ring rather than the attack click.
    let start = (specification.sample_rate as usize * 80 / 1_000).min(mono.len());
    let window_end = (start + specification.sample_rate as usize / 2).min(mono.len());
    let mut window = mono[start..window_end].to_vec();
    if window.len() < specification.sample_rate as usize / 10 {
        bail!("sample is too short for reviewed pitch analysis");
    }
    let mean = window.iter().sum::<f64>() / window.len() as f64;
    let window_denominator = (window.len() - 1).max(1) as f64;
    for (index, sample) in window.iter_mut().enumerate() {
        let hann = 0.5 - 0.5 * (std::f64::consts::TAU * index as f64 / window_denominator).cos();
        *sample = (*sample - mean) * hann;
    }
    let mut powers = Vec::with_capacity(981);
    for frequency in 20..=1_000 {
        let coefficient = 2.0
            * (std::f64::consts::TAU * frequency as f64 / specification.sample_rate as f64).cos();
        let (mut previous, mut older) = (0.0, 0.0);
        for &sample in &window {
            let current = sample + coefficient * previous - older;
            older = previous;
            previous = current;
        }
        powers.push(previous * previous + older * older - coefficient * previous * older);
    }
    let (peak_index, peak_power) = powers
        .iter()
        .copied()
        .enumerate()
        .max_by(|left, right| left.1.total_cmp(&right.1))
        .context("dominant-pitch candidate missing")?;
    if !peak_power.is_finite() || peak_power <= 0.0 {
        bail!("no stable dominant-pitch candidate found");
    }
    let frequency = (peak_index + 20) as f64;
    let total_power = powers.iter().sum::<f64>();
    let relative_power = peak_power / total_power.max(peak_power);
    println!(
        "{}: dominant candidate {:.3} Hz (relative scanned power {:.3}, review required before manifest use)",
        path.display(),
        frequency,
        relative_power
    );
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn pretty_manifest(manifest: &KitManifest) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(manifest)?;
    bytes.push(b'\n');
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_factory_families_have_distinct_bounded_processing() {
        let rock = processing(Family::BigRock);
        let metal = processing(Family::IndustrialMetal);
        let house = processing(Family::ElectronicHouse);
        assert_ne!(rock, metal);
        assert_ne!(metal, house);
        for settings in [rock, metal, house] {
            assert!(settings.ceiling_dbfs <= -1.0);
            assert!(settings.room_amount <= 1.0);
        }
    }

    #[test]
    fn generated_factory_packages_compile_and_validate() {
        let directory =
            std::env::temp_dir().join(format!("shr-kit-factory-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        factory(&directory).unwrap();
        for family in [
            Family::BigRock,
            Family::IndustrialMetal,
            Family::ElectronicHouse,
        ] {
            let package = directory.join(format!("{}.shrkit", family.id()));
            let prepared =
                load_package(&package, ProjectKey::default(), &KitTuning::default()).unwrap();
            assert_eq!(prepared.manifest.kit_id, family.id());
        }
        fs::remove_dir_all(directory).unwrap();
    }
}
