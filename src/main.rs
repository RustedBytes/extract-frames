#[cfg(test)]
mod tests;

use {
    anyhow::{Context, Result},
    clap::Parser,
    glob::glob,
    image::RgbImage,
    log::{debug, error, info},
    num_traits::cast,
    rayon::prelude::*,
    std::{
        env,
        fs::{create_dir_all, remove_dir_all, remove_file},
        io::Error,
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
}

/// Number of frames to skip between extracted frames
/// For 30fps video, skipping 30 frames extracts 1 frame per second
/// Adjust this value to control extraction frequency and output size
const FRAMES_BETWEEN_EXTRACTED: usize = 30;

/// Duration in seconds for each video segment when splitting videos
/// for parallel processing. Default is 5 seconds per segment.
const SEGMENT_DURATION_SECONDS: f64 = 5.0;

/// File naming pattern for ffmpeg segment output files using printf-style
/// formatting %09d creates zero-padded 9-digit numbers (e.g.,
/// `output_000000001.mp4`)
const SEGMENT_OUTPUT_PATTERN: &str = "segments/output_%09d.mp4";

/// Glob pattern to match all PNG frame images in the frames directory
/// Used for cleanup operations and file enumeration
const FRAME_FILES_PATTERN: &str = "frames/*.png";

/// Glob pattern to match all MP4 segment files in the segments directory
/// Used for finding and cleaning up temporary segment files after processing
const SEGMENTED_FILES_PATTERN: &str = "segments/*.mp4";

/// Finds all files matching the given glob pattern and returns their paths.
///
/// This function wraps the glob crate functionality with proper error handling
/// and converts the results to owned `PathBuf` instances. It's used throughout
/// the application for discovering video segments, frame images, and other
/// generated files.
///
/// # Arguments
/// * `path` - A path pattern supporting glob wildcards (*, ?, \[abc\], etc.)
///
/// # Returns
/// * `Ok(Vec<PathBuf>)` - Vector of matching file paths
/// * `Err` - If the pattern is invalid UTF-8 or glob matching fails
///
/// # Examples
/// ```
/// let paths = get_files("frames/*.png")?; // Find all PNG frames
/// let segments = get_files("segments/*.mp4")?; // Find all MP4 segments
/// ```
fn get_files(path: impl AsRef<Path>) -> Result<Vec<PathBuf>> {
    let pattern_str = path
        .as_ref()
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid UTF-8 path for glob pattern: {}", path.as_ref().display()))?;

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
/// * `Err(Vec<Error>)` containing all errors encountered during removal
///   attempts
///
/// # Logging
/// * Logs successful removals at debug level
/// * Logs individual file removal errors at error level
fn remove_files(paths: &[PathBuf]) -> Result<(), Vec<Error>> {
    let errors: Vec<_> = paths
        .iter()
        .filter_map(|path| {
            if let Err(err) = remove_file(path) {
                error!("Failed to remove file {}: {}", path.display(), err);
                Some(err)
            } else {
                debug!("Successfully removed file: {}", path.display());
                None
            }
        })
        .collect();

    if errors.is_empty() { Ok(()) } else { Err(errors) }
}

/// Cleans up the working directories by removing all PNG images in the `frames`
/// folder and all MP4 segments in the `segments` folder. Logs the result.
fn cleanup_temporary_files() {
    let paths: Vec<_> = [FRAME_FILES_PATTERN, SEGMENTED_FILES_PATTERN]
        .iter()
        .filter_map(|pattern| get_files(pattern).ok())
        .flatten()
        .collect();

    match remove_files(&paths) {
        Ok(()) => {
            info!("All previous items (n={}) were successfully removed.", paths.len());
        },
        Err(errors) => {
            error!("Encountered {} errors during file cleanup.", errors.len());
        },
    }
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
fn remove_folder(path: &Path) -> Result<()> {
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
    path: &Path,
    segment_output_pattern: &str,
    segmented_files_path: impl AsRef<Path>,
) -> Result<Vec<PathBuf>> {
    info!("Starting ffmpeg process in the background...");

    let mut child_process = Command::new("ffmpeg")
        .arg("-v")
        .arg("quiet")
        .arg("-i")
        .arg(path)
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
        anyhow::bail!("ffmpeg failed with exit code: {}", status.code().unwrap_or(-1));
    }

    get_files(segmented_files_path)
}

/// Decodes video frames by dropping frames according to
/// `FRAMES_BETWEEN_EXTRACTED` constant.
///
/// This function implements the basic frame extraction method that processes
/// videos sequentially. It's memory-efficient and works well for smaller
/// videos or single-core processing. The `FRAMES_BETWEEN_EXTRACTED` constant
/// determines which frames are extracted (e.g., every 30th frame for 30fps
/// video = 1fps output).
///
/// # Arguments
/// * `frame_prefix` - String prefix for output PNG filenames (e.g., "segment-1"
///   creates "segment-1_0.png")
/// * `video_path` - Source video file to decode
/// * `frames_path` - Directory where PNG frame images will be saved
///
/// # Performance Notes
/// * Frames are processed in decode order without seeking (faster)
/// * Memory usage scales with `FRAMES_BETWEEN_EXTRACTED` value (lower = more
///   memory)
/// * Single-threaded operation unless called within parallel context
fn decode_frames_dropping(frame_prefix: &str, video_path: &Path, frames_path: &Path) -> Result<()> {
    if !video_path.exists() {
        anyhow::bail!("Input video path does not exist: {}", video_path.display());
    }
    if !frames_path.exists() {
        anyhow::bail!("Output frames path does not exist: {}", frames_path.display());
    }

    let start = Instant::now();

    let mut decoder = Decoder::new(video_path).context("failed to create decoder")?;

    let (width, height) = decoder.size();
    let fps = f64::from(decoder.frame_rate());

    debug!("Width: {width}, height: {height}");
    debug!("FPS: {fps}");

    for (n, frame_result) in decoder.decode_iter().enumerate().step_by(FRAMES_BETWEEN_EXTRACTED) {
        match frame_result {
            Ok((ts, frame)) => {
                let frame_time = ts.as_secs_f64();
                debug!("Frame time: {frame_time}");

                if let Some(rgb) = frame.as_slice() {
                    let path = frames_path.join(format!("{frame_prefix}_{n}.png"));
                    save_rgb_to_image(rgb, width, height, &path)?;
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

    info!("Elapsed frame {frame_prefix}: {:.2?}", start.elapsed());

    Ok(())
}

/// Decodes one frame per second by seeking to specific timestamps.
///
/// This experimental function uses precise seeking to extract exactly one
/// frame per second of video. It's more accurate for consistent temporal
/// sampling but significantly slower due to seek overhead. Uses rayon for
/// parallel PNG saving to improve performance.
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
fn decode_frames_seeking(video_path: &Path) -> Result<()> {
    let start = Instant::now();

    let mut decoder = Decoder::new(video_path).context("failed to create decoder")?;

    let fps = f64::from(decoder.frame_rate());
    let duration_time = decoder.duration().expect("failed to get duration");
    let duration_seconds = duration_time.as_secs_f64();
    let (width, height) = decoder.size();

    debug!("Width: {width}, height: {height}");

    if duration_seconds < 0.0 {
        let n_frames = decoder.frames().expect("failed to get number of frames");
        let frame_count = cast(n_frames).unwrap_or(0);
        let duration_seconds = if fps > 0.0 { f64::from(frame_count) / fps } else { 0.0 };

        info!("Frame count: {frame_count}");
        info!("Estimated duration (from frames): {duration_seconds} seconds");
    } else {
        info!("Duration: {duration_seconds} seconds");
    }

    info!("FPS: {fps}");

    let mut frames_decoded = Vec::new();

    let duration_in_seconds = cast(duration_seconds.ceil()).unwrap_or(0);
    let fps_c = cast(fps.ceil()).unwrap_or(0);

    for second in 0..duration_in_seconds {
        let frame_number = second * fps_c;

        match decoder.seek_to_frame(frame_number) {
            Ok(()) => {
                debug!("Successfully sought to frame near index {frame_number}.");

                match decoder.decode() {
                    Ok((ts, frame)) => {
                        let frame_time = ts.as_secs_f64();
                        debug!("Frame time: {frame_time}");

                        if let Some(rgb) = frame.as_slice() {
                            frames_decoded.push(rgb.to_vec());
                        } else {
                            error!("Failed to get frame buffer as slice for frame");
                        }
                    },
                    Err(e) => {
                        error!("Error decoding frame: {e}");
                    },
                }
            },
            Err(e) => {
                error!("Error seeking to frame: {e}");
            },
        }
    }

    info!("Elapsed: {:.2?}", start.elapsed());
    info!("Frames decoded: {}", frames_decoded.len());

    let start = Instant::now();
    frames_decoded.par_iter().enumerate().for_each(|(n, rgb)| {
        let path = PathBuf::from(format!("frames/{n}.png"));

        if let Err(e) = save_rgb_to_image(rgb, width, height, path.as_path()) {
            error!("Error saving image {n}: {e:?}");
        }
    });
    info!("Elapsed saving: {:.2?}", start.elapsed());

    Ok(())
}

/// Saves raw RGB pixel data as a PNG image at the specified path.
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
fn save_rgb_to_image(raw_pixels: &[u8], width: u32, height: u32, path: &Path) -> Result<()> {
    let img_buffer: RgbImage = RgbImage::from_raw(width, height, raw_pixels.to_vec())
        .context("Could not create ImageBuffer from raw data.")?;

    img_buffer.save(path).context("Error saving image")?;
    debug!("Image successfully saved to {}", path.display());

    Ok(())
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
fn main() -> Result<()> {
    let args = Args::parse();

    tracing_subscriber::fmt::init();
    video_rs::init().expect("video-rs failed to initialize");

    create_dir_all("frames").context("failed to create frames directory")?;
    create_dir_all("segments").context("failed to create segments directory")?;

    cleanup_temporary_files();

    let path = env::current_dir().context("failed to get current path")?;
    let frames_path = path.join("frames");

    if args.multicore {
        let segments = split_into_segments(&args.file, SEGMENT_OUTPUT_PATTERN, SEGMENTED_FILES_PATTERN)?;

        info!("Segments: {}", segments.len());

        let start = Instant::now();
        segments.par_iter().enumerate().for_each(|(n, path)| {
            let prefix = format!("segment-{n}");

            if let Err(e) = decode_frames_dropping(&prefix, path, &frames_path) {
                error!("Error processing segment {n}: {e:?}");
            }
        });

        info!("Elapsed total: {:.2?}", start.elapsed());
    } else if args.use_seek {
        // FIXME: The seek-based method is experimental and may not produce correct
        // output.
        decode_frames_seeking(&args.file)?;
    } else {
        decode_frames_dropping("full", &args.file, &frames_path)?;
    }

    let segments_dir = Path::new("segments");
    remove_folder(segments_dir)?;

    Ok(())
}
