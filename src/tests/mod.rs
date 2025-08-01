use std::fs::File;
use std::fs::{create_dir_all, read_dir};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempfile::tempdir;

use crate::{
    cleanup, get_files, read_by_dropping, remove_files, remove_folder, save_rgb_to_image, split_into_segments,
};

const SEGMENT_OUTPUT_PATTERN: &str = "test-segments/output_%09d.mp4";
const SEGMENTED_FILES_PATTERN: &str = "test-segments/*.mp4";

/// Helper to create a small dummy MP4 for testing (requires ffmpeg).
fn create_dummy_video(dest: &Path) {
    // Generate a 120-second black video using ffmpeg (must be installed)
    let status = Command::new("ffmpeg")
        .arg("-y")
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg("color=c=black:s=64x64:d=120:r=30")
        .arg("-c:v")
        .arg("libx264")
        .arg(dest)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("Failed to run ffmpeg to create dummy video");
    assert!(status.success(), "ffmpeg did not produce test video");
}

#[test]
fn test_ffmpeg_exists() {
    let output = Command::new("ffmpeg").arg("-version").output();

    assert!(
        output.is_ok(),
        "Failed to execute 'ffmpeg'. Is ffmpeg installed and available in PATH?"
    );

    let output = output.unwrap();
    assert!(
        output.status.success(),
        "'ffmpeg' did not exit successfully. Output: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_get_files_returns_files() {
    // Setup a temporary folder and file
    let tmp_dir = tempfile::tempdir().unwrap();
    let file_path = tmp_dir.path().join("testfile.txt");
    File::create(&file_path).unwrap();

    let binding = tmp_dir.path().join("testfile.*");
    let files = get_files(&binding.to_string_lossy()).unwrap();

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

    let result = save_rgb_to_image(&raw_pixels, width, height, &img_path);
    assert!(result.is_ok());

    assert!(img_path.exists());
}

#[test]
fn test_get_files_empty_pattern() {
    let tmp_dir = tempdir().unwrap();
    let binding = tmp_dir.path().join("doesnotexist.*");
    let result = get_files(&binding.to_string_lossy());
    assert!(result.is_ok());

    let files = result.unwrap();

    assert!(files.is_empty());
}

#[test]
fn test_get_files_invalid_pattern_returns_err() {
    // Test that an invalid glob pattern results in an error.
    let result = get_files("[invalid[pattern");
    assert!(result.is_err());
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

    // An error should be returned because one file was missing.
    assert!(result.is_err());
    // The file that did exist should still have been removed.
    assert!(!file_path.exists());
}

#[test]
fn test_save_rgb_to_image_invalid_data() {
    let tmp_dir = tempdir().unwrap();
    let img_path = tmp_dir.path().join("bad.png");

    // Provide fewer bytes than needed for a 2x2 image
    let bad_pixels = vec![255u8; 2 * 2 * 2]; // should be 2*2*3=12

    // Should not panic, but image crate will likely error internally
    let result = save_rgb_to_image(&bad_pixels, 2, 2, &img_path);

    assert!(result.is_err());
}

#[test]
fn test_save_rgb_to_image_overwrite() {
    let tmp_dir = tempdir().unwrap();
    let img_path = tmp_dir.path().join("overwrite.png");

    let width = 1;
    let height = 1;
    let red_pixel = [255u8, 0, 0];
    let pixels = red_pixel.repeat((width * height) as usize);

    let result = save_rgb_to_image(&pixels, width, height, &img_path);
    assert!(result.is_ok());

    // Overwrite with another color
    let green_pixel = [0u8, 255, 0];
    let pixels = green_pixel.repeat((width * height) as usize);

    let result = save_rgb_to_image(&pixels, width, height, &img_path);
    assert!(result.is_ok());

    assert!(img_path.exists());
}

// Directory utility tests (may interfere with real data if run outside tempdir)
#[test]
fn test_remove_folder_on_empty_dir() {
    let tmp_dir = tempdir().unwrap();
    let folder = tmp_dir.path().join("toremove");
    create_dir_all(&folder).unwrap();

    remove_folder(folder.as_path()).unwrap();

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

    let segment_output_pattern = segments_dir.join("output_%09d.mp4");
    let segmented_files_pattern = segments_dir.join("*.mp4");

    // Ensure empty before run
    let files = get_files(&segmented_files_pattern.to_string_lossy()).unwrap();
    let result = remove_files(&files);
    assert!(result.is_ok());

    // Call the function
    let result = split_into_segments(
        &video_path,
        &segment_output_pattern.to_string_lossy(),
        &segmented_files_pattern.to_string_lossy(),
    );
    assert!(result.is_ok());

    let segments = result.unwrap();

    // Should produce at least one segment file
    assert!(!segments.is_empty(), "Should create at least one segment");
    for seg in &segments {
        assert!(seg.exists(), "Segment file should exist: {:?}", seg);
    }
}

#[test]
fn test_split_into_segments_handles_nonexistent_file() {
    let nonexistent = PathBuf::from("this_file_does_not_exist.mp4");
    let result = split_into_segments(&nonexistent, SEGMENT_OUTPUT_PATTERN, SEGMENTED_FILES_PATTERN);
    assert!(result.is_err(), "Should return an error on a nonexistent input file");
}

#[test]
fn test_read_by_dropping_creates_expected_frames() {
    let prefix = "test";
    let tmp_dir = tempdir().unwrap();
    let video_path = tmp_dir.path().join("input.mp4");
    let frames_dir = tmp_dir.path().join("frames");

    create_dummy_video(&video_path);
    create_dir_all(&frames_dir).unwrap();

    let result = read_by_dropping(prefix, &video_path, &frames_dir);
    assert!(result.is_ok());

    let frames = read_dir(frames_dir).unwrap();
    let png_files: Vec<_> = frames
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "png"))
        .collect();

    assert!(!png_files.is_empty(), "No PNG frames were created");
}
