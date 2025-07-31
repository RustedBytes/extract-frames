use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use glob::glob;
use image::RgbImage;
use log::{debug, error, info};
use num_traits::cast;
use rayon::prelude::*;
use video_rs::decode::Decoder;

fn main() {
    tracing_subscriber::fmt::init();
    video_rs::init().unwrap();

    let use_seek = false;
    let use_multicore = true;

    if use_multicore {
        let filename = PathBuf::from("video.mp4");
        let segments = split_into_segments(&filename);

        info!("Segments: {}", segments.len());

        let start = Instant::now();
        segments.par_iter().enumerate().for_each(|(n, path)| {
            let prefix = format!("segment-{n}");

            read_by_dropping(prefix.as_str(), path.clone());
        });

        info!("Elapsed total: {:.2?}", start.elapsed());

        return;
    }

    if use_seek {
        // seems like this method does not work
        read_by_seeks();
    } else {
        let filename = PathBuf::from("video.webm");
        read_by_dropping("full", filename);
    }
}

fn split_into_segments(source: &Path) -> Vec<PathBuf> {
    let args = [
        "-v",
        "quiet",
        "-i",
        source.to_str().unwrap(),
        "-c",
        "copy",
        "-map",
        "0",
        "-segment_time",
        "5",
        "-f",
        "segment",
        "-reset_timestamps",
        "1",
        "segments/output_%03d.mp4",
    ];

    info!("Starting ffmpeg process in the background...");

    let result = Command::new("ffmpeg")
        .args(args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn();

    match result {
        Ok(mut child_process) => {
            info!(
                "Waiting for ffmpeg (PID: {}) to finish...",
                child_process.id()
            );

            // Block and wait for the process to complete
            let status = child_process.wait().expect("Failed to wait for process");

            info!("Process finished with status: {status}");
            if status.success() {
                info!("ffmpeg completed successfully!");
            } else {
                error!(
                    "ffmpeg failed with exit code: {}",
                    status.code().unwrap_or(-1)
                );
            }
        }
        Err(e) => {
            error!("Failed to start ffmpeg process: {e}");
        }
    }

    let mut segments = Vec::new();
    let paths = glob("segments/*.mp4").expect("Failed to read glob pattern");

    for entry in paths {
        match entry {
            Ok(path) => {
                info!("Found segment: {}", path.display());

                segments.push(path);
            }
            Err(e) => error!("Error processing path: {e:?}"),
        }
    }

    segments
}

fn read_by_dropping(prefix: &str, source: PathBuf) {
    let start = Instant::now();

    let mut decoder = Decoder::new(source).expect("failed to create decoder");

    let (width, height) = decoder.size();
    let fps = f64::from(decoder.frame_rate());

    debug!("Width: {width}, height: {height}");
    debug!("FPS: {fps}");

    for (n, frame) in decoder.decode_iter().enumerate() {
        if !n.is_multiple_of(30) {
            debug!("skipping {n} frame, prefix: {prefix}");
            continue;
        }

        if let Ok((ts, frame)) = frame {
            let frame_time = ts.as_secs_f64();
            debug!("Frame time: {frame_time}");

            let rgb = frame.to_owned().into_raw_vec_and_offset().0;
            let path = PathBuf::from(format!("frames/{prefix}_{n}.png"));

            save_rgb_to_image(&rgb, width, height, path.as_path());
        } else {
            break;
        }
    }

    info!("Elapsed prefix {prefix}: {:.2?}", start.elapsed());
}

fn read_by_seeks() {
    let start = Instant::now();

    let source = PathBuf::from("video.mp4");
    let mut decoder = Decoder::new(source).expect("failed to create decoder");

    let fps = f64::from(decoder.frame_rate());
    let duration_time = decoder.duration().unwrap();
    let duration_seconds = duration_time.as_secs_f64();
    let (width, height) = decoder.size();

    debug!("Width: {width}, height: {height}");

    if duration_seconds < 0.0 {
        let frame_count = cast(decoder.frames().unwrap()).unwrap_or(0);
        let duration_seconds = if fps > 0.0 {
            f64::from(frame_count) / fps
        } else {
            0.0
        };

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

                        let rgb = frame.to_owned().into_raw_vec_and_offset().0;

                        frames_decoded.push(rgb);
                    }
                    Err(e) => {
                        error!("Error decoding frame: {e}");
                    }
                }
            }
            Err(e) => {
                error!("Error seeking to frame: {e}");
            }
        }
    }

    info!("Elapsed: {:.2?}", start.elapsed());
    info!("RGBs: {}", frames_decoded.len());

    let start = Instant::now();
    frames_decoded.par_iter().enumerate().for_each(|(n, rgb)| {
        let path = PathBuf::from(format!("frames/{n}.png"));

        save_rgb_to_image(rgb, width, height, path.as_path());
    });
    info!("Elapsed saving: {:.2?}", start.elapsed());
}

fn save_rgb_to_image(raw_pixels: &[u8], width: u32, height: u32, path: &Path) {
    let img_buffer: RgbImage =
        if let Some(img) = RgbImage::from_raw(width, height, raw_pixels.to_vec()) {
            img
        } else {
            error!(
                "Error: Could not create ImageBuffer from raw data. Check dimensions and data size."
            );
            return;
        };

    match img_buffer.save(path) {
        Ok(()) => debug!("Image successfully saved to {}", path.display()),
        Err(e) => error!("Error saving image: {e}"),
    }
}
