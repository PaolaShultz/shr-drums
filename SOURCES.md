# Sample source provenance

This file owns provenance for sample-backed SHR Drums packages. Large
archives, extracted sources, compiled `.shrkit` directories, and review output
belong below ignored `user/`. They are not tracked source files. The explicitly
cleared Power Studio package is also published as a GitHub release attachment;
its recipe, notices and per-file hashes are tracked here.

## MuldjordKit 2020-10-18

The current acoustic importer uses MuldjordKit:

- original drum recordings: Lars Muldjord;
- stereo SFZ and WAV assembly: roberto@zenvoid.org for FreePats;
- upstream release:
  <https://github.com/freepats/muldjordkit/releases/tag/2020-10-18>;
- archive filename: `MuldjordKit-SFZ+WAV-20201018.7z`;
- archive SHA-256:
  `b18dd10d8eab2b812a6624f25d4e05d5ecbf9d44114557b77ba62323472bddfe`;
- licence: Creative Commons Attribution 4.0 International, CC BY 4.0;
- upstream licence and kit pages:
  <https://freepats.zenvoid.org/Percussion/acoustic-drum-kit.html> and
  <https://drumgizmo.org/wiki/doku.php?id=kits:muldjordkit>.

The output manifest records these extracted-source hashes:

| Source file | SHA-256 |
|---|---|
| `MuldjordKit 20201018.sfz` | `410e17d984b52ab9323b103324ea99e8021cc7631469d81b772ff6d1c4c0f93b` |
| `README.txt` | `d3d69df4db7bd93b568af9cbc901a9ac00bf17a8f98b9229787bd6023cc4d665` |
| `LICENSE.txt` | `9ba9550ad48438d0836ddab3da480b3b69ffa0aac7b7878b5a0039e7ab429411` |

Verify the archive before import. The importer checks for the expected source
layout, but it does not recompute these source-file hashes before building.

The importer creates local Big Rock and Experimental Noise packages. It
selects kick, snare, three toms, closed and open hats, crash, and ride. Each
piece receives three contiguous velocity bands with two deterministic round
robins. The importer renames and hashes copied WAVs, adds modeled bodies to
tunable pieces, preserves selected acoustic cymbals as sampled attacks, and
copies the upstream licence and README into each package. It does not normalize
the recordings.

See the [`import-muldjord` command](README.md#shr-kit-command-reference) for
arguments and refusal behavior.

## VCSL

The Versilian Community Sample Library is a possible CC0 source for later
accessory percussion:
<https://github.com/sgossner/VCSL>.

VCSL is not used by the current importer, recipes, or packages. No VCSL file or
derived asset is distributed by this repository.

## Power Studio 1.0

The [Power Studio recipe](packs/power-studio.recipe.json) locks every selected
Muldjord and Virtuosity source by SHA-256. The builder verifies these before
writing any package. Source identities, modifications, attribution and the
per-file CC BY 4.0 / CC0 distinction are in
[the distributed credits](packs/power-studio/CREDITS.md). This includes original
CC0 synthesized percussion; it does not relabel Muldjord audio as CC0.

Virtuosity Drums is by Versilian Studios and Karoryfer Samples, performed by
Austin McMahon at Virtuosity Musical Instruments in Boston. The selected
recordings are pinned to `9f04cf9a734527edfbb0a4eee1f674e45bbf71bc` from
https://github.com/sfzinstruments/virtuosity_drums under CC0 1.0. Its complete
LICENSE and README are hash-locked and copied into the package. Only the
specified overhead drum/cymbal files are used, not its auxiliary-percussion bank.

[Build, validation and public pack](packs/POWER_STUDIO.md).
