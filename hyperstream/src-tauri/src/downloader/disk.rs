use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::path::Path;
use std::collections::VecDeque;

/// Request to write data at a specific offset
#[derive(Debug)]
#[allow(dead_code)]
pub struct WriteRequest {
    pub offset: u64,
    pub data: Vec<u8>,
    pub segment_id: u32,
}

/// Configuration for the disk writer
#[derive(Debug, Clone)]
pub struct DiskWriterConfig {
    /// Maximum number of pending writes before backpressure kicks in
    pub max_pending_writes: usize,
    /// Buffer size for combining adjacent writes
    pub coalesce_threshold: usize,
    /// Whether to use sparse file mode (Windows NTFS)
    pub use_sparse: bool,
}

impl Default for DiskWriterConfig {
    fn default() -> Self {
        Self {
            max_pending_writes: 1000, // ~16MB if 16KB chunks
            coalesce_threshold: 4 * 1024 * 1024, // 4MB coalesce buffer
            use_sparse: true,
        }
    }
}

/// A ring buffer entry that tracks contiguous data
#[derive(Debug)]
struct BufferEntry {
    offset: u64,
    data: Vec<u8>,
}

impl BufferEntry {
    fn end_offset(&self) -> u64 {
        self.offset + self.data.len() as u64
    }

    /// Try to merge with another entry if they're adjacent
    fn try_merge(&mut self, other: &BufferEntry) -> bool {
        if self.end_offset() == other.offset {
            self.data.extend_from_slice(&other.data);
            true
        } else {
            false
        }
    }
}

/// High-performance disk writer with ring buffer and write coalescing
pub struct DiskWriter {
    file: Arc<Mutex<File>>,
    receiver: Receiver<WriteRequest>,
    config: DiskWriterConfig,
    write_count: u64,
    bytes_written: u64,
    /// Set to true when a persistent I/O failure occurs (data was dropped).
    /// The session should periodically check this to abort gracefully.
    io_error_flag: Arc<AtomicBool>,
}

impl DiskWriter {
    pub fn new(file: Arc<Mutex<File>>, receiver: Receiver<WriteRequest>) -> Self {
        Self::with_config(file, receiver, DiskWriterConfig::default())
    }

    pub fn with_config(
        file: Arc<Mutex<File>>, 
        receiver: Receiver<WriteRequest>,
        config: DiskWriterConfig
    ) -> Self {
        Self { 
            file, 
            receiver,
            config,
            write_count: 0,
            bytes_written: 0,
            io_error_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Returns a clone of the I/O error flag. Check with `flag.load(Ordering::Relaxed)`.
    pub fn io_error_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.io_error_flag)
    }

    /// Main write loop - runs in a dedicated thread
    pub fn run(&mut self) {
        let mut pending_buffer: VecDeque<BufferEntry> = VecDeque::new();

        loop {
            // Try to receive without blocking first to batch writes
            match self.receiver.try_recv() {
                Ok(request) => {
                    // Enforce max_pending_writes to prevent unbounded buffer growth on I/O failure
                    if pending_buffer.len() >= self.config.max_pending_writes {
                        eprintln!("[DiskWriter] WARNING: Buffer at capacity ({} entries), forcing flush before accepting new data.", pending_buffer.len());
                        self.flush_buffer(&mut pending_buffer);
                        // If flush couldn't drain (persistent I/O failure), drop oldest entries to prevent OOM
                        while pending_buffer.len() >= self.config.max_pending_writes {
                            if let Some(dropped) = pending_buffer.pop_front() {
                                eprintln!("[DiskWriter] CRITICAL: Dropping write at offset {} ({} bytes) due to persistent I/O failure.", dropped.offset, dropped.data.len());
                                // Signal persistent I/O failure so the session can abort
                                self.io_error_flag.store(true, Ordering::Release);
                            }
                        }
                    }

                    pending_buffer.push_back(BufferEntry {
                        offset: request.offset,
                        data: request.data,
                    });

                    // Coalesce adjacent writes if buffer is getting full
                    if pending_buffer.len() >= 10 {
                        self.coalesce_buffer(&mut pending_buffer);
                    }

                    // Flush if we have enough or buffer is large
                    let total_size: usize = pending_buffer.iter().map(|e| e.data.len()).sum();
                    if total_size >= self.config.coalesce_threshold {
                        self.flush_buffer(&mut pending_buffer);
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    // No more immediate data, flush what we have
                    if !pending_buffer.is_empty() {
                        self.flush_buffer(&mut pending_buffer);
                    }

                    // Now block waiting for next write
                    match self.receiver.recv() {
                        Ok(request) => {
                            pending_buffer.push_back(BufferEntry {
                                offset: request.offset,
                                data: request.data,
                            });
                        }
                        Err(_) => {
                            // Channel closed, flush and exit
                            self.flush_buffer(&mut pending_buffer);
                            break;
                        }
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // Channel closed
                    self.flush_buffer(&mut pending_buffer);
                    break;
                }
            }
        }

        println!("[DiskWriter] Finished. Total writes: {}, Bytes: {}", 
            self.write_count, self.bytes_written);
        
        // Ensure all data is flushed to stable storage before reporting completion
        if let Ok(f) = self.file.lock() {
            if let Err(e) = f.sync_all() {
                eprintln!("[DiskWriter] WARNING: sync_all failed: {}", e);
            }
        }
    }

    /// Sort and merge adjacent buffer entries
    fn coalesce_buffer(&self, buffer: &mut VecDeque<BufferEntry>) {
        if buffer.len() < 2 {
            return;
        }

        // Sort by offset
        let mut entries: Vec<_> = buffer.drain(..).collect();
        entries.sort_by_key(|e| e.offset);

        // Merge adjacent entries
        let mut result: Vec<BufferEntry> = Vec::new();
        for entry in entries {
            if let Some(last) = result.last_mut() {
                if !last.try_merge(&entry) {
                    result.push(entry);
                }
            } else {
                result.push(entry);
            }
        }

        buffer.extend(result);
    }

    /// Flush all pending buffer entries to disk
    fn flush_buffer(&mut self, buffer: &mut VecDeque<BufferEntry>) {
        if buffer.is_empty() { return; }

        let mut file = match self.file.lock() {
            Ok(f) => f,
            Err(e) => {
                eprintln!("[DiskWriter] Lock error: {}", e);
                return; // Keep entries in buffer for retry on next flush
            }
        };

        let mut consecutive_failures = 0u32;
        const MAX_CONSECUTIVE_FAILURES: u32 = 3;

        while let Some(entry) = buffer.pop_front() {
            if Self::perform_write(&mut file, &entry) {
                self.write_count += 1;
                self.bytes_written += entry.data.len() as u64;
                consecutive_failures = 0;
            } else {
                consecutive_failures += 1;
                if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                    eprintln!("[DiskWriter] CRITICAL: {} consecutive write failures, pausing flush. {} entries remaining in buffer.", consecutive_failures, buffer.len() + 1);
                    // Re-queue the failed entry at the front for retry
                    buffer.push_front(entry);
                    break;
                }
                // Re-queue for retry on next flush cycle
                buffer.push_front(entry);
                break;
            }
        }
    }

    /// Write a single buffer entry to disk (Static helper to avoid borrow issues)
    fn perform_write(file: &mut File, entry: &BufferEntry) -> bool {
        if let Err(e) = file.seek(SeekFrom::Start(entry.offset)) {
            eprintln!("[DiskWriter] Seek error at {}: {}", entry.offset, e);
            return false;
        }

        if let Err(e) = file.write_all(&entry.data) {
            eprintln!("[DiskWriter] Write error at {}: {}", entry.offset, e);
            return false;
        }

        true
    }

    #[allow(dead_code)]
    pub fn get_stats(&self) -> (u64, u64) {
        (self.write_count, self.bytes_written)
    }
}

/// Pre-allocate a file to a specific size using sparse file mode on Windows.
/// If the file already exists with content, it is opened WITHOUT truncation
/// to avoid destroying previously downloaded data (safety net for resume).
#[cfg(windows)]
pub fn preallocate_file(path: &Path, size: u64) -> std::io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;

    // Safety: only truncate when creating a genuinely new file.
    let already_has_data = path.exists() && path.metadata().map(|m| m.len() > 0).unwrap_or(false);

    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(!already_has_data) // preserve existing bytes
        .custom_flags(0x00200000) // FILE_FLAG_SPARSE_FILE hint
        .open(path)?;

    // Set file size
    file.set_len(size)?;

    // Try to set as sparse file (NTFS only)
    #[allow(unused)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::IO::DeviceIoControl;
        use windows_sys::Win32::System::Ioctl::FSCTL_SET_SPARSE;
        
        let handle = file.as_raw_handle() as isize;
        let mut bytes_returned: u32 = 0;
        
        unsafe {
            DeviceIoControl(
                handle as *mut std::ffi::c_void,
                FSCTL_SET_SPARSE,
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                0,
                &mut bytes_returned,
                std::ptr::null_mut(),
            );
        }
    }

    Ok(file)
}

/// Pre-allocate a file on non-Windows systems
#[cfg(not(windows))]
pub fn preallocate_file(path: &Path, size: u64) -> std::io::Result<File> {
    let already_has_data = path.exists() && path.metadata().map(|m| m.len() > 0).unwrap_or(false);

    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(!already_has_data)
        .open(path)?;

    file.set_len(size)?;
    
    Ok(file)
}

/// Create a file handle suitable for resume (doesn't truncate)
pub fn open_for_resume(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;
    use std::thread;
    use tempfile::tempdir;

    #[test]
    fn test_disk_writer_basic() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.bin");
        
        let file = File::create(&path).unwrap();
        file.set_len(1000).unwrap();
        let file = Arc::new(Mutex::new(file));
        
        let (tx, rx) = channel();
        let file_clone = Arc::clone(&file);
        
        let handle = thread::spawn(move || {
            let mut writer = DiskWriter::new(file_clone, rx);
            writer.run();
        });

        tx.send(WriteRequest {
            offset: 0,
            data: vec![1, 2, 3, 4],
            segment_id: 0,
        }).unwrap();

        tx.send(WriteRequest {
            offset: 100,
            data: vec![5, 6, 7, 8],
            segment_id: 1,
        }).unwrap();

        drop(tx);
        handle.join().unwrap();

        // Verify
        let mut contents = vec![0u8; 1000];
        let mut f = File::open(&path).unwrap();
        use std::io::Read;
        f.read_exact(&mut contents).unwrap();
        
        assert_eq!(&contents[0..4], &[1, 2, 3, 4]);
        assert_eq!(&contents[100..104], &[5, 6, 7, 8]);
    }
}
