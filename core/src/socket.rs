//! One-line requests over Unix sockets: each connection sends one line and
//! gets one line back. docs/protocol.md lists the requests.

use anyhow::{Context, Result};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::thread;
use std::time::Duration;

/// Sends one line and returns the answer.
pub fn request(path: &Path, line: &str) -> Result<String> {
    let mut stream = UnixStream::connect(path).with_context(|| format!("connecting to {}", path.display()))?;
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    stream.write_all(line.replace('\n', " ").as_bytes())?;
    stream.write_all(b"\n")?;
    let mut answer = String::new();
    stream.read_to_string(&mut answer)?;
    Ok(answer.trim_end().to_string())
}

/// Listens on `path` and answers each request with `handler`, one thread
/// per connection.
pub fn serve<F>(path: &Path, handler: F) -> Result<()>
where
    F: Fn(&str) -> String + Send + Sync + 'static,
{
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let _ = fs::remove_file(path);
    let listener = UnixListener::bind(path).with_context(|| format!("listening on {}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    eprintln!("listening on {}", path.display());
    let handler = std::sync::Arc::new(handler);
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let handler = handler.clone();
        thread::spawn(move || {
            let mut reader = BufReader::new(&stream);
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() {
                return;
            }
            let answer = handler(line.trim_end_matches('\n'));
            let _ = (&stream).write_all(format!("{answer}\n").as_bytes());
        });
    }
    Ok(())
}
