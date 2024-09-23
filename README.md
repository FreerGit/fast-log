[![build & test](https://github.com/freergit/ring-log/actions/workflows/ci.yml/badge.svg)](https://github.com/freergit/ring-log/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/ring-log?style=flat-square)](https://crates.io/crates/ring-log/versions)
[![license](https://img.shields.io/github/license/freergit/ring-log)](https://github.com/freergit/ring-log/blob/main/LICENSE.txt)

# fast-log
High-performance logger with lock-free ring buffer, use this library when you want to log in the hotpath and performance is critical.

## Example
Submitting a log to either stdout or a file is very simple, you just give a closure which evaluates to a string. This is extremely fast, usually less than 100 nanos. A simple example:

```rust
let o = LoggerFileOptions {
    path: "log.txt",
    append_mode: false, // should the logger just append to what's already there or overwrite?
};

// The size is bounded, issuing a new log when the ringbuffer is full will block.
let logger = Logger::new(1024 * 8, Some(o));

// Log to stdout
logger.log(|| format!("To stdout: {}", 42));

// Log to the file, format_log! will prepend the file and LOC location to the log.
logger.log_f(|| format_log!("To log.txt {}", 5)); // path/to/file:LINE: To log.txt 5

// Blocks until all logs are handled.
logger.shutdown();
```
