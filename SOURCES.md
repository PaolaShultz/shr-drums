# Sample source provenance

Large archives, extracted sources, and compiled `.shrkit` packages are local
review material below ignored `user/`. They are not part of the source
repository.

## MuldjordKit 2020-10-18

- Original drum recordings: Lars Muldjord.
- Stereo SFZ/WAV assembly: roberto@zenvoid.org for FreePats.
- Upstream release:
  `https://github.com/freepats/muldjordkit/releases/tag/2020-10-18`
- Downloaded archive:
  `MuldjordKit-SFZ+WAV-20201018.7z`
- Archive SHA-256:
  `b18dd10d8eab2b812a6624f25d4e05d5ecbf9d44114557b77ba62323472bddfe`
- Licence: Creative Commons Attribution 4.0 International (CC BY 4.0).
- Upstream licence and kit pages:
  `https://freepats.zenvoid.org/Percussion/acoustic-drum-kit.html` and
  `https://drumgizmo.org/wiki/doku.php?id=kits:muldjordkit`.

The importer selects velocity layers and round robins, renames copied WAV
files, adds reviewed resonance metadata and modeled body layers to tunable
shells, and preserves acoustic hats, crashes, and rides as sampled one-shots.
It also preserves the upstream licence and README in each local package. It
does not normalize the source hits.

Reproduce after placing and extracting the verified archive below
`user/downloads/`:

```sh
cargo run --locked -p shr-kit -- import-muldjord \
  "user/downloads/muldjord-20201018/MuldjordKit SFZ+WAV-20201018" \
  user/dist
```

## VCSL

Versilian Community Sample Library is a possible CC0 source for later
accessory-percussion coverage:
`https://github.com/sgossner/VCSL`. It is not included in the first local
packages, so no VCSL asset or derived sample is distributed by this slice.
