use gst::{self, prelude::*};
use image::{ImageBuffer, RgbImage};
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    gst::init().unwrap();

    let use_metal = true;
    let use_cuda = false;

    let start = Instant::now();

    let mut frames_decoded = 0;
    let filename = String::from("video.mp4");

    // Assuming original video is 30fps and you want 1fps (every 30th frame)
    // You need to know the source framerate for this to work precisely.
    // If source is not 30fps, adjust the `max-rate` accordingly.
    let mut pipeline_cmd = format!(
        "filesrc location={filename} ! queue ! decodebin ! videoconvert ! videorate ! video/x-raw,format=RGB,framerate=1/1 ! appsink name=mysink"
    );

    if use_metal {
        pipeline_cmd = format!(
            "filesrc location={filename} ! queue ! qtdemux ! h264parse ! vtdec ! videoconvert ! videorate ! video/x-raw,format=RGB,framerate=1/1 ! appsink name=mysink"
        );
    } else if use_cuda {
        pipeline_cmd = format!(
            "filesrc location={filename} ! queue ! qtdemux ! h264parse ! nvh264dec ! nvvideoconvert ! videorate ! video/x-raw,format=RGB,framerate=1/1 ! appsink name=mysink"
        );
    }

    println!("Using pipeline: {pipeline_cmd}");

    let pipeline = gst::parse_launch(&pipeline_cmd).unwrap();

    let pipeline = pipeline
        .downcast::<gst::Pipeline>()
        .expect("Couldn't downcast pipeline");

    let appsink = pipeline
        .by_name("mysink")
        .expect("Appsink element not found")
        .downcast::<gst_app::AppSink>()
        .expect("Element is not an AppSink");

    let width = 1920;
    let height = 1080;

    match pipeline.set_state(gst::State::Playing) {
        Ok(res) => {
            println!("Result: {res:?}");
        }
        Err(err) => {
            eprintln!("Error: {err:?}");
        }
    }

    println!("Pipeline started with videorate. Pulling frames...");

    loop {
        match appsink.pull_sample() {
            Ok(sample) => {
                let buffer = sample.buffer().expect("Failed to get buffer from sample");
                let caps = sample.caps().expect("Failed to get caps from sample");
                let info = gst_video::VideoInfo::from_caps(caps)
                    .expect("Failed to parse video info from caps");

                let map = buffer
                    .map_readable()
                    .expect("Failed to map buffer readable");
                let frame_data = map.as_slice();
                let frame_rgb = frame_data.to_vec();

                println!(
                    "Processed frames {}: Format: {:?}, Size: {}x{}, Data length: {}",
                    frames_decoded,
                    info.format(),
                    info.width(),
                    info.height(),
                    frame_data.len()
                );

                let path = PathBuf::from(format!("frames/{frames_decoded}.png"));
                save_rgb_vec_to_image(frame_rgb, width, height, &path);

                frames_decoded += 1;
            }
            Err(err) => {
                eprintln!("Error pulling sample: {err:?}");
                break;
            }
        }
    }

    match pipeline.set_state(gst::State::Null) {
        Ok(res) => {
            println!("Result: {res:?}");
        }
        Err(err) => {
            eprintln!("Error: {err:?}");
        }
    }

    println!("Elapsed: {:.2?}", start.elapsed());

    unsafe {
        gst::deinit();
    }
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
