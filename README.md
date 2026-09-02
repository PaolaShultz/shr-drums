# SHR Drums

SHR Drums is the in-process drum engine used by
[SHR-DAW](https://github.com/PaolaShultz/shr-daw). It also provides the
offline `shr-kit` package builder and validator.

The repository source, documentation, and tracked recipes are released under
the [MIT licence](LICENSE). Each compiled kit has its own content licence and
provenance in `manifest.json`. The generated modeled recipes declare CC0-1.0;
imported sample packages keep their source licence. See
[Sample source provenance](SOURCES.md) before distributing a kit.

## Engine and host boundary

The live library loads and validates one `.shrkit` package before rendering. It
accepts timestamp-free drum events through a bounded queue and produces
deterministic stereo blocks. The callback path does not allocate, lock, read
files, or own an application process.

The host owns event timing, MIDI and JACK connections, process lifecycle,
Project storage, kit selection, and cleanup. SHR-DAW also owns drum ambience
and tempo delay. The kit bus is limited to filtering, transient and body
shaping, parallel compression, saturation, gain, and output protection.

SHR-DAW pins an exact SHR Drums Git revision in its `Cargo.toml` and compiles
the library into `shr`; it never starts a separate drum service. SHR-DAW also
owns the cleared compiled-kit allowlist, installation paths, Pattern routes,
and failure isolation around this library. This repository owns format 1,
engine bounds, recipe/build tools, and source provenance. A kit shipped by
SHR-DAW is governed by SHR-DAW's allowlist and its package manifest, not merely
by the set of recipe commands listed below.

Electronic House and Acid are fully modeled 27-voice kits with no sample
assignments. Big Rock and Experimental Noise are replaceable acoustic
foundations. Their factory versions use generated attacks; the Muldjord
importer can build sample-backed review packages from the cleared source
described in [SOURCES.md](SOURCES.md).

## Rust and workspace checks

This workspace declares Rust 1.85 as its minimum accepted compiler. It has no
repository toolchain pin, so standalone commands use the active Rust toolchain
when it meets that minimum. Integrated SHR-DAW builds use the exact toolchain
pinned by the SHR-DAW repository.

Normal source checks are:

```sh
cargo check --locked
cargo test --locked
cargo build --locked -p shr-kit
```

Keep source archives, compiled packages, and review output below ignored
`user/`. None of the commands below opens JACK, sends MIDI, starts a synth, or
plays audio.

## `shr-kit` command reference

Use this prefix from the workspace root:

```sh
cargo run --locked -p shr-kit -- COMMAND [ARGUMENTS]
```

| Command | Result and refusal behavior |
|---|---|
| `factory <output-directory>` | Creates `big-rock.shrkit`, `experimental-noise.shrkit`, and `electronic-house.shrkit`. The first two contain generated 48 kHz stereo attack WAVs; Electronic House is fully modeled. Each package refuses replacement when reached. Earlier successful packages remain after a later failure. A failed acoustic package is removed, but a late Electronic House failure can leave its new directory incomplete. |
| `electronic-house <output-directory>` | Creates one fully modeled `electronic-house.shrkit` with 27 voices and no sample files. It refuses to replace that package directory. A failure after directory creation can leave the new package incomplete. |
| `acid <output-directory>` | Creates one fully modeled `acid.shrkit` with 27 voices and no sample files. It refuses to replace that package directory. A failure after directory creation can leave the new package incomplete. |
| `compile <manifest.json> <output.shrkit>` | Validates the source manifest, copies every referenced sample relative to the manifest, writes `manifest.json`, then performs a full package load. The output must end in `.shrkit` and must not exist. A failed copy or load removes the newly created output directory. |
| `validate <manifest.json\|kit.shrkit>` | A file argument validates JSON and schema only. A directory argument performs a full package load with the default Project key and tuning, including paths, WAV decoding, metadata, hashes, and decoded-memory bounds. It writes nothing. |
| `review-electronic-house <kit.shrkit> <output-directory>` | Requires a fully modeled Electronic House package with the expected kit ID, at least 24 voices, an advanced model on every voice, and no samples. It refuses an existing output path. It creates offline review WAVs plus `measurements.tsv`, `VOICE_STRUCTURES.md`, and `REPORT.md`. A later error can leave partial new output. |
| `review-acid <kit.shrkit> <output-directory>` | Applies the same review rules to the Acid kit and creates the same kinds of review WAVs and reports. It refuses an existing output path; a later error can leave partial new output. |
| `import-muldjord <extracted-source> <output-directory>` | Requires the extracted `LICENSE.txt`, `README.txt`, `MuldjordKit 20201018.sfz`, and `samples/` paths. It creates `big-rock-muldjord.shrkit` and `experimental-noise-muldjord.shrkit`, including copied source notices and selected WAVs. Each package refuses replacement when reached and removes its own partial directory after a build failure. If the second package fails or exists, the first may remain. Verify the source archive against [SOURCES.md](SOURCES.md) before running it. |
| `analyze-pitch <sample.wav>` | Reads up to four seconds from a mono or stereo WAV, skips the first 80 ms, scans 20 through 1000 Hz, and prints a dominant body-pitch candidate with relative power. It refuses unsuitable channels, short input, decode errors, or an unstable result. It writes nothing, and the printed value still needs review before use in a manifest. |

Only `review-electronic-house` and `review-acid` create audition and comparison
WAV sets. `factory` and `import-muldjord` may create sample WAVs inside kit
packages, but those files are package assets rather than review renders.

Invalid command shapes print the complete usage list and exit with status 2.
There is no separate successful `--help` path.

The package fields, bounds, enums, and compatibility rules are in
[`.shrkit` format 1](FORMAT.md).
