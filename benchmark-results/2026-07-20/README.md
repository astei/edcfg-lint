# Benchmark artifacts — 2026-07-20

This directory contains the raw Hyperfine JSON retained from the M1 Max/macOS
and DigitalOcean Xeon/Linux experiments. These files are canonical copies of
the temporary benchmark output and should be kept unchanged so published
statistics remain auditable.

## Layout

```text
corrected-optimization/
  macos/              C0–C4 corrected-stage matrices and C5 controls
  linux/              C0–C5 corrected-stage matrices

roslyn-reference/
  macos/              primary profiles, thread scaling, and comparator
  linux/              primary profiles, thread scaling, and comparator
```

The corrected-optimization results use:

- Home Assistant commit `fc281b2faecdc86c0a716dfb4a1f243dde632297`;
- Roslyn commit `104895c7c0c783f5d7c43f05631364eabc6e6d88`;
- `edcfg-lint` source anchor
  `51efae64b620a0c0c0dc76c001dc85adf7405098`;
- three Hyperfine warmups and 20 measured runs per command; and
- full diagnostic-output equivalence before timing.

The untouched Home Assistant control was repeated on macOS after its first
20-run series contained large outliers. Both original series are retained.

See [`HOME_ASSISTANT_BENCHMARK.md`](../../../HOME_ASSISTANT_BENCHMARK.md) for
the corrected-stage experiment and [`ROSLYN_BENCHMARK.md`](../../../ROSLYN_BENCHMARK.md)
for the original Roslyn reference/comparator experiment.

When the Asahi Linux run is performed, store its raw JSON under
`corrected-optimization/asahi-linux/` and add the files to `SHA256SUMS`.

## Integrity

`SHA256SUMS` contains paths relative to this directory. Verify all retained
results from this directory with:

```bash
shasum -a 256 -c SHA256SUMS
```

All 15 JSON files were also parsed successfully with `jq` when archived.
