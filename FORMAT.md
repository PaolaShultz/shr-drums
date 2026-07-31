# `.shrkit` format 1

A kit package is a directory with one UTF-8 `manifest.json` and the relative
WAV assets named by that manifest. Package directories conventionally end in
`.shrkit`; `shr-kit compile` enforces that suffix.

The format is strict. Unknown fields, unknown enum strings, missing required
fields, unsafe paths, invalid bounds, and unsupported format versions are
errors. A full package load also checks WAV encoding, decoded metadata,
SHA-256 hashes, and memory limits before the kit can reach the render callback.

## Package boundary

```text
example.shrkit/
  manifest.json
  samples/
    kick.wav
```

The package loader applies these limits:

- `manifest.json` is at most 1 MiB;
- every sample path is nonempty and relative, with no parent traversal;
- a resolved path, including a symlink target, must stay inside the package;
- sample WAVs are mono or stereo;
- supported encodings are integer PCM from 8 through 32 bits and 32-bit float;
- each sample file is at most 384 MiB on disk;
- declared sample rate, channel count, and frame count must match decoded data;
- each SHA-256 value has exactly 64 hexadecimal characters and must match the
  file, compared without case sensitivity;
- the kit has at most 512 sample assignments and 384 MiB of decoded stereo
  sample memory.

The decoded-memory check counts eight bytes per frame for each assignment.
Mono input is duplicated to stereo in memory.

## Field notation

Every field is required unless its row says "default" or "optional." All
floating-point bounds below are inclusive and require a finite value.

Identifiers such as `kit_id`, voice `id`, `articulation`, and `family` use 1
through 64 lowercase ASCII letters, digits, or hyphens. Display names use 1
through 64 bytes, may not be blank after trimming, and may not contain control
characters. Metadata and engine strings use the same text rules with a
1,024-byte limit.

## Top-level manifest

| Field | Type and rule |
|---|---|
| `format_version` | Integer. Must be `1`. |
| `kit_id` | Identifier. |
| `display_name` | Display text, at most 64 bytes. |
| `metadata` | Required object described below. |
| `engine` | Required compatibility object described below. |
| `max_polyphony` | Integer from 1 through 64. |
| `max_tail_seconds` | Number from 0.05 through 16. |
| `articulations` | Map from articulation name to note 0 through 127. Default: `{}`. Every key must match an articulation used by at least one voice. |
| `voices` | Array of 1 through 128 voice objects. Voice IDs and trigger notes must be unique. |
| `processing` | Required kit-bus object described below. |

`metadata` has five required text fields: `author`, `source`, `licence`,
`attribution`, and `modification_notes`. `source_hashes` is a string-to-string
map with default `{}`. The current schema does not impose a hash syntax on
that map, so package authors should record the algorithm and use full values.
[SOURCES.md](SOURCES.md) owns provenance for sample-backed public recipes.

`engine` has two required text fields: `minimum` and `maximum_exclusive`.
Current recipes use version strings such as `0.2.0` and `1.0.0`. Format
validation checks that the strings are nonempty and within the text bound; it
does not parse or compare their versions. A host that selects packages must
honor the declared range.

## Voice fields

| Field | Type and rule |
|---|---|
| `id` | Unique identifier. |
| `display_name` | Display text, at most 64 bytes. |
| `trigger_note` | Unique integer from 0 through 127. |
| `articulation` | Identifier. Every sample assignment must use the same value. |
| `family` | Identifier used for grouping. |
| `kind` | `sampled`, `modeled`, or `hybrid`. |
| `choke_group` | Optional integer. `null` or omission means no group; otherwise use 1 through 255. |
| `choke_release_ms` | Number from 0 through 500. Default: `0`. |
| `gain_db` | Number from -60 through 18. |
| `pan` | Number from -1 through 1. |
| `envelope` | Required object with `attack_ms` 0 through 1,000, `hold_ms` 0 through 10,000, `decay_ms` 0 through 30,000, and `release_ms` 0 through 30,000. |
| `samples` | Sample-assignment array. Default: `[]`. |
| `base_pitch_hz` | Optional number from 15 through 20,000. It is required when `follow_key.type` is not `excluded`. |
| `tuning_limits` | Required object. `down_cents` is -2,400 through 0; `up_cents` is 0 through 2,400. |
| `follow_key` | Required tagged object described below. |
| `modeled` | Optional legacy modeled object. |
| `advanced_model` | Optional advanced modeled object. Default: `null`. |

Voice kinds set minimum source requirements:

- `sampled` needs at least one sample assignment;
- `modeled` needs either `modeled` or `advanced_model`;
- `hybrid` needs at least one sample assignment and either model object.

The schema does not reject extra valid source data beyond those minimums.

### Follow-key objects

The supported tagged forms are:

```json
{"type": "excluded"}
{"type": "tonic"}
{"type": "scale-degree", "degree": 1, "octave": 0}
{"type": "chord-degree", "degree": 1, "octave": 0}
```

`degree` is 1 through 7 and `octave` is -2 through 2. Any rule other than
`excluded` requires `base_pitch_hz`.

## Sample assignments

Every sample entry has these required fields:

| Field | Type and rule |
|---|---|
| `path` | Relative package path with no parent traversal. |
| `velocity_min` | Integer from 1 through 127. |
| `velocity_max` | Integer from `velocity_min` through 127. |
| `round_robin` | Integer from 1 through 255. |
| `articulation` | Must equal the owning voice's articulation. |
| `gain_db` | Required number. The current schema has no separate range check; keep it finite and conservative. |
| `metadata.sample_rate` | Integer from 8,000 through 384,000. |
| `metadata.channels` | Integer `1` or `2`. |
| `metadata.frames` | Positive integer. |
| `sha256` | Exactly 64 hexadecimal characters. |

For each voice, velocity bands must begin at 1, remain contiguous, and end at
127. Within one band, round-robin numbers must be the contiguous sequence
1 through N. A duplicate velocity-minimum, velocity-maximum, and round-robin
tuple is an error.

## Legacy modeled object

The optional `modeled` object has six required fields:

| Field | Inclusive bound |
|---|---|
| `body_hz` | 15 through 8,000 |
| `body_decay_ms` | 5 through 30,000 |
| `pitch_drop_cents` | -2,400 through 2,400 |
| `noise_amount` | 0 through 1 |
| `noise_decay_ms` | 1 through 30,000 |
| `metallic_amount` | 0 through 1 |

This object remains unchanged in format 1 for existing modeled and hybrid
packages.

## Advanced modeled object

SHR Drums 0.2 adds `advanced_model` without changing the legacy object. Every
field below is required except `modes` and `bursts`, which default to empty
arrays.

Supported enum strings:

- `algorithm`: `kick`, `snare`, `clap`, `hat`, `tom`, `cymbal`,
  `percussion`;
- `oscillator` and mode `shape`: `sine`, `triangle`, `pulse`, `shaped`;
- noise `filter`: `low-pass`, `high-pass`, `band-pass`;
- drive `curve`: `soft-clip`, `hard-clip`, `cubic`, `fold`.

| Object | Fields and inclusive bounds |
|---|---|
| root | `base_hz` 15 through 20,000; required `algorithm`, `oscillator`, `pitch`, `body`, `click`, `noise`, `modulation`, `master_drive`, `stereo`, `velocity`, and `variation`; `modes` has at most 12 entries; `bursts` has at most 8. |
| `pitch` | `start_cents` and `mid_cents` -4,800 through 4,800; `attack_ms` 0 through 500; `decay_ms` 0.1 through 5,000. |
| `body` | `level` 0 through 4; `decay_ms` 1 through 30,000; `pulse_width` 0.05 through 0.95; `shape` 0 through 1; `overtone_level` 0 through 2; `overtone_ratio` 0.25 through 16; required `drive`. |
| `click` | `level` 0 through 4; `decay_ms` 0.1 through 500; `tone_hz` 20 through 20,000; `noise_mix` 0 through 1; `high_pass_hz` 5 through 20,000; required `drive`. |
| `noise` | `level` 0 through 4; `attack_ms` 0 through 2,000; `decay_ms` 0.1 through 30,000; `tail_level` 0 through 4; `tail_decay_ms` 0.1 through 30,000; required `filter`; `cutoff_hz` 20 through 20,000; `resonance` 0 through 0.98; `colour` -1 through 1; required `drive`. |
| each `mode` | `ratio` 0.05 through 64; `level` 0 through 4; `decay_ms` 0.1 through 30,000; `pan` -1 through 1; required `shape` and `drive`. |
| each `burst` | `time_ms` 0 through 500; `decay_ms` 0.1 through 1,000; `level` 0 through 4; `pan` -1 through 1. Entries must be ordered by nondecreasing `time_ms`. |
| `modulation` | `fm_ratio` 0 through 32; `fm_index` 0 through 20; `phase_amount` 0 through 4; `ring_ratio` 0 through 32; `ring_amount` 0 through 1; `feedback` -0.95 through 0.95. |
| any `drive` | `pre_gain_db` -24 through 36; `amount` 0 through 1; required `curve`; `post_gain_db` -36 through 24. This applies to body, click, noise, every mode, and `master_drive`. |
| `stereo` | `width` 0 through 1; `micro_delay_ms` 0 through 2. |
| `velocity` | `click`, `noise`, `drive`, `decay`, `brightness`, and `pitch` are each -1 through 2. |
| `variation` | `pitch_cents` 0 through 100; `timing_ms` 0 through 5; `level` 0 through 0.5; `stereo` 0 through 1. |

Each algorithm also needs an audible source structure:

- `kick`: body level and click level are both above zero;
- `snare`: at least two modes and noise level above zero;
- `clap`: at least three bursts and noise level above zero;
- `hat`: at least three modes and noise level above zero;
- `tom`: body level above zero and at least two modes;
- `cymbal`: at least five modes and noise tail level above zero;
- `percussion`: at least one active body, click, noise, mode, or burst source.

An advanced voice hard-retriggers its own previous oscillator and noise state.
Cross-voice choke groups still use `choke_release_ms`. Sampled and legacy
hybrid voices keep their existing overlap behavior.

## Kit processing

All processing fields are required:

| Field | Inclusive bound |
|---|---|
| `high_pass_hz` | 5 through 500 |
| `low_pass_hz` | 1,000 through 24,000 and greater than `high_pass_hz` |
| `saturation` | 0 through 1 |
| `transient` | -1 through 1 |
| `body` | -1 through 1 |
| `parallel_compression` | 0 through 1 |
| `room_amount` | 0 through 1 |
| `room_decay` | 0 through 1 |
| `output_gain_db` | -24 through 12 |
| `ceiling_dbfs` | -12 through -0.5 |

`room_amount` and `room_decay` are required compatibility fields. The
in-process engine validates and preserves them but does not render them. New
factory packages set `room_amount` to zero. SHR-DAW owns drum reverb and tempo
delay as separate Project effects.

Voice-local clipping and kit saturation are authored colour stages. The kit
ceiling remains the final finite host-facing protection.

## Runtime key and tuning data

Project key and tuning data are host inputs, not fields in `manifest.json`.
Their public enum strings are:

- Project mode: `major` or `natural-minor`;
- tuning mode: `off`, `follow-key`, or `manual`.

The Project tonic is 0 through 11. Manual tuning may name at most 128 known
voice IDs. Each piece may set `target_pitch_class` to `null` or 0 through 11
and `cents_adjustment` to -2,400 through 2,400. The engine clamps the requested
result to that voice's `tuning_limits`.

`off` uses authored pitch. `follow-key` applies the voice's `follow_key` rule
to the host Project key. `manual` applies the selected pitch class and cents
offset for each named piece.

## Compatibility

Omitting `choke_release_ms` loads it as `0`. Omitting `advanced_model` loads it
as absent. This keeps existing format 1 packages readable in SHR Drums 0.2.
Advanced factory recipes declare an engine minimum of `0.2.0`. A strict 0.1
reader does not know `advanced_model` and refuses the package instead of
substituting the legacy oscillator.

The current reader accepts format version 1 only. A newer format version or an
unknown field is an error. The defaults above are specific compatibility rules,
not general forward compatibility.

## Small valid manifest

This one-voice example is the public schema test fixture in JSON form. It has
no sample assets. Replace its metadata and licence with truthful package data.

```json
{
  "format_version": 1,
  "kit_id": "test-kit",
  "display_name": "Test Kit",
  "metadata": {
    "author": "Test",
    "source": "Generated fixture",
    "licence": "CC0-1.0",
    "attribution": "None",
    "modification_notes": "Generated",
    "source_hashes": {}
  },
  "engine": {
    "minimum": "0.1.0",
    "maximum_exclusive": "1.0.0"
  },
  "max_polyphony": 8,
  "max_tail_seconds": 2.0,
  "articulations": {},
  "voices": [
    {
      "id": "kick",
      "display_name": "Kick",
      "trigger_note": 36,
      "articulation": "hit",
      "family": "kick",
      "kind": "modeled",
      "choke_group": null,
      "choke_release_ms": 0.0,
      "gain_db": -6.0,
      "pan": 0.0,
      "envelope": {
        "attack_ms": 0.1,
        "hold_ms": 2.0,
        "decay_ms": 500.0,
        "release_ms": 30.0
      },
      "samples": [],
      "base_pitch_hz": 32.703,
      "tuning_limits": {
        "down_cents": -1200,
        "up_cents": 1200
      },
      "follow_key": {
        "type": "tonic"
      },
      "modeled": {
        "body_hz": 32.703,
        "body_decay_ms": 500.0,
        "pitch_drop_cents": 600.0,
        "noise_amount": 0.05,
        "noise_decay_ms": 15.0,
        "metallic_amount": 0.0
      },
      "advanced_model": null
    }
  ],
  "processing": {
    "high_pass_hz": 10.0,
    "low_pass_hz": 20000.0,
    "saturation": 0.1,
    "transient": 0.0,
    "body": 0.0,
    "parallel_compression": 0.0,
    "room_amount": 0.0,
    "room_decay": 0.2,
    "output_gain_db": -6.0,
    "ceiling_dbfs": -1.0
  }
}
```

Use [`shr-kit validate`](README.md#shr-kit-command-reference) for schema-only
or full-package validation.
