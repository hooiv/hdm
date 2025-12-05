use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
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
        }
    }

    /// Main write loop - runs in a dedicated thread
    pub fn run(&mut self) {
        let mut pending_buffer: VecDeque<BufferEntry> = VecDeque::new();

        loop {
            // Try to receive without blocking first to batch writes
            match self.receiver.try_recv() {
                Ok(request) => {
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
        while let Some(entry) = buffer.pop_front() {
            self.write_entry(&entry);
        }
    }

    /// Write a single buffer entry to disk
    fn write_entry(&mut self, entry: &BufferEntry) {
        let mut file = match self.file.lock() {
            Ok(f) => f,
            Err(e) => {
                eprintln!("[DiskWriter] Lock error: {}", e);
                return;
            }
        };

        if let Err(e) = file.seek(SeekFrom::Start(entry.offset)) {
            eprintln!("[DiskWriter] Seek error at {}: {}", entry.offset, e);
            return;
        }

        if let Err(e) = file.write_all(&entry.data) {
            eprintln!("[DiskWriter] Write error at {}: {}", entry.offset, e);
            return;
        }

        self.write_count += 1;
        self.bytes_written += entry.data.len() as u64;
    }

    #[allow(dead_code)]
    pub fn get_stats(&self) -> (u64, u64) {
        (self.write_count, self.bytes_written)
    }
}

/// Pre-allocate a file to a specific size using sparse file mode on Windows
#[cfg(windows)]
pub fn preallocate_file(path: &Path, size: u64) -> std::io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
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
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
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
