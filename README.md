# eddy

A fast `editorconfig` linter written in Rust. It is many times faster than [`editorconfig-checker`](https://github.com/editorconfig-checker/editorconfig-checker).

Still a work in progress, in particular the harness is mostly vibe-coded and needs a serious refresh.

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

`editorconfig-checker` 3.6.0:

```
  Time (mean ± σ):     11.137 s ±  0.055 s    [User: 84.998 s, System: 3.541 s]
  Range (min … max):   11.051 s … 11.185 s    5 runs
```

`eddy`:

```
  Time (mean ± σ):     432.1 ms ± 133.7 ms    [User: 1394.8 ms, System: 2495.2 ms]
  Range (min … max):   322.0 ms … 586.6 ms    5 runs
```

`eddy` is 25.8x faster.