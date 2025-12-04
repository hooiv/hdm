use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};

pub struct WriteRequest {
    pub offset: u64,
    pub data: Vec<u8>,
}

pub struct DiskWriter {
    file: Arc<Mutex<File>>,
    receiver: Receiver<WriteRequest>,
}

impl DiskWriter {
    pub fn new(file: Arc<Mutex<File>>, receiver: Receiver<WriteRequest>) -> Self {
        Self { file, receiver }
    }

    pub fn run(&self) {
        while let Ok(request) = self.receiver.recv() {
            let mut file = self.file.lock().unwrap();
            if let Err(e) = file.seek(SeekFrom::Start(request.offset)) {
                eprintln!("Disk seek error: {}", e);
                continue;
            }
            if let Err(e) = file.write_all(&request.data) {
                eprintln!("Disk write error: {}", e);
                continue;
            }
        }
    }
}
