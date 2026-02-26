# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-02-26

### Added

- Single file compression via `compress` subcommand
- Batch directory compression via `batch` subcommand with recursive support
- JPEG compression using mozjpeg encoder
- PNG optimization using oxipng
- WebP encoding using libwebp (lossy and lossless)
- AVIF encoding using ravif/rav1e (lossy and lossless)
- Resize during compression with fit and exact modes (Lanczos3)
- Progressive JPEG support
- Metadata stripping by default
- Compression stats output (original size, compressed size, savings %)
