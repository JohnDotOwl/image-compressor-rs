use anyhow::{Context, Result, bail};
use image::imageops::FilterType;
use image::{DynamicImage, ImageFormat};
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Supported compression output formats
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Jpeg,
    Png,
    WebP,
    Avif,
}

impl OutputFormat {
    pub fn from_extension(extension: &str) -> Result<Self> {
        match normalize_extension(extension)?.as_str() {
            "jpg" | "jpeg" => Ok(Self::Jpeg),
            "png" => Ok(Self::Png),
            "webp" => Ok(Self::WebP),
            "avif" => Ok(Self::Avif),
            other => bail!("unsupported output format: {other}"),
        }
    }
}

/// How to resize
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeMode {
    Fit,
    Exact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResizeOptions {
    pub width: u32,
    pub height: u32,
    pub mode: ResizeMode,
}

impl ResizeOptions {
    pub fn new(width: u32, height: u32, mode: ResizeMode) -> Result<Self> {
        if width == 0 || height == 0 {
            bail!("resize width and height must be greater than zero");
        }
        Ok(Self {
            width,
            height,
            mode,
        })
    }
}

/// Main configuration for compression
#[derive(Debug, Clone, Copy)]
pub struct CompressOptions {
    pub overwrite: bool,
    pub quality: Option<u8>,
    pub lossless: bool,
    pub progressive: bool,
    pub strip_metadata: bool,
    pub resize: Option<ResizeOptions>,
    pub png_level: Option<u8>,
    pub avif_speed: Option<u8>,
}

impl Default for CompressOptions {
    fn default() -> Self {
        Self {
            overwrite: false,
            quality: None,
            lossless: false,
            progressive: false,
            strip_metadata: true,
            resize: None,
            png_level: None,
            avif_speed: None,
        }
    }
}

/// Stats for a single compression operation
#[derive(Debug, Clone, Copy)]
pub struct CompressionStats {
    pub original_bytes: u64,
    pub compressed_bytes: u64,
    pub savings_percent: f64,
}

/// Batch operation report
#[derive(Debug, Default, Clone, Copy)]
pub struct BatchReport {
    pub compressed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub total_original_bytes: u64,
    pub total_compressed_bytes: u64,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn compress_image_file(
    input: &Path,
    output: &Path,
    options: &CompressOptions,
) -> Result<CompressionStats> {
    validate_input_and_output(input, output, options)?;

    let input_bytes = fs::read(input)
        .with_context(|| format!("failed to read input file: {}", input.display()))?;
    let original_bytes = input_bytes.len() as u64;

    let ext = output
        .extension()
        .and_then(|v| v.to_str())
        .context("output path must include a file extension")?;
    let format = OutputFormat::from_extension(ext)?;

    // Special case: PNG input → PNG output without resize — run oxipng directly
    let compressed = if format == OutputFormat::Png && options.resize.is_none() {
        let is_png = image::guess_format(&input_bytes)
            .map(|f| f == ImageFormat::Png)
            .unwrap_or(false);
        if is_png {
            compress_png(&input_bytes, None, options)?
        } else {
            let image = decode_and_resize(&input_bytes, options)?;
            compress_png(&[], Some(&image), options)?
        }
    } else {
        let image = decode_and_resize(&input_bytes, options)?;
        match format {
            OutputFormat::Jpeg => compress_jpeg(&image, options)?,
            OutputFormat::Png => compress_png(&[], Some(&image), options)?,
            OutputFormat::WebP => compress_webp(&image, options)?,
            OutputFormat::Avif => compress_avif(&image, options)?,
        }
    };

    fs::write(output, &compressed)
        .with_context(|| format!("failed to write output file: {}", output.display()))?;

    let compressed_bytes = compressed.len() as u64;
    let savings_percent = if original_bytes > 0 {
        (1.0 - compressed_bytes as f64 / original_bytes as f64) * 100.0
    } else {
        0.0
    };

    Ok(CompressionStats {
        original_bytes,
        compressed_bytes,
        savings_percent,
    })
}

pub fn compress_directory(
    input_dir: &Path,
    output_dir: &Path,
    to_extension: &str,
    options: &CompressOptions,
    recursive: bool,
) -> Result<BatchReport> {
    if !input_dir.is_dir() {
        bail!("input directory not found: {}", input_dir.display());
    }

    fs::create_dir_all(output_dir).with_context(|| {
        format!(
            "failed to create output directory: {}",
            output_dir.display()
        )
    })?;

    let to_extension = normalize_extension(to_extension)?;
    let files = collect_input_files(input_dir, recursive)?;
    let mut report = BatchReport::default();

    for source_path in files {
        let Ok(relative_path) = source_path.strip_prefix(input_dir) else {
            report.failed += 1;
            continue;
        };

        let mut target_path = output_dir.join(relative_path);
        target_path.set_extension(&to_extension);

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).ok();
        }

        if target_path.exists() && !options.overwrite {
            report.skipped += 1;
            continue;
        }

        let source_name = source_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?");
        let target_name = target_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?");

        match compress_image_file(&source_path, &target_path, options) {
            Ok(stats) => {
                println!(
                    "compressed {} \u{2192} {} ({} \u{2192} {}, saved {:.1}%)",
                    source_name,
                    target_name,
                    format_size(stats.original_bytes),
                    format_size(stats.compressed_bytes),
                    stats.savings_percent,
                );
                report.compressed += 1;
                report.total_original_bytes += stats.original_bytes;
                report.total_compressed_bytes += stats.compressed_bytes;
            }
            Err(err) => {
                eprintln!("failed {}: {err:#}", source_name);
                report.failed += 1;
            }
        }
    }

    Ok(report)
}

// ---------------------------------------------------------------------------
// Format-specific encoders
// ---------------------------------------------------------------------------

fn compress_jpeg(image: &DynamicImage, options: &CompressOptions) -> Result<Vec<u8>> {
    let rgb = image.to_rgb8();
    let (width, height) = (rgb.width() as usize, rgb.height() as usize);
    let pixels = rgb.as_raw();

    let mut comp = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_RGB);
    comp.set_size(width, height);
    comp.set_quality(options.quality.unwrap_or(85) as f32);

    if options.progressive {
        comp.set_scan_optimization_mode(mozjpeg::ScanMode::AllComponentsTogether);
    }

    let mut comp = comp.start_compress(Vec::new())?;
    comp.write_scanlines(pixels)
        .context("failed to write JPEG scanlines")?;
    let result = comp.finish()?;

    Ok(result)
}

fn compress_png(
    input_bytes: &[u8],
    image: Option<&DynamicImage>,
    options: &CompressOptions,
) -> Result<Vec<u8>> {
    let png_bytes = if let Some(img) = image {
        let mut buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Png)
            .context("failed to encode PNG")?;
        buf
    } else {
        input_bytes.to_vec()
    };

    let level = options.png_level.unwrap_or(2);
    let mut opts = oxipng::Options::from_preset(level);
    if options.strip_metadata {
        opts.strip = oxipng::StripChunks::Safe;
    }

    oxipng::optimize_from_memory(&png_bytes, &opts).context("PNG optimization failed")
}

fn compress_webp(image: &DynamicImage, options: &CompressOptions) -> Result<Vec<u8>> {
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    let encoder = webp::Encoder::from_rgba(rgba.as_raw(), width, height);

    let memory = if options.lossless {
        encoder.encode_lossless()
    } else {
        let quality = options.quality.unwrap_or(85) as f32;
        encoder.encode(quality)
    };

    Ok(memory.to_vec())
}

fn compress_avif(image: &DynamicImage, options: &CompressOptions) -> Result<Vec<u8>> {
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();

    let pixels: Vec<rgb::RGBA8> = rgba
        .pixels()
        .map(|p| rgb::RGBA8::new(p[0], p[1], p[2], p[3]))
        .collect();

    let img = imgref::Img::new(pixels, width as usize, height as usize);

    let quality = if options.lossless {
        100.0
    } else {
        options.quality.unwrap_or(80) as f32
    };
    let speed = options.avif_speed.unwrap_or(4);

    let encoder = ravif::Encoder::new()
        .with_quality(quality)
        .with_speed(speed)
        .with_alpha_quality(quality);

    let result = encoder
        .encode_rgba(img.as_ref())
        .context("AVIF encoding failed")?;

    Ok(result.avif_file)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn decode_and_resize(bytes: &[u8], options: &CompressOptions) -> Result<DynamicImage> {
    let mut image = if let Ok(format) = image::guess_format(bytes) {
        image::load_from_memory_with_format(bytes, format).context("failed to decode image")?
    } else {
        image::load_from_memory(bytes).context("failed to decode image")?
    };

    if let Some(resize) = options.resize {
        image = resize_image(image, resize);
    }

    Ok(image)
}

fn resize_image(image: DynamicImage, resize: ResizeOptions) -> DynamicImage {
    match resize.mode {
        ResizeMode::Fit => image.resize(resize.width, resize.height, FilterType::Lanczos3),
        ResizeMode::Exact => image.resize_exact(resize.width, resize.height, FilterType::Lanczos3),
    }
}

fn validate_input_and_output(input: &Path, output: &Path, options: &CompressOptions) -> Result<()> {
    if !input.is_file() {
        bail!("input file not found: {}", input.display());
    }

    if output.exists() && !options.overwrite {
        bail!(
            "output file exists (use --overwrite to replace): {}",
            output.display()
        );
    }

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory: {}", parent.display()))?;
    }

    Ok(())
}

fn collect_input_files(input_dir: &Path, recursive: bool) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    if recursive {
        for entry in WalkDir::new(input_dir)
            .into_iter()
            .filter_map(|entry| entry.ok())
        {
            if entry.file_type().is_file() {
                files.push(entry.into_path());
            }
        }
    } else {
        for entry in fs::read_dir(input_dir)
            .with_context(|| format!("failed to read directory: {}", input_dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                files.push(path);
            }
        }
    }

    Ok(files)
}

fn normalize_extension(extension: &str) -> Result<String> {
    let extension = extension.trim().trim_start_matches('.');
    if extension.is_empty() {
        bail!("format/extension cannot be empty");
    }
    Ok(extension.to_ascii_lowercase())
}

pub fn format_size(bytes: u64) -> String {
    if bytes >= 1_000_000 {
        format!("{:.1} MB", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{:.0} KB", bytes as f64 / 1_000.0)
    } else {
        format!("{} B", bytes)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_supported_output_format_extensions() {
        assert_eq!(
            OutputFormat::from_extension("jpg").unwrap(),
            OutputFormat::Jpeg
        );
        assert_eq!(
            OutputFormat::from_extension("jpeg").unwrap(),
            OutputFormat::Jpeg
        );
        assert_eq!(
            OutputFormat::from_extension("png").unwrap(),
            OutputFormat::Png
        );
        assert_eq!(
            OutputFormat::from_extension("webp").unwrap(),
            OutputFormat::WebP
        );
        assert_eq!(
            OutputFormat::from_extension("avif").unwrap(),
            OutputFormat::Avif
        );
    }

    #[test]
    fn reject_unknown_output_extension() {
        assert!(OutputFormat::from_extension("bmp").is_err());
        assert!(OutputFormat::from_extension("gif").is_err());
        assert!(OutputFormat::from_extension("tiff").is_err());
    }

    #[test]
    fn normalize_extension_values() {
        assert_eq!(normalize_extension(" JPG ").unwrap(), "jpg");
        assert_eq!(normalize_extension(".WebP").unwrap(), "webp");
        assert_eq!(normalize_extension("AVIF").unwrap(), "avif");
    }

    #[test]
    fn reject_empty_extension() {
        assert!(normalize_extension("").is_err());
        assert!(normalize_extension("  ").is_err());
        assert!(normalize_extension(".").is_err());
    }

    #[test]
    fn validate_resize_bounds() {
        assert!(ResizeOptions::new(0, 200, ResizeMode::Fit).is_err());
        assert!(ResizeOptions::new(200, 0, ResizeMode::Fit).is_err());
        assert!(ResizeOptions::new(200, 100, ResizeMode::Fit).is_ok());
    }

    #[test]
    fn default_compress_options() {
        let opts = CompressOptions::default();
        assert!(!opts.overwrite);
        assert!(opts.quality.is_none());
        assert!(!opts.lossless);
        assert!(!opts.progressive);
        assert!(opts.strip_metadata);
        assert!(opts.resize.is_none());
        assert!(opts.png_level.is_none());
        assert!(opts.avif_speed.is_none());
    }

    #[test]
    fn format_size_display() {
        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(1_500), "2 KB");
        assert_eq!(format_size(2_400_000), "2.4 MB");
    }
}
