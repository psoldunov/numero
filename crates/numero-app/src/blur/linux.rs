//! Blur behind a window's translucent parts, through the compositor's
//! ext-background-effect-v1.
//!
//! GPUI asks for blur only through KDE's older `org_kde_kwin_blur`, which
//! `KWin` 6.7 no longer offers, so the app asks itself: one second
//! `wayland-client` handle on GPUI's own `wl_display`, shared by every
//! window, binds the effect manager once and attaches an effect to each
//! window's surface. The region is rounded to the window's corners and
//! applies on GPUI's next commit. The caller places it: `KWin` reads it from
//! the window geometry's corner rather than the surface's.
//!
//! The manager also says whether the compositor blurs at all, and
//! [`available`] passes that on: where nothing blurs, the views paint their
//! chrome opaque rather than show the desktop sharp behind it.
//!
//! The compositor treats a blur region set on a destroyed surface as a
//! protocol error, which would end GPUI's connection too, so a [`Blur`] is
//! only ever updated while its window is drawing, and is dropped with it.

use std::mem::ManuallyDrop;

use gpui::{App, Bounds, Corners, Global, Pixels, Window};
use raw_window_handle::{
    HasDisplayHandle as _, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
};
use wayland_backend::client::{Backend, ObjectId};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_compositor::WlCompositor;
use wayland_client::protocol::wl_region::WlRegion;
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy as _, QueueHandle, WEnum, delegate_noop,
};
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_manager_v1::{
    self, Capability, ExtBackgroundEffectManagerV1,
};
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1;

/// A region's rectangles, as `[x, y, width, height]`.
type Region = Vec<[i32; 4]>;

/// Joins GPUI's display through `window`, once for the whole app, so that
/// [`available`] can answer before the window's first frame. Call it while
/// the window's view is built.
pub(crate) fn connect(window: &Window, cx: &mut App) {
    if !cx.has_global::<Shared>() {
        cx.set_global(Shared(Effects::join(window).map(ManuallyDrop::new)));
    }
}

/// Whether the compositor blurs behind a window's translucent parts.
pub(crate) fn available(cx: &App) -> bool {
    effects(cx).is_some_and(|effects| effects.capabilities.blur)
}

/// Takes in what the compositor has said since the last frame, and returns
/// whether [`available`] changed with it.
pub(crate) fn refresh(cx: &mut App) -> bool {
    if !cx.has_global::<Shared>() {
        return false;
    }
    let Some(effects) = cx.global_mut::<Shared>().0.as_deref_mut() else {
        return false;
    };
    let before = effects.capabilities;
    if let Err(error) = effects.queue.dispatch_pending(&mut effects.capabilities) {
        tracing::warn!(%error, "couldn't read the compositor's blur events");
    }
    effects.capabilities != before
}

fn effects(cx: &App) -> Option<&Effects> {
    cx.try_global::<Shared>()?.0.as_deref()
}

/// The app's one handle on the compositor's background effects, or `None`
/// where there is none to have: no Wayland, or no background effects.
///
/// Never dropped: GPUI may close its display before the app's globals go,
/// and a guest backend that cleaned up after that would touch freed memory.
/// The process exit takes the connection with it.
struct Shared(Option<ManuallyDrop<Effects>>);

impl Global for Shared {}

struct Effects {
    connection: Connection,
    queue: EventQueue<Capabilities>,
    compositor: WlCompositor,
    /// Kept for the effects it makes and the capabilities it reports.
    manager: ExtBackgroundEffectManagerV1,
    capabilities: Capabilities,
}

impl Effects {
    /// Joins the display `window` is on and binds the effect manager.
    fn join(window: &Window) -> Option<Self> {
        let RawDisplayHandle::Wayland(display) = window.display_handle().ok()?.as_raw() else {
            return None;
        };
        // SAFETY: GPUI's `wl_display` lives as long as the app, and the
        // connection is never dropped (see `Shared`).
        // The backend is a guest: it leaves GPUI's connection open.
        #[expect(
            unsafe_code,
            reason = "joins GPUI's Wayland connection; GPUI has no blur for this protocol"
        )]
        let backend = unsafe { Backend::from_foreign_display(display.display.as_ptr().cast()) };
        let connection = Connection::from_backend(backend);
        let (globals, mut queue) = registry_queue_init::<Capabilities>(&connection).ok()?;
        let handle = queue.handle();
        let Ok(manager) = globals.bind::<ExtBackgroundEffectManagerV1, _, _>(&handle, 1..=1, ())
        else {
            tracing::debug!("the compositor has no ext-background-effect-v1; no blur");
            return None;
        };
        let compositor = globals
            .bind::<WlCompositor, _, _>(&handle, 1..=6, ())
            .ok()?;
        // The manager reports its capabilities as soon as it is bound.
        let mut capabilities = Capabilities::default();
        if let Err(error) = queue.roundtrip(&mut capabilities) {
            tracing::warn!(%error, "couldn't hear what blur the compositor offers");
            return None;
        }
        tracing::debug!(blur = capabilities.blur, "joined ext-background-effect-v1");
        Some(Self {
            connection,
            queue,
            compositor,
            manager,
            capabilities,
        })
    }
}

/// What the compositor says it applies behind a surface. Also the state the
/// joined connection's events are dispatched to.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Capabilities {
    blur: bool,
}

/// The blur behind one window.
pub(crate) struct Blur {
    connection: Connection,
    effect: ExtBackgroundEffectSurfaceV1,
    /// The region last set, so an unchanged frame sends nothing.
    region: Option<Region>,
}

impl Blur {
    /// Attaches an effect to `window`'s surface. `None` where the window
    /// isn't a Wayland surface or the compositor has no background effects.
    pub(crate) fn attach(window: &Window, cx: &mut App) -> Option<Self> {
        connect(window, cx);
        let effects = effects(cx)?;
        // GPUI's own `window_handle` names the window, not its surface.
        let RawWindowHandle::Wayland(surface) =
            HasWindowHandle::window_handle(window).ok()?.as_raw()
        else {
            return None;
        };
        // SAFETY: the pointer is GPUI's live `wl_surface` for this window,
        // and the id is only used here, while the window draws.
        #[expect(
            unsafe_code,
            reason = "names GPUI's wl_surface on the joined connection"
        )]
        let id =
            unsafe { ObjectId::from_ptr(WlSurface::interface(), surface.surface.as_ptr().cast()) }
                .ok()?;
        let surface = WlSurface::from_id(&effects.connection, id).ok()?;
        let effect = effects
            .manager
            .get_background_effect(&surface, &effects.queue.handle(), ());
        Some(Self {
            connection: effects.connection.clone(),
            effect,
            region: None,
        })
    }

    /// Blurs behind `bounds` of the surface, its corners rounded to
    /// `radii`. Sends nothing when that hasn't changed.
    pub(crate) fn set(&mut self, bounds: Bounds<Pixels>, radii: Corners<Pixels>, cx: &App) {
        let region = rounded(bounds, radii);
        if self.region.as_ref() == Some(&region) {
            return;
        }
        let Some(effects) = effects(cx) else {
            return;
        };
        let wl_region = effects
            .compositor
            .create_region(&effects.queue.handle(), ());
        for [x, y, width, height] in &region {
            wl_region.add(*x, *y, *width, *height);
        }
        self.effect.set_blur_region(Some(&wl_region));
        wl_region.destroy();
        self.region = Some(region);
        self.flush();
    }

    fn flush(&self) {
        if let Err(error) = self.connection.flush() {
            tracing::warn!(%error, "couldn't send the blur region");
        }
    }
}

impl Drop for Blur {
    fn drop(&mut self) {
        self.effect.destroy();
        self.flush();
    }
}

impl Dispatch<ExtBackgroundEffectManagerV1, ()> for Capabilities {
    fn event(
        state: &mut Self,
        _: &ExtBackgroundEffectManagerV1,
        event: ext_background_effect_manager_v1::Event,
        &(): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_background_effect_manager_v1::Event::Capabilities { flags } = event {
            state.blur = blurs(flags);
        }
    }
}

/// Whether the manager's `flags` include blur, whatever flags from a later
/// version come with it.
fn blurs(flags: WEnum<Capability>) -> bool {
    let flags = match flags {
        WEnum::Value(flags) => flags,
        WEnum::Unknown(raw) => Capability::from_bits_truncate(raw),
    };
    flags.contains(Capability::Blur)
}

impl Dispatch<WlRegistry, GlobalListContents> for Capabilities {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as wayland_client::Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(Capabilities: WlCompositor);
delegate_noop!(Capabilities: WlRegion);
delegate_noop!(Capabilities: ExtBackgroundEffectSurfaceV1);

/// `bounds` as whole-pixel rectangles, one per row where a corner bends.
fn rounded(bounds: Bounds<Pixels>, radii: Corners<Pixels>) -> Region {
    let (x, y) = (whole(bounds.origin.x), whole(bounds.origin.y));
    let (width, height) = (whole(bounds.size.width), whole(bounds.size.height));
    if width <= 0 || height <= 0 {
        return Vec::new();
    }
    let most = width.min(height) / 2;
    let radius = |corner: Pixels| whole(corner).clamp(0, most);
    let (top_left, top_right) = (radius(radii.top_left), radius(radii.top_right));
    let (bottom_left, bottom_right) = (radius(radii.bottom_left), radius(radii.bottom_right));
    let top = top_left.max(top_right);
    let bottom = bottom_left.max(bottom_right);
    let row = |at: i32, row: i32, left: i32, right: i32| {
        let (left, right) = (inset(left, row), inset(right, row));
        [x + left, at, width - left - right, 1]
    };
    let mut region: Region = (0..top)
        .map(|n| row(y + n, n, top_left, top_right))
        .collect();
    region.push([x, y + top, width, height - top - bottom]);
    region.extend((0..bottom).map(|n| row(y + height - 1 - n, n, bottom_left, bottom_right)));
    region
}

/// How far the `row`th row from a corner of `radius` stands in from its
/// side, so the whole row stays inside the curve. A row that reached past
/// it would blur a stair-step of the desktop outside the window's rounded,
/// antialiased corner.
fn inset(radius: i32, row: i32) -> i32 {
    if row >= radius {
        return 0;
    }
    // The row's outer edge is the narrowest part of the circle it spans.
    let from_centre = radius - row;
    radius - (radius * radius - from_centre * from_centre).isqrt()
}

/// `pixels`, rounded to a whole logical pixel.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a window's logical size fits an i32 many times over"
)]
fn whole(pixels: Pixels) -> i32 {
    f32::from(pixels).round() as i32
}

#[cfg(test)]
mod tests {
    use gpui::{Bounds, Corners, point, px, size};
    use wayland_client::WEnum;

    use super::{Capability, blurs, inset, rounded};

    #[test]
    fn a_corner_row_stands_in_most_at_the_edge_and_not_at_all_past_the_radius() {
        assert!(inset(12, 0) >= 6);
        assert!(inset(12, 0) > inset(12, 6));
        assert_eq!(inset(12, 12), 0);
        assert_eq!(inset(0, 0), 0);
    }

    #[test]
    fn the_region_covers_the_bounds_with_one_row_per_rounded_line() {
        let bounds = Bounds::new(point(px(20.), px(20.)), size(px(200.), px(100.)));
        let region = rounded(bounds, Corners::all(px(12.)));
        assert_eq!(region.len(), 12 + 1 + 12);
        let middle = region[12];
        assert_eq!(middle, [20, 32, 200, 76]);
        let rows: i32 = region.iter().map(|rect| rect[3]).sum();
        assert_eq!(rows, 100);
    }

    #[test]
    fn square_corners_make_one_rectangle() {
        let bounds = Bounds::new(point(px(0.), px(0.)), size(px(80.), px(40.)));
        assert_eq!(rounded(bounds, Corners::all(px(0.))), vec![[0, 0, 80, 40]]);
    }

    #[test]
    fn only_the_blur_flag_says_the_compositor_blurs() {
        assert!(blurs(WEnum::Value(Capability::Blur)));
        assert!(!blurs(WEnum::Value(Capability::empty())));
        assert!(blurs(WEnum::Unknown(0b101)));
        assert!(!blurs(WEnum::Unknown(0b100)));
    }
}
