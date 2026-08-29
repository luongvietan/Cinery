//! Command-boundary harness for the MVP acceptance test.
//!
//! The harness owns real project storage (tempdir + SQLite + migrations)
//! but only ever mutates product state through public Tauri command
//! functions with command DTOs — exactly what the frontend invokes.
//! Service/repository functions may construct fixtures (images on disk)
//! but must never be used to advance acceptance flow state.

#![allow(dead_code)]

use cinematic_desktop_lib::assets::service::AssetService;
use cinematic_desktop_lib::project::service::ProjectService;
use std::path::{Path, PathBuf};
use tempfile::{tempdir, TempDir};

pub struct CommandHarness {
    temp: TempDir,
    pub root: PathBuf,
}

impl CommandHarness {
    pub fn new(name: &str) -> Self {
        let temp = tempdir().unwrap();
        let root = temp.path().join(name);
        Self { temp, root }
    }

    /// Creates a deterministic source image inside the project (fixture
    /// setup only — never an acceptance mutation).
    pub fn image(&self, name: &str, pixel: [u8; 4]) -> PathBuf {
        let path = self.root.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let image: image::RgbaImage = image::ImageBuffer::from_pixel(8, 8, image::Rgba(pixel));
        image.save(&path).unwrap();
        path
    }

    pub fn new_asset(&self, asset_type: &str, label: &str) -> String {
        AssetService::create_asset(&self.root, asset_type, label, None)
            .unwrap()
            .id
    }

    /// Imports a candidate version from a fixture image (fixture setup).
    pub fn import_version(&self, asset_id: &str, source: &Path) -> String {
        AssetService::import_asset_version(&self.root, asset_id, source, None)
            .unwrap()
            .id
    }

    pub fn reopen(&self) {
        ProjectService::open(&self.root).unwrap();
    }
}
