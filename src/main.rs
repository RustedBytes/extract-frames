use std::path::{Path, PathBuf};
use std::time::Instant;

use image::RgbImage;
use rayon::prelude::*;
use video_rs::decode::Decoder;
use num_traits::cast;

fn main() {
    let use_seek = false;

    if use_seek {
        // seems like this method does not work
        read_by_seeks();
    } else {
        read_by_dropping();
    }
}

fn read_by_dropping() {
    tracing_subscriber::fmt::init();
    video_rs::init().unwrap();

    let start = Instant::now();

    let source = PathBuf::from("video.webm");
    let mut decoder = Decoder::new(source).expect("failed to create decoder");

    let (width, height) = decoder.size();
    let fps = f64::from(decoder.frame_rate());

    println!("Width: {width}, height: {height}");
    println!("FPS: {fps}");

    for (n, frame) in decoder.decode_iter().enumerate() {
        if !n.is_multiple_of(30) {
            println!("skipping {n} frame");
            continue;
        }

        if let Ok((ts, frame)) = frame {
            let frame_time = ts.as_secs_f64();
            println!("Frame time: {frame_time}");

            let rgb = frame.to_owned().into_raw_vec_and_offset().0;
            let path = PathBuf::from(format!("frames/{n}.png"));

            save_rgb_vec_to_image(&rgb, width, height, path.as_path());
        } else {
            break;
        }
    }

    println!("Elapsed: {:.2?}", start.elapsed());
}

fn read_by_seeks() {
    tracing_subscriber::fmt::init();
    video_rs::init().unwrap();

    let start = Instant::now();

    let source = PathBuf::from("video.mp4");
    let mut decoder = Decoder::new(source).expect("failed to create decoder");

    let fps = f64::from(decoder.frame_rate());
    let duration_time = decoder.duration().unwrap();
    let duration_seconds = duration_time.as_secs_f64();
    let (width, height) = decoder.size();

    println!("Width: {width}, height: {height}");

    if duration_seconds < 0.0 {
        let frame_count = cast(decoder.frames().unwrap()).unwrap_or(0);
        let duration_seconds = if fps > 0.0 { f64::from(frame_count) / fps } else { 0.0 };

        println!("Frame count: {frame_count}");
        println!("Estimated duration (from frames): {duration_seconds} seconds");
    } else {
        println!("Duration: {duration_seconds} seconds");
    }

    println!("FPS: {fps}");

    let mut frames_decoded = Vec::new();

    let duration_in_seconds = cast(duration_seconds.ceil()).unwrap_or(0);
    let fps_c = cast(fps.ceil()).unwrap_or(0);

    for second in 0..duration_in_seconds {
        let frame_number = second * fps_c;

        match decoder.seek_to_frame(frame_number) {
            Ok(()) => {
                println!("Successfully sought to frame near index {frame_number}.");

                match decoder.decode() {
                    Ok((ts, frame)) => {
                        let frame_time = ts.as_secs_f64();
                        println!("Frame time: {frame_time}");

                        let rgb = frame.to_owned().into_raw_vec_and_offset().0;

                        frames_decoded.push(rgb);
                    }
                    Err(e) => {
                        eprintln!("Error decoding frame: {e}");
                    }
                }
            }
            Err(e) => {
                eprintln!("Error seeking to frame: {e}");
            }
        }
    }

    println!("Elapsed: {:.2?}", start.elapsed());
    println!("RGBs: {}", frames_decoded.len());

    let start = Instant::now();
    frames_decoded.par_iter().enumerate().for_each(|(n, rgb)| {
        let path = PathBuf::from(format!("frames/{n}.png"));

        save_rgb_vec_to_image(rgb, width, height, path.as_path());
    });
    println!("Elapsed saving: {:.2?}", start.elapsed());
}

fn save_rgb_vec_to_image(raw_pixels: &[u8], width: u32, height: u32, path: &Path) {
    let img_buffer: RgbImage = if let Some(img) = RgbImage::from_raw(width, height, raw_pixels.to_vec()) {
        img
    } else {
        eprintln!(
            "Error: Could not create ImageBuffer from raw data. Check dimensions and data size."
        );
        return;
    };

    match img_buffer.save(path) {
        Ok(()) => println!("Image successfully saved to {}", path.display()),
        Err(e) => eprintln!("Error saving image: {e}"),
    }
}
