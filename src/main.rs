use image::{ImageBuffer, RgbImage};
use rayon::prelude::*;
use std::path::PathBuf;
use std::time::Instant;
use video_rs::decode::Decoder;

fn main() {
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
        let frame_count = decoder.frames().unwrap() as f64;
        let duration_seconds = if fps > 0.0 { frame_count / fps } else { 0.0 };

        println!("Frame count: {frame_count}");
        println!("Estimated duration (from frames): {duration_seconds} seconds");
    } else {
        println!("Duration: {duration_seconds} seconds");
    }

    println!("FPS: {fps}");

    let mut frames_decoded: Vec<Vec<u8>> = Vec::new();

    let duration_in_seconds = duration_seconds.round() as i32;
    for second in 0..duration_in_seconds {
        let frame_number = (f64::from(second) * fps).round() as i64;

        match decoder.seek_to_frame(frame_number) {
            Ok(()) => {
                println!("Successfully sought to frame near index {frame_number}.",);
            }
            Err(e) => {
                eprintln!("Error seeking to frame: {e}");
            }
        }

        match decoder.decode() {
            Ok((ts, frame)) => {
                println!("{}", ts.as_secs_f64());

                let rgb = frame.to_owned().into_raw_vec_and_offset().0;

                frames_decoded.push(rgb);
            }
            Err(e) => {
                eprintln!("Error decoding frame: {e}");
            }
        }
    }

    println!("Elapsed: {:.2?}", start.elapsed());
    println!("RGBs: {}", frames_decoded.len());

    let start = Instant::now();
    frames_decoded.par_iter().enumerate().for_each(|(n, rgb)| {
        let path = PathBuf::from(format!("frames/{n}.png"));
        let rgb_data = rgb.to_vec();

        save_rgb_vec_to_image(rgb_data, width, height, &path);
    });
    println!("Elapsed saving: {:.2?}", start.elapsed());
}

fn save_rgb_vec_to_image(raw_pixels: Vec<u8>, width: u32, height: u32, path: &PathBuf) {
    let img_buffer: RgbImage = if let Some(img) = ImageBuffer::from_raw(width, height, raw_pixels) {
        img
    } else {
        eprintln!(
            "Error: Could not create ImageBuffer from raw data. Check dimensions and data size."
        );
        return;
    };

    match img_buffer.save(&path) {
        Ok(()) => println!("Image successfully saved to {path:?}"),
        Err(e) => eprintln!("Error saving image: {e}"),
    }
}
