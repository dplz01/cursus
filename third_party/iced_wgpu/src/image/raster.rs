use crate::core::Size;
use crate::core::image;
use crate::graphics;
use crate::image::atlas::{self, Atlas};

use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::{Arc, Weak};

pub type Image = graphics::image::Buffer;

/// Entry in cache corresponding to an image handle
#[derive(Debug)]
pub enum Memory {
    /// Image data on host
    Host(Image),
    /// Storage entry
    Device {
        entry: atlas::Entry,
        bind_group: Option<Arc<wgpu::BindGroup>>,
        allocation: Option<Weak<image::Memory>>,
    },
    Error(image::Error),
}

impl Memory {
    pub fn load(handle: &image::Handle) -> Self {
        match graphics::image::load(handle) {
            Ok(image) => Self::Host(image),
            Err(error) => Self::Error(error),
        }
    }

    pub fn dimensions(&self) -> Size<u32> {
        match self {
            Memory::Host(image) => {
                let (width, height) = image.dimensions();

                Size::new(width, height)
            }
            Memory::Device { entry, .. } => entry.size(),
            Memory::Error(_) => Size::new(1, 1),
        }
    }

    pub fn host(&self) -> Option<Image> {
        match self {
            Memory::Host(image) => Some(image.clone()),
            Memory::Device { .. } | Memory::Error(_) => None,
        }
    }
}

/// Cursus patch: bytes of images kept although not drawn lately. Upstream
/// dropped every image not drawn since the last new one, so scrolling a chat
/// with pictures decoded the ones just off screen again from disk on the
/// render thread each time they came back (scroll stutter on phones).
const IDLE_BUDGET_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, Default)]
pub struct Cache {
    map: FxHashMap<image::Id, Memory>,
    hits: FxHashSet<image::Id>,
    should_trim: bool,
    /// Cursus patch: the trim generation each image was last drawn before.
    last_used: FxHashMap<image::Id, u64>,
    generation: u64,
}

impl Cache {
    pub fn get_mut(&mut self, handle: &image::Handle) -> Option<&mut Memory> {
        let _ = self.hits.insert(handle.id());

        self.map.get_mut(&handle.id())
    }

    pub fn insert(&mut self, handle: &image::Handle, memory: Memory) {
        let _ = self.map.insert(handle.id(), memory);
        let _ = self.hits.insert(handle.id());

        self.should_trim = true;
    }

    pub fn contains(&self, handle: &image::Handle) -> bool {
        self.map.contains_key(&handle.id())
    }

    pub fn trim(
        &mut self,
        atlas: &mut Atlas,
        on_drop: impl Fn(Arc<wgpu::BindGroup>),
    ) {
        // Only trim if new entries have landed in the `Cache`
        if !self.should_trim {
            return;
        }

        self.generation += 1;
        for id in &self.hits {
            let _ = self.last_used.insert(*id, self.generation);
        }

        // Cursus patch: images not drawn lately stay (least recently drawn
        // dropped first) while they fit in IDLE_BUDGET_BYTES.
        let hits = &self.hits;
        let mut idle: Vec<(u64, image::Id, u64)> = self
            .map
            .iter()
            .filter(|(id, memory)| {
                !hits.contains(*id)
                    && !is_held(memory)
                    && !matches!(memory, Memory::Error(_))
            })
            .map(|(id, memory)| {
                let size = memory.dimensions();
                let bytes = u64::from(size.width) * u64::from(size.height) * 4;
                let used = self.last_used.get(id).copied().unwrap_or(0);
                (used, *id, bytes)
            })
            .collect();
        let mut idle_bytes: u64 = idle.iter().map(|(_, _, bytes)| bytes).sum();
        idle.sort_unstable_by_key(|(used, _, _)| *used);
        let mut evict = FxHashSet::default();
        for (_, id, bytes) in idle {
            if idle_bytes <= IDLE_BUDGET_BYTES {
                break;
            }
            idle_bytes -= bytes;
            let _ = evict.insert(id);
        }

        self.map.retain(|id, memory| {
            // Retain active allocations
            if is_held(memory) {
                return true;
            }

            let retain = hits.contains(id)
                || (!evict.contains(id) && !matches!(memory, Memory::Error(_)));

            if !retain {
                log::debug!("Dropping image allocation: {id:?}");

                if let Memory::Device {
                    entry, bind_group, ..
                } = memory
                {
                    if let Some(bind_group) = bind_group.take() {
                        on_drop(bind_group);
                    } else {
                        atlas.remove(entry);
                    }
                }
            }

            retain
        });
        let map = &self.map;
        self.last_used.retain(|id, _| map.contains_key(id));

        self.hits.clear();
        self.should_trim = false;
    }
}

/// An allocation the application still holds keeps its image.
fn is_held(memory: &Memory) -> bool {
    matches!(memory, Memory::Device { allocation: Some(allocation), .. } if allocation.strong_count() > 0)
}
