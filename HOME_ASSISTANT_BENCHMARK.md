# Corrected-behavior optimization benchmark

This experiment reconstructs `edcfg-lint`'s historical optimization sequence
on top of the corrected implementation. Unlike the original development-history
comparison, every derived executable performs the same logical work and emits
byte-for-byte identical diagnostics.

The source anchor is commit
`51efae64b620a0c0c0dc76c001dc85adf7405098`. Benchmark-only Cargo features
selectively remove optimizations while retaining that commit's corrected
behavior for:

- config-parent-relative glob application;
- `root` handling and parent/child precedence;
- limited `indent_size` support as a `tab_width` fallback;
- independent `indent_style` enforcement;
- BOM-aware binary classification;
- final-newline handling; and
- file, line, next-line, and block skip directives.

## Headline result

| Machine | Corpus | Corrected serial baseline | Fully optimized derived build | Overall speedup |
|---|---|---:|---:|---:|
| M1 Max, macOS | Home Assistant | 2.661 s | 553.4 ms | **4.81×** |
| M1 Max, macOS | Roslyn | 8.418 s | 859.4 ms | **9.80×** |
| Xeon, Linux | Home Assistant | 2.787 s | 458.2 ms | **6.08×** |
| Xeon, Linux | Roslyn | 12.037 s | 1.964 s | **6.13×** |
| M1 Max, Asahi Linux | Home Assistant | 1.603 s | 89.1 ms | **17.99×** |
| M1 Max, Asahi Linux | Roslyn | 7.319 s | 314.4 ms | **23.28×** |

On the M1 Max, the fully optimized build checked approximately 41,800 Home
Assistant files or 37,000 Roslyn files per wall-clock second. The Linux droplet
processed approximately 50,500 and 16,200 files per second respectively. Under
Asahi Linux, the M1 Max processed approximately 260,000 and 101,200 files per
second respectively.

Parallel traversal was the largest optimization on both corpora in all three
environments.
Removing the legacy MIME-probe cost and caching correctly resolved EditorConfig
files also produced material improvements. On macOS, fetching line-check
properties once per file halved user CPU on Home Assistant without changing
wall time, which remained dominated by filesystem/system work; on Linux, the
same optimization halved Home Assistant wall time. The platform contrast is a
central result of the experiment.

## M1 Max machine and protocol

- Date: 2026-07-20
- Model: 16-inch MacBook Pro (`MacBookPro18,2`)
- CPU: Apple M1 Max
- Cores: 10 (8 performance, 2 efficiency)
- Memory: 64 GB
- OS: macOS 26.5, build 25F71
- Architecture: arm64
- Filesystem/storage: APFS on internal Apple SSD/NVMe storage
- Rust: `rustc 1.97.1 (8bab26f4f 2026-07-14)`
- Cargo: 1.97.1
- Hyperfine: 1.20.0
- Cargo profile: `release-lto`

All timing measurements were warm-cache runs. Hyperfine performed three
warmups followed by 20 measured runs per executable. Nonzero exit codes were
expected because both pinned corpora contain findings. Individual diagnostics
were suppressed with `--count` during timing, but validation and exit status
were unchanged.

Peak memory was measured separately. Each reported value is the median of
three independent `time -l` maximum-resident-set-size observations, with the
complete three-observation range shown. Memory measurements were not part of
the Hyperfine samples.

## Corpora

### Home Assistant

- Repository: `https://github.com/home-assistant/core.git`
- Commit: `fc281b2faecdc86c0a716dfb4a1f243dde632297`
- Commit subject: `Firefly III fix background task (#160935)`
- Tracked files: 23,229
- Checkout size on this machine: 232 MiB
- Checked files: 23,138
- Failed files: 53

Home Assistant does not contain the configuration used by the original
article. The benchmark places the following fixture in the checkout's parent
directory, exactly as the historical experiment did. Its SHA-256 is
`0c5016a64ff884872deb03d58e7f730dc8783cddc7d225638cd9f3a0c9d86e06`.

```ini
root = true

[*]
indent_style = space
indent_size = 4
end_of_line = lf
charset = utf-8
trim_trailing_whitespace = true
insert_final_newline = true

[*.py]
indent_style = space
indent_size = 4

[*.{js,json,yml,yaml}]
indent_style = space
indent_size = 2

[*.md]
trim_trailing_whitespace = false
```

All variants used the current limited `indent_size` semantics. The fixture's
`indent_size` values can supply a missing `tab_width`, but do not enable a
generic indentation-width rule.

### Roslyn

- Repository: `https://github.com/dotnet/roslyn.git`
- Commit: `104895c7c0c783f5d7c43f05631364eabc6e6d88`
- Commit subject: `[main] Update dependencies from dotnet/arcade (#84554)`
- Tracked files: 32,378
- Checkout size on this machine: 576 MiB
- Checked files: 31,830
- Failed files: 12,125
- `.editorconfig` files: 37

Roslyn supplies its own root and nested EditorConfig files. It exercises a
larger and more varied configuration topology than the synthetic parent-level
Home Assistant fixture.

## Corrected variants

| Stage | Parallel traversal | Legacy MIME probe | Resolver cache | Line properties |
|---|---|---|---|---|
| C0: corrected baseline | No | Yes, advisory | No | Fetched per line |
| C1: parallel | Yes | Yes, advisory | No | Fetched per line |
| C2: binary handling | Yes | Removed | No | Fetched per line |
| C3: resolver cache | Yes | Removed | Positive and negative | Fetched per line |
| C4: property amortization | Yes | Removed | Positive and negative | Fetched once per file |
| C5: untouched current control | Production source | Production source | Production source | Production source |

### C0: corrected baseline

C0 uses `WalkBuilder::build()` and processes entries synchronously on the
calling thread. It is a true serial traversal, rather than the parallel walker
configured with one worker. It retains the corrected BOM-aware classifier,
uncached corrected resolver, current line checks, and current skip behavior.

To represent the cost removed by the historical MIME-classification change,
C0 also performs `infer::get_from_path()` and passes its result through
`std::hint::black_box`. The result is advisory and never controls skipping.
The corrected BOM-aware NUL-byte classifier remains authoritative, so the
probe adds historical-style work without reintroducing historical behavior.

### C1: parallel traversal

C1 replaces only the synchronous walker with the production parallel walker,
worker closures, and result channel. All other baseline costs remain enabled.

### C2: efficient binary handling

C2 removes the redundant `infer` MIME probe. It does not compare two competing
classifiers: every stage uses the current rule that recognizes Unicode BOMs and
otherwise searches the first 8,000 bytes for a NUL byte. This makes C1-to-C2 a
corrected-behavior measurement of the legacy probe's cost.

### C3: corrected resolver cache

C3 enables the production positive-and-negative DashMap cache. Its uncached
counterpart uses the same eager parser, stores the config parent, applies each
section to a config-parent-relative path, preserves precedence, and stops at
`root = true`. Only config reopening, reparsing, and repeated negative lookups
differ.

### C4: property amortization

C4 constructs `LineCheckConfig` once per file. C3 reconstructs the same
corrected values for every checked line. No property defaults or rule meanings
change.

### C5: untouched-current control

C5 was built from an unmodified archive of commit `51efae6`, without the
benchmark feature framework. It is not part of the incremental speedup chain.
It checks whether the scaffolding or small traversal refactor perturbed the
fully optimized result.

## Behavioral equivalence

Before timing, every executable was run without `--count` against both complete
corpora. All variants exited with status 1 and produced identical, sorted,
full diagnostic output.

| Corpus | Full-output SHA-256 shared by C0–C5 |
|---|---|
| Home Assistant | `b34018d5034a5c3b197cb6a1e2569d653fb574c0abbd4950d5a02784585ede3a` |
| Roslyn | `a71ac50c646b6e6f3215705e6fc703b3777583861b6f988ff396b510059b5481` |

The benchmark-feature baseline also passed all 45 automated tests: 41
unit/resolver/harness tests and four black-box CLI tests. The default build
passes the same suite.

The equivalence claim is therefore stronger than matching summary counts. It
covers file selection, skip decisions, config resolution, diagnostic kinds,
line numbers, expected/actual values, ordering, summaries, and exit status as
exposed by the CLI.

## Home Assistant results

| Stage | Mean ± standard deviation | Median | Range | User CPU | System CPU | Incremental speedup | Overall speedup |
|---|---:|---:|---:|---:|---:|---:|---:|
| C0: corrected baseline | 2.661 ± 0.038 s | 2.645 s | 2.623–2.764 s | 1.316 s | 1.332 s | 1.00× | 1.00× |
| C1: parallel | 925.4 ± 48.4 ms | 912.2 ms | 891.9–1103.5 ms | 1.913 s | 6.368 s | **2.88×** | 2.88× |
| C2: binary handling | 747.7 ± 9.5 ms | 748.6 ms | 730.6–762.8 ms | 1.877 s | 4.618 s | **1.24×** | 3.56× |
| C3: resolver cache | 553.1 ± 16.1 ms | 556.0 ms | 525.7–578.2 ms | 1.624 s | 3.343 s | **1.35×** | 4.81× |
| C4: property amortization | 553.4 ± 15.9 ms | 553.7 ms | 524.4–580.4 ms | 0.762 s | 4.178 s | 1.00× | **4.81×** |

Parallelization reduced wall time by approximately 65%. The 1.10-second C1
maximum was a high outlier; the median-to-median comparison is slightly more
favorable than the mean comparison.

Removing the advisory MIME probe reduced mean wall time by approximately 19%,
and enabling the corrected resolver cache reduced it by another 26%.

Property amortization cut user CPU by approximately 2.13×, from 1.624 seconds
to 0.762 seconds. Wall time did not improve because average system CPU rose
from 3.343 to 4.178 seconds and the two wall-time distributions are effectively
identical. This reproduces the important qualitative finding from the original
macOS experiment: an algorithmic CPU improvement can be hidden by parallel
filesystem overhead.

## Roslyn results

| Stage | Mean ± standard deviation | Median | Range | User CPU | System CPU | Incremental speedup | Overall speedup |
|---|---:|---:|---:|---:|---:|---:|---:|
| C0: corrected baseline | 8.418 ± 0.065 s | 8.401 s | 8.310–8.537 s | 6.194 s | 2.199 s | 1.00× | 1.00× |
| C1: parallel | 1.716 ± 0.040 s | 1.704 s | 1.647–1.814 s | 8.375 s | 7.102 s | **4.91×** | 4.91× |
| C2: binary handling | 1.526 ± 0.032 s | 1.519 s | 1.469–1.574 s | 8.433 s | 5.122 s | **1.12×** | 5.52× |
| C3: resolver cache | 1.020 ± 0.025 s | 1.017 s | 982.3–1097.3 ms | 6.082 s | 3.465 s | **1.50×** | 8.25× |
| C4: property amortization | 859.4 ± 15.1 ms | 857.4 ms | 836.7–902.1 ms | 3.268 s | 4.597 s | **1.19×** | **9.80×** |

Parallel traversal reduced mean wall time by approximately 80%. The gain is
larger than Home Assistant's because Roslyn makes the corrected serial resolver
perform much more ancestor/config work across a larger and more deeply nested
tree.

Removing the MIME probe reduced mean wall time by approximately 11%. Enabling
the resolver cache then reduced it by approximately 33%, a 1.50× incremental
speedup. Roslyn's real nested configuration topology makes this a stronger
cache workload than Home Assistant's single synthetic parent config.

Unlike Home Assistant, property amortization reduced Roslyn wall time by about
16%, or a 1.19× incremental speedup. User CPU fell by approximately 1.86×,
from 6.082 to 3.268 seconds. The larger CPU saving was sufficient to overcome
the accompanying increase in system CPU.

## Memory observations

| Stage | Home Assistant median (range) | Roslyn median (range) |
|---|---:|---:|
| C0: corrected baseline | 7.34 MiB (7.25–7.83) | 18.44 MiB (18.31–18.48) |
| C1: parallel | 15.05 MiB (14.94–15.11) | 30.77 MiB (29.48–31.36) |
| C2: binary handling | 15.30 MiB (15.06–15.50) | 32.89 MiB (31.16–33.16) |
| C3: resolver cache | 14.91 MiB (14.78–15.75) | 32.94 MiB (32.47–33.55) |
| C4: property amortization | 15.39 MiB (14.77–15.55) | 29.78 MiB (28.80–29.84) |
| C5: untouched current | 15.50 MiB (15.16–15.73) | 30.33 MiB (29.06–32.77) |

Parallel traversal approximately doubled observed Home Assistant peak RSS and
increased Roslyn peak RSS by about two thirds. Absolute memory remained small:
the optimized builds stayed around 15 MiB for Home Assistant and 30 MiB for
Roslyn. The cache itself did not produce a clear memory increase beyond normal
run-to-run variation. Property amortization modestly reduced Roslyn's median
peak RSS.

## Untouched-current control

The untouched build produced the same full-output hashes as C0–C4.

On Roslyn, its 20-run result was:

```text
Time (mean ± σ):     866.3 ms ± 26.5 ms
Median:              857.7 ms
Range:               827.9–909.9 ms
```

C4's median was 857.4 ms, only 0.03% lower. This is compelling evidence that
the benchmark feature framework did not perturb the final Roslyn result.

Home Assistant's untouched control was noisier. Two independent 20-run series
had medians of 566.1 and 555.5 ms, compared with C4's 553.7 ms. Their means
were 582.7 and 640.7 ms because each series contained one or more unusually
slow samples, including 988.3 and 1025.6 ms maxima. The second median is within
0.4% of C4; even the first is within 2.3%. The medians support the same
no-material-perturbation conclusion, while the means are intentionally not
used to make that claim.

## DigitalOcean Linux result

The complete experiment was repeated on the same DigitalOcean host used for
the Roslyn tool-comparison benchmark. The source, feature combinations, corpus
commits, Home Assistant fixture, warmup count, measured-run count, and
correctness protocol were identical.

### Linux environment

- Date: 2026-07-20
- Droplet: DigitalOcean Regular Intel Dedicated CPU, NYC1
- CPU exposed to the guest: Intel Xeon Platinum 8168 at 2.70 GHz
- CPU topology exposed to the guest: 4 cores, 1 thread per core
- Memory visible to the guest: 7.8 GiB; no swap
- OS: Ubuntu 24.04.4 LTS
- Kernel: Linux 6.8.0-124-generic
- Architecture: x86_64
- Virtualization: KVM/QEMU
- Filesystem/storage: ext4 on the droplet's virtual block device
- Rust: `rustc 1.97.1 (8bab26f4f 2026-07-14)`
- Cargo: 1.97.1
- Hyperfine: 1.20.0
- Cargo profile: `release-lto`

All Linux timing samples were warm-cache measurements with three warmups and
20 measured runs. Peak RSS was measured separately with GNU `time -v`; each
reported value is the median and range of three observations.

### Linux behavioral equivalence

All six Linux executables exited with status 1 and produced the exact same
full-output hashes as all six macOS executables:

| Corpus | Full-output SHA-256 shared across both machines and C0–C5 |
|---|---|
| Home Assistant | `b34018d5034a5c3b197cb6a1e2569d653fb574c0abbd4950d5a02784585ede3a` |
| Roslyn | `a71ac50c646b6e6f3215705e6fc703b3777583861b6f988ff396b510059b5481` |

The Linux fixture SHA-256 was also
`0c5016a64ff884872deb03d58e7f730dc8783cddc7d225638cd9f3a0c9d86e06`.
Checked/failed counts remained 23,138/53 and 31,830/12,125. This establishes
that every stage and both platforms measured the same externally observable
work rather than merely similar checkouts.

### Linux Home Assistant results

| Stage | Mean ± standard deviation | Median | Range | User CPU | System CPU | Incremental speedup | Overall speedup |
|---|---:|---:|---:|---:|---:|---:|---:|
| C0: corrected baseline | 2.787 ± 0.021 s | 2.786 s | 2.756–2.825 s | 2.179 s | 0.608 s | 1.00× | 1.00× |
| C1: parallel | 1.205 ± 0.004 s | 1.205 s | 1.198–1.213 s | 3.819 s | 0.962 s | **2.31×** | 2.31× |
| C2: binary handling | 1.116 ± 0.004 s | 1.115 s | 1.110–1.126 s | 3.644 s | 0.778 s | **1.08×** | 2.50× |
| C3: resolver cache | 917.3 ± 34.6 ms | 909.1 ms | 901.3–1059.9 ms | 3.171 s | 0.423 s | **1.22×** | 3.04× |
| C4: property amortization | 458.2 ± 4.3 ms | 458.2 ms | 451.5–466.0 ms | 1.357 s | 0.434 s | **2.00×** | **6.08×** |

Parallel traversal reduced mean wall time by approximately 57%. Removing the
legacy MIME probe reduced it by another 7%, and enabling the resolver cache
reduced it by approximately 18%. C3 contained a 1.06-second high outlier; its
median was 909.1 ms and the remaining distribution was tight.

Property amortization was the largest post-parallel Linux improvement. User
CPU fell by approximately 2.34×, from 3.171 seconds to 1.357 seconds, while
system CPU remained essentially flat. Consequently, the CPU improvement was
visible almost directly in wall time: C4 was 2.00× faster than C3.

This differs sharply from macOS, where the same Home Assistant step reduced
user CPU by 2.13× but did not improve wall time because high parallel system
CPU dominated the run.

### Linux Roslyn results

| Stage | Mean ± standard deviation | Median | Range | User CPU | System CPU | Incremental speedup | Overall speedup |
|---|---:|---:|---:|---:|---:|---:|---:|
| C0: corrected baseline | 12.037 ± 0.051 s | 12.032 s | 11.948–12.197 s | 10.965 s | 1.071 s | 1.00× | 1.00× |
| C1: parallel | 4.780 ± 0.028 s | 4.781 s | 4.738–4.857 s | 17.320 s | 1.682 s | **2.52×** | 2.52× |
| C2: binary handling | 4.602 ± 0.036 s | 4.603 s | 4.555–4.695 s | 16.868 s | 1.423 s | **1.04×** | 2.62× |
| C3: resolver cache | 3.128 ± 0.142 s | 3.102 s | 2.955–3.465 s | 11.509 s | 0.853 s | **1.47×** | 3.85× |
| C4: property amortization | 1.964 ± 0.190 s | 1.910 s | 1.731–2.508 s | 6.691 s | 0.989 s | **1.59×** | **6.13×** |

Parallel traversal reduced mean wall time by approximately 60%. The legacy
MIME-probe removal was modest on this corpus and host, improving wall time by
about 4%. Correct resolver caching was worth a 1.47× incremental speedup, very
close to the M1 Max's 1.50× Roslyn result.

Property amortization reduced user CPU by approximately 1.72× and wall time by
approximately 37%, a 1.59× incremental speedup. C3 and C4 were more variable
than the first three stages; their medians were 3.102 and 1.910 seconds. The
median-to-median property speedup was 1.62×, consistent with the mean result.

### Linux memory observations

| Stage | Home Assistant median (range) | Roslyn median (range) |
|---|---:|---:|
| C0: corrected baseline | 5.76 MiB (5.64–5.88) | 13.51 MiB (13.26–13.51) |
| C1: parallel | 11.76 MiB (11.26–12.13) | 21.63 MiB (19.00–22.13) |
| C2: binary handling | 11.88 MiB (11.00–12.00) | 22.00 MiB (21.71–22.00) |
| C3: resolver cache | 11.50 MiB (11.38–11.63) | 23.63 MiB (23.00–23.75) |
| C4: property amortization | 11.50 MiB (11.25–12.13) | 23.88 MiB (23.75–23.88) |
| C5: untouched current | 12.25 MiB (12.00–12.50) | 20.13 MiB (19.88–22.50) |

The Linux observations show the same broad memory shape as macOS: serial
traversal is smallest, parallel traversal roughly doubles Home Assistant RSS,
and all optimized variants remain small in absolute terms. Linux peak RSS was
generally lower: approximately 11–12 MiB for optimized Home Assistant and
20–24 MiB for optimized Roslyn.

The C4/C5 Roslyn memory difference is larger than the timing difference and
shows that code-layout or scheduling changes from the feature framework can
affect peak concurrency. It does not affect the output-equivalence result, and
both values remain below the corresponding macOS observations.

### Linux untouched-current control

The untouched control was slightly faster than C4 on both Linux corpora:

| Corpus | C4 mean / median | C5 mean / median | Median difference |
|---|---:|---:|---:|
| Home Assistant | 458.2 / 458.2 ms | 450.6 / 450.3 ms | C5 1.7% faster |
| Roslyn | 1.964 / 1.910 s | 1.931 / 1.908 s | C5 0.1% faster |

Roslyn C4 and C5 both had high samples, so their medians are more informative
than the means. Together with exact output equivalence, the small control
differences support treating C4 as production-equivalent while retaining C5
as the literal untouched result.

### Cross-platform comparison

| Optimization | macOS Home Assistant | DO Linux Home Assistant | Asahi Home Assistant | macOS Roslyn | DO Linux Roslyn | Asahi Roslyn |
|---|---:|---:|---:|---:|---:|---:|
| Parallel traversal | 2.88× | 2.31× | 7.54× | 4.91× | 2.52× | 8.25× |
| Remove MIME probe | 1.24× | 1.08× | 1.04× | 1.12× | 1.04× | 1.01× |
| Resolver cache | 1.35× | 1.22× | 1.15× | 1.50× | 1.47× | 1.39× |
| Property amortization | 1.00× | 2.00× | 1.98× | 1.19× | 1.59× | 2.00× |
| Overall | **4.81×** | **6.08×** | **17.99×** | **9.80×** | **6.13×** | **23.28×** |

The M1 Max and Xeon were close on the Home Assistant serial baseline: 2.661
versus 2.787 seconds. The M1 then led through C1, C2, and C3 because its ten
cores and high single-core performance outweighed macOS filesystem overhead.
At C4, Linux became approximately 1.21× faster in wall time—458 versus 553 ms—
because property amortization exposed the much lower Linux system-time cost.

For Home Assistant C4, macOS averaged 0.762 seconds of user CPU and 4.178
seconds of system CPU. Linux averaged 1.357 and 0.434 seconds respectively.
The M1 performed the user-space work with much less CPU, but macOS spent about
9.6× as much accounted time in the kernel. Linux therefore won the final wall
time despite slower user-space execution and fewer cores.

Roslyn remained faster on the M1 throughout. Its C4 mean was 859 ms versus
1.964 seconds on the droplet, a 2.28× wall-time advantage. Linux again used
far less system CPU—0.989 versus 4.597 seconds—but consumed about twice the user
CPU and had only four cores available. Roslyn's larger content-checking load
and nested configuration topology allowed the M1's CPU and parallel-capacity
advantages to dominate.

The corrected experiment therefore strengthens, rather than weakens, the
original platform conclusion: an optimization's wall-time value depends on
where the corpus and OS place the bottleneck. Property amortization can be
invisible on an APFS-dominated workload, decisive on Linux, and still useful
but less dominant on a CPU-heavier macOS corpus.

## Asahi Linux result

The complete C0–C5 experiment was also run natively on the same M1 Max under
Asahi Linux. The source, feature combinations, corpus commits, Home Assistant
fixture, correctness protocol, three warmups, and 20 measured runs per command
matched the macOS and DigitalOcean experiments. The final measurements used
the internal SSD-backed Btrfs filesystem; a preliminary `/tmp` run was
discarded after identifying that `/tmp` was RAM-backed `tmpfs` on this system.

### Asahi environment

- Date: 2026-07-20
- Model: 16-inch MacBook Pro (`MacBookPro18,2`)
- CPU: Apple M1 Max
- CPU topology: 10 physical/logical cores (8 performance, 2 efficiency)
- Memory: 64 GB installed; 62 GiB visible to the OS; 8 GiB zram swap
- OS: Fedora Linux Asahi Remix 44, KDE Plasma Desktop Edition
- Kernel: Linux 7.0.13-400.asahi.fc44.aarch64+16k
- Architecture: aarch64, 16 KiB pages
- Filesystem/storage: Btrfs with zstd compression on the internal Apple
  AP4096R SSD/NVMe
- Rust: `rustc 1.97.1 (8bab26f4f 2026-07-14)`
- Cargo: 1.97.1
- Hyperfine: 1.20.0
- Cargo profile: `release-lto`
- CPU frequency boost: reported disabled by `lscpu`

The measurements were warm-cache runs in the normal desktop environment;
there was no dedicated CPU isolation or background-load control.

### Asahi behavioral equivalence

All six executables exited with status 1 and produced the same complete-output
hashes as the macOS and DigitalOcean runs:

| Corpus | Full-output SHA-256 shared by C0–C5 |
|---|---|
| Home Assistant | `b34018d5034a5c3b197cb6a1e2569d653fb574c0abbd4950d5a02784585ede3a` |
| Roslyn | `a71ac50c646b6e6f3215705e6fc703b3777583861b6f988ff396b510059b5481` |

The fixture hash was
`0c5016a64ff884872deb03d58e7f730dc8783cddc7d225638cd9f3a0c9d86e06`,
and the checked/failed counts remained 23,138/53 and 31,830/12,125.

### Asahi Home Assistant results

| Stage | Mean ± standard deviation | Median | Range | User CPU | System CPU | Incremental speedup | Overall speedup |
|---|---:|---:|---:|---:|---:|---:|---:|
| C0: corrected baseline | 1.603 ± 0.002 s | 1.604 s | 1.599–1.610 s | 1.354 s | 0.238 s | 1.00× | 1.00× |
| C1: parallel | 212.5 ± 9.9 ms | 208.9 ms | 203.3–238.5 ms | 1.632 s | 385.5 ms | **7.54×** | 7.54× |
| C2: binary handling | 204.2 ± 4.7 ms | 203.4 ms | 198.3–214.2 ms | 1.562 s | 305.3 ms | 1.04× | 7.85× |
| C3: resolver cache | 176.9 ± 4.0 ms | 176.3 ms | 172.4–188.7 ms | 1.479 s | 139.2 ms | **1.15×** | 9.07× |
| C4: property amortization | 89.1 ± 7.1 ms | 86.5 ms | 78.6–103.5 ms | 611.7 ms | 152.8 ms | **1.98×** | **17.99×** |

Parallel traversal was unusually effective on this native ten-core Linux
system, reducing wall time by approximately 87%. Property amortization again
made its CPU saving directly visible in wall time: user CPU fell by about
2.42× from C3 to C4, while mean wall time improved by 1.98×.

### Asahi Roslyn results

| Stage | Mean ± standard deviation | Median | Range | User CPU | System CPU | Incremental speedup | Overall speedup |
|---|---:|---:|---:|---:|---:|---:|---:|
| C0: corrected baseline | 7.319 ± 0.022 s | 7.315 s | 7.288–7.359 s | 6.826 s | 428.3 ms | 1.00× | 1.00× |
| C1: parallel | 887.1 ± 12.5 ms | 882.8 ms | 871.8–913.4 ms | 7.890 s | 560.9 ms | **8.25×** | 8.25× |
| C2: binary handling | 876.3 ± 11.6 ms | 875.9 ms | 857.6–910.0 ms | 7.880 s | 503.7 ms | 1.01× | 8.35× |
| C3: resolver cache | 630.1 ± 9.1 ms | 627.7 ms | 620.1–662.1 ms | 5.668 s | 224.3 ms | **1.39×** | 11.61× |
| C4: property amortization | 314.4 ± 6.3 ms | 315.0 ms | 306.6–331.7 ms | 2.670 s | 221.5 ms | **2.00×** | **23.28×** |

Roslyn showed the same broad shape. Parallel traversal produced an 8.25×
first-step speedup, removing the MIME probe was effectively neutral, resolver
caching improved wall time by 1.39×, and property amortization halved it
again. The optimized build processed approximately 101,200 checked files per
wall-clock second.

### Asahi untouched-current control

| Corpus | C4 mean / median | C5 mean / median | Median difference |
|---|---:|---:|---:|
| Home Assistant | 89.1 / 86.5 ms | 85.7 / 84.8 ms | C5 2.0% faster |
| Roslyn | 314.4 / 315.0 ms | 320.1 / 317.8 ms | C5 0.9% slower |

The small C4/C5 differences, together with exact output equivalence, support
the conclusion that the benchmark feature framework did not materially
perturb the optimized result on Asahi Linux.

## Binary SHA-256 values

### M1 Max binaries

| Executable | SHA-256 |
|---|---|
| C0 | `038e4f1f17d54783ce8babd7301e660a4ef7091b340b819d70038df71ec74fc2` |
| C1 | `b05390fcfbe4996d0836a5ecd49a003c274db88180f1c9183be8b2f0fb2cf728` |
| C2 | `45f9fd17571a6019ca717af76b5336f28a0cdf9d804c69f69aa030c56e321635` |
| C3 | `c2a1af0095dfb249cab06efbec3d4e56afd35df160468957774c82f6e9aaf05f` |
| C4 | `58dfe70a940d80ae05ce8bb727c1c0a153a64b3ab17833c424a4893dd604b783` |
| C5 | `eb396c3c754647e03790394fc0e3e743e7d38ff4072b44bd5bff9cfa5696a617` |

### Linux binaries

| Executable | SHA-256 |
|---|---|
| C0 | `3810b7e0aa4c76661aa697919647320e687de5a73b1aea485da8a36a174e737f` |
| C1 | `e01788b964456f87be15ca63c1b509223c7690b76edfddaa45714e2230acdc79` |
| C2 | `9803600707cc5f131a1ff9e5151a2ff0745eb7ffcf940bada1e8bab03fe49699` |
| C3 | `2914a1deaeacc49cd9d5d8899ecdfc7171495b3c43f0da92274856a311d426ac` |
| C4 | `9e161feee3f227fec3f7205aea54cd9f6db8c7e6d21658d7ab6a0a4d2def7b5a` |
| C5 | `7922369c717e173a9b561b26ade2d08f0ce2e2832309686506269914eb588223` |

### Asahi Linux binaries

| Executable | SHA-256 |
|---|---|
| C0 | `83534b14ce227caa16c6c69a061bb223b936e457354ddb7acddf81d14f202a91` |
| C1 | `639c599edf18e9abada3e2d65487a1e53aa27ce6620b3c765b8a85b09fa4f2c1` |
| C2 | `8d9bc95f7d7526f1766b757d9a61d6a8508f7cde44ead5d6ee040c1c8aa11d06` |
| C3 | `e70da628cf91c5b443537ce1f9009a0e550c1f06d55dc508e5552137e53d5add` |
| C4 | `f621e57c18a073528d1c3bef044b90ea540ccc0e7ba01dd426b2226a228ebab3` |
| C5 | `b8b8aa2ef7c837f551edddded263aa8edd7090f1af300bfee33bca83b9f50d2b` |

## Reproducing the variants

The benchmark features are cumulative removals from the default optimized
build:

```text
C0: bench-serial-walk, bench-legacy-mime-probe,
    bench-uncached-resolver, bench-per-line-properties
C1: bench-legacy-mime-probe, bench-uncached-resolver,
    bench-per-line-properties
C2: bench-uncached-resolver, bench-per-line-properties
C3: bench-per-line-properties
C4: no benchmark features
```

Build each variant with `cargo build --profile release-lto`, supplying the
listed comma-separated features through `--features`. Copy or rename the
executable after every build so the next feature combination cannot overwrite
it.

Before timing another platform, capture complete output from every binary and
verify exact equivalence:

```bash
edcfg-lint-C0 home-assistant > C0-home-assistant.txt
edcfg-lint-C4 home-assistant > C4-home-assistant.txt
cmp C0-home-assistant.txt C4-home-assistant.txt

edcfg-lint-C0 roslyn > C0-roslyn.txt
edcfg-lint-C4 roslyn > C4-roslyn.txt
cmp C0-roslyn.txt C4-roslyn.txt
```

The expected exit status is 1. The expected summaries are:

```text
Checked 23138 files, 53 failed
Checked 31830 files, 12125 failed
```

Run each corpus from its parent directory so config discovery and relative
paths match this experiment. The timing protocol is:

```bash
hyperfine \
  --warmup 3 \
  --runs 20 \
  --ignore-failure \
  --export-json corrected-variants.json \
  'edcfg-lint-C0 --count corpus' \
  'edcfg-lint-C1 --count corpus' \
  'edcfg-lint-C2 --count corpus' \
  'edcfg-lint-C3 --count corpus' \
  'edcfg-lint-C4 --count corpus'
```

Record OS, CPU, physical/logical cores, RAM, storage, filesystem, Rust,
Hyperfine, corpus commits, fixture hash, binary hashes, checked/failed counts,
and any background activity or power-mode constraints.

## Raw timing data

Canonical copies of the raw Hyperfine JSON are retained in
`benchmark-results/2026-07-20/corrected-optimization`, separated into `macos`,
`linux`, and `asahi-linux` directories. The original temporary copies may
still exist under `/tmp/eddy-corrected-benchmark`, but are not required to
preserve the results.

| File | SHA-256 |
|---|---|
| `home-assistant.json` | `0b389e1a68df72fad6065a8ae59ae566f3f6755af0f5deac35a738fdb588cff8` |
| `roslyn.json` | `bdbf921bce8619d13394775423a5ba99cf9c5d1886be2d58a4b1c99327ff5a67` |
| `untouched-home-assistant.json` | `ef129b2a878342939ba23515617541150700a1a1f2b1ada6c29601ce2a180a31` |
| `untouched-home-assistant-rerun.json` | `12b03c44356bae53d989939578143d60f2dcc8831e42e37598e69cafcf87fd9e` |
| `untouched-roslyn.json` | `2548cb81a80fdfe57f65bb8d9fa0d3fd099a329484b39de8eb2c310625805b50` |
| `linux-home-assistant.json` | `c9832186d02c2a4f3cc6851f83cd5d3822f1eb6b8dd76473e6f039d467602c20` |
| `linux-roslyn.json` | `939d667fee62f15c391ce0120c5e0343c0eb9c39970afef4613abea3c587efa4` |
| `asahi-linux/home-assistant.json` | `656b66500353959bdb250701ff119f017f14f48e728fea84f4c1459bad8f15bb` |
| `asahi-linux/roslyn.json` | `7bc12479a242c4f0654dfe72df68a60b38db1818ab582ac90c228135c644cd9e` |

The first untouched-Home-Assistant checksum above is intentionally included
for completeness despite its outlier-heavy mean.

## Interpretation

The corrected experiment preserves the original article's central conclusions
without relying on the original versions' changing behavior:

1. Parallelism is the dominant first optimization on both multicore machines.
2. Redundant file classification is costly, particularly on macOS/APFS.
3. Correct positive and negative config caching materially reduces filesystem
   and parsing work, especially on Roslyn's nested configuration tree.
4. Per-file property extraction is a genuine algorithmic CPU improvement even
   when wall time is masked by filesystem overhead.
5. Corpus topology and platform bottlenecks materially change the incremental
   and overall speedup factors.

The exact overall factor is therefore not a universal property of the program.
Across the three environments it ranged from 4.81× to 17.99× for Home Assistant
and from 6.13× to 23.28× for Roslyn. The robust claim is that all four
optimizations remain justified after correctness is held constant, while their
relative importance depends strongly on the repository, operating system, and
available parallel capacity.
