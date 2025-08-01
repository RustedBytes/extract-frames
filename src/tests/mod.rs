use std::fs::File;
use std::fs::{create_dir_all, read_dir};
use std::panic::catch_unwind;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tempfile::tempdir;

use crate::{
    cleanup, get_files, read_by_dropping, remove_files, remove_folder, save_rgb_to_image, split_into_segments,
};

/// Helper to create a small dummy MP4 for testing (requires ffmpeg).
fn create_dummy_video(dest: &PathBuf) {
    // Generate a 120-second black video using ffmpeg (must be installed)
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=64x64:d=120:r=30",
            "-c:v",
            "libx264",
            dest.to_str().unwrap(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("Failed to run ffmpeg to create dummy video");
    assert!(status.success(), "ffmpeg did not produce test video");
}

#[test]
fn test_get_files_returns_files() {
    // Setup a temporary folder and file
    let tmp_dir = tempfile::tempdir().unwrap();
    let file_path = tmp_dir.path().join("testfile.txt");
    File::create(&file_path).unwrap();

    let pattern = format!("{}/testfile.*", tmp_dir.path().display());
    let files = get_files(&pattern);

    assert_eq!(files.len(), 1);
    assert_eq!(files[0], file_path);
}

#[test]
fn test_remove_files_removes_existing_files() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let file_path = tmp_dir.path().join("removable.txt");
    File::create(&file_path).unwrap();

    let files = vec![file_path.clone()];
    let result = remove_files(&files);

    assert!(result.is_ok());
    assert!(!file_path.exists());
}

#[test]
fn test_remove_files_handles_missing_files() {
    let file_path = PathBuf::from("nonexistentfile.txt");
    let files = vec![file_path];
    let result = remove_files(&files);

    assert!(result.is_err());
}

#[test]
fn test_save_rgb_to_image_saves_png() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let img_path = tmp_dir.path().join("output.png");

    // Create a small 2x2 red image
    let width = 2;
    let height = 2;
    let red_pixel = [255u8, 0, 0];
    let raw_pixels = red_pixel.repeat((width * height) as usize);

    save_rgb_to_image(&raw_pixels, width, height, &img_path);

    assert!(img_path.exists());
}

#[test]
fn test_get_files_empty_pattern() {
    let tmp_dir = tempdir().unwrap();
    let pattern = format!("{}/doesnotexist.*", tmp_dir.path().display());
    let files = get_files(&pattern);
    assert!(files.is_empty());
}

#[test]
#[should_panic]
fn test_get_files_invalid_pattern_panics() {
    // Invalid glob pattern
    let _ = get_files("[invalid[pattern");
}

#[test]
fn test_remove_files_with_empty_list() {
    let files: Vec<PathBuf> = vec![];
    let result = remove_files(&files);
    assert!(result.is_ok());
}

#[test]
fn test_remove_files_with_existing_and_missing_files() {
    let tmp_dir = tempdir().unwrap();
    let file_path = tmp_dir.path().join("file.txt");
    File::create(&file_path).unwrap();

    let missing_file = tmp_dir.path().join("missing.txt");
    let files = vec![file_path.clone(), missing_file.clone()];
    let result = remove_files(&files);

    assert!(result.is_err());
    assert!(!file_path.exists());
}

#[test]
fn test_save_rgb_to_image_invalid_data() {
    let tmp_dir = tempdir().unwrap();
    let img_path = tmp_dir.path().join("bad.png");

    // Provide fewer bytes than needed for a 2x2 image
    let bad_pixels = vec![255u8; 2 * 2 * 2]; // should be 2*2*3=12

    // Should not panic, but image crate will likely error internally
    save_rgb_to_image(&bad_pixels, 2, 2, &img_path);
}

#[test]
fn test_save_rgb_to_image_overwrite() {
    let tmp_dir = tempdir().unwrap();
    let img_path = tmp_dir.path().join("overwrite.png");

    let width = 1;
    let height = 1;
    let red_pixel = [255u8, 0, 0];
    let pixels = red_pixel.repeat((width * height) as usize);

    save_rgb_to_image(&pixels, width, height, &img_path);

    // Overwrite with another color
    let green_pixel = [0u8, 255, 0];
    let pixels = green_pixel.repeat((width * height) as usize);

    save_rgb_to_image(&pixels, width, height, &img_path);

    assert!(img_path.exists());
}

// Directory utility tests (may interfere with real data if run outside tempdir)
#[test]
fn test_remove_folder_on_empty_dir() {
    let tmp_dir = tempdir().unwrap();
    let folder = tmp_dir.path().join("toremove");
    create_dir_all(&folder).unwrap();
    remove_folder(folder.to_str().unwrap());
    assert!(!folder.exists());
}

#[test]
fn test_cleanup_on_empty_dirs() {
    // Should not panic if frames/ and segments/ do not exist or are empty
    cleanup();
}

#[test]
fn test_split_into_segments_creates_segments() {
    let tmp_dir = tempdir().unwrap();
    let video_path = tmp_dir.path().join("input.mp4");
    let segments_dir = tmp_dir.path().join("segments");

    create_dir_all(&segments_dir).unwrap();
    create_dummy_video(&video_path);

    // Ensure empty before run
    let pattern = format!("{}/*.mp4", segments_dir.to_string_lossy());
    let files = get_files(&pattern);
    let result = remove_files(&files);
    assert!(result.is_ok());

    // Call the function
    let segments = split_into_segments(&video_path);

    // Should produce at least one segment file
    assert!(!segments.is_empty(), "Should create at least one segment");
    for seg in &segments {
        assert!(seg.exists(), "Segment file should exist: {:?}", seg);
    }
}

#[test]
fn test_split_into_segments_handles_nonexistent_file() {
    let nonexistent = PathBuf::from("this_file_does_not_exist.mp4");
    let result = catch_unwind(|| {
        split_into_segments(&nonexistent);
    });
    assert!(result.is_err(), "Should panic or error on nonexistent input file");
}

#[test]
fn test_read_by_dropping_creates_expected_frames() {
    let prefix = "test";
    let tmp_dir = tempdir().unwrap();
    let video_path = tmp_dir.path().join("input.mp4");
    let frames_dir = tmp_dir.path().join("frames");

    create_dummy_video(&video_path);
    create_dir_all(&frames_dir).unwrap();

    read_by_dropping(prefix, &video_path, &frames_dir);

    let frames = read_dir(frames_dir).unwrap();
    let png_files: Vec<_> = frames
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "png"))
        .collect();

    assert!(!png_files.is_empty(), "No PNG frames were created");
}
