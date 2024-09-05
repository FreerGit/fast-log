use std::cell::UnsafeCell;
use std::fs::File;
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self};

#[derive(Clone)]
enum LogType {
    Ephemeral,
    File,
}

struct LogEntry {
    closure: Box<dyn FnOnce() -> String + Send>,
    to: LogType,
}

struct RingBuffer {
    buffer: Vec<AtomicUsize>,
    entries: Vec<UnsafeCell<Option<LogEntry>>>,
    head: AtomicUsize,
    tail: AtomicUsize,
    size: usize,
    is_empty: AtomicBool,
    shutdown: AtomicBool,
}

unsafe impl Sync for RingBuffer {}

impl RingBuffer {
    fn new(size: usize) -> Self {
        RingBuffer {
            size,
            buffer: (0..size).map(|_| AtomicUsize::new(0)).collect(),
            entries: (0..size).map(|_| UnsafeCell::new(None)).collect(),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            is_empty: AtomicBool::new(true),
            shutdown: AtomicBool::new(false),
        }
    }

    fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Release);
    }

    fn should_shutdown(&self) -> bool {
        self.shutdown.load(Ordering::Acquire)
    }

    fn is_empty(&self) -> bool {
        self.is_empty.load(Ordering::Acquire)
    }

    fn push(&self, entry: &mut LogEntry) -> bool {
        let mut head = self.head.load(Ordering::Relaxed);
        loop {
            let next = (head + 1) % self.size;
            if next == self.tail.load(Ordering::Relaxed) {
                return false; // Buffer is full
            }
            match self
                .head
                .compare_exchange(head, next, Ordering::Release, Ordering::Relaxed)
            {
                Ok(_) => {
                    unsafe {
                        // TODO
                        *self.entries[head].get() = Some(std::mem::replace(
                            entry,
                            LogEntry {
                                closure: Box::new(|| "Error: LogEntry moved".to_string()),
                                to: entry.to.clone(),
                            },
                        ));
                    }
                    self.buffer[head].store(1, Ordering::Release);
                    self.is_empty.store(false, Ordering::Release);
                    return true;
                }
                Err(x) => head = x,
            }
        }
    }

    fn pop(&self) -> Option<LogEntry> {
        let mut tail = self.tail.load(Ordering::Relaxed);
        loop {
            if self.buffer[tail].load(Ordering::Acquire) == 0 {
                return None; // Buffer is empty
            }
            let next = (tail + 1) % self.size;
            match self
                .tail
                .compare_exchange_weak(tail, next, Ordering::Release, Ordering::Relaxed)
            {
                Ok(_) => {
                    let entry = unsafe { (*self.entries[tail].get()).take() };
                    self.buffer[tail].store(0, Ordering::Release);
                    if next == self.head.load(Ordering::Relaxed) {
                        self.is_empty.store(true, Ordering::Release);
                    }
                    return entry;
                }
                Err(x) => tail = x,
            }
        }
    }

    fn wait_for_new_entries(&self) {
        while self.is_empty() && !self.should_shutdown() {
            thread::yield_now();
        }
    }
}

pub struct LoggerFileOptions {
    pub file_path: String,
    pub append_mode: bool,
}

pub struct Logger {
    buffer: Arc<RingBuffer>,
}

impl Logger {
    pub fn new(size: usize, options: Option<LoggerFileOptions>) -> Self {
        let buffer = Arc::new(RingBuffer::new(size));
        let buffer_clone = buffer.clone();

        if let Some(ref options) = options {
            if !std::path::Path::new(&options.file_path).exists() {
                panic!(
                    "The provided file: \"{}\" does not exist",
                    options.file_path
                )
            }
        }

        thread::spawn(move || {
            let mut file = None;
            if let Some(op) = options {
                file = Some(
                    File::options()
                        .write(true)
                        .append(op.append_mode)
                        .open(op.file_path)
                        .unwrap(),
                );
            }

            loop {
                if let Some(entry) = buffer_clone.pop() {
                    let mut message = (entry.closure)();

                    match entry.to {
                        LogType::File => {
                            message.push('\n');
                            file.as_mut()
                                .unwrap()
                                .write_all(message.as_bytes())
                                .unwrap()
                        }
                        LogType::Ephemeral => println!("{}", message),
                    };
                } else if buffer_clone.should_shutdown() && buffer_clone.is_empty() {
                    break;
                } else {
                    buffer_clone.wait_for_new_entries();
                }
            }
        });

        Logger { buffer }
    }

    pub fn shutdown(&self) {
        self.buffer.shutdown();
        while !self.buffer.is_empty() {
            thread::yield_now();
        }
    }

    pub fn log<F>(&self, f: F)
    where
        F: FnMut() -> String + Send + 'static,
    {
        let mut entry = LogEntry {
            closure: Box::new(f),
            to: LogType::Ephemeral,
        };
        while !self.buffer.push(&mut entry) {
            thread::yield_now();
        }
    }

    pub fn log_f<F>(&self, f: F)
    where
        F: FnMut() -> String + Send + 'static,
    {
        let mut entry = LogEntry {
            closure: Box::new(f),
            to: LogType::File,
        };
        while !self.buffer.push(&mut entry) {
            thread::yield_now();
        }
    }
}
