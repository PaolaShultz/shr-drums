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

Trigger notes always remain GM percussion notes. OFF uses authored tuning.
FOLLOW KEY uses each voice's authored rule and the host Project tonic/mode.
MANUAL applies a per-piece pitch-class target and cents offset. Broadband
voices use `excluded` unless a kit author explicitly supplies a reviewed
pitched rule in a future compatible format.

Format evolution is strict: a newer format or an unknown field is an error,
not permission to invent live behavior.
