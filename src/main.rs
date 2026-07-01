#[cfg(test)]
mod tests;

use ffmpeg_next::format::{Pixel, input};
use ffmpeg_next::media::Type;
use ffmpeg_next::software::scaling::{context::Context as ScalingContext, flag::Flags};
use ffmpeg_next::util::frame::video::Video;
use image::{
    ColorType, ExtendedColorType, ImageEncoder, RgbImage,
    codecs::{jpeg::JpegEncoder, png::PngEncoder},
    imageops::FilterType as ResizeFilterType,
};
use num_traits::{ToPrimitive, cast};
use oxipng::Options as OxipngOptions;
use {
    anyhow::{Context, Error, Result, anyhow, bail},
    clap::{Parser, ValueEnum},
    glob::glob,
    log::{debug, error, info},
    rayon::prelude::*,
    std::{
        env,
        fs::{create_dir_all, remove_dir_all, remove_file},
        path::{Path, PathBuf},
        process::{Command, Stdio},
        time::Instant,
    },
    video_rs::{decode::Decoder, error::Error::DecodeExhausted},
};

/// Command line argument parser using clap derive macro
/// Defines the CLI interface for the frame extraction application
///
/// # Fields
/// * `file` - Input video file path (default: "video.mp4")
/// * `use_seek` - Enable seek-based frame extraction method
/// * `multicore` - Enable parallel processing using multiple CPU cores
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the input video file to process
    ///
    /// Specifies the video file from which frames will be extracted. The file
    /// must be in a format supported by the underlying video processing
    /// library.
    ///
    /// # Default
    /// If not specified, defaults to "video.mp4" in the current directory.
    ///
    /// # Supported Formats
    /// Common formats like MP4, AVI, MOV, MKV are typically supported, though
    /// actual support depends on the system's codec installation.
    #[arg(short, long, default_value = "video.mp4")]
    file: PathBuf,

    /// Enable seek-based frame extraction method
    ///
    /// When enabled, extracts exactly one frame per second by seeking to
    /// specific timestamps rather than processing sequentially. This method
    /// is more accurate for temporal sampling but significantly slower due
    /// to seek overhead.
    ///
    /// # Performance Impact
    /// * Much slower than sequential processing due to seek operations
    /// * More CPU intensive due to decoding from keyframes
    /// * May skip frames in areas with sparse keyframes
    /// * Incompatible with --multicore flag
    #[arg(long)]
    use_seek: bool,

    /// Enable multi-core parallel processing
    ///
    /// When enabled, splits the input video into time-based segments and
    /// processes them in parallel using all available CPU cores. This can
    /// significantly reduce processing time for large videos on multi-core
    /// systems.
    ///
    /// # Requirements
    /// * ffmpeg must be installed and available in system PATH
    /// * Sufficient disk space for temporary segment files
    /// * Incompatible with --use-seek flag
    #[arg(long, action = clap::ArgAction::SetTrue)]
    multicore: bool,

    /// Number of frames to skip between extracted frames
    ///
    /// Controls the extraction frequency by specifying how many frames to skip
    /// between each extracted frame. For example, with a 30fps video, setting
    /// this to 30 will extract 1 frame per second. Lower values extract more
    /// frames and create larger output.
    ///
    /// # Default
    /// If not specified, defaults to 30 frames.
    ///
    /// # Examples
    /// * 15 for 30fps video = 2 frames per second
    /// * 30 for 30fps video = 1 frame per second
    /// * 60 for 30fps video = 1 frame every 2 seconds
    #[arg(long, default_value_t = 30)]
    frames_between: usize,

    /// Resize output images to this width in pixels
    ///
    /// Can be used with --output-height for exact dimensions, or on its own
    /// to preserve the source aspect ratio.
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
    output_width: Option<u32>,

    /// Resize output images to this height in pixels
    ///
    /// Can be used with --output-width for exact dimensions, or on its own
    /// to preserve the source aspect ratio.
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
    output_height: Option<u32>,

    /// Output image format for extracted frames
    #[arg(long, value_enum, default_value_t = ImageFormat::Png)]
    output_format: ImageFormat,

    /// JPEG quality from 1 to 100
    ///
    /// Only applies when --output-format jpeg is used. Lower values produce
    /// smaller files with more visible compression artifacts.
    #[arg(long, default_value_t = 90, value_parser = clap::value_parser!(u8).range(1..=100))]
    jpeg_quality: u8,

    /// PNG compression level
    ///
    /// Higher compression usually creates smaller files but takes longer.
    #[arg(long, value_enum, default_value_t = PngCompression::Default)]
    png_compression: PngCompression,

    /// Disable lossless PNG optimization with oxipng
    ///
    /// When enabled, PNG files are written directly after image encoding.
    /// This can make PNG output faster at the cost of larger files.
    #[arg(long, action = clap::ArgAction::SetTrue)]
    no_png_optimization: bool,

    /// Render all extracted frames into one combined image
    ///
    /// When enabled, extracted frames are arranged left-to-right in an
    /// automatically sized near-square grid and saved as one image at
    /// frames/full-pane.<format> instead of saving each frame separately.
    #[arg(long, action = clap::ArgAction::SetTrue)]
    output_full_pane: bool,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ImageFormat {
    Png,
    Jpeg,
}

impl ImageFormat {
    fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum PngCompression {
    Fast,
    Default,
    Best,
}

impl PngCompression {
    fn oxipng_preset(self) -> u8 {
        match self {
            Self::Fast => 1,
            Self::Default => 2,
            Self::Best => 6,
        }
    }

    fn oxipng_options(self) -> OxipngOptions {
        OxipngOptions::from_preset(self.oxipng_preset())
    }
}

impl std::fmt::Display for PngCompression {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PngCompression::Fast => f.write_str("fast"),
            PngCompression::Default => f.write_str("default"),
            PngCompression::Best => f.write_str("best"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct OutputOptions {
    width: Option<u32>,
    height: Option<u32>,
    format: ImageFormat,
    jpeg_quality: u8,
    png_compression: PngCompression,
    optimize_png: bool,
}

impl From<&Args> for OutputOptions {
    fn from(args: &Args) -> Self {
        Self {
            width: args.output_width,
            height: args.output_height,
            format: args.output_format,
            jpeg_quality: args.jpeg_quality,
            png_compression: args.png_compression,
            optimize_png: !args.no_png_optimization,
        }
    }
}

#[derive(Debug)]
struct ExtractedFrame {
    source_index: usize,
    image: RgbImage,
}

/// Duration in seconds for each video segment when splitting videos
/// for parallel processing. Default is 5 seconds per segment.
const SEGMENT_DURATION_SECONDS: f64 = 5.0;

/// File naming pattern for ffmpeg segment output files using printf-style
/// formatting %09d creates zero-padded 9-digit numbers (e.g.,
/// `output_000000001.mp4`)
const SEGMENT_OUTPUT_PATTERN: &str = "segments/output_%09d.mp4";

/// Glob patterns to match all frame images in the frames directory.
/// Used for cleanup operations and file enumeration.
const FRAME_FILES_PATTERNS: &[&str] = &["frames/*.png", "frames/*.jpg", "frames/*.jpeg"];

/// Glob pattern to match all MP4 segment files in the segments directory
/// Used for finding and cleaning up temporary segment files after processing
const SEGMENTED_FILES_PATTERN: &str = "segments/*.mp4";

/// Finds all files matching the given glob pattern and returns their paths.
///
/// This function wraps the glob crate functionality with proper error handling
/// and converts the results to `AsRef<Path>` instances. It's used throughout
/// the application for discovering video segments, frame images, and other
/// generated files.
///
/// # Arguments
/// * `path` - A path pattern supporting glob wildcards (*, ?, \[abc\], etc.)
///
/// # Returns
/// * `Ok(Vec<impl AsRef<Path>>)` - Vector of matching file paths
/// * `Err` - If the pattern is invalid UTF-8 or glob matching fails
///
/// # Examples
/// ```
/// let paths = get_files("frames/*.png")?; // Find all PNG frames
/// let segments = get_files("segments/*.mp4")?; // Find all MP4 segments
/// ```
fn get_files(path: impl AsRef<Path>) -> Result<Vec<impl AsRef<Path>>> {
    let pattern_str = path
        .as_ref()
        .to_str()
        .ok_or_else(|| anyhow!("Invalid UTF-8 path for glob pattern: {}", path.as_ref().display()))?;

    let paths = glob(pattern_str)
        .with_context(|| format!("Failed to read glob pattern '{pattern_str}'"))?
        .filter_map(Result::ok)
        .collect();

    Ok(paths)
}

/// Attempts to remove all files in the specified slice with batch error
/// handling.
///
/// Unlike `std::fs::remove_file` which stops at the first error, this function
/// attempts to remove all specified files and collects all encountered errors.
/// This is useful for cleanup operations where partial success is acceptable.
///
/// # Arguments
/// * `paths` - Slice of file paths to attempt removal on
///
/// # Returns
/// * `Ok(())` if all files were successfully removed or if the input was empty
/// * `Err(anyhow::Error)` containing an error encountered during removal
///   attempt
///
/// # Logging
/// * Logs successful removals at debug level
/// * Logs individual file removal errors at error level
fn remove_files(paths: &[impl AsRef<Path>]) -> Result<(), Error> {
    let errors: Vec<_> = paths
        .iter()
        .filter_map(|path| {
            let path = path.as_ref();
            if let Err(err) = remove_file(path) {
                error!("Failed to remove file {}: {err}", path.display());
                Some(err)
            } else {
                debug!("Successfully removed file: {}", path.display());
                None
            }
        })
        .collect();

    if !errors.is_empty() {
        return Err(anyhow!("Failed to remove files, enable logging to see them"));
    }

    Ok(())
}

/// Cleans up the working directories by removing frame images in the `frames`
/// folder and all MP4 segments in the `segments` folder. Logs the result.
fn cleanup_temporary_files() -> Result<(), Error> {
    let paths: Vec<_> = FRAME_FILES_PATTERNS
        .iter()
        .chain([SEGMENTED_FILES_PATTERN].iter())
        .filter_map(|pattern| get_files(pattern).ok())
        .flatten()
        .collect();

    remove_files(&paths)
}

/// Removes a directory and all its contents recursively.
///
/// This function wraps `std::fs::remove_dir_all` with additional error context
/// to provide meaningful error messages when directory removal fails.
///
/// # Arguments
/// * `path` - Path to the directory to remove
///
/// # Returns
/// * `Ok(())` if directory was successfully removed
/// * `Err` with context if removal failed
///
/// # Examples
/// ```
/// remove_folder(Path::new("temporary_files"))?;
/// ```
fn remove_folder(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    remove_dir_all(path).with_context(|| format!("Failed to remove folder '{}'", path.display()))
}

/// Uses ffmpeg to split the source video file into several segments.
///
/// This function performs stream copying (not re-encoding) to split a large
/// video into smaller time-based segments. It's designed for parallel
/// processing scenarios where multiple cores can work on different segments
/// simultaneously. The segmentation preserves video quality while enabling
/// parallel frame extraction.
///
/// # Arguments
/// * `path` - Path to the source video file to be segmented
/// * `segment_output_pattern` - ffmpeg formatting pattern for output filenames
///   (e.g., "output_%09d.mp4" creates `output_000000001.mp4`,
///   `output_000000002.mp4`, etc.)
/// * `segmented_files_pattern` - glob pattern to find the created segment files
///
/// # Returns
/// * `Ok(Vec<PathBuf>)` - Paths to all generated segment files
/// * `Err` - If ffmpeg fails or file discovery encounters errors
///
/// # Examples
/// ```
/// let segments = split_into_segments(
///     Path::new("input.mp4"),
///     "segments/output_%09d.mp4",
///     "segments/*.mp4",
/// )?;
/// assert!(!segments.is_empty());
/// ```
///
/// # ffmpeg Parameters Explained
/// * `-v quiet` - Suppress most ffmpeg output
/// * `-c copy` - Stream copy (no re-encoding, very fast)
/// * `-map 0` - Copy all streams from input
/// * `-segment_time` - Target duration of each segment
/// * `-f segment` - Use segment muxer for splitting
/// * `-reset_timestamps 1` - Reset timestamps for each segment
fn split_into_segments(
    path: impl AsRef<Path>,
    segment_output_pattern: &str,
    segmented_files_path: impl AsRef<Path>,
) -> Result<Vec<impl AsRef<Path>>> {
    info!("Starting ffmpeg process in the background...");

    let mut child_process = Command::new("ffmpeg")
        .arg("-v")
        .arg("quiet")
        .arg("-i")
        .arg(path.as_ref())
        .arg("-c")
        .arg("copy")
        .arg("-map")
        .arg("0")
        .arg("-segment_time")
        .arg(SEGMENT_DURATION_SECONDS.to_string())
        .arg("-f")
        .arg("segment")
        .arg("-reset_timestamps")
        .arg("1")
        .arg(segment_output_pattern)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to start ffmpeg process")?;

    let status = child_process.wait().context("Failed to wait for ffmpeg process")?;

    if !status.success() {
        bail!("ffmpeg failed with exit code: {}", status.code().unwrap_or(-1));
    }

    get_files(segmented_files_path)
}

/// Decodes video frames by dropping frames according to
/// `frames_between_extracted` parameter.
///
/// This function implements the basic frame extraction method that processes
/// videos sequentially. It's memory-efficient and works well for smaller
/// videos or single-core processing. The `frames_between_extracted` parameter
/// determines which frames are extracted (e.g., every 30th frame for 30fps
/// video = 1fps output).
///
/// # Arguments
/// * `frame_prefix` - String prefix for output PNG filenames (e.g., "segment-1"
///   creates "segment-1_0.png")
/// * `video_path` - Source video file to decode
/// * `frames_path` - Directory where PNG frame images will be saved
/// * `frames_between_extracted` - Number of frames to skip between extracted
///   frames
///
/// # Performance Notes
/// * Frames are processed in decode order without seeking (faster)
/// * Memory usage scales with `frames_between_extracted` value (lower = more
///   memory)
/// * Single-threaded operation unless called within parallel context
fn decode_frames_dropping(
    frame_prefix: &str,
    video_path: impl AsRef<Path>,
    frames_path: impl AsRef<Path>,
    frames_between_extracted: usize,
    output_options: OutputOptions,
) -> Result<()> {
    let frames = extract_frames_dropping(
        video_path,
        frames_path.as_ref(),
        frames_between_extracted,
        output_options,
    )?;

    for frame in frames {
        let path = frames_path.as_ref().join(format!(
            "{frame_prefix}_{}.{}",
            frame.source_index,
            output_options.format.extension()
        ));
        write_rgb_image(&frame.image, &path, output_options)?;
    }

    Ok(())
}

fn extract_frames_dropping(
    video_path: impl AsRef<Path>,
    frames_path: impl AsRef<Path>,
    frames_between_extracted: usize,
    output_options: OutputOptions,
) -> Result<Vec<ExtractedFrame>> {
    let video_path = video_path.as_ref();
    let frames_path = frames_path.as_ref();

    if !video_path.exists() {
        bail!("Input video path does not exist: {video_path:?}");
    }
    if !frames_path.exists() {
        bail!("Output frames path does not exist: {frames_path:?}");
    }

    let start = Instant::now();

    let mut decoder = Decoder::new(video_path).context("failed to create decoder")?;

    let (width, height) = decoder.size();
    let fps = f64::from(decoder.frame_rate());

    debug!("Width: {width}, height: {height}");
    debug!("FPS: {fps}");

    let mut frames = Vec::new();

    for (n, frame_result) in decoder.decode_iter().enumerate().step_by(frames_between_extracted) {
        match frame_result {
            Ok((ts, frame)) => {
                let frame_time = ts.as_secs_f64();
                debug!("Frame time: {frame_time}");

                if let Some(rgb) = frame.as_slice() {
                    frames.push(ExtractedFrame {
                        source_index: n,
                        image: rgb_to_image(rgb, width, height, output_options)?,
                    });
                } else {
                    error!("Failed to get frame buffer as slice for frame {n}");
                }
            },
            Err(e) => {
                if let DecodeExhausted = e {
                    info!("Decoding finished, stream exhausted");
                    break;
                }
                error!("Decoding failed: {e:?}");
            },
        }
    }

    info!("Elapsed frame extraction: {:.2?}", start.elapsed());

    Ok(frames)
}

/// Decodes one frame per second by seeking to specific timestamps.
///
/// This function uses precise seeking to extract exactly one
/// frame per second of video. It's more accurate for consistent temporal
/// sampling but significantly slower due to seek overhead.
///
/// # Approach
/// 1. Calculate video duration and determine target timestamps (1s, 2s, 3s,
///    ...)
/// 2. Seek to each timestamp and decode one frame
/// 3. Save all frames in parallel using rayon
///
/// # Limitations
/// * Seek accuracy depends on video keyframe spacing
/// * May skip frames in areas with sparse keyframes
/// * Higher CPU usage due to seeking overhead
fn decode_frames_seeking(
    frame_prefix: &str,
    video_path: impl AsRef<Path>,
    frames_path: impl AsRef<Path>,
    output_options: OutputOptions,
) -> Result<()> {
    let frames = extract_frames_seeking(video_path, output_options)?;

    for frame in frames {
        let path = frames_path.as_ref().join(format!(
            "{frame_prefix}_{}.{}",
            frame.source_index,
            output_options.format.extension()
        ));
        write_rgb_image(&frame.image, &path, output_options)?;
    }

    Ok(())
}

fn extract_frames_seeking(video_path: impl AsRef<Path>, output_options: OutputOptions) -> Result<Vec<ExtractedFrame>> {
    let mut ictx = input(&video_path)?;

    let input_stream = ictx
        .streams()
        .best(Type::Video)
        .ok_or(ffmpeg_next::Error::StreamNotFound)?;
    let video_stream_index = input_stream.index();

    let duration: f64 = cast(ictx.duration()).ok_or(ffmpeg_next::Error::from(ffmpeg_next::ffi::EINVAL))?;
    let duration_secs = if ictx.duration() == ffmpeg_next::ffi::AV_NOPTS_VALUE {
        0.0
    } else {
        duration / f64::from(ffmpeg_next::ffi::AV_TIME_BASE)
    };

    let duration_sec_int: i64 = cast(duration_secs).ok_or(ffmpeg_next::Error::from(ffmpeg_next::ffi::EINVAL))?;
    let fps = input_stream.rate().numerator();

    let context_decoder = ffmpeg_next::codec::context::Context::from_parameters(input_stream.parameters())?;
    let mut video_decoder = context_decoder.decoder().video()?;

    let width = video_decoder.width();
    let height = video_decoder.height();

    debug!("Width: {width}, height: {height}");
    debug!("Total duration: {duration_secs:.2} seconds");
    debug!("FPS: {fps}");

    let mut scaler = ScalingContext::get(
        video_decoder.format(),
        width,
        height,
        Pixel::RGB24,
        width,
        height,
        Flags::BILINEAR,
    )?;

    let receive_and_process_frame = |decoder: &mut ffmpeg_next::decoder::Video,
                                     scaler: &mut ScalingContext,
                                     n: i64|
     -> Result<ExtractedFrame, Error> {
        let mut decoded = Video::empty();
        if decoder.receive_frame(&mut decoded).is_ok() {
            let mut rgb_frame = Video::empty();
            scaler.run(&decoded, &mut rgb_frame)?;

            Ok(ExtractedFrame {
                source_index: n.to_usize().context("Frame index exceeds supported size")?,
                image: rgb_to_image(rgb_frame.data(0), width, height, output_options)?,
            })
        } else {
            // This can happen if the packet didn't contain a full frame
            Err(Error::from(ffmpeg_next::Error::from(ffmpeg_next::ffi::EAGAIN)))
        }
    };

    let mut frames = Vec::new();

    let tb = i64::from(ffmpeg_next::ffi::AV_TIME_BASE);
    for n in 0..duration_sec_int as i64 {
        let seek_target = n * tb;

        ictx.seek(seek_target, ..seek_target)?;

        for (stream, packet) in ictx.packets() {
            if stream.index() == video_stream_index {
                video_decoder.send_packet(&packet)?;
                if let Ok(frame) = receive_and_process_frame(&mut video_decoder, &mut scaler, n) {
                    frames.push(frame);
                    // Frame found and saved, break to the next second
                    break;
                }
            }
        }
    }

    Ok(frames)
}

/// Saves raw RGB pixel data as an image at the specified path.
///
/// Takes a slice of raw RGB pixel data and creates a PNG image file with
/// the specified dimensions. The function handles the conversion from raw
/// bytes to image format and saves the result to disk.
///
/// # Arguments
/// * `raw_pixels` - A slice of raw RGB pixel data (3 bytes per pixel)
/// * `width` - The width of the image in pixels
/// * `height` - The height of the image in pixels
/// * `path` - The destination file path where the image will be saved
///
/// # Returns
/// * `Ok(())` if image was successfully saved
/// * `Err` if conversion or saving failed
///
/// # Panics
/// This function does not panic but returns errors for invalid inputs.
///
/// # Examples
/// ```
/// let red_pixel = [255u8, 0, 0];
/// let pixels = red_pixel.repeat(4); // 2x2 image
/// save_rgb_to_image(&pixels, 2, 2, Path::new("red_square.png"))?;
/// ```
fn calculate_output_size(width: u32, height: u32, output_options: OutputOptions) -> Result<(u32, u32)> {
    match (output_options.width, output_options.height) {
        (Some(output_width), Some(output_height)) => Ok((output_width, output_height)),
        (Some(output_width), None) => {
            let output_height = scaled_dimension(height, output_width, width)?;
            Ok((output_width, output_height))
        },
        (None, Some(output_height)) => {
            let output_width = scaled_dimension(width, output_height, height)?;
            Ok((output_width, output_height))
        },
        (None, None) => Ok((width, height)),
    }
}

fn scaled_dimension(dimension: u32, target_other_dimension: u32, source_other_dimension: u32) -> Result<u32> {
    if source_other_dimension == 0 {
        bail!("Cannot resize image with zero source dimension");
    }

    let scaled = u64::from(dimension)
        .checked_mul(u64::from(target_other_dimension))
        .context("Resized image dimension overflowed")?
        / u64::from(source_other_dimension);

    scaled
        .max(1)
        .to_u32()
        .context("Resized image dimension exceeds supported size")
}

fn rgb_to_image(raw_pixels: &[u8], width: u32, height: u32, output_options: OutputOptions) -> Result<RgbImage> {
    let img_buffer =
        RgbImage::from_raw(width, height, raw_pixels.to_vec()).context("Could not create RgbImage from raw data.")?;

    let (output_width, output_height) = calculate_output_size(width, height, output_options)?;
    let img_buffer = if output_width == width && output_height == height {
        img_buffer
    } else {
        image::imageops::resize(&img_buffer, output_width, output_height, ResizeFilterType::Lanczos3)
    };

    Ok(img_buffer)
}

#[cfg(test)]
fn save_rgb_to_image(
    raw_pixels: &[u8],
    width: u32,
    height: u32,
    path: impl AsRef<Path>,
    output_options: OutputOptions,
) -> Result<()> {
    let img_buffer = rgb_to_image(raw_pixels, width, height, output_options)?;
    write_rgb_image(&img_buffer, path, output_options)
}

fn write_rgb_image(img_buffer: &RgbImage, path: impl AsRef<Path>, output_options: OutputOptions) -> Result<()> {
    match output_options.format {
        ImageFormat::Png => {
            let mut png_data = Vec::new();
            let encoder = PngEncoder::new(&mut png_data);
            encoder
                .write_image(
                    img_buffer.as_raw(),
                    img_buffer.width(),
                    img_buffer.height(),
                    ExtendedColorType::Rgb8,
                )
                .context("Error encoding PNG image")?;

            let output_png = if output_options.optimize_png {
                oxipng::optimize_from_memory(&png_data, &output_options.png_compression.oxipng_options())
                    .with_context(|| format!("Error optimizing PNG with oxipng {}", output_options.png_compression))?
            } else {
                png_data
            };
            std::fs::write(path.as_ref(), output_png)
                .with_context(|| format!("Error saving PNG image {}", path.as_ref().display()))?;
        },
        ImageFormat::Jpeg => {
            let mut output = std::fs::File::create(path.as_ref())
                .with_context(|| format!("Error creating image {}", path.as_ref().display()))?;
            let encoder = JpegEncoder::new_with_quality(&mut output, output_options.jpeg_quality);
            encoder
                .write_image(
                    img_buffer.as_raw(),
                    img_buffer.width(),
                    img_buffer.height(),
                    ColorType::Rgb8.into(),
                )
                .context("Error saving JPEG image")?;
        },
    }

    Ok(())
}

fn calculate_full_pane_grid(frame_count: usize) -> Result<(usize, usize)> {
    if frame_count == 0 {
        bail!("Cannot render full pane without extracted frames");
    }

    let columns = (frame_count as f64)
        .sqrt()
        .ceil()
        .to_usize()
        .context("Grid column count overflowed")?;
    let rows = frame_count.div_ceil(columns);

    Ok((columns, rows))
}

fn render_full_pane(frames: &[ExtractedFrame], path: impl AsRef<Path>, output_options: OutputOptions) -> Result<()> {
    let first_frame = frames
        .first()
        .context("Cannot render full pane without extracted frames")?;
    let tile_width = first_frame.image.width();
    let tile_height = first_frame.image.height();

    for frame in frames {
        if frame.image.width() != tile_width || frame.image.height() != tile_height {
            bail!("Cannot render full pane from frames with different dimensions");
        }
    }

    let (columns, rows) = calculate_full_pane_grid(frames.len())?;
    let columns_u32 = columns.to_u32().context("Grid column count exceeds supported size")?;
    let rows_u32 = rows.to_u32().context("Grid row count exceeds supported size")?;
    let canvas_width = tile_width
        .checked_mul(columns_u32)
        .context("Full pane image width overflowed")?;
    let canvas_height = tile_height
        .checked_mul(rows_u32)
        .context("Full pane image height overflowed")?;
    let mut pane = RgbImage::new(canvas_width, canvas_height);

    for (n, frame) in frames.iter().enumerate() {
        let column = n % columns;
        let row = n / columns;
        let x = tile_width
            .checked_mul(column.to_u32().context("Grid column index exceeds supported size")?)
            .context("Full pane x offset overflowed")?;
        let y = tile_height
            .checked_mul(row.to_u32().context("Grid row index exceeds supported size")?)
            .context("Full pane y offset overflowed")?;

        image::imageops::overlay(&mut pane, &frame.image, i64::from(x), i64::from(y));
    }

    write_rgb_image(&pane, path, output_options)
}

fn full_pane_output_path(frames_path: impl AsRef<Path>, output_options: OutputOptions) -> PathBuf {
    frames_path
        .as_ref()
        .join(format!("full-pane.{}", output_options.format.extension()))
}

/// Main entry point for the frame extraction application.
///
/// Parses command line arguments, initializes dependencies, and executes
/// the frame extraction process based on user selections. Supports
/// sequential processing, seek-based extraction, and parallel segment
/// processing.
///
/// # Processing Modes
/// * Standard (default) - Sequential frame dropping with
///   `FRAMES_BETWEEN_EXTRACTED` interval
/// * Seek-based (--use-seek) - Extract one frame per second using seeking
/// * Parallel (--multicore) - Process video segments in parallel
///
/// # Workflow
/// 1. Initialize logging and video processing libraries
/// 2. Create frames/ and segments/ directories
/// 3. Clean up previous files
/// 4. Process video based on selected mode
/// 5. Remove temporary segment files
///
/// # Arguments
/// See Args struct for detailed command line options.
///
/// # Returns
/// * `Ok(())` on successful completion
/// * `Err` with error details if processing fails
///
/// # Example Usage
/// ```bash
/// # Basic frame extraction (every 30th frame)
/// cargo run -- --file input.mp4
///
/// # Extract one frame per second
/// cargo run -- --file input.mp4 --use-seek
///
/// # Parallel processing for large videos
/// cargo run -- --file input.mp4 --multicore
/// ```
fn main() -> Result<(), Error> {
    let args = Args::parse();

    tracing_subscriber::fmt::init();

    create_dir_all("frames").context("failed to create frames directory")?;
    create_dir_all("segments").context("failed to create segments directory")?;

    cleanup_temporary_files()?;

    let path = env::current_dir().context("failed to get current path")?;
    let frames_path = path.join("frames");
    let output_options = OutputOptions::from(&args);

    if args.multicore {
        video_rs::init().expect("video-rs failed to initialize");

        let segments = split_into_segments(&args.file, SEGMENT_OUTPUT_PATTERN, SEGMENTED_FILES_PATTERN)?;

        info!("Segments: {}", segments.len());

        let start = Instant::now();
        let frames_between = args.frames_between;
        if args.output_full_pane {
            let mut segment_paths: Vec<PathBuf> = segments.iter().map(|path| path.as_ref().to_path_buf()).collect();
            segment_paths.sort();

            let mut segment_frames: Vec<_> = segment_paths
                .par_iter()
                .enumerate()
                .map(|(n, path)| {
                    extract_frames_dropping(path, &frames_path, frames_between, output_options)
                        .map(|frames| (n, frames))
                })
                .collect::<Result<Vec<_>>>()?;
            segment_frames.sort_by_key(|(n, _)| *n);

            let frames: Vec<_> = segment_frames.into_iter().flat_map(|(_, frames)| frames).collect();
            render_full_pane(
                &frames,
                full_pane_output_path(&frames_path, output_options),
                output_options,
            )?;
        } else {
            segments.par_iter().enumerate().for_each(|(n, path)| {
                let prefix = format!("segment-{n}");

                if let Err(e) = decode_frames_dropping(&prefix, path, &frames_path, frames_between, output_options) {
                    error!("Error processing segment {n}: {e:?}");
                }
            });
        }

        info!("Elapsed total: {:.2?}", start.elapsed());
    } else if args.use_seek {
        ffmpeg_next::init().expect("ffmpeg-next failed to initialize");

        if args.output_full_pane {
            let frames = extract_frames_seeking(&args.file, output_options)?;
            render_full_pane(
                &frames,
                full_pane_output_path(&frames_path, output_options),
                output_options,
            )?;
        } else {
            decode_frames_seeking("full", &args.file, &frames_path, output_options)?;
        }
    } else {
        if args.output_full_pane {
            let frames = extract_frames_dropping(&args.file, &frames_path, args.frames_between, output_options)?;
            render_full_pane(
                &frames,
                full_pane_output_path(&frames_path, output_options),
                output_options,
            )?;
        } else {
            decode_frames_dropping("full", &args.file, &frames_path, args.frames_between, output_options)?;
        }
    }

    let segments_dir = Path::new("segments");
    remove_folder(segments_dir)?;

    Ok(())
}
