# edcfg-lint

A fast `editorconfig` linter written in Rust. It is many times faster than [`editorconfig-checker`](https://github.com/editorconfig-checker/editorconfig-checker).

Still a work in progress, in particular the harness is mostly vibe-coded and needs a serious refresh. However, it already approaches the limits of what the hardware and OS can do despite this.

## How it works inside

There honestly isn't a lot of magic: we use standard Rust crates.

* `ignore` is used for walking and parallelism.
* `ec4rs` is used as our EditorConfig core.
* `memchr` is used for some fast (substring and character) searches.
* `encoding_rs` is used to handle charset detection and decoding.

edcfg-lint primarily comprises of:

* The harness (`src/main.rs`)
* The checker (`src/file.rs`)
* A caching version of `ec4rs::properties_of` (`src/ec.rs`)

These crates are highly optimized (we're using a bunch of stuff that makes `ripgrep` so fast).

## Performance

Benchmarked on an 16-inch MacBook Pro with M1 Max chip with 64 GB of memory and 4 TB SSD. Linting was done over `home-assistant/core` commit `fc281b2faecdc86c0a716dfb4a1f243dde632297`.

<details>

<summary>`.editorconfig` used</summary>

```yaml
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

</details>

`editorconfig-checker` 3.6.0 (without `-disable-indent-size`, as edcfg-lint doesn't do this check at all):

```
  Time (mean ± σ):      3.668 s ±  0.021 s    [User: 18.099 s, System: 2.113 s]
  Range (min … max):    3.655 s …  3.692 s    3 runs
```

`edcfg-lint`:

```
  Time (mean ± σ):     432.1 ms ± 133.7 ms    [User: 1394.8 ms, System: 2495.2 ms]
  Range (min … max):   322.0 ms … 586.6 ms    5 runs
```

`edcfg-lint` is ~8x faster.
