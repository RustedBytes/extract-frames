use std::fs::File;
use std::path::PathBuf;

use crate::{get_files, remove_files, save_rgb_to_image};

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
