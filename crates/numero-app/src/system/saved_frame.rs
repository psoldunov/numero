//! The window's size and place, kept in `<config dir>/numero/window.json`
//! and given back at the next start, as Numi keeps `windowFrame`
//! (`docs/numi-spec-ui.md` §6.2). The settings file stays the user's; this
//! one is the app's own.
//!
//! Wayland doesn't tell a window where it is, nor let it choose, so there
//! only the size is kept. A place is given back only while some part of
//! the window would still be on its display: a monitor unplugged since
//! leaves the window centred on the main one instead.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use gpui::{Bounds, Pixels, Point, Size, point, px, size};
use serde::{Deserialize, Serialize};

const FILE: &str = "numero/window.json";

/// Numi's window: 370 by 360, centred.
pub(crate) const DEFAULT_SIZE: Size<Pixels> = Size {
    width: px(370.),
    height: px(360.),
};

/// The smallest window the app lays out in.
pub(crate) const MIN_SIZE: Size<Pixels> = Size {
    width: px(300.),
    height: px(200.),
};

/// No display is this large; a bigger size is a broken file.
const MAX_SIDE: f32 = 20_000.;

/// How much of the window must stay on its display, in pixels each way, for
/// its place to be given back.
const MIN_VISIBLE: f32 = 48.;

/// The window as it was last seen.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(crate) struct SavedFrame {
    pub width: f32,
    pub height: f32,
    /// Its top-left corner, relative to its display as GPUI reports it;
    /// `None` where the platform doesn't say (Wayland).
    #[serde(default)]
    pub x: Option<f32>,
    #[serde(default)]
    pub y: Option<f32>,
    /// The display it was on.
    #[serde(default)]
    pub display: Option<u64>,
    /// Whether it filled the screen; the size is then the one it goes back
    /// to.
    #[serde(default)]
    pub maximized: bool,
}

/// A display, as [`place`] needs it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Screen {
    pub id: u64,
    pub bounds: Bounds<Pixels>,
}

/// Where to open the window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Placement {
    /// The display to open it on; `None` for the main one.
    pub display: Option<u64>,
    /// Its place and size, or only its size, to centre it.
    pub frame: Frame,
    pub maximized: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Frame {
    At(Bounds<Pixels>),
    Centered(Size<Pixels>),
}

/// Where the window opens, given what was saved and the displays there are
/// now: the saved size, cut to the display, and the saved place when the
/// window would still show on its display; else Numi's default, centred.
pub(crate) fn place(saved: Option<&SavedFrame>, screens: &[Screen]) -> Placement {
    let Some(saved) = saved.filter(|saved| saved.is_sane()) else {
        return Placement {
            display: None,
            frame: Frame::Centered(DEFAULT_SIZE),
            maximized: false,
        };
    };
    let screen = saved
        .display
        .and_then(|id| screens.iter().find(|screen| screen.id == id));
    let mut window_size = size(px(saved.width), px(saved.height));
    if let Some(screen) = screen {
        window_size.width = window_size.width.min(screen.bounds.size.width);
        window_size.height = window_size.height.min(screen.bounds.size.height);
    }
    let window_size = size(
        window_size.width.max(MIN_SIZE.width),
        window_size.height.max(MIN_SIZE.height),
    );
    let origin = match (saved.x, saved.y, screen) {
        (Some(x), Some(y), Some(screen)) => {
            let frame = Bounds::new(point(px(x), px(y)), window_size);
            shows_on(&frame, &screen.bounds).then_some(frame.origin)
        }
        _ => None,
    };
    Placement {
        display: screen.map(|screen| screen.id),
        frame: origin.map_or(Frame::Centered(window_size), |origin: Point<Pixels>| {
            Frame::At(Bounds::new(origin, window_size))
        }),
        maximized: saved.maximized,
    }
}

impl SavedFrame {
    /// A size a window could have, and a place that is a number.
    fn is_sane(&self) -> bool {
        let side = |value: f32| value.is_finite() && value > 0. && value <= MAX_SIDE;
        let coordinate =
            |value: Option<f32>| value.is_none_or(|v| v.is_finite() && v.abs() <= MAX_SIDE);
        side(self.width) && side(self.height) && coordinate(self.x) && coordinate(self.y)
    }
}

/// Whether at least [`MIN_VISIBLE`] of `frame` lies on `screen` each way.
fn shows_on(frame: &Bounds<Pixels>, screen: &Bounds<Pixels>) -> bool {
    let left = frame.origin.x.max(screen.origin.x);
    let right = (frame.origin.x + frame.size.width).min(screen.origin.x + screen.size.width);
    let top = frame.origin.y.max(screen.origin.y);
    let bottom = (frame.origin.y + frame.size.height).min(screen.origin.y + screen.size.height);
    right - left >= px(MIN_VISIBLE) && bottom - top >= px(MIN_VISIBLE)
}

/// Where the frame is kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FrameFile {
    path: PathBuf,
}

impl FrameFile {
    pub(crate) fn from_env() -> Option<Self> {
        dirs::config_dir().map(|dir| Self::in_dir(&dir))
    }

    pub(crate) fn in_dir(dir: &Path) -> Self {
        Self {
            path: dir.join(FILE),
        }
    }

    /// The frame saved last, if there is a readable one.
    pub(crate) fn load(&self) -> Option<SavedFrame> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return None,
            Err(error) => {
                tracing::warn!(%error, path = %self.path.display(), "couldn't read the window's frame");
                return None;
            }
        };
        serde_json::from_slice(&bytes)
            .inspect_err(|error| {
                tracing::warn!(%error, path = %self.path.display(), "the window's frame is unreadable");
            })
            .ok()
    }

    pub(crate) fn save(&self, frame: &SavedFrame) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_vec_pretty(frame).map_err(io::Error::other)?;
        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, json)?;
        fs::rename(&temporary, &self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN: Screen = Screen {
        id: 1,
        bounds: Bounds {
            origin: Point {
                x: px(0.),
                y: px(0.),
            },
            size: Size {
                width: px(1920.),
                height: px(1080.),
            },
        },
    };

    fn saved(x: Option<f32>, y: Option<f32>) -> SavedFrame {
        SavedFrame {
            width: 500.,
            height: 420.,
            x,
            y,
            display: Some(1),
            maximized: false,
        }
    }

    #[test]
    fn a_saved_frame_loads_back() {
        let dir = tempfile::tempdir().unwrap();
        let file = FrameFile::in_dir(dir.path());
        assert_eq!(file.load(), None);

        let frame = saved(Some(40.), Some(80.));
        file.save(&frame).unwrap();

        assert_eq!(file.load(), Some(frame));
    }

    #[test]
    fn a_broken_file_is_no_frame() {
        let dir = tempfile::tempdir().unwrap();
        let file = FrameFile::in_dir(dir.path());
        fs::create_dir_all(dir.path().join("numero")).unwrap();
        fs::write(dir.path().join(FILE), "{ nope").unwrap();
        assert_eq!(file.load(), None);
    }

    #[test]
    fn nothing_saved_is_numis_window_centred() {
        assert_eq!(
            place(None, &[MAIN]),
            Placement {
                display: None,
                frame: Frame::Centered(DEFAULT_SIZE),
                maximized: false,
            }
        );
    }

    #[test]
    fn a_frame_on_its_display_comes_back_where_it_was() {
        let placement = place(Some(&saved(Some(40.), Some(80.))), &[MAIN]);
        assert_eq!(placement.display, Some(1));
        assert_eq!(
            placement.frame,
            Frame::At(Bounds::new(
                point(px(40.), px(80.)),
                size(px(500.), px(420.))
            ))
        );
    }

    #[test]
    fn a_frame_off_its_display_is_centred_at_its_size() {
        let placement = place(Some(&saved(Some(5000.), Some(80.))), &[MAIN]);
        assert_eq!(placement.frame, Frame::Centered(size(px(500.), px(420.))));
        // Hanging off the edge but mostly on screen is fine.
        let placement = place(Some(&saved(Some(1800.), Some(-20.))), &[MAIN]);
        assert!(matches!(placement.frame, Frame::At(_)));
    }

    #[test]
    fn a_display_gone_since_leaves_the_window_on_the_main_one() {
        let mut frame = saved(Some(40.), Some(80.));
        frame.display = Some(7);
        let placement = place(Some(&frame), &[MAIN]);
        assert_eq!(placement.display, None);
        assert_eq!(placement.frame, Frame::Centered(size(px(500.), px(420.))));
    }

    #[test]
    fn wayland_keeps_only_the_size() {
        let placement = place(Some(&saved(None, None)), &[MAIN]);
        assert_eq!(placement.frame, Frame::Centered(size(px(500.), px(420.))));
    }

    #[test]
    fn sizes_out_of_range_are_cut_or_dropped() {
        let mut huge = saved(Some(0.), Some(0.));
        huge.width = 9000.;
        let placement = place(Some(&huge), &[MAIN]);
        assert_eq!(
            placement.frame,
            Frame::At(Bounds::new(
                point(px(0.), px(0.)),
                size(px(1920.), px(420.))
            ))
        );

        let mut tiny = saved(None, None);
        tiny.width = 10.;
        let placement = place(Some(&tiny), &[MAIN]);
        assert_eq!(
            placement.frame,
            Frame::Centered(size(MIN_SIZE.width, px(420.)))
        );

        for broken in [f32::NAN, -5., 0., 1e9] {
            let mut frame = saved(Some(0.), Some(0.));
            frame.height = broken;
            assert_eq!(
                place(Some(&frame), &[MAIN]).frame,
                Frame::Centered(DEFAULT_SIZE)
            );
        }
        let mut frame = saved(Some(f32::INFINITY), Some(0.));
        frame.display = Some(1);
        assert_eq!(
            place(Some(&frame), &[MAIN]).frame,
            Frame::Centered(DEFAULT_SIZE)
        );
    }

    #[test]
    fn a_maximized_window_comes_back_maximized() {
        let mut frame = saved(Some(40.), Some(80.));
        frame.maximized = true;
        assert!(place(Some(&frame), &[MAIN]).maximized);
    }
}
