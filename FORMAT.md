# `.shrkit` format 1

A package is a directory whose name ends in `.shrkit`. It contains one
UTF-8 `manifest.json` and the relative assets named by that manifest.
`shr-kit validate` rejects unknown fields, invalid bounds, unsafe paths,
unsupported WAV encodings, metadata mismatches, and SHA-256 mismatches before
the package can be prepared for the render callback.

Format 1 records stable kit and voice IDs, display and provenance metadata,
engine compatibility, GM trigger and articulation mappings, velocity and
round-robin assignments, choke groups, envelopes, gain and pan, reviewed base
pitch, bounded tuning and FOLLOW KEY rules, modeled/hybrid parameters, bus
processing defaults, WAV metadata, and per-asset integrity hashes.

SHR Drums 0.2 adds an optional strict `advanced_model` voice object while
retaining the original `modeled` object unchanged. Absence of
`advanced_model` and `choke_release_ms` migrates in memory to the legacy model
and an immediate choke, so existing Big Rock, Experimental Noise, and Project
kit/tuning references remain readable. New advanced voices declare a minimum
engine of 0.2.0; an older strict engine refuses their unknown model instead of
silently substituting the legacy oscillator.

The advanced object is a bounded modeled graph: algorithm and oscillator
identity; a two-stage pitch envelope; independent body, click, coloured-noise,
mode, and tail layers; fixed-count resonant modes and noise bursts; FM, phase,
ring, and feedback modulation; low/high/band-pass noise filtering; per-layer
and master drive curves; velocity-to-character response; seeded variation; and
bounded stereo spread/micro-delay. Every numeric field, mode/burst count, choke
release, feedback path, tail, and output is validated before preparation.
Retriggering one advanced voice replaces its prior oscillator/noise instance,
matching electronic drum-machine behavior and bounding repeated-hit work.
Cross-voice choke groups still use the authored release. Legacy sampled and
hybrid packages retain their existing overlap behavior.
Voice-local clipping and kit saturation are intentional colour stages. The
separate kit ceiling remains the final finite host-facing protection.
The Acid factory kit exercises the same graph without adding samples or a
melodic bass voice: all 27 attacks, bodies, metallic tails, timed clap/shaker
bursts, tuned drums, and FX are rendered by the callback from validated model
parameters.

`room_amount` and `room_decay` remain required compatibility fields in format
1, but the in-process engine no longer renders them. New factory packages set
them to zero. SHR-DAW owns ambience and delay as separate, Project-persisted
effects so an audible repeating delay is never presented as room sound.

Trigger notes always remain GM percussion notes. OFF uses authored tuning.
FOLLOW KEY uses each voice's authored rule and the host Project tonic/mode.
MANUAL applies a per-piece pitch-class target and cents offset. Broadband
voices use `excluded` unless a kit author explicitly supplies a reviewed
pitched rule in a future compatible format.

Format evolution is strict: a newer format or an unknown field is an error,
not permission to invent live behavior. Optional fields documented above have
explicit safe legacy defaults; they are not generic forward compatibility.
