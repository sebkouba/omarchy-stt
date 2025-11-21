//! Circular buffer for continuous audio recording
//!
//! This module provides a thread-safe circular buffer for storing PCM audio samples.
//! It's designed to hold a fixed amount of recent audio data (e.g., 2 minutes) and
//! allow extraction of arbitrary ranges for on-demand WAV file creation.

use std::sync::{Arc, Mutex};

/// A circular buffer for storing i16 PCM audio samples
///
/// The buffer has a fixed capacity and overwrites old data when full.
/// It tracks the total number of samples written to support accurate
/// range extraction even after wraparound.
#[derive(Debug)]
pub struct CircularBuffer {
    buffer: Vec<i16>,
    capacity: usize,
    write_index: usize,   // Current write position (0..capacity)
    total_written: usize, // Total samples written since creation (monotonically increasing)
}

impl CircularBuffer {
    /// Create a new circular buffer with the given capacity
    ///
    /// # Arguments
    /// * `capacity` - Maximum number of i16 samples to store
    ///
    /// # Example
    /// ```rust
    /// use transcribe_rs::circular_buffer::CircularBuffer;
    ///
    /// // 2 minutes @ 16kHz = 1,920,000 samples = 3.66 MB
    /// let buffer = CircularBuffer::new(1_920_000);
    /// ```
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: vec![0; capacity],
            capacity,
            write_index: 0,
            total_written: 0,
        }
    }

    /// Write samples to the buffer
    ///
    /// Returns the starting index (in total_written space) where these samples were written.
    /// This index can be used later to extract a range starting from this position.
    ///
    /// # Arguments
    /// * `samples` - Slice of i16 samples to write
    ///
    /// # Returns
    /// The global index where the first sample was written
    pub fn write_samples(&mut self, samples: &[i16]) -> usize {
        let start_index = self.total_written;

        for &sample in samples {
            self.buffer[self.write_index] = sample;
            self.write_index = (self.write_index + 1) % self.capacity;
            self.total_written += 1;
        }

        start_index
    }

    /// Get the current write index (total samples written)
    ///
    /// This value can be saved and later used with `extract_range` to retrieve
    /// the audio recorded between two points in time.
    pub fn get_current_index(&self) -> usize {
        self.total_written
    }

    /// Extract a range of samples from the buffer
    ///
    /// Extracts samples from `start_index` (inclusive) up to `sample_count` samples.
    /// If the requested range is no longer in the buffer (overwritten), returns only
    /// the available portion or an empty vector.
    ///
    /// # Arguments
    /// * `start_index` - Starting index (in total_written space)
    /// * `sample_count` - Number of samples to extract
    ///
    /// # Returns
    /// Vector of samples in chronological order. May be shorter than requested
    /// if data has been overwritten or if start_index is beyond current position.
    ///
    /// # Example
    /// ```rust
    /// use transcribe_rs::circular_buffer::CircularBuffer;
    ///
    /// let mut buffer = CircularBuffer::new(100);
    /// let start = buffer.get_current_index();
    /// buffer.write_samples(&vec![1, 2, 3, 4, 5]);
    /// let samples = buffer.extract_range(start, 5);
    /// assert_eq!(samples, vec![1, 2, 3, 4, 5]);
    /// ```
    pub fn extract_range(&self, start_index: usize, sample_count: usize) -> Vec<i16> {
        // Check if start_index is still in the buffer
        let oldest_available = self.total_written.saturating_sub(self.capacity);

        if start_index < oldest_available {
            // Data has been overwritten
            // Return samples from oldest available to current, up to sample_count
            let available_start = oldest_available;
            let available_count = (self.total_written - available_start).min(sample_count);
            return self.extract_range_internal(available_start, available_count);
        }

        if start_index >= self.total_written {
            // Start is in the future
            return Vec::new();
        }

        // Calculate actual sample count (may be less than requested)
        let available = self.total_written - start_index;
        let actual_count = sample_count.min(available);

        self.extract_range_internal(start_index, actual_count)
    }

    /// Internal helper to extract samples without bounds checking
    fn extract_range_internal(&self, start_index: usize, count: usize) -> Vec<i16> {
        let mut result = Vec::with_capacity(count);

        for i in 0..count {
            let global_index = start_index + i;
            let buffer_index = global_index % self.capacity;
            result.push(self.buffer[buffer_index]);
        }

        result
    }

    /// Get buffer statistics for debugging
    pub fn stats(&self) -> BufferStats {
        BufferStats {
            capacity: self.capacity,
            total_written: self.total_written,
            write_index: self.write_index,
            oldest_available: self.total_written.saturating_sub(self.capacity),
            fullness_percent: if self.total_written >= self.capacity {
                100.0
            } else {
                (self.total_written as f64 / self.capacity as f64) * 100.0
            },
        }
    }
}

/// Statistics about the buffer state
#[derive(Debug)]
pub struct BufferStats {
    pub capacity: usize,
    pub total_written: usize,
    pub write_index: usize,
    pub oldest_available: usize,
    pub fullness_percent: f64,
}

/// Thread-safe wrapper around CircularBuffer
pub type SharedBuffer = Arc<Mutex<CircularBuffer>>;

/// Create a new shared circular buffer
pub fn create_shared_buffer(capacity: usize) -> SharedBuffer {
    Arc::new(Mutex::new(CircularBuffer::new(capacity)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_write_read() {
        let mut buffer = CircularBuffer::new(100);

        let start = buffer.get_current_index();
        assert_eq!(start, 0);

        buffer.write_samples(&[1, 2, 3, 4, 5]);

        let samples = buffer.extract_range(start, 5);
        assert_eq!(samples, vec![1, 2, 3, 4, 5]);

        let current = buffer.get_current_index();
        assert_eq!(current, 5);
    }

    #[test]
    fn test_wraparound_write() {
        let mut buffer = CircularBuffer::new(10);

        // Write 15 samples (will wrap around)
        buffer.write_samples(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        buffer.write_samples(&[11, 12, 13, 14, 15]);

        assert_eq!(buffer.write_index, 5); // Wrapped to index 5
        assert_eq!(buffer.total_written, 15);
    }

    #[test]
    fn test_wraparound_extraction() {
        let mut buffer = CircularBuffer::new(10);

        // Write 15 samples
        buffer.write_samples(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        let start = buffer.get_current_index(); // 10
        buffer.write_samples(&[11, 12, 13, 14, 15]);

        // Extract samples that span the wraparound
        let samples = buffer.extract_range(start, 5);
        assert_eq!(samples, vec![11, 12, 13, 14, 15]);
    }

    #[test]
    fn test_overwrite_old_data() {
        let mut buffer = CircularBuffer::new(10);

        // Write 15 samples (first 5 get overwritten)
        buffer.write_samples(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        buffer.write_samples(&[11, 12, 13, 14, 15]);

        // Try to extract from the beginning (should only get samples 6-15)
        let samples = buffer.extract_range(0, 15);
        assert_eq!(samples, vec![6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
    }

    #[test]
    fn test_extract_more_than_capacity() {
        let mut buffer = CircularBuffer::new(10);

        buffer.write_samples(&[1, 2, 3, 4, 5]);

        // Try to extract more than available
        let samples = buffer.extract_range(0, 100);
        assert_eq!(samples, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_extract_from_future() {
        let mut buffer = CircularBuffer::new(100);

        buffer.write_samples(&[1, 2, 3]);

        // Try to extract from future index
        let samples = buffer.extract_range(100, 10);
        assert_eq!(samples, Vec::<i16>::new());
    }

    #[test]
    fn test_multiple_extractions() {
        let mut buffer = CircularBuffer::new(100);

        let start1 = buffer.get_current_index();
        buffer.write_samples(&[1, 2, 3]);

        let start2 = buffer.get_current_index();
        buffer.write_samples(&[4, 5, 6]);

        let start3 = buffer.get_current_index();
        buffer.write_samples(&[7, 8, 9]);

        // Extract each segment
        let seg1 = buffer.extract_range(start1, 3);
        let seg2 = buffer.extract_range(start2, 3);
        let seg3 = buffer.extract_range(start3, 3);

        assert_eq!(seg1, vec![1, 2, 3]);
        assert_eq!(seg2, vec![4, 5, 6]);
        assert_eq!(seg3, vec![7, 8, 9]);
    }

    #[test]
    fn test_buffer_stats() {
        let mut buffer = CircularBuffer::new(100);

        let stats = buffer.stats();
        assert_eq!(stats.capacity, 100);
        assert_eq!(stats.total_written, 0);
        assert_eq!(stats.fullness_percent, 0.0);

        buffer.write_samples(&[1; 50]);
        let stats = buffer.stats();
        assert_eq!(stats.total_written, 50);
        assert_eq!(stats.fullness_percent, 50.0);

        buffer.write_samples(&[1; 50]);
        let stats = buffer.stats();
        assert_eq!(stats.total_written, 100);
        assert_eq!(stats.fullness_percent, 100.0);

        buffer.write_samples(&[1; 10]);
        let stats = buffer.stats();
        assert_eq!(stats.total_written, 110);
        assert_eq!(stats.fullness_percent, 100.0);
    }

    #[test]
    fn test_shared_buffer() {
        let buffer = create_shared_buffer(100);

        {
            let mut buf = buffer.lock().unwrap();
            buf.write_samples(&[1, 2, 3]);
        }

        {
            let buf = buffer.lock().unwrap();
            let samples = buf.extract_range(0, 3);
            assert_eq!(samples, vec![1, 2, 3]);
        }
    }

    #[test]
    fn test_large_buffer_wraparound() {
        // Simulate 2 minutes @ 16kHz
        let capacity = 1_920_000;
        let mut buffer = CircularBuffer::new(capacity);

        // Write 3 minutes of data (will wrap around once)
        let chunk_size = 16000; // 1 second
        for i in 0..180 {
            let chunk: Vec<i16> = vec![i as i16; chunk_size];
            buffer.write_samples(&chunk);
        }

        assert_eq!(buffer.total_written, 180 * chunk_size);
        assert!(buffer.total_written > capacity);

        // Extract last 10 seconds
        let start = buffer.total_written - (10 * chunk_size);
        let samples = buffer.extract_range(start, 10 * chunk_size);
        assert_eq!(samples.len(), 10 * chunk_size);
    }
}
