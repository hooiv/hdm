use std::thread;

pub fn play_complete() {
    thread::spawn(|| {
        println!("🎵 [Sound] Download Complete!");
    });
}

pub fn play_error() {
    thread::spawn(|| {
        println!("🎵 [Sound] Error!");
    });
}

pub fn play_startup() {
    thread::spawn(|| {
        println!("🎵 [Sound] Startup");
    });
}
