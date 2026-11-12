# Power Studio pack

Power Studio 1.0 is a public 30-voice sample pack: 171 stereo PCM24 WAVs at
48 kHz, about 167.3 MiB decoded sample memory, and a format-1 `.shrkit` manifest.
The runtime engine is unchanged; it works with SHR Drums 0.2.1.

[Download the pack](https://github.com/PaolaShultz/shr-drums/releases/tag/power-studio-v1.0.0)
· [Note map and use](power-studio/README.md)
· [Credits and licences](power-studio/CREDITS.md)
· [Locked recipe](power-studio.recipe.json)

## Sources and processing

Muldjord supplies the rock drums, two crashes, two rides, both ride bells and
china. Virtuosity supplies the studio/sizzle/flat-ride crashes, side-stick,
rimshot and additional hat articulations. Synthesized clap, splash, tambourine,
cowbell and kick reinforcement are original CC0 material. The pack is a mix
of CC BY 4.0 and CC0, not a blanket CC0 release. Every WAV has a source/licence
record in `PROVENANCE.json`; full upstream notices travel with the pack.

The recipe selects Muldjord takes from the upstream SFZ velocity bands at
velocities 32, 80 and 120. Virtuosity selections retain its documented layer
ordering. Most voices have three bands and two alternating takes. Side-stick,
rimshot and flat-ride crash have three selected layers with one take each;
these sources use dynamic layers rather than round robins. Selection and every
source-file SHA-256 are explicit in the recipe.

The offline builder uses a common reference gain per voice to retain relative
layer and take dynamics. Drums receive stereo-linked 8:1 RMS parallel
compression with a preserved dry attack, then saturation. Cymbals receive
milder peak shaping. Source stereo is retained; there is no artificial room
baked onto cymbals. Samples are trimmed with endpoint fades. Render-time voice
holds preserve the baked envelope, and hats share a short choke release.
Kit-bus processing remains bounded. It is not tempo-synchronized sidechain
pumping; Project effects can provide additional treatment.

## Reproduce without audio hardware

Use Python 3.11 and Rust 1.97.1. Pinned Python packages are kept in an isolated
environment. `fetch` downloads only the SHA-verified Muldjord archive and pinned
Virtuosity files. It needs `7z`; mismatched cached sources are refused.
`build` is offline, checks every source hash before creating output, refuses
existing output and removes its own incomplete package on failure.

```sh
python3.11 -m venv user/power-tools
user/power-tools/bin/pip install -r scripts/power-studio-requirements.txt
user/power-tools/bin/python scripts/build_power_studio.py fetch --sources user/power-sources
user/power-tools/bin/python scripts/build_power_studio.py build \
  --sources user/power-sources --output user/kits/power-studio.shrkit
cargo +1.97.1 run --locked -p shr-kit -- validate user/kits/power-studio.shrkit
cargo +1.97.1 run --locked --release -p shr-kit -- \
  review-power-studio user/kits/power-studio.shrkit user/power-review
user/power-tools/bin/python -m unittest discover -s scripts -p 'test_*.py'
```

`review-power-studio` requires the complete note map, refuses an existing output
path, and writes isolated voice renders, a full-kit phrase and measurements.
It checks finite bounded output, rising velocity energy and exact repeat
renders through the actual engine. It does not play sound or access devices.
Review output is private test material, not part of the release asset.

Two builds from the verified sources with the pinned tools produce identical
package files on the development x86_64 Linux host. The production-engine review
passes all 30 voices at three velocities, including repeated hits. These checks
establish software integrity, not listening preference or Raspberry Pi timing.

## Publication

The Git repository holds the recipe, build software, source hashes, credits,
usage guide and tests. The compiled package is a release attachment, not a Git
source directory. The tarball contains one `power-studio.shrkit/` root with all
WAVs, manifest, provenance, usage, source notices and licences. The adjacent
SHA256SUMS authenticates the downloaded bytes against the published digest.
SHR-DAW separately allowlists and ships the same package bytes.
