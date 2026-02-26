use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use image_compressor_rs::{
    BatchReport, CompressOptions, ResizeMode, ResizeOptions, compress_directory,
    compress_image_file, format_size,
};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "image-compressor-rs",
    version,
    about = "Fast image compression CLI using best-in-class encoders (mozjpeg, oxipng, webp, ravif)"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compress a single image file
    Compress {
        /// Input image path
        input: PathBuf,
        /// Output image path (format determined by extension)
        output: PathBuf,
        #[arg(long, value_parser = clap::value_parser!(u8).range(1..=100))]
        quality: Option<u8>,
        /// Lossless mode (WebP, AVIF)
        #[arg(long, default_value_t = false)]
        lossless: bool,
        /// Progressive JPEG
        #[arg(long, default_value_t = false)]
        progressive: bool,
        /// Preserve EXIF/metadata (default: strip)
        #[arg(long, default_value_t = false)]
        keep_metadata: bool,
        /// Resize dimensions (WIDTHxHEIGHT)
        #[arg(long, value_parser = parse_resize)]
        resize: Option<ResizeInput>,
        /// Resize strategy
        #[arg(long, value_enum, default_value_t = ResizeModeArg::Fit)]
        resize_mode: ResizeModeArg,
        /// Overwrite existing output files
        #[arg(long, default_value_t = false)]
        overwrite: bool,
        /// PNG optimization level (1-6)
        #[arg(long, value_parser = clap::value_parser!(u8).range(1..=6))]
        png_level: Option<u8>,
        /// AVIF encoding speed (1=slow/best, 10=fast)
        #[arg(long, value_parser = clap::value_parser!(u8).range(1..=10))]
        avif_speed: Option<u8>,
    },
    /// Compress all images in a directory
    Batch {
        /// Input directory
        input_dir: PathBuf,
        /// Output directory
        output_dir: PathBuf,
        /// Target format (jpg, png, webp, avif)
        #[arg(long, value_name = "FORMAT")]
        to: String,
        /// Process subdirectories
        #[arg(long, default_value_t = false)]
        recursive: bool,
        #[arg(long, value_parser = clap::value_parser!(u8).range(1..=100))]
        quality: Option<u8>,
        /// Lossless mode (WebP, AVIF)
        #[arg(long, default_value_t = false)]
        lossless: bool,
        /// Progressive JPEG
        #[arg(long, default_value_t = false)]
        progressive: bool,
        /// Preserve EXIF/metadata (default: strip)
        #[arg(long, default_value_t = false)]
        keep_metadata: bool,
        /// Resize dimensions (WIDTHxHEIGHT)
        #[arg(long, value_parser = parse_resize)]
        resize: Option<ResizeInput>,
        /// Resize strategy
        #[arg(long, value_enum, default_value_t = ResizeModeArg::Fit)]
        resize_mode: ResizeModeArg,
        /// Overwrite existing output files
        #[arg(long, default_value_t = false)]
        overwrite: bool,
        /// PNG optimization level (1-6)
        #[arg(long, value_parser = clap::value_parser!(u8).range(1..=6))]
        png_level: Option<u8>,
        /// AVIF encoding speed (1=slow/best, 10=fast)
        #[arg(long, value_parser = clap::value_parser!(u8).range(1..=10))]
        avif_speed: Option<u8>,
    },
}

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Compress {
            input,
            output,
            quality,
            lossless,
            progressive,
            keep_metadata,
            resize,
            resize_mode,
            overwrite,
            png_level,
            avif_speed,
        } => {
            let options = build_compress_options(
                overwrite,
                quality,
                lossless,
                progressive,
                keep_metadata,
                resize,
                resize_mode,
                png_level,
                avif_speed,
            )?;

            let stats = compress_image_file(&input, &output, &options).with_context(|| {
                format!(
                    "failed to compress {} \u{2192} {}",
                    input.display(),
                    output.display()
                )
            })?;

            let input_name = input.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            let output_name = output.file_name().and_then(|n| n.to_str()).unwrap_or("?");
            println!(
                "compressed {} \u{2192} {} ({} \u{2192} {}, saved {:.1}%)",
                input_name,
                output_name,
                format_size(stats.original_bytes),
                format_size(stats.compressed_bytes),
                stats.savings_percent,
            );
        }
        Commands::Batch {
            input_dir,
            output_dir,
            to,
            recursive,
            quality,
            lossless,
            progressive,
            keep_metadata,
            resize,
            resize_mode,
            overwrite,
            png_level,
            avif_speed,
        } => {
            let options = build_compress_options(
                overwrite,
                quality,
                lossless,
                progressive,
                keep_metadata,
                resize,
                resize_mode,
                png_level,
                avif_speed,
            )?;

            let report = compress_directory(&input_dir, &output_dir, &to, &options, recursive)
                .with_context(|| {
                    format!(
                        "failed batch compression from {} to {}",
                        input_dir.display(),
                        output_dir.display()
                    )
                })?;

            print_batch_summary(&report);
        }
    }

    Ok(())
}

fn print_batch_summary(report: &BatchReport) {
    let total_saved = report
        .total_original_bytes
        .saturating_sub(report.total_compressed_bytes);
    let savings_percent = if report.total_original_bytes > 0 {
        (1.0 - report.total_compressed_bytes as f64 / report.total_original_bytes as f64) * 100.0
    } else {
        0.0
    };

    println!(
        "batch complete: compressed={}, failed={}, skipped={}, saved {} ({:.1}%)",
        report.compressed,
        report.failed,
        report.skipped,
        format_size(total_saved),
        savings_percent,
    );
}

#[derive(Clone, Copy, Debug)]
struct ResizeInput {
    width: u32,
    height: u32,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ResizeModeArg {
    Fit,
    Exact,
}

impl From<ResizeModeArg> for ResizeMode {
    fn from(value: ResizeModeArg) -> Self {
        match value {
            ResizeModeArg::Fit => ResizeMode::Fit,
            ResizeModeArg::Exact => ResizeMode::Exact,
        }
    }
}

fn parse_resize(value: &str) -> std::result::Result<ResizeInput, String> {
    let normalized = value.trim().to_ascii_lowercase();
    let (width, height) = normalized
        .split_once('x')
        .ok_or_else(|| "resize must be in WIDTHxHEIGHT format (example: 1920x1080)".to_string())?;

    let width = width
        .parse::<u32>()
        .map_err(|_| "resize width must be an integer".to_string())?;
    let height = height
        .parse::<u32>()
        .map_err(|_| "resize height must be an integer".to_string())?;

    if width == 0 || height == 0 {
        return Err("resize width and height must be greater than zero".to_string());
    }

    Ok(ResizeInput { width, height })
}

#[allow(clippy::too_many_arguments)]
fn build_compress_options(
    overwrite: bool,
    quality: Option<u8>,
    lossless: bool,
    progressive: bool,
    keep_metadata: bool,
    resize: Option<ResizeInput>,
    resize_mode: ResizeModeArg,
    png_level: Option<u8>,
    avif_speed: Option<u8>,
) -> Result<CompressOptions> {
    let resize = resize
        .map(|value| ResizeOptions::new(value.width, value.height, resize_mode.into()))
        .transpose()?;

    Ok(CompressOptions {
        overwrite,
        quality,
        lossless,
        progressive,
        strip_metadata: !keep_metadata,
        resize,
        png_level,
        avif_speed,
    })
}
