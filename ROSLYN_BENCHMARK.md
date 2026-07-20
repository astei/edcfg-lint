# Roslyn benchmark

This document defines a reproducible `edcfg-lint` benchmark using
[`dotnet/roslyn`](https://github.com/dotnet/roslyn). Roslyn is large, contains
many file types, and has a root `.editorconfig` plus numerous nested
`.editorconfig` files.

## Reference corpus

- Repository: `https://github.com/dotnet/roslyn.git`
- Commit: `104895c7c0c783f5d7c43f05631364eabc6e6d88`
- Commit date: 2026-07-18 04:38:57 UTC
- Commit subject: `[main] Update dependencies from dotnet/arcade (#84554)`
- Checkout size: 576 MiB on the reference machine
- Tracked files: 32,378
- Distinct tracked directories: 4,687
- `.editorconfig` files: 37
- Root `.editorconfig`: 316 lines, 13,087 bytes

As an approximate source-corpus description, `scc 3.6.0` recognized 24,836
source/text files containing 9,423,043 lines and 376,018,893 bytes. The largest
groups were 16,931 C# files, 3,643 Visual Basic files, and 2,638 plain-text
files.

The root configuration exercises several relevant paths:

- `indent_style = space` applies to all files.
- C#, C# script, Visual Basic, and Visual Basic script files specify
  `insert_final_newline = true` and `charset = utf-8-bom`.
- Shell scripts specify `end_of_line = lf`.
- Brace alternatives and anchored recursive globs are used throughout.
- Many .NET-specific properties are parsed but intentionally ignored by
  `edcfg-lint`.
- Nested configuration files exercise config discovery, relative glob
  matching, inheritance, `root`, and the parsed-config cache.

## Reference implementation

- `edcfg-lint` commit: `51efae64b620a0c0c0dc76c001dc85adf7405098`
- Version: 0.1.0
- Cargo profile: `release-lto`
- Reference binary SHA-256:
  `a81f433b232e3996c7226e9a9592ed57b9d2cc96622a963243037880e364fa86`
- Rust: `rustc 1.97.1 (8bab26f4f 2026-07-14)`

## Reference machine

- Date: 2026-07-20
- Model: 16-inch MacBook Pro (`MacBookPro18,2`)
- CPU: Apple M1 Max
- Cores: 10 (8 performance, 2 efficiency)
- Memory: 64 GB
- OS: macOS 26.5, build 25F71
- Architecture: arm64
- Filesystem/storage: APFS on internal Apple SSD/NVMe storage
- Hyperfine: 1.20.0
- Comparison tool: editorconfig-checker 3.8.0

The timed runs were warm-cache, steady-state measurements. Hyperfine performed
warmups before collecting samples. They are not cold-start filesystem
measurements.

## Correctness and corpus smoke checks

Default traversal:

```text
Checked 31830 files, 12125 failed
```

Fuller traversal (`--hidden --max-file-size 0`):

```text
Checked 31994 files, 12161 failed
```

The nonzero exit status is expected. A major source of findings is Roslyn's
`charset = utf-8-bom` rule: many matching source files do not contain a UTF-8
BOM. `--count` suppresses individual diagnostics during timing but does not
change validation or the exit status.

## Results

### Primary profiles

| Profile | Checked | Runs | Mean ± standard deviation | Median | Range | Peak memory |
|---|---:|---:|---:|---:|---:|---:|
| Default | 31,830 | 20 | 906.3 ± 25.9 ms | 907.9 ms | 861.1–950.0 ms | 32.2 MiB |
| Hidden, no size limit | 31,994 | 20 | 899.2 ± 14.4 ms | 900.3 ms | 873.9–925.3 ms | 56.8 MiB |

The 7 ms difference between the two means is smaller than the noise in the
default sample and should not be interpreted as the fuller traversal being
faster. The meaningful difference is memory: removing the file-size limit
increased observed peak memory by approximately 24.6 MiB.

Default command:

```text
Time (mean ± σ):     906.3 ms ±  25.9 ms    [User: 3341.6 ms, System: 4719.4 ms]
Range (min … max):   861.1 ms … 950.0 ms    20 runs
```

Fuller traversal:

```text
Time (mean ± σ):     899.2 ms ±  14.4 ms    [User: 3411.5 ms, System: 4648.8 ms]
Range (min … max):   873.9 ms … 925.3 ms    20 runs
```

### Thread scaling

All thread-scaling measurements used the default traversal profile.

| Workers | Runs | Mean ± standard deviation | Median | Range | Peak memory | Speedup vs. 1 worker |
|---:|---:|---:|---:|---:|---:|---:|
| 1 | 15 | 3.452 ± 0.059 s | 3.458 s | 3.390–3.569 s | 22.4 MiB | 1.00× |
| 2 | 15 | 2.031 ± 0.028 s | 2.024 s | 1.990–2.086 s | 24.5 MiB | 1.70× |
| 4 | 15 | 1.262 ± 0.032 s | 1.258 s | 1.207–1.335 s | 26.0 MiB | 2.74× |
| 8 | 15 | 913.8 ± 74.8 ms | 883.9 ms | 844.4–1069.7 ms | 31.4 MiB | 3.78× |
| 10 | 15 | 913.0 ± 84.8 ms | 893.5 ms | 861.8–1212.8 ms | 31.8 MiB | 3.78× |
| Automatic | 15 | 949.1 ± 78.5 ms | 925.4 ms | 875.1–1172.7 ms | 33.0 MiB | 3.64× |

The workload scales well through four workers and then reaches a practical
plateau around eight workers on this machine. The 8-worker and 10-worker means
are statistically indistinguishable. Ten workers had a 1.21-second outlier, so
its fractionally lower mean is not evidence that it is faster than eight.

### editorconfig-checker comparison

`editorconfig-checker` was invoked with `-disable-indent-size` because
`edcfg-lint` intentionally does not perform a generic indent-size check.
Standard output was redirected to `/dev/null`; the tool has no equivalent of
`edcfg-lint --count`.

| Tool | Files considered | Runs | Mean ± standard deviation | Median | Range | Peak memory |
|---|---:|---:|---:|---:|---:|---:|
| edcfg-lint 0.1.0 | 31,830 checked | 20 | 906.3 ± 25.9 ms | 907.9 ms | 861.1–950.0 ms | 32.2 MiB |
| editorconfig-checker 3.8.0 | 32,007 dry-run entries | 10 | 7.790 ± 0.076 s | 7.807 s | 7.668–7.900 s | 263.4 MiB |

On this machine and corpus, `edcfg-lint` was approximately **8.6× faster** and
used approximately **8.2× less peak memory**.

This comparison is close but not perfectly apples-to-apples:

- The tools' built-in exclusion and traversal policies differ.
- `editorconfig-checker -dry-run` listed 177 more files than `edcfg-lint`
  reported as checked, a difference of about 0.6%.
- `editorconfig-checker` must still construct and write its diagnostics even
  though they are redirected, while `edcfg-lint --count` avoids rendering
  individual diagnostics.
- The tools can differ in rule interpretation and diagnostic granularity.
  `editorconfig-checker` reported 27,794 errors, while `edcfg-lint` reported
  12,125 failed files; those units are not directly comparable.

Raw comparison output:

```text
Time (mean ± σ):      7.790 s ±  0.076 s    [User: 34.976 s, System: 3.241 s]
Range (min … max):    7.668 s … 7.900 s    10 runs
```

## DigitalOcean Intel dedicated-vCPU result

The same pinned source revisions and benchmark protocol were run on a second
machine. This was a DigitalOcean Regular Intel Dedicated CPU droplet in NYC1,
so the vCPUs were dedicated to the droplet even though the machine itself was
virtualized under KVM/QEMU.

### Environment

- Date: 2026-07-20
- Droplet: 4 dedicated Intel vCPUs, 8 GB memory
- CPU exposed to the guest: Intel Xeon Platinum 8168 at 2.70 GHz
- CPU topology exposed to the guest: 4 cores, 1 thread per core
- Memory visible to the guest: 7.8 GiB; no swap
- OS: Ubuntu 24.04.4 LTS
- Kernel: Linux 6.8.0-124-generic
- Architecture: x86_64
- Virtualization: KVM/QEMU
- Filesystem/storage: ext4 on the droplet's virtual block device
- Rust: `rustc 1.97.1 (8bab26f4f 2026-07-14)`
- Hyperfine: 1.20.0
- Comparison tool: editorconfig-checker 3.8.0
- Checkout size: 596 MiB
- `edcfg-lint` binary SHA-256:
  `7922369c717e173a9b561b26ade2d08f0ce2e2832309686506269914eb588223`

The checked/failed smoke-test counts exactly matched the reference machine:
31,830/12,125 for default traversal and 31,994/12,161 for the fuller
traversal. `editorconfig-checker -dry-run` also produced the same 32,007-entry
count. This is useful evidence that the two machines measured the same logical
workload rather than merely similar checkouts.

As on the reference machine, all timing samples were warm-cache measurements.
Peak memory is a separate one-pass observation from GNU `time -v`; it is not a
statistic computed from Hyperfine's timing samples.

### Primary profiles

| Profile | Checked | Runs | Mean ± standard deviation | Median | Range | Observed peak memory |
|---|---:|---:|---:|---:|---:|---:|
| Default | 31,830 | 20 | 1.914 ± 0.120 s | 1.901 s | 1.769–2.159 s | 20.9 MiB |
| Hidden, no size limit | 31,994 | 20 | 2.038 ± 0.164 s | 2.005 s | 1.814–2.372 s | 32.9 MiB |

Default command:

```text
Time (mean ± σ):      1.914 s ±  0.120 s    [User: 6.564 s, System: 0.917 s]
Range (min … max):    1.769 s … 2.159 s    20 runs
```

Fuller traversal:

```text
Time (mean ± σ):      2.038 s ±  0.164 s    [User: 6.895 s, System: 1.067 s]
Range (min … max):    1.814 s … 2.372 s    20 runs
```

The fuller profile was about 6.5% slower and used approximately 12 MiB more
peak memory in the separate observations. The additional 164 checked files
alone do not explain the memory increase: removing the size limit also permits
larger matching files to be read.

### Thread scaling

| Workers | Runs | Mean ± standard deviation | Median | Range | Observed peak memory | Speedup vs. 1 worker |
|---:|---:|---:|---:|---:|---:|---:|
| 1 | 15 | 4.011 ± 0.022 s | 4.012 s | 3.978–4.044 s | 18.6 MiB | 1.00× |
| 2 | 15 | 2.456 ± 0.347 s | 2.418 s | 2.020–3.197 s | 19.9 MiB | 1.63× |
| 4 | 15 | 1.919 ± 0.121 s | 1.887 s | 1.755–2.181 s | 24.8 MiB | 2.09× |
| 8 | 15 | 1.825 ± 0.062 s | 1.805 s | 1.768–1.986 s | 29.5 MiB | 2.20× |
| Automatic | 15 | 1.898 ± 0.156 s | 1.900 s | 1.716–2.271 s | 22.6 MiB | 2.11× |

Scaling is strong from one to two workers, but becomes modest after four on a
four-vCPU guest. Eight workers had the lowest measured mean, approximately 5%
below four workers, but that small difference should not be generalized from
this sample. It may reflect useful overlap during filesystem waits, ordinary
guest scheduling variance, or both. The two-worker sample was unusually
variable and included a 3.20-second high outlier.

### editorconfig-checker comparison

| Tool | Files considered | Runs | Mean ± standard deviation | Median | Range | Observed peak memory |
|---|---:|---:|---:|---:|---:|---:|
| edcfg-lint 0.1.0 | 31,830 checked | 20 | 1.914 ± 0.120 s | 1.901 s | 1.769–2.159 s | 20.9 MiB |
| editorconfig-checker 3.8.0 | 32,007 dry-run entries | 10 | 24.548 ± 0.060 s | 24.541 s | 24.476–24.651 s | 209.9 MiB |

On this droplet and corpus, `edcfg-lint` was approximately **12.8× faster**
and its separate peak-RSS observation was approximately **10.1× smaller**.
The same feature-parity and output-rendering caveats described for the Mac
comparison apply here.

```text
Time (mean ± σ):     24.548 s ±  0.060 s    [User: 77.696 s, System: 2.741 s]
Range (min … max):   24.476 s … 24.651 s    10 runs
```

### Cross-platform observations

The M1 Max completed the default profile approximately 2.11× faster in wall
time (0.906 seconds versus 1.914 seconds). The one-worker gap was much smaller:
the M1 Max was approximately 1.16× faster (3.452 seconds versus 4.011 seconds).
Most of the larger default-profile gap therefore comes from parallel capacity:
the Mac has ten physical cores while this guest has four dedicated vCPUs.

CPU accounting also shows materially different platform behavior. The Linux
default run averaged 6.564 seconds of user CPU and 0.917 seconds of system CPU,
compared with 3.342 and 4.719 seconds respectively on macOS. Linux spent much
less accounted time in the kernel, while the Xeon consumed approximately twice
the user CPU for the same checked/failed result. Those figures are consistent
with different CPU and filesystem implementations, but they do not isolate a
single cause and should not be treated as a controlled OS comparison.

At their default settings, the machines processed approximately 16,634 and
35,120 checked files per wall-clock second on the droplet and Mac respectively.
The comparator slowed more sharply across machines: its 24.548-second droplet
mean was approximately 3.15× its 7.790-second Mac mean. Consequently,
`edcfg-lint`'s relative wall-time advantage increased from 8.6× on the Mac to
12.8× on the droplet.

## Reproducing the benchmark

### 1. Build edcfg-lint

From the `edcfg-lint` checkout:

```bash
git checkout 51efae64b620a0c0c0dc76c001dc85adf7405098
cargo build --profile release-lto
```

Set an absolute path to the resulting executable. Do not include this variable
assignment inside the Hyperfine command string; substitute its value into the
commands so process-launch behavior is consistent across platforms.

```bash
EDCFG_LINT_BIN=/absolute/path/to/eddy/target/release-lto/edcfg-lint
```

### 2. Create the pinned corpus checkout

```bash
BENCHMARK_ROOT=/tmp/edcfg-lint-benchmark
mkdir -p "$BENCHMARK_ROOT"
git init "$BENCHMARK_ROOT/roslyn"
git -C "$BENCHMARK_ROOT/roslyn" remote add origin https://github.com/dotnet/roslyn.git
git -C "$BENCHMARK_ROOT/roslyn" fetch --depth 1 origin 104895c7c0c783f5d7c43f05631364eabc6e6d88
git -C "$BENCHMARK_ROOT/roslyn" checkout --detach FETCH_HEAD
git -C "$BENCHMARK_ROOT/roslyn" rev-parse HEAD
```

The last command must print:

```text
104895c7c0c783f5d7c43f05631364eabc6e6d88
```

### 3. Record the environment

Record at least:

- OS and version
- CPU model
- physical and logical core counts
- RAM
- storage type
- filesystem
- `rustc --version`
- `hyperfine --version`
- `editorconfig-checker --version`, if running the comparison
- `shasum -a 256 "$EDCFG_LINT_BIN"` or the platform equivalent

### 4. Run smoke checks

From `BENCHMARK_ROOT`:

```bash
"$EDCFG_LINT_BIN" --count roslyn
"$EDCFG_LINT_BIN" --count --hidden --max-file-size 0 roslyn
```

Both commands are expected to exit with status 1 because the pinned corpus has
findings. Record the complete summary lines. If the checked or failed counts
differ from the reference counts, investigate before comparing timings.

### 5. Run the primary benchmarks

```bash
hyperfine \
  --warmup 3 \
  --runs 20 \
  --ignore-failure \
  --export-json edcfg-default.json \
  "$EDCFG_LINT_BIN --count roslyn"
```

```bash
hyperfine \
  --warmup 3 \
  --runs 20 \
  --ignore-failure \
  --export-json edcfg-full.json \
  "$EDCFG_LINT_BIN --count --hidden --max-file-size 0 roslyn"
```

### 6. Run thread scaling

```bash
hyperfine \
  --warmup 2 \
  --runs 15 \
  --ignore-failure \
  --export-json thread-scaling.json \
  "$EDCFG_LINT_BIN --count --jobs 1 roslyn" \
  "$EDCFG_LINT_BIN --count --jobs 2 roslyn" \
  "$EDCFG_LINT_BIN --count --jobs 4 roslyn" \
  "$EDCFG_LINT_BIN --count --jobs 8 roslyn" \
  "$EDCFG_LINT_BIN --count --jobs 10 roslyn" \
  "$EDCFG_LINT_BIN --count roslyn"
```

Adapt the explicit worker counts to the target system, but always retain 1, 2,
4, and automatic selection. Record any local activity or thermal condition
that may have affected outliers.

### 7. Run the optional comparison

From the Roslyn checkout itself:

```bash
editorconfig-checker -dry-run | wc -l
```

```bash
hyperfine \
  --warmup 2 \
  --runs 10 \
  --ignore-failure \
  --export-json editorconfig-checker.json \
  'editorconfig-checker -disable-indent-size > /dev/null'
```

On Windows, use the platform's null sink and quoting rules, for example
`> NUL` under `cmd.exe`. Record traversal-count differences and do not present
the comparison as exact feature parity.

## Cross-platform result template

Copy this section for each additional machine:

```text
Environment name:
Date:
OS/version:
CPU:
Physical/logical cores:
RAM:
Storage:
Filesystem:
Rust version:
Hyperfine version:
edcfg-lint binary SHA-256:
editorconfig-checker version:

Roslyn commit: 104895c7c0c783f5d7c43f05631364eabc6e6d88
Default checked/failed:
Full checked/failed:

Default mean/stddev/median/range/peak memory:
Full mean/stddev/median/range/peak memory:

1 worker:
2 workers:
4 workers:
Additional worker counts:
Automatic workers:

editorconfig-checker dry-run count:
editorconfig-checker mean/stddev/median/range/peak memory:

Notes on background load, power mode, virtualization, and thermal state:
```
