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

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the video file
    #[arg(short, long, default_value = "video.mp4")]
    file: PathBuf,

    /// Use the seek method for frame extraction
    #[arg(long)]
    use_seek: bool,

    /// Use multi-core processing
    #[arg(long, action = clap::ArgAction::SetTrue)]
    multicore: bool,
}

const FRAME_SKIP: usize = 30;

const SEGMENT_TIME: &str = "5";
const SEGMENT_OUTPUT_PATTERN: &str = "segments/output_%09d.mp4";
const FRAME_FILES_PATTERN: &str = "frames/*.png";
const SEGMENTED_FILES_PATTERN: &str = "segments/*.mp4";

/// Finds all files matching the given glob pattern and returns their paths.
///
/// # Arguments
/// * `pattern` - A glob pattern as a string slice.
///
/// # Returns
/// * `Vec<PathBuf>` - A vector of found file paths.
fn get_files(pattern: &str) -> Result<Vec<PathBuf>> {
    let paths = glob(pattern)
        .with_context(|| format!("Failed to read glob pattern '{pattern}'"))?
        .filter_map(Result::ok)
        .collect();

    Ok(paths)
}

/// Attempts to remove all files in the specified slice.
///
/// This function will try to remove every file path provided. If any removal
/// operations fail, it will continue with the remaining files and then return
/// an `Err` containing a vector of all `std::io::Error`s encountered.
///
/// # Returns
/// * `Ok(())` if all files were successfully removed.
/// * `Err(Vec<Error>)` if one or more files could not be removed.
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
fn cleanup() {
    let files: Vec<_> = [FRAME_FILES_PATTERN, SEGMENTED_FILES_PATTERN]
        .iter()
        .flat_map(|pattern| get_files(pattern))
        .flatten()
        .collect();

    match remove_files(&files) {
        Ok(()) => {
            info!("All previous files {} items were successfully removed.", files.len());
        },
        Err(errors) => {
            error!("Encountered {} errors during file cleanup.", errors.len());
        },
    }
}

/// Removes a directory and its contents at the given path.
///
/// # Arguments
/// * `path` - The path to the folder to remove.
fn remove_folder(path: &Path) -> Result<()> {
    remove_dir_all(path).with_context(|| format!("Failed to remove folder '{}'", path.display()))
}

/// Uses ffmpeg to split the source video file into several segments and saves
/// them to disk. Waits for ffmpeg to finish, then returns the list of generated
/// segment paths.
///
/// # Arguments
/// * `path` - Path to the source video file.
/// * `segment_output_pattern` - The output pattern for ffmpeg to name segment
///   files (e.g., "output_%09d.mp4").
/// * `segmented_files_pattern` - A glob pattern to find the generated segment
///   files (e.g., "output_*.mp4").
///
/// # Returns
/// * `Result<Vec<PathBuf>>` - Paths to the generated video segments.
fn split_into_segments(
    path: &Path,
    segment_output_pattern: &str,
    segmented_files_pattern: &str,
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
        .arg(SEGMENT_TIME)
        .arg("-f")
        .arg("segment")
        .arg("-reset_timestamps")
        .arg("1")
        .arg(segment_output_pattern)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .context("Failed to start ffmpeg process")?;

    let status = child_process.wait().context("Failed to wait for ffmpeg process")?;

    if !status.success() {
        anyhow::bail!("ffmpeg failed with exit code: {}", status.code().unwrap_or(-1));
    }

    get_files(segmented_files_pattern)
}

/// Decodes video frames from the given source video by dropping frames
/// according to `FRAME_SKIP`, and saves each decoded frame as a PNG image with
/// the given prefix. Logs decoding progress and timing information.
///
/// # Arguments
/// * `prefix` - Prefix for the output PNG filenames.
/// * `video_path` - Path to the video file to decode.
/// * `frames_path` - Path to the frames directory.
fn read_by_dropping(prefix: &str, video_path: &Path, frames_path: &Path) -> Result<()> {
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

    for (n, frame_result) in decoder
        .decode_iter()
        .enumerate()
        .filter(|(n, _)| n.is_multiple_of(FRAME_SKIP))
    {
        match frame_result {
            Ok((ts, frame)) => {
                let frame_time = ts.as_secs_f64();
                debug!("Frame time: {frame_time}");

                let rgb = frame.as_slice().unwrap();
                let path = frames_path.join(format!("{prefix}_{n}.png"));

                save_rgb_to_image(rgb, width, height, &path)?;
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

    info!("Elapsed prefix {prefix}: {:.2?}", start.elapsed());

    Ok(())
}

/// Decodes one frame per second by seeking to each second of the video, then
/// saves all frames as PNG images. Uses multi-core processing to speed up
/// saving. Logs decoding and processing times.
fn read_by_seeks(video_path: &Path) -> Result<()> {
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

                        let rgb = frame.as_slice().unwrap();

                        frames_decoded.push(rgb.to_vec());
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
/// Logs success or error.
///
/// # Arguments
/// * `raw_pixels` - A slice of raw RGB pixel data.
/// * `width` - The width of the image.
/// * `height` - The height of the image.
/// * `path` - The destination file path.
fn save_rgb_to_image(raw_pixels: &[u8], width: u32, height: u32, path: &Path) -> Result<()> {
    let img_buffer: RgbImage = RgbImage::from_raw(width, height, raw_pixels.to_owned())
        .context("Could not create ImageBuffer from raw data.")?;

    img_buffer.save(path).context("Error saving image")?;
    debug!("Image successfully saved to {}", path.display());

    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();

    tracing_subscriber::fmt::init();
    video_rs::init().expect("video-rs failed to initialize");

    create_dir_all("frames").context("failed to create frames directory")?;
    create_dir_all("segments").context("failed to create segments directory")?;

    cleanup();

    let path = env::current_dir().context("failed to get current path")?;
    let frames_path = path.join("frames");

    if args.multicore {
        let segments = split_into_segments(&args.file, SEGMENT_OUTPUT_PATTERN, SEGMENTED_FILES_PATTERN)?;

        info!("Segments: {}", segments.len());

        let start = Instant::now();
        segments.par_iter().enumerate().for_each(|(n, path)| {
            let prefix = format!("segment-{n}");

            if let Err(e) = read_by_dropping(prefix.as_str(), path, &frames_path) {
                error!("Error processing segment {n}: {e:?}");
            }
        });

        info!("Elapsed total: {:.2?}", start.elapsed());
    } else if args.use_seek {
        // FIXME: The seek-based method is experimental and may not produce correct
        // output.
        read_by_seeks(&args.file)?;
    } else {
        read_by_dropping("full", &args.file, &frames_path)?;
    }

    let segments_dir = Path::new("segments");
    remove_folder(segments_dir)?;

    Ok(())
}
