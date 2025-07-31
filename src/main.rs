use {
    glob::glob,
    image::RgbImage,
    log::{debug, error, info},
    num_traits::cast,
    rayon::prelude::*,
    std::{
        fs,
        io::Error,
        path::{Path, PathBuf},
        process::{Command, Stdio},
        time::Instant,
    },
    video_rs::{decode::Decoder, error::Error::DecodeExhausted},
};

const USE_SEEK: bool = false;
const USE_MULTICORE: bool = true;
const FRAME_SKIP: usize = 30;

const SEGMENT_TIME: &str = "5";
const SEGMENT_OUTPUT_PATTERN: &str = "segments/output_%09d.mp4";

const TEST_FILE: &str = "video.mp4";

fn get_files(pattern: &str) -> Vec<PathBuf> {
    glob(pattern)
        .expect("Failed to read glob pattern")
        .filter_map(|entry| {
            match entry {
                Ok(path) => {
                    debug!("Found segment: {}", path.display());
                    Some(path)
                },
                Err(e) => {
                    error!("Error processing path: {e:?}");
                    None
                },
            }
        })
        .collect()
}

fn remove_files(files_to_remove: &[PathBuf]) -> Result<(), Vec<Error>> {
    let errors: Vec<_> = files_to_remove
        .iter()
        .filter_map(|path| {
            if let Err(err) = fs::remove_file(path) {
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

fn cleanup() {
    let files: Vec<_> = ["frames/*.png", "segments/*.mp4"]
        .iter()
        .flat_map(|pattern| get_files(pattern))
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

fn remove_folder(path: &str) {
    match fs::remove_dir_all(path) {
        Ok(()) => {
            debug!("Successfully removed folder: {path}");
        },
        Err(e) => {
            error!("Error removing folder: {e}");
        },
    }
}

fn split_into_segments(source: &Path) -> Vec<PathBuf> {
    let source_path = source.to_str().expect("failed to convert to &str");

    let args = [
        "-v",
        "quiet",
        "-i",
        source_path,
        "-c",
        "copy",
        "-map",
        "0",
        "-segment_time",
        SEGMENT_TIME,
        "-f",
        "segment",
        "-reset_timestamps",
        "1",
        SEGMENT_OUTPUT_PATTERN,
    ];

    info!("Starting ffmpeg process in the background...");

    let result = Command::new("ffmpeg")
        .args(args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn();

    match result {
        Ok(mut child_process) => {
            info!("Waiting for ffmpeg (PID: {}) to finish...", child_process.id());

            let status = child_process.wait().expect("Failed to wait for process");

            info!("Process finished with status: {status}");
            if status.success() {
                info!("ffmpeg completed successfully!");
            } else {
                panic!("ffmpeg failed with exit code: {}", status.code().unwrap_or(-1));
            }
        },
        Err(e) => {
            error!("Failed to start ffmpeg process: {e}");
        },
    }

    get_files("segments/*.mp4")
}

fn read_by_dropping(prefix: &str, source: &Path) {
    let start = Instant::now();

    let mut decoder = Decoder::new(source).expect("failed to create decoder");

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
                let path = PathBuf::from(format!("frames/{prefix}_{n}.png"));

                save_rgb_to_image(rgb, width, height, &path);
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
}

fn read_by_seeks() {
    let start = Instant::now();

    let source = PathBuf::from(TEST_FILE);
    let mut decoder = Decoder::new(source).expect("failed to create decoder");

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

        save_rgb_to_image(rgb, width, height, path.as_path());
    });
    info!("Elapsed saving: {:.2?}", start.elapsed());
}

fn save_rgb_to_image(raw_pixels: &[u8], width: u32, height: u32, path: &Path) {
    let img_buffer: RgbImage = if let Some(img) = RgbImage::from_raw(width, height, raw_pixels.to_vec()) {
        img
    } else {
        error!("Error: Could not create ImageBuffer from raw data. Check dimensions and data size.");
        return;
    };

    match img_buffer.save(path) {
        Ok(()) => debug!("Image successfully saved to {}", path.display()),
        Err(e) => error!("Error saving image: {e}"),
    }
}

fn main() {
    tracing_subscriber::fmt::init();
    video_rs::init().expect("video-rs failed to initialize");

    fs::create_dir_all("frames").expect("failed to create frames directory");
    fs::create_dir_all("segments").expect("failed to create segments directory");

    cleanup();

    if USE_MULTICORE {
        let filename = PathBuf::from(TEST_FILE);
        let segments = split_into_segments(&filename);

        info!("Segments: {}", segments.len());

        let start = Instant::now();
        segments.par_iter().enumerate().for_each(|(n, path)| {
            let prefix = format!("segment-{n}");

            read_by_dropping(prefix.as_str(), path);
        });

        info!("Elapsed total: {:.2?}", start.elapsed());

        remove_folder("segments");

        return;
    }

    if USE_SEEK {
        // seems like this method does not work
        read_by_seeks();
    } else {
        let filename = PathBuf::from(TEST_FILE);
        read_by_dropping("full", &filename);
    }
}
