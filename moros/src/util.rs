use std::io::Write;

pub(crate) fn latlon_from_path(path: &str) -> Option<(f64, f64)> {
    // two floats, separated by a comma
    path.split_once(',').and_then(|(lat, lon)| {
        lat.parse::<f64>()
            .and_then(|lat| lon.parse::<f64>().map(|lon| (lat, lon)))
            .ok()
    })
}

pub(crate) fn wants_plaintext(req: &caveman::Request) -> bool {
    // If text/plain comes before anything with html
    // in the accept header
    for accept in req
        .headers()
        .get_all(caveman::http::HeaderName::from_static("accept"))
    {
        // y no &[u8].contains(b"needle")?
        // https://github.com/rust-lang/rust/issues/134149
        if accept.as_bytes().windows(4).any(|w| w == b"html") {
            break;
        }
        if accept == "text/plain" {
            return true;
        }
    }

    // Or the query string contains txt=1
    caveman::parse_qs(req.uri().query().unwrap_or_default())
        .flatten()
        .any(|(key, value)| key == "txt" && value == "1")
}

// preserve starting /; strip last one
// so that mathing /path also matches /path/
pub(crate) fn normalize(mut path: &str) -> &str {
    if path.len() > 1
        && let Some(prefix) = path.strip_suffix("/")
    {
        path = prefix;
    }
    path
}

// Shitty fmt::Write adapter for stdout
// erases io errors into fmt::Error
// https://github.com/rust-lang/libs-team/issues/133
pub(crate) struct FmtStdout(std::io::StdoutLock<'static>);

impl FmtStdout {
    pub(crate) fn new() -> Self {
        let stdout = std::io::stdout();
        let guard = stdout.lock();
        Self(guard)
    }
}

impl std::fmt::Write for FmtStdout {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.0
            .write_all(s.as_bytes())
            .map_err(|_err| std::fmt::Error)
    }
}
