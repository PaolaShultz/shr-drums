# SHR Drums

SHR Drums is the in-process drum engine and offline kit toolchain used by
SHR-DAW. The live library has no JACK or application-process ownership. It
accepts bounded timestamp-free MIDI-style events and renders deterministic
stereo blocks from a fully validated, preloaded kit.

`shr-kit` validates source manifests, checks sample metadata and hashes, and
compiles directory packages ending in `.shrkit`. Large source archives and
compiled packages belong below ignored `user/`, not in Git.

```sh
cargo run --locked -p shr-kit -- factory user/dist
cargo run --locked -p shr-kit -- electronic-house user/private-review
cargo run --locked -p shr-kit -- acid user/private-review
cargo run --locked -p shr-kit -- validate \
  user/private-review/acid.shrkit
```

The factory command produces three local review packages. The two acoustic
recipes are deliberately replaceable foundations: importing cleared acoustic
samples keeps their attack while modeled body resonators provide precise
tuning. Electronic House and Acid are fully modeled 27-voice packages with no
sample assignments. Acid uses the same bounded graph primitives for a harder
acid-house/acid-techno palette; it is a drum kit, not a bass synthesizer.
Ambience and tempo delay are separate Project effects in SHR-DAW; the library
bus provides only bounded filtering, transient/body shaping, parallel
compression, saturation, gain, and output protection.

See [the package contract](FORMAT.md) and [sample provenance](SOURCES.md).
