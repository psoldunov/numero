//! What the app is asked to open: files, from the command line, the file
//! manager or Finder, and `numero://` links.
//!
//! Numi registers `numi://` but does nothing with it (`docs/numi-spec.md`
//! §12.7). Numero's scheme does:
//!
//! - `numero://new?text=<text>` adds a calculation holding the text;
//! - `numero://open?name=<name>` opens the calculation of that name;
//! - `numero://show` shows the window.
//!
//! Each shows the window. A file is read as UTF-8 text and becomes a new
//! calculation, as Numi's `application(_:openFile:)` does.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::ffi::{OsStrExt as _, OsStringExt as _};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The scheme, without its colon.
pub(crate) const SCHEME: &str = "numero";

/// The largest file opened as a calculation: Numi's calculations are a few
/// lines; anything this big is not one.
const MAX_FILE_BYTES: u64 = 4 << 20;

/// The longest text a link may carry.
const MAX_TEXT_BYTES: usize = 1 << 20;

/// One thing to open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Opening {
    /// A file, by its absolute path.
    File(PathBuf),
    /// A `numero://` link.
    Link(Link),
}

/// What a `numero://` link asks for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Link {
    /// A new calculation holding `text`.
    New { text: String },
    /// The calculation named `name`.
    Open { name: String },
    /// Only the window.
    Show,
}

impl Opening {
    /// Whether the opening is well formed: a file by an absolute path, a
    /// link within its limits. Checked again on what another instance sent.
    pub(crate) fn is_valid(&self) -> bool {
        match self {
            Self::File(path) => path.is_absolute(),
            Self::Link(Link::New { text }) => text.len() <= MAX_TEXT_BYTES,
            Self::Link(Link::Open { name }) => !name.trim().is_empty() && name.len() <= 255,
            Self::Link(Link::Show) => true,
        }
    }
}

/// Reads a `numero://` link.
pub(crate) fn parse_link(url: &str) -> Result<Link, String> {
    let url = url.trim();
    let rest = url
        .split_once(':')
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case(SCHEME))
        .map(|(_, rest)| rest)
        .ok_or_else(|| format!("{url:?} isn't a {SCHEME}:// link"))?;
    let rest = rest.strip_prefix("//").unwrap_or(rest);
    let (rest, _fragment) = rest.split_once('#').unwrap_or((rest, ""));
    let (action, query) = rest.split_once('?').unwrap_or((rest, ""));
    let action = action.trim_end_matches('/').to_ascii_lowercase();
    let fields = query_fields(query)?;
    let field = |name: &str| {
        fields
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    };
    let link = match action.as_str() {
        "new" => Link::New {
            text: field("text").unwrap_or_default(),
        },
        "open" => Link::Open {
            name: field("name").ok_or("numero://open needs a name")?,
        },
        "show" | "" => Link::Show,
        other => return Err(format!("numero:// doesn't do {other:?}")),
    };
    if Opening::Link(link.clone()).is_valid() {
        Ok(link)
    } else {
        Err(format!("{url:?} asks for too much, or for nothing"))
    }
}

/// The `key=value` fields of a query, decoded as a form is: `+` is a space.
fn query_fields(query: &str) -> Result<Vec<(String, String)>, String> {
    query
        .split('&')
        .filter(|field| !field.is_empty())
        .map(|field| {
            let (key, value) = field.split_once('=').unwrap_or((field, ""));
            let decode = |part: &str| {
                let bytes = percent_decode(&part.replace('+', " "))?;
                String::from_utf8(bytes).map_err(|_| "a link's text isn't UTF-8".to_owned())
            };
            Ok((decode(key)?, decode(value)?))
        })
        .collect()
}

/// `%XX` escapes turned back into the bytes they stand for.
fn percent_decode(text: &str) -> Result<Vec<u8>, String> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while let Some(&byte) = bytes.get(index) {
        if byte == b'%' {
            let escape = bytes
                .get(index + 1..index + 3)
                .and_then(|hex| std::str::from_utf8(hex).ok())
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
                .ok_or_else(|| format!("{text:?} has a broken % escape"))?;
            decoded.push(escape);
            index += 3;
        } else {
            decoded.push(byte);
            index += 1;
        }
    }
    Ok(decoded)
}

/// The path a `file://` URL names, if it names a local one.
pub(crate) fn path_from_file_url(url: &str) -> Option<PathBuf> {
    let rest = url
        .split_once(':')
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("file"))?
        .1
        .strip_prefix("//")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    if !rest.starts_with('/') {
        return None;
    }
    let path = percent_decode(rest).ok()?;
    Some(PathBuf::from(OsString::from_vec(path)))
}

/// What the platform handed over, a URL or a path, as an opening. Logs and
/// drops what it can't open.
pub(crate) fn from_platform(url: &str) -> Option<Opening> {
    let opening = if let Some(path) = path_from_file_url(url) {
        Some(Opening::File(path))
    } else {
        parse_link(url).map(Opening::Link).ok()
    };
    if opening.is_none() {
        tracing::warn!(
            url,
            "asked to open something that isn't a file or a numero:// link"
        );
    }
    opening
}

/// What the command line asks to open: links, file URLs and paths, the
/// relative ones taken from `cwd`. Options (`-…`) are skipped.
pub(crate) fn from_args(args: impl IntoIterator<Item = OsString>, cwd: &Path) -> Vec<Opening> {
    args.into_iter()
        .filter_map(|arg| {
            if arg.as_bytes().starts_with(b"-") {
                tracing::debug!(?arg, "skipping an option");
                return None;
            }
            if let Some(text) = arg.to_str()
                && text.contains("://")
            {
                return match parse_link(text) {
                    Ok(link) => Some(Opening::Link(link)),
                    Err(_) => path_from_file_url(text).map(Opening::File).or_else(|| {
                        tracing::warn!(text, "can't open this link");
                        None
                    }),
                };
            }
            if let Some(text) = arg.to_str()
                && text
                    .get(..SCHEME.len() + 1)
                    .is_some_and(|start| start.eq_ignore_ascii_case("numero:"))
            {
                return parse_link(text)
                    .inspect_err(|error| tracing::warn!(%error, "can't open this link"))
                    .ok()
                    .map(Opening::Link);
            }
            Some(Opening::File(cwd.join(OsStr::new(&arg))))
        })
        .collect()
}

/// The text of the file at `path`, to open as a calculation.
pub(crate) fn read_calculation(path: &Path) -> Result<String, String> {
    let shown = path.display();
    let metadata = fs::metadata(path).map_err(|error| format!("{shown}: {error}"))?;
    if !metadata.is_file() {
        return Err(format!("{shown} isn't a file"));
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err(format!("{shown} is too big to be a calculation"));
    }
    let bytes = fs::read(path).map_err(|error| format!("{shown}: {error}"))?;
    String::from_utf8(bytes).map_err(|_| format!("{shown} isn't UTF-8 text"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(url: &str) -> Link {
        parse_link(url).unwrap()
    }

    #[test]
    fn new_carries_its_text_decoded() {
        assert_eq!(
            link("numero://new?text=5%20inches%20in%20cm%0A%2410+in+EUR"),
            Link::New {
                text: "5 inches in cm\n$10 in EUR".to_owned()
            }
        );
        assert_eq!(
            link("numero://new"),
            Link::New {
                text: String::new()
            }
        );
        assert_eq!(
            link("NUMERO://New/?text=%C3%A9"),
            Link::New {
                text: "é".to_owned()
            }
        );
    }

    #[test]
    fn open_names_a_calculation() {
        assert_eq!(
            link("numero://open?name=Groceries%202026"),
            Link::Open {
                name: "Groceries 2026".to_owned()
            }
        );
        assert!(parse_link("numero://open").is_err());
        assert!(parse_link("numero://open?name=").is_err());
        assert!(parse_link("numero://open?name=%20").is_err());
    }

    #[test]
    fn show_and_the_bare_scheme_show_the_window() {
        assert_eq!(link("numero://show"), Link::Show);
        assert_eq!(link("numero://"), Link::Show);
        assert_eq!(link("numero:show"), Link::Show);
    }

    #[test]
    fn anything_else_is_refused() {
        assert!(parse_link("numero://delete?name=x").is_err());
        assert!(parse_link("numi://new?text=1").is_err());
        assert!(parse_link("https://numero/new").is_err());
        assert!(parse_link("numero://new?text=%zz").is_err());
        assert!(parse_link("numero://new?text=%ff").is_err());
        let huge = format!("numero://new?text={}", "1".repeat(MAX_TEXT_BYTES + 1));
        assert!(parse_link(&huge).is_err());
    }

    #[test]
    fn a_file_url_is_a_local_path() {
        assert_eq!(
            path_from_file_url("file:///Users/me/My%20Sums.numi"),
            Some(PathBuf::from("/Users/me/My Sums.numi"))
        );
        assert_eq!(
            path_from_file_url("file://localhost/tmp/a.txt"),
            Some(PathBuf::from("/tmp/a.txt"))
        );
        assert_eq!(path_from_file_url("file://server/share/a.txt"), None);
        assert_eq!(path_from_file_url("numero://show"), None);
    }

    #[test]
    fn the_platform_hands_over_files_and_links() {
        assert_eq!(
            from_platform("file:///tmp/a.numi"),
            Some(Opening::File(PathBuf::from("/tmp/a.numi")))
        );
        assert_eq!(
            from_platform("numero://show"),
            Some(Opening::Link(Link::Show))
        );
        assert_eq!(from_platform("https://numi.app"), None);
    }

    #[test]
    fn the_command_line_opens_paths_links_and_skips_options() {
        let args = [
            "--autostart",
            "sums.numi",
            "/abs/b.txt",
            "numero://new?text=2*3",
            "file:///c.numi",
        ]
        .map(OsString::from);
        assert_eq!(
            from_args(args, Path::new("/home/me")),
            [
                Opening::File(PathBuf::from("/home/me/sums.numi")),
                Opening::File(PathBuf::from("/abs/b.txt")),
                Opening::Link(Link::New {
                    text: "2*3".to_owned()
                }),
                Opening::File(PathBuf::from("/c.numi")),
            ]
        );
        assert!(from_args([OsString::from("numero://nope")], Path::new("/")).is_empty());
    }

    #[test]
    fn only_absolute_files_and_bounded_links_are_valid() {
        assert!(Opening::File(PathBuf::from("/a")).is_valid());
        assert!(!Opening::File(PathBuf::from("a")).is_valid());
        assert!(
            !Opening::Link(Link::Open {
                name: "x".repeat(300)
            })
            .is_valid()
        );
    }

    #[test]
    fn a_text_file_reads_as_a_calculation_and_anything_else_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let text = dir.path().join("a.numi");
        fs::write(&text, "1 + 2\n").unwrap();
        assert_eq!(read_calculation(&text).unwrap(), "1 + 2\n");

        let binary = dir.path().join("b.numi");
        fs::write(&binary, [0xff, 0xfe, 0x00]).unwrap();
        assert!(read_calculation(&binary).is_err());
        assert!(read_calculation(dir.path()).is_err());
        assert!(read_calculation(&dir.path().join("missing")).is_err());
    }
}
