//! The `diaspar-asset://localhost/<page-id>/<reference>` protocol, which serves
//! images and attachments to the webview. Every request is resolved by the
//! Document Service, which refuses anything outside the workspace
//! (ARCHITECTURE §6.6).
//!
//! Audio and video players ask for byte ranges, so a `Range` request reads
//! only that part of the file, at most `MAX_CHUNK` bytes per answer (the
//! player asks for the rest). Requests are answered off the main thread.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use tauri::http::{Method, Request, Response, StatusCode, header};
use tauri::{AppHandle, Manager, Runtime, UriSchemeContext, UriSchemeResponder};

use crate::session::Session;

pub const SCHEME: &str = "diaspar-asset";

/// The most bytes one range answer carries.
const MAX_CHUNK: u64 = 4 * 1024 * 1024;

/// Extensions the "/" menu's Image, Audio and Video items pick from. The
/// editor chooses a player from the same lists (media.ts).
pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "avif", "heic", "svg"];
pub const AUDIO_EXTENSIONS: &[&str] = &["mp3", "m4a", "aac", "wav", "flac", "ogg", "oga", "opus"];
pub const VIDEO_EXTENSIONS: &[&str] = &["mp4", "m4v", "mov", "webm", "ogv"];

fn extension(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

fn content_type(path: &Path) -> &'static str {
    match extension(path).as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "heic" => "image/heic",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "aac" => "audio/aac",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "ogg" | "oga" | "opus" => "audio/ogg",
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "ogv" => "video/ogg",
        _ => "application/octet-stream",
    }
}

/// What a `Range` header asks for, given the file's length (RFC 9110 §14).
#[derive(Debug, PartialEq, Eq)]
enum Wanted {
    /// No usable range: the whole file. A header Diaspar can't parse is
    /// ignored, as the RFC allows.
    Whole,
    /// Bytes `start..=end`, already clamped to the file.
    Part(u64, u64),
    /// The range starts past the end of the file.
    Unsatisfiable,
}

fn wanted(range: Option<&str>, len: u64) -> Wanted {
    let Some(range) = range else {
        return Wanted::Whole;
    };
    let Some((unit, specs)) = range.split_once('=') else {
        return Wanted::Whole;
    };
    if !unit.trim().eq_ignore_ascii_case("bytes") {
        return Wanted::Whole;
    }
    // Players ask for one range. Of several, the first is answered.
    let spec = specs.split(',').next().unwrap_or_default().trim();
    let Some((first, last)) = spec.split_once('-') else {
        return Wanted::Whole;
    };
    let number = |s: &str| {
        let s = s.trim();
        (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
            .then(|| s.parse::<u64>().ok())
            .flatten()
    };
    match (first.trim().is_empty(), number(first), number(last)) {
        // `bytes=-n`: the last n bytes.
        (true, _, Some(n)) => {
            if n == 0 || len == 0 {
                Wanted::Unsatisfiable
            } else {
                Wanted::Part(len.saturating_sub(n), len - 1)
            }
        }
        // `bytes=a-` and `bytes=a-b`.
        (false, Some(start), end) if end.is_some() || last.trim().is_empty() => {
            if end.is_some_and(|end| end < start) {
                Wanted::Whole
            } else if start >= len {
                Wanted::Unsatisfiable
            } else {
                Wanted::Part(start, end.map_or(len - 1, |end| end.min(len - 1)))
            }
        }
        _ => Wanted::Whole,
    }
}

fn status(code: StatusCode) -> Response<Vec<u8>> {
    Response::builder()
        .status(code)
        .body(Vec::new())
        .expect("a static response is valid")
}

/// Answers a request for the file at `path`, which the Document Service
/// already resolved inside the workspace.
fn serve(path: &Path, method: &Method, range: Option<&str>) -> std::io::Result<Response<Vec<u8>>> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    let builder = Response::builder()
        .header(header::CONTENT_TYPE, content_type(path))
        .header(header::ACCEPT_RANGES, "bytes")
        .header("X-Content-Type-Options", "nosniff")
        // SVGs are images here: no scripts, no outside requests.
        .header(
            header::CONTENT_SECURITY_POLICY,
            "default-src 'none'; style-src 'unsafe-inline'",
        );
    let head = method == Method::HEAD;
    let response = match wanted(range, len) {
        Wanted::Whole => {
            let mut bytes = Vec::new();
            if !head {
                file.read_to_end(&mut bytes)?;
            }
            builder
                .status(StatusCode::OK)
                .header(header::CONTENT_LENGTH, len)
                .body(bytes)
        }
        Wanted::Part(start, end) => {
            let end = end.min(start + MAX_CHUNK - 1);
            let count = end - start + 1;
            let mut bytes = Vec::new();
            if !head {
                file.seek(SeekFrom::Start(start))?;
                file.take(count).read_to_end(&mut bytes)?;
            }
            builder
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{len}"))
                .header(header::CONTENT_LENGTH, count)
                .body(bytes)
        }
        Wanted::Unsatisfiable => builder
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header(header::CONTENT_RANGE, format!("bytes */{len}"))
            .body(Vec::new()),
    };
    Ok(response.unwrap_or_else(|_| status(StatusCode::INTERNAL_SERVER_ERROR)))
}

fn respond<R: Runtime>(app: &AppHandle<R>, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    if request.method() != Method::GET && request.method() != Method::HEAD {
        return status(StatusCode::METHOD_NOT_ALLOWED);
    }
    let path = request.uri().path().trim_start_matches('/');
    let Some((page_id, reference)) = path.split_once('/') else {
        return status(StatusCode::BAD_REQUEST);
    };
    let session = app.state::<Session>();
    let resolved = match session.read(|ws| ws.resolve_asset(page_id, reference)) {
        Ok(path) => path,
        Err(err) => {
            log::debug!("asset request refused: {}", err.message);
            return status(StatusCode::NOT_FOUND);
        }
    };
    let range = request
        .headers()
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok());
    serve(&resolved, request.method(), range).unwrap_or_else(|err| {
        log::warn!("couldn't read attachment {resolved:?}: {err}");
        status(StatusCode::NOT_FOUND)
    })
}

pub fn handle<R: Runtime>(
    ctx: UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let app = ctx.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || responder.respond(respond(&app, &request)));
}

/// Extensions Open refuses, because opening them runs something: apps,
/// scripts, installers, and files that only point at other files or sites.
const PROGRAM_EXTENSIONS: &[&str] = &[
    "app",
    "command",
    "tool",
    "terminal",
    "sh",
    "bash",
    "zsh",
    "csh",
    "tcsh",
    "ksh",
    "fish",
    "py",
    "pyw",
    "pl",
    "rb",
    "php",
    "lua",
    "tcl",
    "js",
    "jar",
    "scpt",
    "scptd",
    "applescript",
    "workflow",
    "action",
    "shortcut",
    "pkg",
    "mpkg",
    "mobileconfig",
    "prefpane",
    "saver",
    "kext",
    "plugin",
    "bundle",
    "osax",
    "qlgenerator",
    "service",
    "xpc",
    "dylib",
    "so",
    "webloc",
    "inetloc",
    "fileloc",
    "url",
];

/// Whether a file card may open `path` in its default app (ADR-049). A note
/// from somewhere else can carry any file, and opening a program or a file
/// with the executable bit runs it, so those are only shown in Finder.
pub fn may_open(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        match std::fs::metadata(path) {
            Ok(meta) if meta.is_file() && meta.permissions().mode() & 0o111 == 0 => {}
            _ => return false,
        }
    }
    !PROGRAM_EXTENSIONS.contains(&extension(path).as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clip.mp4");
        std::fs::write(&path, bytes).unwrap();
        (dir, path)
    }

    fn header(response: &Response<Vec<u8>>, name: header::HeaderName) -> &str {
        response.headers()[name].to_str().unwrap()
    }

    #[test]
    fn ranges_are_read_as_the_rfc_says() {
        assert_eq!(wanted(None, 10), Wanted::Whole);
        assert_eq!(wanted(Some("bytes=0-1"), 10), Wanted::Part(0, 1));
        assert_eq!(wanted(Some("bytes=4-"), 10), Wanted::Part(4, 9));
        assert_eq!(wanted(Some("bytes=-3"), 10), Wanted::Part(7, 9));
        assert_eq!(wanted(Some("bytes=-30"), 10), Wanted::Part(0, 9));
        assert_eq!(wanted(Some("bytes=2-99"), 10), Wanted::Part(2, 9));
        assert_eq!(wanted(Some("Bytes = 1-2, 5-6"), 10), Wanted::Part(1, 2));
        assert_eq!(wanted(Some("bytes=10-"), 10), Wanted::Unsatisfiable);
        assert_eq!(wanted(Some("bytes=10-20"), 10), Wanted::Unsatisfiable);
        assert_eq!(wanted(Some("bytes=-0"), 10), Wanted::Unsatisfiable);
        assert_eq!(wanted(Some("bytes=0-"), 0), Wanted::Unsatisfiable);
        // Malformed headers are ignored.
        for bad in [
            "bytes=5-2",
            "bytes=a-b",
            "bytes=",
            "bytes=-",
            "items=0-1",
            "0-1",
            "bytes=+1-2",
        ] {
            assert_eq!(wanted(Some(bad), 10), Wanted::Whole, "{bad}");
        }
    }

    #[test]
    fn a_range_request_reads_only_that_part() {
        let bytes: Vec<u8> = (0..=255).collect();
        let (_dir, path) = file(&bytes);

        let whole = serve(&path, &Method::GET, None).unwrap();
        assert_eq!(whole.status(), StatusCode::OK);
        assert_eq!(whole.body(), &bytes);
        assert_eq!(header(&whole, header::ACCEPT_RANGES), "bytes");
        assert_eq!(header(&whole, header::CONTENT_TYPE), "video/mp4");

        let part = serve(&path, &Method::GET, Some("bytes=10-19")).unwrap();
        assert_eq!(part.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(part.body(), &bytes[10..20]);
        assert_eq!(header(&part, header::CONTENT_RANGE), "bytes 10-19/256");
        assert_eq!(header(&part, header::CONTENT_LENGTH), "10");

        let open = serve(&path, &Method::GET, Some("bytes=250-")).unwrap();
        assert_eq!(open.body(), &bytes[250..]);
        let suffix = serve(&path, &Method::GET, Some("bytes=-6")).unwrap();
        assert_eq!(suffix.body(), &bytes[250..]);
        assert_eq!(header(&suffix, header::CONTENT_RANGE), "bytes 250-255/256");

        let past = serve(&path, &Method::GET, Some("bytes=256-")).unwrap();
        assert_eq!(past.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(header(&past, header::CONTENT_RANGE), "bytes */256");
        assert!(past.body().is_empty());

        let head = serve(&path, &Method::HEAD, None).unwrap();
        assert_eq!(header(&head, header::CONTENT_LENGTH), "256");
        assert!(head.body().is_empty());
    }

    #[test]
    fn a_long_range_is_answered_in_chunks() {
        let size = MAX_CHUNK as usize + 100;
        let (_dir, path) = file(&vec![9u8; size]);
        let first = serve(&path, &Method::GET, Some("bytes=0-")).unwrap();
        assert_eq!(first.body().len(), MAX_CHUNK as usize);
        assert_eq!(
            header(&first, header::CONTENT_RANGE),
            format!("bytes 0-{}/{size}", MAX_CHUNK - 1)
        );
        let rest = serve(&path, &Method::GET, Some(&format!("bytes={MAX_CHUNK}-"))).unwrap();
        assert_eq!(rest.body().len(), 100);
    }

    #[test]
    fn media_types_follow_the_extension() {
        let cases = [
            ("a.MP3", "audio/mpeg"),
            ("a.m4a", "audio/mp4"),
            ("a.wav", "audio/wav"),
            ("a.ogg", "audio/ogg"),
            ("a.mp4", "video/mp4"),
            ("a.mov", "video/quicktime"),
            ("a.webm", "video/webm"),
            ("a.svg", "image/svg+xml"),
            ("a.html", "application/octet-stream"),
            ("noextension", "application/octet-stream"),
        ];
        for (name, expected) in cases {
            assert_eq!(content_type(Path::new(name)), expected, "{name}");
        }
        for ext in IMAGE_EXTENSIONS
            .iter()
            .chain(AUDIO_EXTENSIONS)
            .chain(VIDEO_EXTENSIONS)
        {
            assert_ne!(
                content_type(Path::new(&format!("a.{ext}"))),
                "application/octet-stream",
                "{ext} can be picked, so it needs a type"
            );
        }
    }

    #[test]
    #[cfg(unix)]
    fn programs_and_executables_are_not_opened() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let make = |name: &str, mode: u32| {
            let path = dir.path().join(name);
            std::fs::write(&path, b"x").unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
            path
        };
        assert!(may_open(&make("Report.pdf", 0o644)));
        assert!(may_open(&make("notes.txt", 0o600)));
        assert!(!may_open(&make("run.command", 0o644)));
        assert!(!may_open(&make("Setup.PKG", 0o644)));
        assert!(!may_open(&make("site.webloc", 0o644)));
        assert!(
            !may_open(&make("tool", 0o755)),
            "the executable bit runs it in Terminal"
        );
        assert!(!may_open(&make("Report 2.pdf", 0o744)));
        assert!(!may_open(dir.path()), "folders aren't opened");
        assert!(!may_open(&dir.path().join("missing.pdf")));
    }
}
