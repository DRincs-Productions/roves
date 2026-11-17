Packed packaged layout (`--content-compress auto`, the `mach bundle` default)
for the probe's no-argument entry point (`src/launch.rs`): `launch.json` names
`dist/`, which holds `manifest.json` and zstd-compressed `.pack` archives:
`__boot__.pack` (entry document and module) and `data.pack` (`data/lazy.json`,
not named by the entry document, extracted on first access). Generated from
`../bundle-loose/dist` with the shared packer, as `mach bundle` runs it; the
test `launch::tests::packed_fixture_matches_the_loose_sources` fails when the
two drift. To regenerate, from the repository root:

```sh
cargo run --locked -p roves-content-packer -- pack \
  --input support/blitz-v8-probe/fixtures/bundle-loose/dist \
  --output support/blitz-v8-probe/fixtures/bundle-packed/dist \
  --level 1 --max-pack-size 500M --html-file game.html \
  --name "Roves bundle fixture"
```
