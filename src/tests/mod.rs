use anyhow::{Context, Result, anyhow};
use std::fs::File;
use std::fs::{create_dir_all, read_dir};
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::tempdir;

use crate::{
    cleanup_temporary_files, decode_frames_dropping, decode_frames_seeking, get_files, remove_files, remove_folder,
    save_rgb_to_image, split_into_segments,
};

/// Helper to create a small dummy MP4 for testing (requires ffmpeg).
///
/// Creates a 120-second black video with dimensions 64x64 at 30fps using
/// ffmpeg. This function is used for generating test video content without
/// requiring external video files. Expects ffmpeg to be installed and available
/// in PATH.
///
/// # Arguments
/// * `dest` - Path where the dummy video file should be created
///
/// # Panics
/// * If ffmpeg command fails to execute
/// * If ffmpeg exits with non-zero status code
fn create_dummy_video(dest: impl AsRef<Path>) -> Result<impl AsRef<Path>> {
    // Generate a 120-second black video using ffmpeg (must be installed)
    let ffmpeg_result = Command::new("ffmpeg")
        .arg("-y")
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg("color=c=black:s=64x64:d=120:r=30")
        .arg("-c:v")
        .arg("libx264")
        .arg(dest.as_ref())
        .output()
        .context("Failed to run ffmpeg to create dummy video")?;

    assert!(
        ffmpeg_result.status.success(),
        "ffmpeg did not produce test video. stderr: {}",
        String::from_utf8_lossy(&ffmpeg_result.stderr)
    );

    Ok(dest)
}

/// Verifies that ffmpeg is installed and accessible in the system PATH.
///
/// This test ensures the testing environment has ffmpeg available, which is
/// required for creating dummy videos and running video processing tests.
/// The test checks both command execution success and proper version output.
#[test]
fn test_ffmpeg_exists() -> Result<()> {
    let output = Command::new("ffmpeg").arg("-version").output()?;

    assert!(
        output.status.success(),
        "'ffmpeg' did not exit successfully. Output: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    Ok(())
}

/// Tests that get_files correctly returns matching files for valid patterns.
///
/// Creates a temporary directory with a test file and verifies that the
/// glob pattern matching works correctly. This test ensures the file
/// discovery functionality works as expected in normal conditions.
#[test]
fn test_get_files_matches() -> Result<()> {
    // Setup a temporary folder and file
    let tmp_dir = tempdir()?;
    let file_path = tmp_dir.path().join("testfile.txt");
    File::create(&file_path)?;

    let path = tmp_dir.path().join("testfile.*");
    let paths = get_files(path).unwrap_or_else(|e| panic!("Failed to get files: {e:?}"));

    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0].as_ref(), file_path);

    Ok(())
}

/// Tests remove_files function handles missing files gracefully.
///
/// Verifies that attempting to remove non-existent files returns appropriate
/// errors rather than panicking. This ensures robust error handling in
/// cleanup operations.
#[test]
fn test_remove_files_handles_errors() -> Result<()> {
    let tmp_dir = tempdir()?;
    let file_path = tmp_dir.path().join("removable.txt");
    File::create(&file_path)?;

    let paths = vec![file_path.clone()];
    let result = remove_files(&paths);

    assert!(result.is_ok());
    assert!(!file_path.exists());

    let file_path = PathBuf::from("nonexistentfile.txt");
    let paths = vec![file_path];
    let result = remove_files(&paths);

    assert!(result.is_err());

    Ok(())
}

/// Tests basic PNG image creation functionality.
///
/// Creates a small 2x2 red PNG image and verifies it's properly saved to disk.
/// This test covers the core image saving functionality used throughout
/// the application for frame extraction.
#[test]
fn test_save_rgb_to_image_saves_png() -> Result<()> {
    let tmp_dir = tempdir()?;
    let img_path = tmp_dir.path().join("output.png");

    // Create a small 2x2 red image
    let width = 2;
    let height = 2;
    let red_pixel = [255u8, 0, 0];
    let raw_pixels = red_pixel.repeat((width * height) as usize);

    let result = save_rgb_to_image(&raw_pixels, width, height, &img_path);
    assert!(result.is_ok());

    assert!(img_path.exists());

    Ok(())
}

/// Tests that get_files returns an empty vector for patterns matching no files.
///
/// Creates a temporary directory and uses a glob pattern that matches nothing,
/// verifying that the function correctly returns Ok(vec![]) rather than an
/// error. This ensures graceful handling of non-matching patterns.
#[test]
fn test_get_files_empty_pattern() -> Result<()> {
    let tmp_dir = tempdir()?;
    let path = tmp_dir.path().join("doesnotexist.*");
    let paths = get_files(path)?;

    assert!(paths.is_empty());

    Ok(())
}

/// Tests that get_files properly handles invalid glob patterns.
///
/// Passes a malformed glob pattern to get_files and verifies that it returns
/// an error rather than panicking. This ensures robust error handling for
/// user-provided patterns.
#[test]
fn test_get_files_invalid_pattern_returns_err() -> Result<()> {
    // Test that an invalid glob pattern results in an error.
    let result = get_files("[invalid[pattern");
    assert!(result.is_err());

    Ok(())
}

/// Tests that remove_files handles empty input gracefully.
///
/// Verifies that calling remove_files with an empty vector returns Ok(())
/// without attempting any filesystem operations. This confirms the function
/// handles edge cases properly.
#[test]
fn test_remove_files_with_empty_list() -> Result<()> {
    let paths: Vec<PathBuf> = vec![];
    let result = remove_files(&paths);
    assert!(result.is_ok());

    Ok(())
}

/// Tests that remove_files returns error when some files are missing.
///
/// Creates a mix of existing and non-existing files and verifies that
/// remove_files correctly removes existing files while returning an error
/// for the missing ones. This ensures proper error reporting in mixed
/// scenarios.
#[test]
fn test_remove_files_with_existing_and_missing_files() -> Result<()> {
    let tmp_dir = tempdir()?;
    let file_path = tmp_dir.path().join("file.txt");
    File::create(&file_path)?;

    let missing_file = tmp_dir.path().join("missing.txt");
    let paths = vec![file_path.clone(), missing_file.clone()];
    let result = remove_files(&paths);

    // An error should be returned because one file was missing.
    assert!(result.is_err());
    // The file that did exist should still have been removed.
    assert!(!file_path.exists());

    Ok(())
}

/// Tests error handling for invalid pixel data.
///
/// Provides insufficient pixel data for the specified dimensions to verify
/// that the image creation handles malformed input gracefully without
/// panicking or corrupting memory.
#[test]
fn test_save_rgb_to_image_invalid_data() -> Result<()> {
    let tmp_dir = tempdir()?;
    let img_path = tmp_dir.path().join("bad.png");

    // Provide fewer bytes than needed for a 2x2 image
    let bad_pixels = vec![255u8; 2 * 2 * 2]; // should be 2*2*3=12
    let result = save_rgb_to_image(&bad_pixels, 2, 2, &img_path);

    assert!(result.is_err());

    Ok(())
}

/// Tests that save_rgb_to_image can successfully overwrite existing files.
///
/// Creates a PNG image, then saves another image with the same path,
/// verifying that the operation succeeds and the file gets updated.
/// This confirms that file overwriting works as expected.
#[test]
fn test_save_rgb_to_image_overwrite() -> Result<()> {
    let tmp_dir = tempdir()?;
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

    Ok(())
}

/// Tests that remove_folder successfully removes empty directories.
///
/// Creates a temporary directory and verifies that remove_folder
/// can remove it without errors. This test ensures the directory
/// cleanup functionality works correctly for empty folders.
#[test]
fn test_remove_folder_on_empty_dir() -> Result<()> {
    let tmp_dir = tempdir()?;
    let folder = tmp_dir.path().join("toremove");
    create_dir_all(&folder)?;

    remove_folder(folder.as_path())?;

    assert!(!folder.exists());

    Ok(())
}

/// Tests that cleanup function handles empty or non-existent directories
/// gracefully.
///
/// Verifies that calling cleanup_temporary_files() when frames/ and segments/
/// directories are empty or don't exist doesn't cause panics or errors. This
/// ensures the cleanup process is robust during initial runs or after manual
/// cleanup.
#[test]
fn test_cleanup_on_empty_dirs() -> Result<()> {
    // Should not panic if frames/ and segments/ do not exist or are empty
    cleanup_temporary_files()?;

    Ok(())
}

/// Tests that the segmentation process creates segment files as expected.
///
/// This test generates a dummy input video in a temporary directory, invokes
/// the split_into_segments function, and asserts that at least one output
/// segment file is created in the segments directory. It verifies that the
/// segmentation logic works end-to-end and that output files are actually
/// produced on disk.
#[test]
fn test_split_into_segments_creates_segments() -> Result<()> {
    let tmp_dir = tempdir()?;

    let video_path = tmp_dir.path().join("input.mp4");
    let segments_dir = tmp_dir.path().join("segments");
    create_dir_all(&segments_dir)?;

    let video_path = create_dummy_video(video_path)?;

    let segment_output_pattern = segments_dir.join("output_%09d.mp4");
    let segmented_files_path = segments_dir.join("*.mp4");

    let segment_output_pattern = segment_output_pattern.to_str().ok_or_else(|| {
        anyhow!(
            "Invalid UTF-8 path for glob pattern: {}",
            segment_output_pattern.display()
        )
    })?;

    // Ensure empty before run
    let paths = get_files(segmented_files_path.clone())?;
    let result = remove_files(&paths);
    assert!(result.is_ok());

    // Call the function
    let result = split_into_segments(video_path, segment_output_pattern, segmented_files_path);
    assert!(result.is_ok());

    let segments = result.unwrap();

    // Should produce at least one segment file
    assert!(!segments.is_empty(), "Should create at least one segment");
    for seg in segments {
        let seg = seg.as_ref();
        assert!(seg.exists(), "Segment file should exist: {seg:?}");
    }

    Ok(())
}

/// Tests that split_into_segments gracefully handles a nonexistent input file.
///
/// Attempts to segment a video file that does not exist and asserts that the
/// function returns an error. This ensures proper error handling for missing
/// or invalid input paths.
#[test]
fn test_split_into_segments_handles_nonexistent_file() -> Result<()> {
    let nonexistent = PathBuf::from("this_file_does_not_exist.mp4");
    let dummy_segment_output_pattern = "test-segments/output_%09d.mp4";
    let dummy_segmented_files_pattern = "test-segments/*.mp4";

    let result = split_into_segments(
        &nonexistent,
        dummy_segment_output_pattern,
        dummy_segmented_files_pattern,
    );
    assert!(result.is_err(), "Should return an error on a nonexistent input file");

    Ok(())
}

/// Tests that video segmentation creates actual segment files.
///
/// Uses the dummy video creation helper to generate a test video, then
/// verifies that the segmentation process actually produces the expected
/// MP4 segment files. This is a key integration test for the core video
/// processing functionality.
#[test]
fn test_decode_frames_dropping_creates_expected_frames() -> Result<()> {
    let tmp_dir = tempdir()?;

    let video_path = tmp_dir.path().join("input.mp4");
    let frames_dir = tmp_dir.path().join("frames");

    let video_path = create_dummy_video(video_path)?;
    create_dir_all(&frames_dir)?;

    let prefix = "test";
    decode_frames_dropping(prefix, video_path, &frames_dir)?;

    let frames = read_dir(frames_dir).context("Failed to read frames_dir")?;
    let png_files: Vec<_> = frames
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "png"))
        .collect();

    assert!(!png_files.is_empty(), "No PNG frames were created");

    Ok(())
}

/// Tests error handling for mismatched dimensions.
///
/// Verifies that `save_rgb_to_image` returns an error when the provided
/// dimensions (width * height) do not match the length of the pixel buffer.
/// This ensures the function validates input dimensions against buffer size.
#[test]
fn test_save_rgb_to_image_invalid_dimensions() -> Result<()> {
    let tmp_dir = tempdir()?;

    let img_path = tmp_dir.path().join("invalid.png");
    let raw_pixels = vec![255u8; 12]; // valid for 2x2 image
    let result = save_rgb_to_image(&raw_pixels, 3, 2, &img_path); // invalid dimensions
    assert!(result.is_err());

    Ok(())
}

/// Tests that `split_into_segments` returns an error for invalid ffmpeg output
/// patterns.
///
/// This test provides an output pattern that is invalid for ffmpeg's segment
/// muxer (e.g., missing a number formatter like `%09d`). It verifies that the
/// function correctly captures the ffmpeg error and returns a `Result::Err`,
/// ensuring robust error handling for invalid ffmpeg arguments.
#[test]
fn test_split_into_segments_invalid_output_pattern() -> Result<()> {
    let tmp_dir = tempdir()?;

    let video_path = tmp_dir.path().join("input.mp4");
    create_dummy_video(&video_path)?;

    let segments_dir = tmp_dir.path().join("segments");
    create_dir_all(&segments_dir)?;

    let invalid_pattern = "invalid_pattern"; // not a valid ffmpeg output pattern
    let result = split_into_segments(&video_path, invalid_pattern, "segments/*.mp4");
    assert!(result.is_err());

    Ok(())
}

/// Tests error handling for `decode_frames_seeking` with a nonexistent video
/// file.
///
/// Verifies that `decode_frames_seeking` returns an error when the input video
/// path does not exist. This ensures the function gracefully handles invalid
/// file paths instead of panicking.
#[test]
fn test_decode_frames_seeking_invalid_video_path() -> Result<()> {
    let nonexistent = PathBuf::from("nonexistent.mp4");
    let nonexistent2 = PathBuf::from("nonexistent-folder");
    let result = decode_frames_seeking("test", &nonexistent, &nonexistent2);
    assert!(result.is_err());

    Ok(())
}

/// Tests error handling for `decode_frames_dropping` with a nonexistent output
/// path.
///
/// Verifies that `decode_frames_dropping` returns an error when the specified
/// output directory for frames does not exist. This ensures the function
/// performs necessary pre-checks on output paths.
#[test]
fn test_decode_frames_dropping_invalid_frames_path() -> Result<()> {
    let tmp_dir = tempdir()?;

    let video_path = tmp_dir.path().join("input.mp4");
    create_dummy_video(&video_path)?;

    let frames_path = tmp_dir.path().join("nonexistent");
    let result = decode_frames_dropping("test", &video_path, &frames_path);
    assert!(result.is_err());

    Ok(())
}

/// Tests error handling for `decode_frames_dropping` with a nonexistent video
/// file.
///
/// Verifies that `decode_frames_dropping` returns an error when the input video
/// path does not exist. This ensures the function gracefully handles invalid
/// file paths instead of panicking.
#[test]
fn test_decode_frames_dropping_invalid_video_path() -> Result<()> {
    let tmp_dir = tempdir()?;

    let video_path = tmp_dir.path().join("nonexistent.mp4");
    let frames_path = tmp_dir.path().join("frames");
    create_dir_all(&frames_path)?;

    let result = decode_frames_dropping("test", &video_path, &frames_path);
    assert!(result.is_err());

    Ok(())
}
