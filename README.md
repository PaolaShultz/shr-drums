# SHR Drums

SHR Drums is the in-process drum engine and offline kit toolchain used by
SHR-DAW. The live library has no JACK or application-process ownership. It
accepts bounded timestamp-free MIDI-style events and renders deterministic
stereo blocks from a fully validated, preloaded kit.

`shr-kit` validates source manifests, checks sample metadata and hashes, and
compiles directory packages ending in `.shrkit`. Large source archives and
compiled packages belong below ignored `user/`, not in Git.

```sh
cargo run -p shr-kit -- validate recipes/electronic-house.json
cargo run -p shr-kit -- factory user/dist
```

The factory command produces three local review packages. The two acoustic
recipes are deliberately replaceable foundations: importing cleared acoustic
samples keeps their attack and room while modeled body resonators provide
precise tuning.

See [the package contract](FORMAT.md) and [sample provenance](SOURCES.md).
