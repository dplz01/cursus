//! A bounded, session-local gallery decode cache. No file decoding on the UI thread.
use iced::widget::image::Handle;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Default)]
pub(crate) struct Cache {
    generation: u64,
    items: Vec<(PathBuf, String)>,
    window: Vec<PathBuf>,
    ready: HashMap<PathBuf, Handle>,
    pending: HashMap<PathBuf, u64>,
    failed: HashSet<PathBuf>,
}
impl Cache {
    pub fn reset(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.items.clear();
        self.window.clear();
        self.ready.clear();
        self.failed.clear();
        // In-flight workers still count toward the concurrency budget until they finish.
    }
    pub fn begin(&mut self, items: Vec<(PathBuf, String)>) {
        self.reset();
        self.items = items;
    }
    pub fn items(&self) -> &[(PathBuf, String)] {
        &self.items
    }
    pub fn get(&self, path: &Path) -> Option<Handle> {
        self.ready.get(path).cloned()
    }
    pub fn failed(&self, path: &Path) -> bool {
        self.failed.contains(path)
    }
    pub fn plan(&mut self, paths: &[PathBuf], index: usize) -> Vec<(PathBuf, u64)> {
        self.window = [Some(index), index.checked_add(1), index.checked_sub(1)]
            .into_iter()
            .flatten()
            .filter_map(|i| paths.get(i).cloned())
            .collect();
        self.ready.retain(|path, _| self.window.contains(path));
        self.failed.retain(|path| self.window.contains(path));
        let mut jobs = vec![];
        for path in &self.window {
            if self.pending.len() >= 2 {
                break;
            }
            if !self.ready.contains_key(path)
                && !self.failed.contains(path)
                && !self.pending.contains_key(path)
            {
                self.pending.insert(path.clone(), self.generation);
                jobs.push((path.clone(), self.generation));
            }
        }
        jobs
    }
    pub fn complete(&mut self, path: PathBuf, generation: u64, result: Result<Handle, String>) {
        if self.pending.get(&path) == Some(&generation) {
            self.pending.remove(&path);
        }
        if generation != self.generation || !self.window.contains(&path) {
            return;
        }
        match result {
            Ok(handle) => {
                self.ready.insert(path, handle);
            }
            Err(_) => {
                self.failed.insert(path);
            }
        }
    }
}

pub(crate) fn decode(path: &Path) -> Result<Handle, String> {
    let started = std::time::Instant::now();
    let image = crate::media::open_upright(path)?
        .thumbnail(2048, 2048)
        .to_rgba8();
    let (width, height) = image.dimensions();
    tracing::debug!(
        elapsed_ms = started.elapsed().as_millis(),
        width,
        height,
        "gallery image decoded"
    );
    Ok(Handle::from_rgba(width, height, image.into_raw()))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pixel() -> Handle {
        Handle::from_rgba(1, 1, vec![0, 0, 0, 255])
    }
    #[test]
    fn only_current_and_adjacent_images_are_decoded_with_a_bounded_budget() {
        let paths: Vec<_> = (0..100)
            .map(|i| PathBuf::from(format!("image-{i}.png")))
            .collect();
        let mut cache = Cache::default();
        let jobs = cache.plan(&paths, 50);
        assert_eq!(
            jobs.iter().map(|j| j.0.clone()).collect::<Vec<_>>(),
            vec![paths[50].clone(), paths[51].clone()]
        );
        assert!(cache.plan(&paths, 50).is_empty());
        for (path, generation) in jobs {
            cache.complete(path, generation, Ok(pixel()));
        }
        let jobs = cache.plan(&paths, 50);
        assert_eq!(jobs[0].0, paths[49]);
        for (path, generation) in jobs {
            cache.complete(path, generation, Ok(pixel()));
        }
        assert!(cache.plan(&paths, 50).is_empty());
        assert!(cache.get(&paths[50]).is_some());
        assert_eq!(cache.ready.len(), 3);
        let _ = cache.plan(&paths, 80);
        assert!(cache.ready.is_empty());
    }
    #[test]
    fn closing_or_locking_rejects_late_results() {
        let mut cache = Cache::default();
        let paths = vec![PathBuf::from("private.png")];
        let job = cache.plan(&paths, 0).remove(0);
        cache.reset();
        cache.complete(job.0.clone(), job.1, Ok(pixel()));
        assert!(cache.get(&job.0).is_none());
        assert!(cache.pending.is_empty());
    }
    #[test]
    fn decoded_images_are_memory_backed_and_bounded_to_android_viewer_resolution() {
        let path =
            std::env::temp_dir().join(format!("cursus-gallery-decode-{}.png", std::process::id()));
        ::image::RgbImage::new(3000, 1000).save(&path).unwrap();
        let image = decode(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        match image {
            Handle::Rgba { width, height, .. } => {
                assert_eq!(width, 2048);
                assert!(height <= 2048);
            }
            _ => panic!("gallery must not defer file decoding to the renderer"),
        }
    }
}
