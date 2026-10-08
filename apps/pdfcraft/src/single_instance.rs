//! One running app per user: launching it again hands the files to the running one.
//!
//! Double-clicking a PDF (or Explorer's Open, Outlook's attachments, a shortcut) starts a new
//! process. When the user already has the app running, that process passes the files on to it,
//! where they open as tabs in the window used last, which comes to the front, and exits.
//!
//! The running app (the *primary*) listens on a random TCP port on `127.0.0.1` only, and writes
//! the port and a random 128-bit token to `instance.json` in the user's settings folder, readable
//! by the user alone (0600 on Unix; the per-user profile's permissions on Windows). A later
//! process reads the file, connects, and sends one line of JSON:
//!
//! ```text
//! {"pedeefe":1,"token":"<32 hex digits>","open":["/absolute/path/a.pdf", …]}
//! ```
//!
//! The primary answers `{"ok":true}` (or `{"ok":false,"error":…}`) and closes the connection. A
//! connection that doesn't present the token gets nothing done. The request is bounded (size,
//! number and length of paths, time), paths must be absolute, and the primary opens them like any
//! file the user picks (untrusted input, never a crash). Other users on the machine can reach the
//! port but can't read the token; nothing listens beyond the machine.
//!
//! Two processes started together (Explorer opens one per selected file) agree on one primary
//! through `instance.lock`, created exclusively by whichever comes first; the others wait for the
//! primary's `instance.json` and pass it their files. A lock left behind by a crash is ignored
//! after a few seconds, and a stale `instance.json` (nothing answering) is replaced. If anything
//! goes wrong, the process simply runs on its own, as before.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use pdfcraft_ui_egui::OsEvent;
use serde_json::{Value, json};

/// The protocol's version, sent as `"pedeefe"` with every request.
const VERSION: u64 = 1;
/// The longest request read (bytes): the paths below and the JSON around them.
const MAX_REQUEST: u64 = 1 << 20;
/// The most files one request may name, and the longest path (bytes).
const MAX_PATHS: usize = 256;
const MAX_PATH_LEN: usize = 4096;
/// How long either side waits for the other.
const IO_TIMEOUT: Duration = Duration::from_secs(2);
const CONNECT_TIMEOUT: Duration = Duration::from_millis(500);
/// How long a process waits for another one that is becoming the primary.
const ELECTION_WAIT: Duration = Duration::from_secs(4);
/// A lock older than this was left by a process that died while becoming the primary.
const STALE_LOCK: Duration = Duration::from_secs(10);

const INFO_FILE: &str = "instance.json";
const LOCK_FILE: &str = "instance.lock";

/// Why a request was refused.
#[derive(Debug, PartialEq, Eq)]
pub enum Refused {
    Malformed(String),
    WrongVersion,
    BadToken,
    BadPath(String),
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed(e) => write!(f, "malformed request: {e}"),
            Self::WrongVersion => write!(f, "unsupported protocol version"),
            Self::BadToken => write!(f, "wrong token"),
            Self::BadPath(e) => write!(f, "bad path: {e}"),
        }
    }
}

/// The request a second process sends: open these files (none: just come to the front).
pub fn encode_request(token: &str, paths: &[String]) -> String {
    json!({ "pedeefe": VERSION, "token": token, "open": paths }).to_string()
}

/// Check a request line against the primary's token and return the files it asks to open.
pub fn parse_request(line: &str, token: &str) -> Result<Vec<String>, Refused> {
    let v: Value = serde_json::from_str(line.trim()).map_err(|e| Refused::Malformed(e.to_string()))?;
    let obj = v.as_object().ok_or_else(|| Refused::Malformed("not an object".into()))?;
    // The token first: nothing else about a request without it is looked at.
    let presented = obj.get("token").and_then(Value::as_str).ok_or(Refused::BadToken)?;
    if !constant_time_eq(presented, token) {
        return Err(Refused::BadToken);
    }
    if obj.get("pedeefe").and_then(Value::as_u64) != Some(VERSION) {
        return Err(Refused::WrongVersion);
    }
    let list = match obj.get("open") {
        None | Some(Value::Null) => return Ok(Vec::new()),
        Some(Value::Array(a)) => a,
        Some(_) => return Err(Refused::Malformed("`open` is not a list".into())),
    };
    if list.len() > MAX_PATHS {
        return Err(Refused::Malformed(format!("more than {MAX_PATHS} files")));
    }
    list.iter()
        .map(|p| {
            let p = p.as_str().ok_or_else(|| Refused::BadPath("not a string".into()))?;
            if p.is_empty() || p.len() > MAX_PATH_LEN || p.contains('\0') {
                return Err(Refused::BadPath("empty, too long or with a NUL".into()));
            }
            if !Path::new(p).is_absolute() {
                return Err(Refused::BadPath(format!("{p} is not absolute")));
            }
            Ok(p.to_string())
        })
        .collect()
}

/// Where a primary can be reached, as written to `instance.json`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstanceInfo {
    pub port: u16,
    pub token: String,
    pub pid: u32,
}

impl InstanceInfo {
    pub fn to_json(&self) -> String {
        json!({ "port": self.port, "token": self.token, "pid": self.pid }).to_string()
    }

    /// Read `instance.json`'s contents; `None` for anything malformed.
    pub fn parse(text: &str) -> Option<Self> {
        let v: Value = serde_json::from_str(text).ok()?;
        let port = u16::try_from(v.get("port")?.as_u64()?).ok().filter(|p| *p != 0)?;
        let token = v.get("token")?.as_str().filter(|t| t.len() == 32 && t.bytes().all(|b| b.is_ascii_hexdigit()))?.to_string();
        let pid = u32::try_from(v.get("pid").and_then(Value::as_u64).unwrap_or(0)).unwrap_or(0);
        Some(Self { port, token, pid })
    }

    fn read(dir: &Path) -> Option<Self> {
        let mut text = String::new();
        std::fs::File::open(dir.join(INFO_FILE)).ok()?.take(4096).read_to_string(&mut text).ok()?;
        Self::parse(&text)
    }
}

/// What starting up decided.
pub enum Outcome {
    /// The running app took the files: this process exits.
    Forwarded,
    /// This process is the primary; keep the server for as long as the app runs.
    Primary(Server),
    /// Run on its own (no settings folder, a failure); the reason is logged.
    Alone(String),
}

/// Hand `files` (absolute paths) to the running app, or become the one later launches hand theirs
/// to. `dir` is the user's settings folder.
pub fn start(dir: &Path, files: &[String]) -> Outcome {
    if let Err(e) = std::fs::create_dir_all(dir) {
        return Outcome::Alone(format!("{}: {e}", dir.display()));
    }
    let deadline = Instant::now() + ELECTION_WAIT;
    loop {
        if let Some(info) = InstanceInfo::read(dir)
            && forward(&info, files).is_ok()
        {
            return Outcome::Forwarded;
        }
        let lock = dir.join(LOCK_FILE);
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&lock) {
            Ok(_) => {
                // A primary may have finished starting between the read above and the lock.
                let outcome = match InstanceInfo::read(dir).map(|info| forward(&info, files)) {
                    Some(Ok(())) => Outcome::Forwarded,
                    _ => match Server::start(dir) {
                        Ok(server) => Outcome::Primary(server),
                        Err(e) => Outcome::Alone(format!("single-instance server: {e}")),
                    },
                };
                let _ = std::fs::remove_file(&lock);
                return outcome;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let stale = std::fs::metadata(&lock)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| SystemTime::now().duration_since(t).ok())
                    .is_some_and(|age| age > STALE_LOCK);
                if stale {
                    let _ = std::fs::remove_file(&lock);
                    continue;
                }
                if Instant::now() >= deadline {
                    return Outcome::Alone("another launch is still starting".into());
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Outcome::Alone(format!("{}: {e}", lock.display())),
        }
    }
}

/// Send the files to the primary at `info`; `Ok` once it has accepted them.
pub fn forward(info: &InstanceInfo, files: &[String]) -> std::io::Result<()> {
    let addr = SocketAddr::from(([127, 0, 0, 1], info.port));
    let mut stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)?;
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    writeln!(stream, "{}", encode_request(&info.token, files))?;
    stream.flush()?;
    let mut reply = String::new();
    BufReader::new(stream.take(4096)).read_line(&mut reply)?;
    let ok = serde_json::from_str::<Value>(&reply).ok().and_then(|v| v.get("ok").and_then(Value::as_bool)) == Some(true);
    if ok { Ok(()) } else { Err(std::io::Error::other(format!("the running app refused: {}", reply.trim()))) }
}

/// What the primary's listener hands the UI, and how it wakes it.
#[derive(Default)]
struct Shared {
    events: Vec<OsEvent>,
    wake: Option<egui::Context>,
}

/// The primary's listener. Dropping it removes `instance.json` (if it is still this one's).
pub struct Server {
    shared: Arc<Mutex<Shared>>,
    dir: PathBuf,
    token: String,
}

impl Server {
    fn start(dir: &Path) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        let token = random_token()?;
        let info = InstanceInfo { port, token: token.clone(), pid: std::process::id() };
        write_private(&dir.join(INFO_FILE), &info.to_json())?;
        let shared = Arc::new(Mutex::new(Shared::default()));
        let queue = shared.clone();
        let expected = token.clone();
        std::thread::Builder::new().name("pdfcraft-instance".into()).spawn(move || {
            // One connection at a time, each bounded in time and size: a local process can't
            // tie up more than this thread.
            for stream in listener.incoming().flatten() {
                serve_one(stream, &expected, &queue);
            }
        })?;
        Ok(Self { shared, dir: dir.to_path_buf(), token })
    }

    /// Call with the UI's context to have it woken when files arrive (they queue until then).
    pub fn waker(&self) -> impl FnOnce(&egui::Context) + 'static {
        let shared = self.shared.clone();
        move |ctx| {
            if let Ok(mut s) = shared.lock() {
                s.wake = Some(ctx.clone());
                if !s.events.is_empty() {
                    ctx.request_repaint();
                }
            }
        }
    }

    /// The requests that arrived since the last call (`PdfCraftApp::os_events`).
    pub fn events(&self) -> impl FnMut() -> Vec<OsEvent> + 'static {
        let shared = self.shared.clone();
        move || shared.lock().map(|mut s| std::mem::take(&mut s.events)).unwrap_or_default()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // Only this process's file: a newer primary may have replaced it.
        let path = self.dir.join(INFO_FILE);
        if std::fs::read_to_string(&path).ok().and_then(|t| InstanceInfo::parse(&t)).is_some_and(|i| i.token == self.token) {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Read one request from `stream`, queue what it asks for and answer.
fn serve_one(stream: TcpStream, token: &str, shared: &Mutex<Shared>) {
    // Loopback only: the listener is bound to 127.0.0.1, this is belt and braces.
    if !stream.peer_addr().is_ok_and(|a| a.ip().is_loopback()) {
        return;
    }
    if stream.set_read_timeout(Some(IO_TIMEOUT)).is_err() || stream.set_write_timeout(Some(IO_TIMEOUT)).is_err() {
        return;
    }
    let Ok(read) = stream.try_clone() else { return };
    let mut line = String::new();
    let reply = match BufReader::new(read.take(MAX_REQUEST)).read_line(&mut line) {
        Err(e) => json!({ "ok": false, "error": format!("unreadable request: {e}") }),
        Ok(_) => match parse_request(&line, token) {
            Ok(paths) => {
                let event = if paths.is_empty() { OsEvent::Activate } else { OsEvent::Open(paths) };
                if let Ok(mut s) = shared.lock() {
                    s.events.push(event);
                    if let Some(ctx) = &s.wake {
                        ctx.request_repaint();
                    }
                }
                json!({ "ok": true })
            }
            Err(e) => {
                log::warn!("single instance: refused a request: {e}");
                json!({ "ok": false, "error": e.to_string() })
            }
        },
    };
    let mut write = stream;
    let _ = writeln!(write, "{reply}").and_then(|()| write.flush());
}

/// Write `text` to `path` so that only the current user can read it.
fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    f.write_all(text.as_bytes())?;
    f.sync_all()
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// 128 random bits from the OS, hex-encoded.
fn random_token() -> std::io::Result<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// The files given on the command line as absolute paths (the running app has another working
/// folder). A path that can't be made absolute is passed as it is and refused there.
pub fn absolute_paths(files: &[String]) -> Vec<String> {
    files.iter().map(|f| std::path::absolute(f).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| f.clone())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef";

    fn abs(name: &str) -> String {
        std::env::temp_dir().join(name).to_string_lossy().into_owned()
    }

    #[test]
    fn a_request_round_trips_and_needs_the_token() {
        let files = vec![abs("a.pdf"), abs("b c.pdf")];
        let line = encode_request(TOKEN, &files);
        assert_eq!(parse_request(&line, TOKEN), Ok(files.clone()));
        assert_eq!(parse_request(&format!("{line}\n"), TOKEN), Ok(files.clone()), "the newline is part of the framing");
        assert_eq!(parse_request(&line, "fedcba9876543210fedcba9876543210"), Err(Refused::BadToken));
        assert_eq!(parse_request(&encode_request("", &files), TOKEN), Err(Refused::BadToken));
        assert_eq!(parse_request(&json!({ "pedeefe": 1, "open": files }).to_string(), TOKEN), Err(Refused::BadToken));
        // No files: just come to the front.
        assert_eq!(parse_request(&encode_request(TOKEN, &[]), TOKEN), Ok(vec![]));
    }

    #[test]
    fn malformed_requests_are_refused() {
        let token_only = |extra: Value| {
            let mut v = json!({ "pedeefe": 1, "token": TOKEN });
            if let (Some(o), Some(e)) = (v.as_object_mut(), extra.as_object()) {
                o.extend(e.clone());
            }
            v.to_string()
        };
        assert!(matches!(parse_request("", TOKEN), Err(Refused::Malformed(_))));
        assert!(matches!(parse_request("not json", TOKEN), Err(Refused::Malformed(_))));
        assert!(matches!(parse_request("[1,2]", TOKEN), Err(Refused::Malformed(_))));
        assert_eq!(parse_request(&json!({ "pedeefe": 2, "token": TOKEN }).to_string(), TOKEN), Err(Refused::WrongVersion));
        assert!(matches!(parse_request(&token_only(json!({ "open": "a.pdf" })), TOKEN), Err(Refused::Malformed(_))));
        assert!(matches!(parse_request(&token_only(json!({ "open": ["relative.pdf"] })), TOKEN), Err(Refused::BadPath(_))));
        assert!(matches!(parse_request(&token_only(json!({ "open": [""] })), TOKEN), Err(Refused::BadPath(_))));
        assert!(matches!(parse_request(&token_only(json!({ "open": [3] })), TOKEN), Err(Refused::BadPath(_))));
        assert!(matches!(parse_request(&token_only(json!({ "open": [format!("{}\0x", abs("a"))] })), TOKEN), Err(Refused::BadPath(_))));
        assert!(matches!(parse_request(&token_only(json!({ "open": [abs(&"x".repeat(5000))] })), TOKEN), Err(Refused::BadPath(_))));
        let many: Vec<String> = (0..300).map(|i| abs(&format!("{i}.pdf"))).collect();
        assert!(matches!(parse_request(&token_only(json!({ "open": many })), TOKEN), Err(Refused::Malformed(_))));
    }

    #[test]
    fn instance_info_round_trips_and_rejects_junk() {
        let info = InstanceInfo { port: 50123, token: TOKEN.into(), pid: 42 };
        assert_eq!(InstanceInfo::parse(&info.to_json()), Some(info));
        assert_eq!(InstanceInfo::parse(""), None);
        assert_eq!(InstanceInfo::parse(r#"{"port":0,"token":"0123456789abcdef0123456789abcdef"}"#), None);
        assert_eq!(InstanceInfo::parse(r#"{"port":70000,"token":"0123456789abcdef0123456789abcdef"}"#), None);
        assert_eq!(InstanceInfo::parse(r#"{"port":5000,"token":"short"}"#), None);
        assert_eq!(InstanceInfo::parse(r#"{"port":5000,"token":"zz23456789abcdef0123456789abcdef"}"#), None);
    }

    #[test]
    fn constant_time_eq_compares_whole_strings() {
        assert!(constant_time_eq(TOKEN, TOKEN));
        assert!(!constant_time_eq(TOKEN, &TOKEN[..31]));
        assert!(!constant_time_eq("a", "b"));
    }

    /// A settings folder of its own for each test.
    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pedeefe-instance-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_second_launch_hands_its_files_to_the_first() -> Result<(), String> {
        let dir = temp_dir("handoff");
        let Outcome::Primary(server) = start(&dir, &[]) else { return Err("the first launch is the primary".into()) };
        let mut events = server.events();
        let files = vec![abs("one.pdf"), abs("two.pdf")];
        assert!(matches!(start(&dir, &files), Outcome::Forwarded), "the second launch forwards");
        assert!(matches!(start(&dir, &[]), Outcome::Forwarded), "a launch without files brings the app forward");
        assert_eq!(events(), vec![OsEvent::Open(files), OsEvent::Activate]);
        assert!(events().is_empty(), "drained");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join(INFO_FILE)).map_err(|e| e.to_string())?.permissions().mode();
            assert_eq!(mode & 0o077, 0, "only the user can read the token");
        }
        assert!(!dir.join(LOCK_FILE).exists(), "the election lock is gone");
        drop(server);
        assert!(!dir.join(INFO_FILE).exists(), "the primary removes its file when it exits");
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn a_wrong_token_or_a_stale_file_is_not_trusted() -> Result<(), String> {
        let dir = temp_dir("token");
        let Outcome::Primary(server) = start(&dir, &[]) else { return Err("the first launch is the primary".into()) };
        let mut events = server.events();
        let mut info = InstanceInfo::read(&dir).ok_or("instance.json written")?;
        info.token = "ffffffffffffffffffffffffffffffff".into();
        assert!(forward(&info, &[abs("x.pdf")]).is_err(), "refused without the token");
        // A raw connection sending junk gets a refusal, not a crash.
        let mut raw = TcpStream::connect(("127.0.0.1", info.port)).map_err(|e| e.to_string())?;
        raw.write_all(b"\xff\xfe garbage\n").map_err(|e| e.to_string())?;
        let mut reply = String::new();
        let _ = BufReader::new(raw).read_line(&mut reply);
        assert!(!reply.contains("\"ok\":true"), "{reply}");
        assert!(events().is_empty(), "nothing was queued");
        drop(server);
        // A file left by an app that is gone: the next launch becomes the primary.
        write_private(&dir.join(INFO_FILE), &InstanceInfo { port: info.port, token: TOKEN.into(), pid: 1 }.to_json()).map_err(|e| e.to_string())?;
        let Outcome::Primary(second) = start(&dir, &[abs("y.pdf")]) else { return Err("a stale file is replaced".into()) };
        assert_ne!(InstanceInfo::read(&dir).map(|i| i.token), Some(TOKEN.to_string()));
        drop(second);
        // A lock left by a launch that crashed while starting doesn't block forever.
        std::fs::write(dir.join(LOCK_FILE), b"").map_err(|e| e.to_string())?;
        let old = SystemTime::now() - Duration::from_secs(60);
        std::fs::File::options().write(true).open(dir.join(LOCK_FILE)).and_then(|f| f.set_modified(old)).map_err(|e| e.to_string())?;
        assert!(matches!(start(&dir, &[]), Outcome::Primary(_)), "a stale lock is ignored");
        let _ = std::fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn relative_paths_become_absolute() {
        let out = absolute_paths(&["doc.pdf".into()]);
        assert!(out.first().is_some_and(|p| Path::new(p).is_absolute()), "{out:?}");
    }
}
