use std::fs::File;
use std::fs::create_dir_all;
use std::path::PathBuf;
use tempfile::tempdir;

use crate::{cleanup, get_files, remove_files, remove_folder, save_rgb_to_image};

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
