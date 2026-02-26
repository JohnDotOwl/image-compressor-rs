# image-compressor-rs

Fast Rust CLI and library for image compression and optimization using best-in-class encoders — compress JPEG, PNG, WebP, and AVIF images with mozjpeg, oxipng, libwebp, and ravif.

## Features

- **Superior compression** using the same specialized engines as Facebook, Google, and Mozilla — not generic ImageMagick
- **Batch image compression** across entire directories with recursive support
- **Four optimized encoders** — mozjpeg (JPEG), oxipng (PNG), libwebp (WebP), ravif (AVIF)
- **Lossy and lossless modes** for WebP and AVIF output
- **Resize images** during compression with fit or exact modes (Lanczos3)
- **Metadata stripping** by default for smaller files (EXIF, ICC profiles)
- **Progressive JPEG** support for faster web loading
- **Fully self-contained** — no external tools like ImageMagick required

| Format | Engine | Notes |
|--------|--------|-------|
| JPEG | **mozjpeg** | ~10% better compression than libjpeg |
| PNG | **oxipng** | Lossless PNG optimizer (like optipng, in pure Rust) |
| WebP | **libwebp** | Google's WebP encoder, lossy + lossless |
| AVIF | **ravif** (rav1e) | Best compression ratios available today |

## Installation

### From source

```bash
git clone https://github.com/JohnDotOwl/image-compressor-rs.git
cd image-compressor-rs
cargo install --path .
```

### Prerequisites

- Rust 1.85+ (edition 2024)
- `nasm` for mozjpeg SIMD optimizations: `brew install nasm` (macOS) or `apt install nasm` (Linux)

## Usage

### Compress a single image

```bash
# JPEG compression with mozjpeg
image-compressor-rs compress photo.jpg out.jpg --quality 80

# Convert + compress to WebP
image-compressor-rs compress photo.png out.webp --quality 85

# Lossless WebP
image-compressor-rs compress photo.png out.webp --lossless

# AVIF with custom speed
image-compressor-rs compress photo.jpg out.avif --quality 70 --avif-speed 2

# Resize + compress
image-compressor-rs compress photo.jpg out.jpg --quality 80 --resize 1920x1080
```

### Batch compress a directory

```bash
# Convert all images to WebP
image-compressor-rs batch ./images/ ./compressed/ --to webp --quality 85

# Recursive with AVIF output
image-compressor-rs batch ./images/ ./compressed/ --to avif --recursive --quality 70

# Optimize PNGs in-place (lossless)
image-compressor-rs batch ./icons/ ./icons-opt/ --to png --png-level 4
```

### Command reference

| Flag | Description | Default |
|------|-------------|---------|
| `--quality <1-100>` | Compression quality | 85 (JPEG/WebP), 80 (AVIF) |
| `--lossless` | Lossless mode (WebP, AVIF) | false |
| `--progressive` | Progressive JPEG | false |
| `--keep-metadata` | Preserve EXIF/metadata | false (strip) |
| `--resize <WxH>` | Resize dimensions | none |
| `--resize-mode <fit\|exact>` | Resize strategy | fit |
| `--overwrite` | Overwrite existing files | false |
| `--png-level <1-6>` | PNG optimization level | 2 |
| `--avif-speed <1-10>` | AVIF encoding speed (1=slow/best) | 4 |
| `--to <FORMAT>` | Target format for batch (jpg/png/webp/avif) | — |
| `--recursive` | Process subdirectories (batch only) | false |

### Output

Single file:

```
compressed photo.png → photo.jpg (2.4 MB → 420 KB, saved 82.9%)
```

Batch:

```
compressed image1.png → image1.webp (2.4 MB → 180 KB, saved 92.5%)
compressed image2.jpg → image2.webp (1.1 MB → 340 KB, saved 69.1%)
batch complete: compressed=15, failed=0, skipped=2, saved 18.4 MB (74.2%)
```

## How It Works

```
Input File
    ↓
[Read file bytes + get original size]
    ↓
[Decode with image crate (auto-detect format)]
    ↓
[Resize if --resize specified] (Lanczos3 filter)
    ↓
[Determine output format from file extension]
    ↓
[Route to specialized encoder]
    ├── JPEG → mozjpeg (quality, progressive scanning)
    ├── PNG  → oxipng (optimization level, metadata stripping)
    ├── WebP → libwebp (quality, lossy/lossless)
    └── AVIF → ravif (quality, speed, lossy/lossless)
    ↓
[Write compressed bytes to output]
    ↓
[Print stats: original → compressed, savings %]
```

For PNG-to-PNG compression, the tool skips the decode/re-encode step and passes the raw bytes directly to oxipng for optimal lossless optimization.

## Supported Input Formats

JPEG, PNG, WebP, GIF, BMP, TIFF — any format the `image` crate can decode.

## Library Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
image-compressor-rs = "0.1"
```

```rust
use image_compressor_rs::{compress_image_file, CompressOptions};
use std::path::Path;

let options = CompressOptions {
    quality: Some(80),
    progressive: true,
    ..CompressOptions::default()
};

let stats = compress_image_file(
    Path::new("input.jpg"),
    Path::new("output.jpg"),
    &options,
).unwrap();

println!("saved {:.1}%", stats.savings_percent);
```

## Contributing

Contributions are welcome. To contribute:

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/my-feature`)
3. Commit your changes
4. Push to the branch and open a Pull Request

Please keep PRs focused and include tests where applicable.

## License

MIT License. See [LICENSE](LICENSE) for details.
