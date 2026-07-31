use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use shr_drums::{
    load_package, AdvancedModel, BodyLayer, ClickLayer, DriveCurve, DriveStage,
    EngineCompatibility, Envelope, FilterMode, FollowKeyRule, KitManifest, KitMetadata,
    KitProcessing, KitTuning, ModelAlgorithm, ModeledParameters, Modulation, NoiseBurst,
    NoiseLayer, OscillatorShape, PitchEnvelope, ProjectKey, ResonantMode, SampleAssignment,
    SampleMetadata, SeededVariation, StereoModel, TuningLimits, VelocityResponse, VoiceKind,
    VoiceManifest, KIT_FORMAT_VERSION,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

mod review;

fn main() -> Result<()> {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    match arguments.as_slice() {
        [command, path] if command == "validate" => validate(Path::new(path)),
        [command, manifest, output] if command == "compile" => {
            compile(Path::new(manifest), Path::new(output))
        }
        [command, output] if command == "factory" => factory(Path::new(output)),
        [command, output] if command == "electronic-house" => {
            build_electronic_house(Path::new(output))
        }
        [command, output] if command == "acid" => build_acid(Path::new(output)),
        [command, kit, output] if command == "review-electronic-house" => review::render_review(
            Path::new(kit),
            Path::new(output),
            "electronic-house",
            &["clipped-kick", "clipped-snare"],
        ),
        [command, kit, output] if command == "review-acid" => review::render_review(
            Path::new(kit),
            Path::new(output),
            "acid",
            &["rave-kick", "industrial-snare"],
        ),
        [command, source, output] if command == "import-muldjord" => {
            import_muldjord(Path::new(source), Path::new(output))
        }
        [command, wav] if command == "analyze-pitch" => analyze_pitch(Path::new(wav)),
        _ => {
            eprintln!(
                "usage:\n  shr-kit validate <manifest.json|kit.shrkit>\n  shr-kit compile <manifest.json> <output.shrkit>\n  shr-kit factory <output-directory>\n  shr-kit electronic-house <output-directory>\n  shr-kit acid <output-directory>\n  shr-kit review-electronic-house <kit.shrkit> <output-directory>\n  shr-kit review-acid <kit.shrkit> <output-directory>\n  shr-kit import-muldjord <extracted-source> <output-directory>\n  shr-kit analyze-pitch <sample.wav>"
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
        Family::ExperimentalNoise,
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
    for family in [Family::BigRock, Family::ExperimentalNoise] {
        build_muldjord_kit(source, output, family)?;
    }
    Ok(())
}

fn build_muldjord_kit(source: &Path, output: &Path, family: Family) -> Result<()> {
    let suffix = match family {
        Family::BigRock => "big-rock-muldjord",
        Family::ExperimentalNoise => "experimental-noise-muldjord",
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
        let pieces: [(&str, &str, u8, f32, &str, Option<u8>, bool, [&str; 6]); 9] = [
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
            (
                "ride",
                "Ride",
                51,
                3_000.0,
                "cymbal",
                None,
                true,
                [
                    "RideL/1-RideL.wav",
                    "RideL/2-RideL.wav",
                    "RideL/5-RideL.wav",
                    "RideL/6-RideL.wav",
                    "RideL/9-RideL.wav",
                    "RideL/10-RideL.wav",
                ],
            ),
        ];
        let mut voices = Vec::new();
        for (id, name, note, pitch, voice_family, choke, broadband, files) in pieces {
            let articulation = match id {
                "closed-hat" => "closed",
                "open-hat" => "open",
                "crash" => "crash",
                "ride" => "ride",
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
            let sampled_only = broadband && matches!(family, Family::BigRock);
            voices.push(VoiceManifest {
                id: id.into(),
                display_name: name.into(),
                trigger_note: note,
                articulation: articulation.into(),
                family: voice_family.into(),
                kind: if sampled_only {
                    VoiceKind::Sampled
                } else {
                    VoiceKind::Hybrid
                },
                choke_group: choke,
                choke_release_ms: 0.0,
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
                        "crash" | "ride" => 7_000.0,
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
                modeled: (!sampled_only).then_some(ModeledParameters {
                    body_hz: pitch,
                    body_decay_ms: if broadband { 180.0 } else { 700.0 },
                    pitch_drop_cents: if id == "kick" { 500.0 } else { 0.0 },
                    noise_amount: if id == "snare" || broadband {
                        0.5
                    } else {
                        0.03
                    },
                    noise_decay_ms: if broadband { 900.0 } else { 120.0 },
                    metallic_amount: if matches!(family, Family::ExperimentalNoise) {
                        0.5
                    } else {
                        0.0
                    },
                }),
                advanced_model: None,
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
                ("ride".into(), 51),
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
    ExperimentalNoise,
    ElectronicHouse,
}

impl Family {
    fn id(self) -> &'static str {
        match self {
            Self::BigRock => "big-rock",
            Self::ExperimentalNoise => "experimental-noise",
            Self::ElectronicHouse => "electronic-house",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::BigRock => "Big Rock",
            Self::ExperimentalNoise => "Experimental Noise",
            Self::ElectronicHouse => "Electronic House",
        }
    }
}

fn build_factory_kit(root: &Path, family: Family) -> Result<()> {
    if matches!(family, Family::ElectronicHouse) {
        return build_electronic_house(root);
    }
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
            ("ride", "Ride", 51, 3_000.0, "cymbal", None, true),
        ];
        let mut voices = Vec::new();
        for (piece_index, (id, name, note, pitch, family_name, choke, broadband)) in
            pieces.into_iter().enumerate()
        {
            let articulation = match id {
                "closed-hat" => "closed",
                "open-hat" => "open",
                "crash" => "crash",
                "ride" => "ride",
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
            let metallic = matches!(family, Family::ExperimentalNoise);
            let sampled_only = sampled && broadband && matches!(family, Family::BigRock);
            let kind = if sampled_only {
                VoiceKind::Sampled
            } else if sampled {
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
                choke_release_ms: 0.0,
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
                        "crash" | "ride" => 5_500.0,
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
                modeled: (!sampled_only).then_some(ModeledParameters {
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
                advanced_model: None,
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
                ("ride".into(), 51),
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

fn build_electronic_house(root: &Path) -> Result<()> {
    let directory = root.join("electronic-house.shrkit");
    if directory.exists() {
        bail!(
            "refusing to replace existing package {}",
            directory.display()
        );
    }
    let specifications = [
        ("gated-hat", "Gated Hat", 22, "hat", 7_900.0, -0.18),
        ("zap", "Zap", 24, "fx", 210.0, -0.18),
        ("laser", "Laser", 25, "fx", 160.0, 0.18),
        ("trash-open-hat", "Trash Open Hat", 26, "hat", 6_100.0, 0.28),
        ("wide-clap", "Wide Clap", 29, "clap", 930.0, 0.0),
        ("clipped-snare", "Clipped Snare", 30, "snare", 184.0, 0.0),
        ("noise-snare", "Noise Snare", 31, "snare", 205.0, 0.08),
        ("tight-kick", "Tight Kick", 33, "kick", 68.0, 0.0),
        ("clipped-kick", "Clipped Kick", 34, "kick", 47.0, 0.0),
        ("sub-kick", "Sub Kick", 35, "kick", 41.0, 0.0),
        ("kick", "House Kick", 36, "kick", 52.0, 0.0),
        ("rim", "Rim", 37, "percussion", 510.0, -0.12),
        ("snare", "Tight Snare", 38, "snare", 196.0, 0.0),
        ("house-clap", "House Clap", 39, "clap", 1_150.0, 0.0),
        ("body-snare", "Body Snare", 40, "snare", 158.0, -0.06),
        ("closed-hat", "Closed Hat", 42, "hat", 8_600.0, -0.12),
        ("pedal-hat", "Pedal Hat", 44, "hat", 7_200.0, 0.1),
        ("low-tom", "Low Tom", 45, "tom", 74.0, -0.28),
        ("open-hat", "Open Hat", 46, "hat", 6_800.0, 0.16),
        ("mid-tom", "Mid Tom", 47, "tom", 103.0, -0.05),
        ("zap-tom", "Zap Tom", 48, "tom", 148.0, 0.14),
        ("crash", "Crash", 49, "cymbal", 620.0, -0.1),
        ("high-tom", "High Tom", 50, "tom", 151.0, 0.25),
        ("ride", "Ride", 51, "cymbal", 870.0, 0.12),
        ("china", "China", 52, "cymbal", 730.0, 0.28),
        ("shaker", "Shaker", 54, "percussion", 4_900.0, -0.2),
        ("cowbell", "Cowbell", 56, "percussion", 540.0, 0.16),
    ];
    let voices = specifications
        .into_iter()
        .map(|(id, name, note, family, base_hz, pan)| {
            let advanced_model = electronic_model(id, base_hz);
            let tunable = matches!(
                id,
                "kick"
                    | "sub-kick"
                    | "tight-kick"
                    | "clipped-kick"
                    | "snare"
                    | "body-snare"
                    | "low-tom"
                    | "mid-tom"
                    | "high-tom"
                    | "zap-tom"
            );
            let follow_key = match id {
                "kick" | "sub-kick" | "tight-kick" | "clipped-kick" | "snare" | "body-snare" => {
                    FollowKeyRule::Tonic
                }
                "low-tom" => FollowKeyRule::ScaleDegree {
                    degree: 1,
                    octave: 0,
                },
                "mid-tom" | "zap-tom" => FollowKeyRule::ScaleDegree {
                    degree: 3,
                    octave: 0,
                },
                "high-tom" => FollowKeyRule::ScaleDegree {
                    degree: 5,
                    octave: 0,
                },
                _ => FollowKeyRule::Excluded,
            };
            let articulation = match id {
                "kick" | "snare" | "low-tom" | "mid-tom" | "high-tom" => "hit",
                "closed-hat" => "closed",
                "open-hat" => "open",
                "crash" => "crash",
                "ride" => "ride",
                _ => id,
            };
            let decay_ms = voice_outer_decay_ms(id);
            VoiceManifest {
                id: id.into(),
                display_name: name.into(),
                trigger_note: note,
                articulation: articulation.into(),
                family: family.into(),
                kind: VoiceKind::Modeled,
                choke_group: matches!(
                    id,
                    "gated-hat" | "trash-open-hat" | "closed-hat" | "pedal-hat" | "open-hat"
                )
                .then_some(1),
                choke_release_ms: match id {
                    "trash-open-hat" | "open-hat" => 11.0,
                    "closed-hat" | "pedal-hat" | "gated-hat" => 4.0,
                    _ => 0.0,
                },
                gain_db: voice_gain_db(id),
                pan,
                envelope: Envelope {
                    attack_ms: if matches!(id, "wide-clap" | "crash") {
                        0.35
                    } else {
                        0.05
                    },
                    hold_ms: if matches!(id, "crash" | "ride" | "china") {
                        8.0
                    } else {
                        1.5
                    },
                    decay_ms,
                    release_ms: if family == "cymbal" { 90.0 } else { 24.0 },
                },
                samples: Vec::new(),
                base_pitch_hz: tunable.then_some(base_hz),
                tuning_limits: if tunable {
                    TuningLimits {
                        down_cents: -1_200,
                        up_cents: 1_200,
                    }
                } else {
                    TuningLimits {
                        down_cents: 0,
                        up_cents: 0,
                    }
                },
                follow_key,
                modeled: None,
                advanced_model: Some(advanced_model),
            }
        })
        .collect::<Vec<_>>();
    let articulations = voices
        .iter()
        .map(|voice| (voice.articulation.clone(), voice.trigger_note))
        .collect();
    let manifest = KitManifest {
        format_version: KIT_FORMAT_VERSION,
        kit_id: "electronic-house".into(),
        display_name: "Electronic House".into(),
        metadata: KitMetadata {
            author: "SHR Drums contributors".into(),
            source: "Deterministic SHR Drums modeled synthesis; no recorded or downloaded audio"
                .into(),
            licence: "CC0-1.0".into(),
            attribution: "No attribution required; all voices are synthesized in process".into(),
            modification_notes: "Rebuilt as 27 distinct modeled voices. Existing notes and voice IDs remain compatible. Voice-local click/body/noise/mode layers use deliberate bounded drive, clipping, modulation, deterministic variation, and stereo shaping before the protected kit ceiling.".into(),
            source_hashes: BTreeMap::new(),
        },
        engine: EngineCompatibility {
            minimum: "0.2.0".into(),
            maximum_exclusive: "1.0.0".into(),
        },
        max_polyphony: 40,
        max_tail_seconds: 7.0,
        articulations,
        voices,
        processing: KitProcessing {
            high_pass_hz: 12.0,
            low_pass_hz: 19_500.0,
            saturation: 0.12,
            transient: 0.38,
            body: 0.28,
            parallel_compression: 0.28,
            room_amount: 0.0,
            room_decay: 0.2,
            output_gain_db: 1.0,
            ceiling_dbfs: -1.0,
        },
    };
    manifest.validate().map_err(anyhow::Error::msg)?;
    fs::create_dir_all(&directory)?;
    fs::write(directory.join("manifest.json"), pretty_manifest(&manifest)?)?;
    load_package(&directory, ProjectKey::default(), &KitTuning::default())
        .map_err(anyhow::Error::msg)?;
    println!("generated {}", directory.display());
    Ok(())
}

fn build_acid(root: &Path) -> Result<()> {
    let directory = root.join("acid.shrkit");
    if directory.exists() {
        bail!(
            "refusing to replace existing package {}",
            directory.display()
        );
    }
    let specifications = [
        (
            "gated-metal-hat",
            "Gated Metal Hat",
            22,
            "hat",
            8_450.0,
            -0.2,
        ),
        ("acid-zap", "Acid Zap", 24, "fx", 235.0, -0.22),
        ("down-laser", "Down Laser", 25, "fx", 175.0, 0.22),
        ("trash-open-hat", "Trash Open Hat", 26, "hat", 5_850.0, 0.32),
        ("warehouse-clap", "Warehouse Clap", 29, "clap", 880.0, 0.0),
        (
            "industrial-snare",
            "Industrial Snare",
            30,
            "snare",
            178.0,
            0.0,
        ),
        ("noise-snare", "Noise Snare", 31, "snare", 212.0, 0.1),
        ("tight-kick", "Tight Kick", 33, "kick", 70.0, 0.0),
        ("rave-kick", "Rave Kick", 34, "kick", 49.0, 0.0),
        ("sub-kick-808", "808 Sub Kick", 35, "kick", 43.0, 0.0),
        ("acid-kick-909", "909 Acid Kick", 36, "kick", 54.0, 0.0),
        ("rim", "Rim", 37, "percussion", 535.0, -0.14),
        ("machine-snare", "Machine Snare", 38, "snare", 202.0, 0.0),
        ("acid-clap", "Acid Clap", 39, "clap", 1_220.0, 0.0),
        ("body-snare", "Body Snare", 40, "snare", 152.0, -0.07),
        ("closed-hat", "Closed Hat", 42, "hat", 8_850.0, -0.14),
        ("pedal-hat", "Pedal Hat", 44, "hat", 7_350.0, 0.12),
        ("low-tom", "Low Tom", 45, "tom", 72.0, -0.3),
        ("open-hat", "Open Hat", 46, "hat", 6_950.0, 0.18),
        ("mid-tom", "Mid Tom", 47, "tom", 101.0, -0.06),
        ("fm-tom", "FM Tom", 48, "tom", 146.0, 0.16),
        ("crash", "Crash", 49, "cymbal", 645.0, -0.12),
        ("high-tom", "High Tom", 50, "tom", 154.0, 0.27),
        ("ride", "Ride", 51, "cymbal", 910.0, 0.14),
        ("acid-china", "Acid China", 52, "cymbal", 760.0, 0.3),
        ("shaker", "Shaker", 54, "percussion", 5_100.0, -0.22),
        ("cowbell", "Cowbell", 56, "percussion", 555.0, 0.18),
    ];
    let voices = specifications
        .into_iter()
        .map(|(id, name, note, family, base_hz, pan)| {
            let tunable = matches!(
                id,
                "acid-kick-909"
                    | "sub-kick-808"
                    | "tight-kick"
                    | "rave-kick"
                    | "machine-snare"
                    | "body-snare"
                    | "low-tom"
                    | "mid-tom"
                    | "high-tom"
                    | "fm-tom"
            );
            let follow_key = match id {
                "acid-kick-909" | "sub-kick-808" | "tight-kick" | "rave-kick" | "machine-snare"
                | "body-snare" => FollowKeyRule::Tonic,
                "low-tom" => FollowKeyRule::ScaleDegree {
                    degree: 1,
                    octave: 0,
                },
                "mid-tom" | "fm-tom" => FollowKeyRule::ScaleDegree {
                    degree: 3,
                    octave: 0,
                },
                "high-tom" => FollowKeyRule::ScaleDegree {
                    degree: 5,
                    octave: 0,
                },
                _ => FollowKeyRule::Excluded,
            };
            let articulation = match id {
                "acid-kick-909" => "hit",
                "machine-snare" => "snare",
                "closed-hat" => "closed",
                "open-hat" => "open",
                "crash" => "crash",
                "ride" => "ride",
                _ => id,
            };
            VoiceManifest {
                id: id.into(),
                display_name: name.into(),
                trigger_note: note,
                articulation: articulation.into(),
                family: family.into(),
                kind: VoiceKind::Modeled,
                choke_group: matches!(
                    id,
                    "gated-metal-hat" | "trash-open-hat" | "closed-hat" | "pedal-hat" | "open-hat"
                )
                .then_some(1),
                choke_release_ms: match id {
                    "trash-open-hat" | "open-hat" => 10.0,
                    "closed-hat" | "pedal-hat" | "gated-metal-hat" => 3.5,
                    _ => 0.0,
                },
                gain_db: acid_voice_gain_db(id),
                pan,
                envelope: Envelope {
                    attack_ms: if matches!(id, "warehouse-clap" | "crash") {
                        0.35
                    } else {
                        0.05
                    },
                    hold_ms: if family == "cymbal" { 8.0 } else { 1.5 },
                    decay_ms: acid_voice_outer_decay_ms(id),
                    release_ms: if family == "cymbal" { 90.0 } else { 22.0 },
                },
                samples: Vec::new(),
                base_pitch_hz: tunable.then_some(base_hz),
                tuning_limits: if tunable {
                    TuningLimits {
                        down_cents: -1_200,
                        up_cents: 1_200,
                    }
                } else {
                    TuningLimits {
                        down_cents: 0,
                        up_cents: 0,
                    }
                },
                follow_key,
                modeled: None,
                advanced_model: Some(acid_model(id, base_hz)),
            }
        })
        .collect::<Vec<_>>();
    let articulations = voices
        .iter()
        .map(|voice| (voice.articulation.clone(), voice.trigger_note))
        .collect();
    let manifest = KitManifest {
        format_version: KIT_FORMAT_VERSION,
        kit_id: "acid".into(),
        display_name: "Acid".into(),
        metadata: KitMetadata {
            author: "SHR Drums contributors".into(),
            source: "Deterministic SHR Drums modeled synthesis; no samples, recordings, downloads, or generated audio assets".into(),
            licence: "CC0-1.0".into(),
            attribution: "No attribution required; every voice is synthesized in real time".into(),
            modification_notes: "Authored as 27 distinct acid-house and acid-techno drum-machine voices. Deliberate body/layer/master clipping, saturation, feedback, FM, ring modulation, wavefolding, parallel compression, and flat-topped transients precede an independent protected kit ceiling.".into(),
            source_hashes: BTreeMap::new(),
        },
        engine: EngineCompatibility {
            minimum: "0.2.0".into(),
            maximum_exclusive: "1.0.0".into(),
        },
        max_polyphony: 40,
        max_tail_seconds: 7.5,
        articulations,
        voices,
        processing: KitProcessing {
            high_pass_hz: 11.0,
            low_pass_hz: 19_200.0,
            saturation: 0.26,
            transient: 0.48,
            body: 0.34,
            parallel_compression: 0.38,
            room_amount: 0.0,
            room_decay: 0.2,
            output_gain_db: 0.5,
            ceiling_dbfs: -1.0,
        },
    };
    manifest.validate().map_err(anyhow::Error::msg)?;
    fs::create_dir_all(&directory)?;
    fs::write(directory.join("manifest.json"), pretty_manifest(&manifest)?)?;
    load_package(&directory, ProjectKey::default(), &KitTuning::default())
        .map_err(anyhow::Error::msg)?;
    println!("generated {}", directory.display());
    Ok(())
}

fn acid_model(id: &str, base_hz: f32) -> AdvancedModel {
    let template = match id {
        "gated-metal-hat" => "gated-hat",
        "acid-zap" => "zap",
        "down-laser" => "laser",
        "warehouse-clap" => "wide-clap",
        "industrial-snare" => "clipped-snare",
        "rave-kick" => "clipped-kick",
        "sub-kick-808" => "sub-kick",
        "acid-kick-909" => "kick",
        "machine-snare" => "snare",
        "acid-clap" => "house-clap",
        "fm-tom" => "zap-tom",
        "acid-china" => "china",
        other => other,
    };
    let mut model = electronic_model(template, base_hz);
    model.master_drive.pre_gain_db = (model.master_drive.pre_gain_db + 2.0).min(24.0);
    model.master_drive.amount = (model.master_drive.amount + 0.08).min(1.0);
    model.velocity.drive = (model.velocity.drive + 0.16).min(2.0);
    model.velocity.brightness = (model.velocity.brightness + 0.12).min(2.0);
    match id {
        "sub-kick-808" => {
            model.oscillator = OscillatorShape::Sine;
            model.pitch = pitch(1_780.0, 250.0, 3.5, 135.0);
            model.body.decay_ms = 1_420.0;
            model.body.level = 1.16;
            model.body.overtone_level = 0.025;
            model.click.level = 0.065;
            model.noise.level = 0.01;
            model.master_drive = drive(7.0, 0.38, DriveCurve::SoftClip, -1.5);
            model.velocity.decay = 0.52;
            model.velocity.pitch = 0.32;
        }
        "acid-kick-909" => {
            model.pitch = pitch(2_780.0, 640.0, 1.6, 64.0);
            model.body.decay_ms = 470.0;
            model.click.level = 0.62;
            model.click.decay_ms = 5.2;
            model.click.tone_hz = 4_450.0;
            model.body.drive = drive(11.0, 0.5, DriveCurve::Cubic, -1.8);
            model.master_drive = drive(10.0, 0.5, DriveCurve::SoftClip, -1.5);
            model.velocity.click = 0.92;
        }
        "tight-kick" => {
            model.body.decay_ms = 145.0;
            model.click.level = 0.82;
            model.click.decay_ms = 2.4;
            model.master_drive = drive(7.0, 0.36, DriveCurve::HardClip, -1.5);
        }
        "rave-kick" => {
            model.body.drive = drive(22.0, 0.9, DriveCurve::HardClip, -4.0);
            model.master_drive = drive(17.0, 0.88, DriveCurve::Fold, -4.5);
            model.modulation.feedback = 0.58;
            model.velocity.drive = 1.2;
        }
        "machine-snare" => {
            model.body.decay_ms = 150.0;
            model.noise.decay_ms = 118.0;
            model.click.level = 0.82;
            model.master_drive = drive(10.0, 0.5, DriveCurve::Cubic, -2.0);
        }
        "body-snare" => {
            model.body.level = 1.08;
            model.body.decay_ms = 440.0;
            model.noise.level = 0.31;
            model.body.drive = drive(10.0, 0.46, DriveCurve::SoftClip, -2.0);
        }
        "noise-snare" => {
            model.noise.level = 1.22;
            model.noise.cutoff_hz = 6_700.0;
            model.noise.decay_ms = 470.0;
            model.velocity.brightness = 1.22;
        }
        "industrial-snare" => {
            model.body.drive = drive(17.0, 0.8, DriveCurve::HardClip, -3.5);
            model.noise.drive = drive(13.0, 0.7, DriveCurve::Cubic, -3.0);
            model.master_drive = drive(18.0, 0.86, DriveCurve::HardClip, -4.5);
            model.modulation.feedback = 0.4;
        }
        "acid-clap" => {
            model.noise.cutoff_hz = 3_650.0;
            model.bursts = vec![
                burst(0.0, 8.0, 1.0, -0.3),
                burst(10.0, 9.0, 0.94, 0.25),
                burst(23.0, 11.0, 0.78, -0.12),
                burst(39.0, 15.0, 0.58, 0.3),
            ];
            model.stereo.width = 0.38;
            model.noise.tail_decay_ms = 330.0;
        }
        "warehouse-clap" => {
            model.noise.cutoff_hz = 2_050.0;
            model.noise.tail_decay_ms = 820.0;
            model.bursts = vec![
                burst(0.0, 18.0, 0.76, -0.82),
                burst(22.0, 20.0, 1.0, 0.72),
                burst(49.0, 24.0, 0.86, -0.42),
                burst(86.0, 30.0, 0.65, 0.84),
                burst(128.0, 36.0, 0.46, -0.68),
            ];
            model.stereo.width = 1.0;
            model.stereo.micro_delay_ms = 1.35;
        }
        "gated-metal-hat" => {
            model.noise.decay_ms = 66.0;
            model.modulation.fm_index = 2.6;
            model.master_drive = drive(10.0, 0.48, DriveCurve::HardClip, -2.5);
        }
        "trash-open-hat" => {
            model.modulation.fm_index = 4.5;
            model.modulation.feedback = 0.28;
            model.noise.drive = drive(13.0, 0.64, DriveCurve::Fold, -2.5);
        }
        "closed-hat" => {
            model.noise.cutoff_hz = 8_100.0;
            model.modes = metallic_modes(105.0, 1);
        }
        "pedal-hat" => {
            model.noise.decay_ms = 34.0;
            model.modes = metallic_modes(65.0, 2);
            model.master_drive = drive(8.0, 0.38, DriveCurve::Cubic, -2.0);
        }
        "open-hat" => {
            model.noise.decay_ms = 920.0;
            model.noise.tail_decay_ms = 1_250.0;
            model.modes = metallic_modes(1_480.0, 3);
        }
        "fm-tom" => {
            model.modulation.fm_index = 5.8;
            model.modulation.phase_amount = 1.15;
            model.modulation.feedback = 0.3;
            model.master_drive = drive(13.0, 0.62, DriveCurve::Fold, -3.0);
        }
        "crash" => {
            model.noise.tail_decay_ms = 7_400.0;
            model.modulation.fm_index = 1.8;
        }
        "ride" => {
            model.click.level = 1.0;
            model.click.tone_hz = 6_350.0;
            model.noise.level = 0.11;
            model.modes = cymbal_modes(1);
        }
        "acid-china" => {
            model.modulation.ring_amount = 0.78;
            model.modulation.feedback = 0.24;
            model.master_drive = drive(13.0, 0.64, DriveCurve::Fold, -3.0);
        }
        "acid-zap" => {
            model.pitch = pitch(3_950.0, 2_150.0, 0.6, 82.0);
            model.modulation.fm_index = 6.2;
            model.master_drive = drive(15.0, 0.72, DriveCurve::Fold, -3.5);
        }
        "down-laser" => {
            model.pitch = pitch(-1_450.0, -3_050.0, 7.0, 185.0);
            model.modulation.phase_amount = 1.45;
            model.stereo.micro_delay_ms = 0.58;
        }
        "rim" => {
            model.click.decay_ms = 1.5;
            model.modes[1].drive = drive(8.0, 0.42, DriveCurve::HardClip, -2.0);
        }
        "cowbell" => {
            model.body.pulse_width = 0.32;
            model.modulation.ring_amount = 0.55;
            model.master_drive = drive(11.0, 0.5, DriveCurve::Cubic, -2.5);
        }
        "shaker" => {
            model.bursts.push(burst(94.0, 18.0, 0.34, -0.7));
            model.noise.cutoff_hz = 6_300.0;
            model.velocity.brightness = 1.35;
        }
        "low-tom" | "mid-tom" | "high-tom" => {
            model.body.drive.pre_gain_db += 2.0;
            model.body.drive.amount = (model.body.drive.amount + 0.12).min(1.0);
            model.velocity.pitch = 0.46;
        }
        _ => {}
    }
    model
}

fn electronic_model(id: &str, base_hz: f32) -> AdvancedModel {
    let algorithm = match id {
        "sub-kick" | "kick" | "tight-kick" | "clipped-kick" => ModelAlgorithm::Kick,
        "snare" | "body-snare" | "noise-snare" | "clipped-snare" => ModelAlgorithm::Snare,
        "house-clap" | "wide-clap" => ModelAlgorithm::Clap,
        "gated-hat" | "closed-hat" | "pedal-hat" | "open-hat" | "trash-open-hat" => {
            ModelAlgorithm::Hat
        }
        "low-tom" | "mid-tom" | "high-tom" | "zap-tom" => ModelAlgorithm::Tom,
        "crash" | "ride" | "china" => ModelAlgorithm::Cymbal,
        _ => ModelAlgorithm::Percussion,
    };
    let mut model = AdvancedModel {
        algorithm,
        oscillator: OscillatorShape::Sine,
        base_hz,
        pitch: PitchEnvelope {
            start_cents: 0.0,
            mid_cents: 0.0,
            attack_ms: 0.0,
            decay_ms: 80.0,
        },
        body: BodyLayer {
            level: 0.75,
            decay_ms: 380.0,
            pulse_width: 0.48,
            shape: 0.55,
            overtone_level: 0.12,
            overtone_ratio: 2.0,
            drive: drive(3.0, 0.2, DriveCurve::SoftClip, 0.0),
        },
        click: ClickLayer {
            level: 0.18,
            decay_ms: 8.0,
            tone_hz: 2_500.0,
            noise_mix: 0.5,
            high_pass_hz: 900.0,
            drive: drive(2.0, 0.15, DriveCurve::Cubic, 0.0),
        },
        noise: NoiseLayer {
            level: 0.08,
            attack_ms: 0.0,
            decay_ms: 90.0,
            tail_level: 0.0,
            tail_decay_ms: 200.0,
            filter: FilterMode::BandPass,
            cutoff_hz: 3_500.0,
            resonance: 0.25,
            colour: 0.0,
            drive: drive(0.0, 0.0, DriveCurve::SoftClip, 0.0),
        },
        modes: Vec::new(),
        bursts: Vec::new(),
        modulation: Modulation {
            fm_ratio: 1.7,
            fm_index: 0.0,
            phase_amount: 0.0,
            ring_ratio: 2.3,
            ring_amount: 0.0,
            feedback: 0.0,
        },
        master_drive: drive(3.0, 0.18, DriveCurve::SoftClip, -1.0),
        stereo: StereoModel {
            width: 0.0,
            micro_delay_ms: 0.0,
        },
        velocity: VelocityResponse {
            click: 0.35,
            noise: 0.25,
            drive: 0.25,
            decay: 0.18,
            brightness: 0.3,
            pitch: 0.12,
        },
        variation: SeededVariation {
            pitch_cents: 0.0,
            timing_ms: 0.0,
            level: 0.0,
            stereo: 0.0,
        },
    };
    match id {
        "sub-kick" => {
            model.pitch = pitch(1_650.0, 320.0, 2.5, 110.0);
            model.body.decay_ms = 1_100.0;
            model.body.level = 1.1;
            model.body.overtone_level = 0.04;
            model.click.level = 0.08;
            model.click.decay_ms = 12.0;
            model.noise.level = 0.015;
            model.master_drive = drive(5.0, 0.28, DriveCurve::SoftClip, -1.0);
            model.velocity.decay = 0.35;
        }
        "kick" => {
            model.oscillator = OscillatorShape::Shaped;
            model.pitch = pitch(2_450.0, 720.0, 2.0, 72.0);
            model.body.decay_ms = 520.0;
            model.body.level = 1.0;
            model.body.overtone_level = 0.2;
            model.click.level = 0.52;
            model.click.decay_ms = 7.0;
            model.click.tone_hz = 3_900.0;
            model.noise.level = 0.05;
            model.body.drive = drive(9.0, 0.42, DriveCurve::Cubic, -1.5);
            model.master_drive = drive(7.0, 0.38, DriveCurve::SoftClip, -1.0);
            model.velocity.drive = 0.58;
        }
        "tight-kick" => {
            model.oscillator = OscillatorShape::Triangle;
            model.pitch = pitch(1_350.0, 430.0, 1.0, 38.0);
            model.body.decay_ms = 175.0;
            model.body.overtone_level = 0.24;
            model.click.level = 0.7;
            model.click.decay_ms = 3.2;
            model.click.tone_hz = 5_100.0;
            model.noise.level = 0.035;
            model.master_drive = drive(4.0, 0.2, DriveCurve::HardClip, -1.0);
            model.velocity.click = 0.75;
        }
        "clipped-kick" => {
            model.oscillator = OscillatorShape::Pulse;
            model.body.pulse_width = 0.42;
            model.pitch = pitch(2_900.0, 900.0, 1.2, 95.0);
            model.body.decay_ms = 430.0;
            model.body.level = 1.25;
            model.body.overtone_level = 0.34;
            model.click.level = 0.42;
            model.noise.level = 0.08;
            model.modulation.feedback = 0.42;
            model.body.drive = drive(18.0, 0.82, DriveCurve::HardClip, -3.0);
            model.master_drive = drive(13.0, 0.78, DriveCurve::Fold, -3.5);
            model.velocity.drive = 0.9;
        }
        "snare" => configure_snare(&mut model, 0),
        "body-snare" => configure_snare(&mut model, 1),
        "noise-snare" => configure_snare(&mut model, 2),
        "clipped-snare" => configure_snare(&mut model, 3),
        "house-clap" => configure_clap(&mut model, false),
        "wide-clap" => configure_clap(&mut model, true),
        "gated-hat" => configure_hat(&mut model, 0),
        "closed-hat" => configure_hat(&mut model, 1),
        "pedal-hat" => configure_hat(&mut model, 2),
        "open-hat" => configure_hat(&mut model, 3),
        "trash-open-hat" => configure_hat(&mut model, 4),
        "low-tom" => configure_tom(&mut model, 0),
        "mid-tom" => configure_tom(&mut model, 1),
        "high-tom" => configure_tom(&mut model, 2),
        "zap-tom" => configure_tom(&mut model, 3),
        "crash" => configure_cymbal(&mut model, 0),
        "ride" => configure_cymbal(&mut model, 1),
        "china" => configure_cymbal(&mut model, 2),
        "rim" => {
            model.oscillator = OscillatorShape::Triangle;
            model.body.level = 0.24;
            model.body.decay_ms = 45.0;
            model.click.level = 0.8;
            model.click.decay_ms = 2.0;
            model.click.tone_hz = 7_200.0;
            model.modes = vec![
                mode(1.0, 0.7, 55.0, -0.15, OscillatorShape::Sine),
                mode(2.71, 0.42, 31.0, 0.18, OscillatorShape::Triangle),
                mode(5.18, 0.21, 18.0, 0.0, OscillatorShape::Sine),
            ];
            model.master_drive = drive(8.0, 0.42, DriveCurve::HardClip, -2.0);
        }
        "cowbell" => {
            model.oscillator = OscillatorShape::Pulse;
            model.body.pulse_width = 0.37;
            model.body.decay_ms = 310.0;
            model.body.level = 0.42;
            model.modes = vec![
                mode(1.0, 0.7, 360.0, -0.22, OscillatorShape::Pulse),
                mode(1.49, 0.68, 270.0, 0.22, OscillatorShape::Pulse),
            ];
            model.modulation.ring_ratio = 1.49;
            model.modulation.ring_amount = 0.42;
            model.noise.level = 0.025;
            model.master_drive = drive(8.0, 0.38, DriveCurve::Cubic, -2.0);
        }
        "zap" => {
            model.oscillator = OscillatorShape::Shaped;
            model.pitch = pitch(3_600.0, 1_900.0, 0.8, 95.0);
            model.body.decay_ms = 280.0;
            model.modulation.fm_ratio = 2.61;
            model.modulation.fm_index = 5.4;
            model.modulation.phase_amount = 0.8;
            model.modulation.feedback = 0.3;
            model.click.level = 0.2;
            model.noise.level = 0.04;
            model.master_drive = drive(12.0, 0.62, DriveCurve::Fold, -3.0);
        }
        "laser" => {
            model.oscillator = OscillatorShape::Triangle;
            model.pitch = pitch(-1_600.0, -2_800.0, 8.0, 170.0);
            model.body.decay_ms = 410.0;
            model.modulation.fm_ratio = 3.13;
            model.modulation.fm_index = 3.2;
            model.modulation.phase_amount = 1.2;
            model.modes = vec![
                mode(1.7, 0.32, 260.0, -0.3, OscillatorShape::Sine),
                mode(3.4, 0.2, 190.0, 0.3, OscillatorShape::Sine),
            ];
            model.stereo.width = 0.55;
            model.stereo.micro_delay_ms = 0.45;
        }
        "shaker" => {
            model.body.level = 0.0;
            model.click.level = 0.0;
            model.noise.level = 0.12;
            model.noise.decay_ms = 150.0;
            model.noise.filter = FilterMode::HighPass;
            model.noise.cutoff_hz = 5_800.0;
            model.noise.colour = -0.55;
            model.bursts = vec![
                burst(0.0, 12.0, 1.0, -0.6),
                burst(18.0, 9.0, 0.7, 0.5),
                burst(39.0, 11.0, 0.62, -0.2),
                burst(67.0, 15.0, 0.48, 0.6),
            ];
            model.variation.timing_ms = 2.2;
            model.variation.level = 0.12;
            model.variation.stereo = 0.7;
            model.stereo.width = 0.85;
            model.stereo.micro_delay_ms = 0.7;
        }
        _ => {}
    }
    model
}

fn configure_snare(model: &mut AdvancedModel, variant: u8) {
    model.oscillator = if variant == 1 {
        OscillatorShape::Triangle
    } else {
        OscillatorShape::Shaped
    };
    model.pitch = pitch(520.0, 120.0, 1.0, 42.0);
    model.body.decay_ms = [180.0, 390.0, 230.0, 260.0][usize::from(variant)];
    model.body.level = [0.52, 0.92, 0.34, 0.66][usize::from(variant)];
    model.body.overtone_level = [0.2, 0.32, 0.14, 0.4][usize::from(variant)];
    model.click.level = [0.68, 0.42, 0.74, 0.82][usize::from(variant)];
    model.click.decay_ms = [3.0, 7.0, 2.2, 4.0][usize::from(variant)];
    model.noise.level = [0.58, 0.38, 1.05, 0.78][usize::from(variant)];
    model.noise.decay_ms = [145.0, 260.0, 410.0, 235.0][usize::from(variant)];
    model.noise.filter = if variant == 2 {
        FilterMode::HighPass
    } else {
        FilterMode::BandPass
    };
    model.noise.cutoff_hz = [4_200.0, 2_400.0, 6_100.0, 3_700.0][usize::from(variant)];
    model.noise.resonance = [0.32, 0.5, 0.18, 0.42][usize::from(variant)];
    model.noise.colour = [0.0, 0.25, -0.38, -0.12][usize::from(variant)];
    model.modes = vec![
        mode(1.0, 0.45, 210.0, -0.18, OscillatorShape::Sine),
        mode(1.57, 0.31, 155.0, 0.16, OscillatorShape::Triangle),
        mode(2.43, 0.18, 95.0, 0.0, OscillatorShape::Sine),
    ];
    model.stereo.width = if variant == 2 { 0.52 } else { 0.25 };
    model.stereo.micro_delay_ms = if variant == 2 { 0.38 } else { 0.12 };
    model.velocity.noise = 0.72;
    model.velocity.drive = 0.65;
    if variant == 3 {
        model.body.drive = drive(14.0, 0.72, DriveCurve::HardClip, -3.0);
        model.noise.drive = drive(10.0, 0.58, DriveCurve::Cubic, -2.5);
        model.master_drive = drive(15.0, 0.78, DriveCurve::HardClip, -4.0);
        model.modulation.feedback = 0.28;
    } else {
        model.master_drive = drive(7.0, 0.35, DriveCurve::SoftClip, -1.5);
    }
}

fn configure_clap(model: &mut AdvancedModel, wide: bool) {
    model.body.level = 0.0;
    model.click.level = if wide { 0.18 } else { 0.3 };
    model.click.decay_ms = 2.0;
    model.noise.level = if wide { 0.16 } else { 0.24 };
    model.noise.decay_ms = if wide { 440.0 } else { 260.0 };
    model.noise.tail_level = if wide { 0.31 } else { 0.2 };
    model.noise.tail_decay_ms = if wide { 720.0 } else { 380.0 };
    model.noise.filter = FilterMode::BandPass;
    model.noise.cutoff_hz = if wide { 2_250.0 } else { 3_300.0 };
    model.noise.resonance = if wide { 0.5 } else { 0.35 };
    model.noise.colour = if wide { 0.3 } else { -0.08 };
    model.bursts = if wide {
        vec![
            burst(0.0, 16.0, 0.8, -0.75),
            burst(19.0, 18.0, 1.0, 0.65),
            burst(43.0, 20.0, 0.82, -0.35),
            burst(76.0, 24.0, 0.62, 0.8),
            burst(111.0, 28.0, 0.45, -0.6),
        ]
    } else {
        vec![
            burst(0.0, 10.0, 1.0, -0.25),
            burst(12.0, 11.0, 0.9, 0.2),
            burst(26.0, 13.0, 0.75, -0.1),
            burst(43.0, 17.0, 0.55, 0.25),
        ]
    };
    model.noise.drive = drive(8.0, 0.42, DriveCurve::Cubic, -2.0);
    model.master_drive = drive(7.0, 0.35, DriveCurve::SoftClip, -1.5);
    model.stereo.width = if wide { 0.95 } else { 0.42 };
    model.stereo.micro_delay_ms = if wide { 1.1 } else { 0.25 };
    model.variation.timing_ms = if wide { 2.8 } else { 1.1 };
    model.variation.level = 0.08;
    model.variation.stereo = if wide { 0.8 } else { 0.3 };
    model.velocity.noise = 0.8;
    model.velocity.brightness = 0.65;
}

fn configure_hat(model: &mut AdvancedModel, variant: u8) {
    let open = variant >= 3;
    model.body.level = 0.05;
    model.body.decay_ms = if open { 420.0 } else { 55.0 };
    model.click.level = [0.35, 0.62, 0.5, 0.34, 0.44][usize::from(variant)];
    model.click.decay_ms = [2.8, 1.4, 1.1, 3.2, 2.4][usize::from(variant)];
    model.noise.level = [0.44, 0.33, 0.22, 0.28, 0.38][usize::from(variant)];
    model.noise.decay_ms = [80.0, 95.0, 42.0, 780.0, 1_180.0][usize::from(variant)];
    model.noise.tail_level = if open { 0.18 } else { 0.0 };
    model.noise.tail_decay_ms = if variant == 4 { 1_650.0 } else { 980.0 };
    model.noise.filter = FilterMode::HighPass;
    model.noise.cutoff_hz = [6_300.0, 7_600.0, 5_700.0, 6_800.0, 4_900.0][usize::from(variant)];
    model.noise.colour = if variant == 4 { -0.5 } else { -0.22 };
    model.modes = metallic_modes(if open { 1_250.0 } else { 120.0 }, variant);
    model.modulation.fm_ratio = 1.414;
    model.modulation.fm_index = if variant == 4 { 3.8 } else { 1.8 };
    model.modulation.ring_ratio = if variant == 2 { 3.73 } else { 2.71 };
    model.modulation.ring_amount = if variant == 4 { 0.72 } else { 0.48 };
    model.noise.drive = drive(
        if variant == 4 { 11.0 } else { 5.0 },
        if variant == 4 { 0.55 } else { 0.24 },
        if variant == 4 {
            DriveCurve::Fold
        } else {
            DriveCurve::SoftClip
        },
        -2.0,
    );
    model.master_drive = drive(6.0, 0.28, DriveCurve::Cubic, -1.5);
    model.stereo.width = [0.32, 0.24, 0.16, 0.56, 0.88][usize::from(variant)];
    model.stereo.micro_delay_ms = [0.12, 0.08, 0.0, 0.35, 0.9][usize::from(variant)];
    model.variation.pitch_cents = 13.0;
    model.variation.level = 0.05;
    model.variation.stereo = 0.35;
    model.velocity.brightness = 0.9;
    model.velocity.noise = 0.55;
    model.velocity.decay = if open { 0.45 } else { 0.2 };
}

fn configure_tom(model: &mut AdvancedModel, variant: u8) {
    model.oscillator = if variant == 1 {
        OscillatorShape::Triangle
    } else if variant == 3 {
        OscillatorShape::Shaped
    } else {
        OscillatorShape::Sine
    };
    model.pitch = match variant {
        0 => pitch(920.0, 310.0, 2.0, 105.0),
        1 => pitch(760.0, 240.0, 1.5, 78.0),
        2 => pitch(610.0, 180.0, 1.0, 58.0),
        _ => pitch(2_100.0, 760.0, 1.0, 145.0),
    };
    model.body.level = [1.0, 0.9, 0.78, 0.7][usize::from(variant)];
    model.body.decay_ms = [690.0, 530.0, 360.0, 420.0][usize::from(variant)];
    model.body.overtone_level = [0.28, 0.36, 0.44, 0.55][usize::from(variant)];
    model.body.overtone_ratio = [1.97, 2.04, 2.13, 2.61][usize::from(variant)];
    model.click.level = [0.22, 0.3, 0.4, 0.34][usize::from(variant)];
    model.noise.level = [0.05, 0.07, 0.08, 0.12][usize::from(variant)];
    model.modes = vec![
        mode(1.47, 0.32, 380.0, -0.2, OscillatorShape::Sine),
        mode(2.18, 0.18, 220.0, 0.18, OscillatorShape::Triangle),
        mode(3.71, 0.09, 110.0, 0.0, OscillatorShape::Sine),
    ];
    model.body.drive = drive(
        [5.0, 8.0, 4.0, 10.0][usize::from(variant)],
        [0.22, 0.38, 0.18, 0.5][usize::from(variant)],
        if variant == 3 {
            DriveCurve::Fold
        } else {
            DriveCurve::SoftClip
        },
        -1.5,
    );
    if variant == 3 {
        model.modulation.fm_ratio = 2.73;
        model.modulation.fm_index = 4.2;
        model.modulation.phase_amount = 0.9;
        model.modulation.feedback = 0.22;
    }
    model.stereo.width = 0.32;
    model.stereo.micro_delay_ms = 0.16;
    model.velocity.pitch = 0.3;
    model.velocity.drive = 0.52;
    model.velocity.decay = 0.32;
}

fn configure_cymbal(model: &mut AdvancedModel, variant: u8) {
    model.body.level = if variant == 1 { 0.18 } else { 0.06 };
    model.body.decay_ms = [900.0, 2_300.0, 520.0][usize::from(variant)];
    model.click.level = [0.72, 0.88, 0.62][usize::from(variant)];
    model.click.decay_ms = [12.0, 4.0, 7.0][usize::from(variant)];
    model.click.tone_hz = [2_200.0, 5_900.0, 3_100.0][usize::from(variant)];
    model.noise.level = [0.46, 0.14, 0.34][usize::from(variant)];
    model.noise.decay_ms = [1_450.0, 720.0, 620.0][usize::from(variant)];
    model.noise.tail_level = [0.28, 0.12, 0.16][usize::from(variant)];
    model.noise.tail_decay_ms = [7_000.0, 6_500.0, 3_200.0][usize::from(variant)];
    model.noise.filter = if variant == 1 {
        FilterMode::BandPass
    } else {
        FilterMode::HighPass
    };
    model.noise.cutoff_hz = [3_700.0, 6_200.0, 2_800.0][usize::from(variant)];
    model.noise.resonance = [0.18, 0.62, 0.38][usize::from(variant)];
    model.noise.colour = [-0.1, 0.1, -0.42][usize::from(variant)];
    model.modes = cymbal_modes(variant);
    model.modulation.fm_ratio = [1.73, 2.31, 1.41][usize::from(variant)];
    model.modulation.fm_index = [1.4, 0.45, 3.1][usize::from(variant)];
    model.modulation.phase_amount = [0.35, 0.12, 0.7][usize::from(variant)];
    model.modulation.ring_ratio = [2.71, 3.97, 1.91][usize::from(variant)];
    model.modulation.ring_amount = [0.32, 0.18, 0.65][usize::from(variant)];
    model.master_drive = drive(
        [6.0, 4.0, 10.0][usize::from(variant)],
        [0.26, 0.14, 0.48][usize::from(variant)],
        if variant == 2 {
            DriveCurve::Fold
        } else {
            DriveCurve::SoftClip
        },
        -2.0,
    );
    model.stereo.width = [0.92, 0.56, 0.82][usize::from(variant)];
    model.stereo.micro_delay_ms = [1.25, 0.28, 0.72][usize::from(variant)];
    model.variation.pitch_cents = [22.0, 8.0, 28.0][usize::from(variant)];
    model.variation.level = 0.04;
    model.variation.stereo = 0.45;
    model.velocity.brightness = 0.78;
    model.velocity.decay = 0.34;
}

fn metallic_modes(decay_ms: f32, variant: u8) -> Vec<ResonantMode> {
    let ratios = [1.0, 1.447, 1.93, 2.617, 3.371, 4.79];
    ratios
        .into_iter()
        .enumerate()
        .map(|(index, ratio)| ResonantMode {
            ratio,
            level: (0.42 - index as f32 * 0.045)
                * if variant == 2 && index > 3 { 0.6 } else { 1.0 },
            decay_ms: decay_ms * (1.0 - index as f32 * 0.075).max(0.42),
            pan: if index % 2 == 0 { -0.72 } else { 0.72 },
            shape: if index % 3 == 0 {
                OscillatorShape::Pulse
            } else {
                OscillatorShape::Sine
            },
            drive: drive(3.0, 0.12, DriveCurve::Cubic, -1.0),
        })
        .collect()
}

fn cymbal_modes(variant: u8) -> Vec<ResonantMode> {
    let ratios: &[f32] = match variant {
        0 => &[1.0, 1.37, 1.79, 2.44, 3.11, 4.07, 5.63, 7.18],
        1 => &[1.0, 1.51, 2.05, 2.88, 3.94, 5.27],
        _ => &[1.0, 1.21, 1.83, 2.17, 3.46, 4.31, 6.72],
    };
    ratios
        .iter()
        .copied()
        .enumerate()
        .map(|(index, ratio)| {
            let base_decay = [6_500.0, 5_500.0, 2_800.0][usize::from(variant)];
            ResonantMode {
                ratio,
                level: (0.46 - index as f32 * 0.038).max(0.12),
                decay_ms: base_decay * (1.0 - index as f32 * 0.055).max(0.5),
                pan: if index % 2 == 0 { -0.82 } else { 0.82 },
                shape: if variant == 2 && index % 2 == 0 {
                    OscillatorShape::Triangle
                } else {
                    OscillatorShape::Sine
                },
                drive: drive(
                    if variant == 2 { 6.0 } else { 2.0 },
                    if variant == 2 { 0.28 } else { 0.08 },
                    DriveCurve::Cubic,
                    -1.0,
                ),
            }
        })
        .collect()
}

fn mode(ratio: f32, level: f32, decay_ms: f32, pan: f32, shape: OscillatorShape) -> ResonantMode {
    ResonantMode {
        ratio,
        level,
        decay_ms,
        pan,
        shape,
        drive: drive(0.0, 0.0, DriveCurve::SoftClip, 0.0),
    }
}

fn burst(time_ms: f32, decay_ms: f32, level: f32, pan: f32) -> NoiseBurst {
    NoiseBurst {
        time_ms,
        decay_ms,
        level,
        pan,
    }
}

fn pitch(start_cents: f32, mid_cents: f32, attack_ms: f32, decay_ms: f32) -> PitchEnvelope {
    PitchEnvelope {
        start_cents,
        mid_cents,
        attack_ms,
        decay_ms,
    }
}

fn drive(pre_gain_db: f32, amount: f32, curve: DriveCurve, post_gain_db: f32) -> DriveStage {
    DriveStage {
        pre_gain_db,
        amount,
        curve,
        post_gain_db,
    }
}

fn voice_gain_db(id: &str) -> f32 {
    match id {
        "sub-kick" => -6.0,
        "kick" | "tight-kick" | "clipped-kick" => -7.0,
        "snare" | "body-snare" | "noise-snare" | "clipped-snare" => -9.0,
        "house-clap" | "wide-clap" => -7.5,
        "closed-hat" | "pedal-hat" | "gated-hat" => -13.0,
        "open-hat" | "trash-open-hat" => -13.0,
        "crash" | "ride" | "china" => -13.0,
        "shaker" => -11.0,
        _ => -9.0,
    }
}

fn voice_outer_decay_ms(id: &str) -> f32 {
    match id {
        "sub-kick" => 1_250.0,
        "kick" | "clipped-kick" => 650.0,
        "tight-kick" => 240.0,
        "snare" => 280.0,
        "body-snare" => 520.0,
        "noise-snare" => 620.0,
        "clipped-snare" => 420.0,
        "house-clap" => 580.0,
        "wide-clap" => 980.0,
        "gated-hat" => 150.0,
        "closed-hat" => 180.0,
        "pedal-hat" => 110.0,
        "open-hat" => 1_650.0,
        "trash-open-hat" => 2_250.0,
        "low-tom" => 850.0,
        "mid-tom" => 680.0,
        "high-tom" => 480.0,
        "zap-tom" => 620.0,
        "crash" => 7_000.0,
        "ride" => 6_500.0,
        "china" => 3_500.0,
        "rim" => 150.0,
        "zap" => 420.0,
        "laser" => 620.0,
        "shaker" => 260.0,
        "cowbell" => 520.0,
        _ => 500.0,
    }
}

fn acid_voice_gain_db(id: &str) -> f32 {
    match id {
        "sub-kick-808" => -6.5,
        "acid-kick-909" | "tight-kick" | "rave-kick" => -7.5,
        "machine-snare" | "body-snare" | "noise-snare" | "industrial-snare" => -9.5,
        "acid-clap" | "warehouse-clap" => -8.0,
        "closed-hat" | "pedal-hat" | "gated-metal-hat" => -13.5,
        "open-hat" | "trash-open-hat" => -13.5,
        "crash" | "ride" | "acid-china" => -13.5,
        "shaker" => -11.5,
        _ => -9.5,
    }
}

fn acid_voice_outer_decay_ms(id: &str) -> f32 {
    match id {
        "sub-kick-808" => 1_550.0,
        "acid-kick-909" => 610.0,
        "rave-kick" => 680.0,
        "tight-kick" => 210.0,
        "machine-snare" => 270.0,
        "body-snare" => 560.0,
        "noise-snare" => 690.0,
        "industrial-snare" => 460.0,
        "acid-clap" => 560.0,
        "warehouse-clap" => 1_080.0,
        "gated-metal-hat" => 135.0,
        "closed-hat" => 170.0,
        "pedal-hat" => 95.0,
        "open-hat" => 1_850.0,
        "trash-open-hat" => 2_450.0,
        "low-tom" => 900.0,
        "mid-tom" => 710.0,
        "high-tom" => 500.0,
        "fm-tom" => 680.0,
        "crash" => 7_400.0,
        "ride" => 6_800.0,
        "acid-china" => 3_700.0,
        "rim" => 145.0,
        "acid-zap" => 430.0,
        "down-laser" => 660.0,
        "shaker" => 290.0,
        "cowbell" => 540.0,
        _ => 500.0,
    }
}

fn processing(family: Family) -> KitProcessing {
    match family {
        Family::BigRock => KitProcessing {
            high_pass_hz: 18.0,
            low_pass_hz: 19_000.0,
            saturation: 0.12,
            transient: 0.22,
            body: 0.18,
            parallel_compression: 0.18,
            room_amount: 0.0,
            room_decay: 0.2,
            output_gain_db: -5.0,
            ceiling_dbfs: -1.5,
        },
        Family::ExperimentalNoise => KitProcessing {
            high_pass_hz: 22.0,
            low_pass_hz: 14_000.0,
            saturation: 0.32,
            transient: 0.25,
            body: 0.1,
            parallel_compression: 0.28,
            room_amount: 0.0,
            room_decay: 0.2,
            output_gain_db: -7.0,
            ceiling_dbfs: -1.5,
        },
        Family::ElectronicHouse => KitProcessing {
            high_pass_hz: 15.0,
            low_pass_hz: 18_000.0,
            saturation: 0.14,
            transient: 0.28,
            body: 0.22,
            parallel_compression: 0.15,
            room_amount: 0.0,
            room_decay: 0.2,
            output_gain_db: -6.0,
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
    use shr_drums::{event_queue, DrumEngine, DrumEvent, StereoFrame};
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;
    use std::collections::BTreeSet;

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

    #[test]
    fn all_factory_families_have_distinct_bounded_processing() {
        let rock = processing(Family::BigRock);
        let noise = processing(Family::ExperimentalNoise);
        let house = processing(Family::ElectronicHouse);
        assert_ne!(rock, noise);
        assert_ne!(noise, house);
        for settings in [rock, noise, house] {
            assert!(settings.ceiling_dbfs <= -1.0);
            assert_eq!(settings.room_amount, 0.0);
            assert!(settings.saturation <= 0.32);
            assert!(settings.parallel_compression <= 0.28);
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
            Family::ExperimentalNoise,
            Family::ElectronicHouse,
        ] {
            let package = directory.join(format!("{}.shrkit", family.id()));
            let prepared =
                load_package(&package, ProjectKey::default(), &KitTuning::default()).unwrap();
            assert_eq!(prepared.manifest.kit_id, family.id());
            if matches!(family, Family::BigRock) {
                let ride = prepared
                    .manifest
                    .voices
                    .iter()
                    .find(|voice| voice.id == "ride")
                    .unwrap();
                assert_eq!(ride.trigger_note, 51);
                for id in ["closed-hat", "open-hat", "crash", "ride"] {
                    let cymbal = prepared
                        .manifest
                        .voices
                        .iter()
                        .find(|voice| voice.id == id)
                        .unwrap();
                    assert_eq!(cymbal.kind, VoiceKind::Sampled);
                    assert!(cymbal.modeled.is_none());
                }
            } else if matches!(family, Family::ElectronicHouse) {
                assert_eq!(prepared.manifest.voices.len(), 27);
                assert!(prepared.samples.is_empty());
                assert!(prepared.manifest.voices.iter().all(|voice| {
                    voice.kind == VoiceKind::Modeled
                        && voice.samples.is_empty()
                        && voice.modeled.is_none()
                        && voice.advanced_model.is_some()
                }));
            }
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn electronic_house_trigger_velocity_choke_tail_and_determinism_contract() {
        let directory = std::env::temp_dir().join(format!(
            "shr-kit-electronic-contract-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&directory);
        build_electronic_house(&directory).unwrap();
        let package = directory.join("electronic-house.shrkit");
        let kit = load_package(&package, ProjectKey::default(), &KitTuning::default()).unwrap();

        let trigger_map = kit
            .manifest
            .voices
            .iter()
            .map(|voice| (voice.trigger_note, voice.id.as_str()))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(trigger_map.len(), 27);
        for (note, id) in [
            (36, "kick"),
            (38, "snare"),
            (42, "closed-hat"),
            (45, "low-tom"),
            (46, "open-hat"),
            (47, "mid-tom"),
            (49, "crash"),
            (50, "high-tom"),
            (51, "ride"),
        ] {
            assert_eq!(trigger_map.get(&note), Some(&id));
        }
        let families = kit
            .manifest
            .voices
            .iter()
            .map(|voice| voice.family.as_str())
            .collect::<Vec<_>>();
        for (family, minimum) in [
            ("kick", 4),
            ("snare", 4),
            ("clap", 2),
            ("hat", 5),
            ("tom", 4),
            ("cymbal", 3),
        ] {
            assert!(
                families
                    .iter()
                    .filter(|candidate| **candidate == family)
                    .count()
                    >= minimum
            );
        }
        let structures = kit
            .manifest
            .voices
            .iter()
            .map(|voice| serde_json::to_vec(voice.advanced_model.as_ref().unwrap()).unwrap())
            .collect::<BTreeSet<_>>();
        assert_eq!(structures.len(), 27);

        let (sender, receiver) = event_queue();
        let mut choke_engine = DrumEngine::new(48_000, kit.clone(), receiver).unwrap();
        sender
            .push(DrumEvent::NoteOn {
                note: 46,
                velocity: 110,
            })
            .unwrap();
        choke_engine.process(&mut [StereoFrame::SILENCE; 128]);
        sender
            .push(DrumEvent::NoteOn {
                note: 42,
                velocity: 110,
            })
            .unwrap();
        choke_engine.process(&mut [StereoFrame::SILENCE; 1]);
        assert_eq!(choke_engine.active_voice_count(), 2);
        for _ in 0..8 {
            choke_engine.process(&mut [StereoFrame::SILENCE; 128]);
        }
        assert_eq!(choke_engine.active_voice_count(), 1);

        let (sender, receiver) = event_queue();
        let mut tail_engine = DrumEngine::new(48_000, kit.clone(), receiver).unwrap();
        for note in trigger_map.keys().copied() {
            sender
                .push(DrumEvent::NoteOn {
                    note,
                    velocity: 127,
                })
                .unwrap();
        }
        let ceiling = 10.0_f32.powf(kit.manifest.processing.ceiling_dbfs / 20.0);
        let callbacks = (kit.manifest.max_tail_seconds * 48_000.0 / 128.0).ceil() as usize + 1;
        let mut output = [StereoFrame::SILENCE; 128];
        for _ in 0..callbacks {
            tail_engine.process(&mut output);
            assert!(output.iter().all(|frame| {
                frame.left.is_finite()
                    && frame.right.is_finite()
                    && frame.left.abs() <= ceiling
                    && frame.right.abs() <= ceiling
            }));
        }
        assert_eq!(tail_engine.active_voice_count(), 0);
        assert!(tail_engine.diagnostics().intentional_clip_events > 0);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn acid_schema_trigger_character_runtime_and_tail_contract() {
        let directory =
            std::env::temp_dir().join(format!("shr-kit-acid-contract-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        build_acid(&directory).unwrap();
        let package = directory.join("acid.shrkit");
        let kit = load_package(&package, ProjectKey::default(), &KitTuning::default()).unwrap();

        assert_eq!(kit.manifest.kit_id, "acid");
        assert_eq!(kit.manifest.display_name, "Acid");
        assert_eq!(kit.manifest.engine.minimum, "0.2.0");
        assert_eq!(kit.manifest.voices.len(), 27);
        assert!(kit.samples.is_empty());
        assert!(kit.manifest.voices.iter().all(|voice| {
            voice.kind == VoiceKind::Modeled
                && voice.samples.is_empty()
                && voice.modeled.is_none()
                && voice.advanced_model.is_some()
        }));

        let trigger_map = kit
            .manifest
            .voices
            .iter()
            .map(|voice| (voice.trigger_note, voice.id.as_str()))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(trigger_map.len(), 27);
        for (note, id) in [
            (36, "acid-kick-909"),
            (38, "machine-snare"),
            (39, "acid-clap"),
            (42, "closed-hat"),
            (45, "low-tom"),
            (46, "open-hat"),
            (47, "mid-tom"),
            (49, "crash"),
            (50, "high-tom"),
            (51, "ride"),
        ] {
            assert_eq!(trigger_map.get(&note), Some(&id));
        }
        let families = kit
            .manifest
            .voices
            .iter()
            .map(|voice| voice.family.as_str())
            .collect::<Vec<_>>();
        for (family, exact) in [
            ("kick", 4),
            ("snare", 4),
            ("clap", 2),
            ("hat", 5),
            ("tom", 4),
            ("cymbal", 3),
        ] {
            assert_eq!(
                families
                    .iter()
                    .filter(|candidate| **candidate == family)
                    .count(),
                exact
            );
        }
        assert_eq!(
            families
                .iter()
                .filter(|candidate| **candidate == "percussion" || **candidate == "fx")
                .count(),
            5
        );
        let structures = kit
            .manifest
            .voices
            .iter()
            .map(|voice| serde_json::to_vec(voice.advanced_model.as_ref().unwrap()).unwrap())
            .collect::<BTreeSet<_>>();
        assert_eq!(structures.len(), 27);

        let (sender, receiver) = event_queue();
        let mut retrigger_engine = DrumEngine::new(48_000, kit.clone(), receiver).unwrap();
        sender
            .push(DrumEvent::NoteOn {
                note: 36,
                velocity: 90,
            })
            .unwrap();
        retrigger_engine.process(&mut [StereoFrame::SILENCE; 128]);
        sender
            .push(DrumEvent::NoteOn {
                note: 36,
                velocity: 127,
            })
            .unwrap();
        retrigger_engine.process(&mut [StereoFrame::SILENCE; 1]);
        assert_eq!(retrigger_engine.active_voice_count(), 1);

        let (sender, receiver) = event_queue();
        let mut choke_engine = DrumEngine::new(48_000, kit.clone(), receiver).unwrap();
        sender
            .push(DrumEvent::NoteOn {
                note: 46,
                velocity: 110,
            })
            .unwrap();
        choke_engine.process(&mut [StereoFrame::SILENCE; 128]);
        sender
            .push(DrumEvent::NoteOn {
                note: 42,
                velocity: 110,
            })
            .unwrap();
        choke_engine.process(&mut [StereoFrame::SILENCE; 1]);
        assert_eq!(choke_engine.active_voice_count(), 2);
        for _ in 0..8 {
            choke_engine.process(&mut [StereoFrame::SILENCE; 128]);
        }
        assert_eq!(choke_engine.active_voice_count(), 1);

        let (sender, receiver) = event_queue();
        let mut allocation_engine = DrumEngine::new(48_000, kit.clone(), receiver).unwrap();
        sender
            .push(DrumEvent::NoteOn {
                note: 34,
                velocity: 127,
            })
            .unwrap();
        let mut allocation_output = [StereoFrame::SILENCE; 128];
        assert_no_allocations(|| allocation_engine.process(&mut allocation_output));

        let (sender, receiver) = event_queue();
        let mut tail_engine = DrumEngine::new(48_000, kit.clone(), receiver).unwrap();
        for note in trigger_map.keys().copied() {
            sender
                .push(DrumEvent::NoteOn {
                    note,
                    velocity: 127,
                })
                .unwrap();
        }
        let callbacks = (kit.manifest.max_tail_seconds * 48_000.0 / 128.0).ceil() as usize + 1;
        let mut output = [StereoFrame::SILENCE; 128];
        for _ in 0..callbacks {
            tail_engine.process(&mut output);
            assert!(output.iter().all(|frame| {
                frame.left.is_finite()
                    && frame.right.is_finite()
                    && frame.left.abs() <= 10.0_f32.powf(-1.0 / 20.0)
                    && frame.right.abs() <= 10.0_f32.powf(-1.0 / 20.0)
            }));
        }
        assert_eq!(tail_engine.active_voice_count(), 0);
        assert!(tail_engine.diagnostics().intentional_clip_events > 0);

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    #[ignore = "exhaustive per-voice quality measurement; run explicitly"]
    fn electronic_house_velocity_quality_matrix() {
        let directory =
            std::env::temp_dir().join(format!("shr-kit-electronic-quality-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        build_electronic_house(&directory).unwrap();
        let kit = load_package(
            &directory.join("electronic-house.shrkit"),
            ProjectKey::default(),
            &KitTuning::default(),
        )
        .unwrap();
        assert_velocity_quality_matrix(&kit);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    #[ignore = "exhaustive per-voice quality measurement; run explicitly"]
    fn acid_velocity_quality_matrix() {
        let directory =
            std::env::temp_dir().join(format!("shr-kit-acid-quality-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        build_acid(&directory).unwrap();
        let kit = load_package(
            &directory.join("acid.shrkit"),
            ProjectKey::default(),
            &KitTuning::default(),
        )
        .unwrap();
        assert_velocity_quality_matrix(&kit);
        fs::remove_dir_all(directory).unwrap();
    }

    fn assert_velocity_quality_matrix(kit: &shr_drums::PreparedKit) {
        let ceiling = 10.0_f32.powf(kit.manifest.processing.ceiling_dbfs / 20.0);
        for voice in &kit.manifest.voices {
            let low = render_voice(kit, voice.trigger_note, 32, 2_048);
            let medium = render_voice(kit, voice.trigger_note, 80, 2_048);
            let high = render_voice(kit, voice.trigger_note, 127, 2_048);
            assert_eq!(
                medium,
                render_voice(kit, voice.trigger_note, 80, 2_048),
                "{} medium-velocity render was not deterministic",
                voice.id
            );
            for render in [&low, &medium, &high] {
                assert!(render.iter().all(|frame| {
                    frame.left.is_finite()
                        && frame.right.is_finite()
                        && frame.left.abs() <= ceiling
                        && frame.right.abs() <= ceiling
                }));
                assert!(render
                    .iter()
                    .any(|frame| frame.left.abs().max(frame.right.abs()) > 0.000_01));
            }
            let energy = |render: &[StereoFrame]| {
                render
                    .iter()
                    .map(|frame| frame.left * frame.left + frame.right * frame.right)
                    .sum::<f32>()
            };
            let low_energy = energy(&low);
            let medium_energy = energy(&medium);
            let high_energy = energy(&high);
            assert!(
                low_energy < medium_energy && medium_energy < high_energy,
                "{} velocity energy was not ordered: {low_energy}, {medium_energy}, {high_energy}",
                voice.id
            );
            let scale = high_energy.sqrt() / low_energy.max(0.000_000_1).sqrt();
            let timbre_difference = low
                .iter()
                .zip(&high)
                .map(|(low, high)| (high.left - low.left * scale).abs())
                .sum::<f32>();
            assert!(
                timbre_difference > 0.01,
                "{} velocity changed only gain",
                voice.id
            );
        }
    }

    fn render_voice(
        kit: &shr_drums::PreparedKit,
        note: u8,
        velocity: u8,
        frames: usize,
    ) -> Vec<StereoFrame> {
        let (sender, receiver) = event_queue();
        let mut engine = DrumEngine::new(48_000, kit.clone(), receiver).unwrap();
        sender.push(DrumEvent::NoteOn { note, velocity }).unwrap();
        let mut output = vec![StereoFrame::SILENCE; frames];
        engine.process(&mut output);
        output
    }
}
