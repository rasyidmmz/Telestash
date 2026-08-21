use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Represents an in-memory cached chunk of media bytes.
#[derive(Clone)]
pub struct CachedChunk {
    pub offset: u64,
    pub data: Vec<u8>,
}

/// A thread-safe ring buffer that stores up to 16MB of forward media pre-fetch data per stream.
/// Uses BTreeMap for O(1) sorted eviction and zero-allocation range invalidation.
pub struct MediaStreamBuffer {
    chunks: BTreeMap<u64, Vec<u8>>,
    max_bytes: usize,
    current_bytes: usize,
}

impl MediaStreamBuffer {
    pub fn new(max_mb: usize) -> Self {
        Self {
            chunks: BTreeMap::new(),
            max_bytes: max_mb * 1024 * 1024,
            current_bytes: 0,
        }
    }

    pub fn get_chunk(&self, offset: u64) -> Option<Vec<u8>> {
        self.chunks.get(&offset).cloned()
    }

    pub fn insert_chunk(&mut self, offset: u64, data: Vec<u8>) {
        if self.chunks.contains_key(&offset) {
            return;
        }

        let chunk_len = data.len();
        // O(1) eviction of the oldest/lowest offset chunk if capacity is exceeded
        while self.current_bytes + chunk_len > self.max_bytes && !self.chunks.is_empty() {
            if let Some((_old_offset, removed)) = self.chunks.pop_first() {
                self.current_bytes = self.current_bytes.saturating_sub(removed.len());
            } else {
                break;
            }
        }

        self.current_bytes += chunk_len;
        self.chunks.insert(offset, data);
    }

    pub fn invalidate_before(&mut self, offset: u64) {
        let cutoff = offset.saturating_sub(2 * 1024 * 1024);
        while let Some(entry) = self.chunks.first_entry() {
            if *entry.key() < cutoff {
                let removed = entry.remove();
                self.current_bytes = self.current_bytes.saturating_sub(removed.len());
            } else {
                break;
            }
        }
    }

    pub fn clear(&mut self) {
        self.chunks.clear();
        self.current_bytes = 0;
    }
}

pub type SharedStreamBuffer = Arc<RwLock<MediaStreamBuffer>>;

pub fn create_stream_buffer(max_mb: usize) -> SharedStreamBuffer {
    Arc::new(RwLock::new(MediaStreamBuffer::new(max_mb)))
}

