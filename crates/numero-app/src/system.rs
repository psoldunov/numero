//! The app's place in the desktop: the menu-bar (tray) item, the global
//! shortcut, opening at login, staying on top, the window's saved frame, the
//! single running instance, and what the app is asked to open (files and
//! `numero://` links).
//!
//! Contract with the rest of the app: the preferences call
//! [`settings_changed`] after they save, and read and write the login item
//! through [`launch_at_login`] and [`set_launch_at_login`], which ask the
//! system rather than the settings file (the system's list keeps the
//! choice, as Numi's helper does).
//!
//! A launch the login started, in menu-bar mode, starts in the tray: the
//! window opens when the tray item, the shortcut or a second launch asks for
//! it, or at once if the tray item can't be shown.
//!
//! Threads (the tray's, the hotkey's, the single instance's) and the
//! platform's callbacks reach the UI as [`Event`]s over one channel, which a
//! task on the main thread drains.
//!
//! - `windows`: showing, hiding and opening the calculator's window.
//! - `saved_frame`: the window's size and place on disk.
//! - `window_level`: always on top, and macOS's Spaces.
//! - `tray`, `hotkey`, `login`: menu-bar mode, the global shortcut and the
//!   login item, per platform.
//! - `instance` and `opening`: the single instance and what it is asked to
//!   open.

mod hotkey;
mod instance;
mod login;
mod opening;
mod saved_frame;
mod tray;
mod window_level;
mod windows;

use std::ffi::OsString;
use std::path::PathBuf;

use async_channel::{Receiver, Sender};
use gpui::{App, Application, Global, Subscription, Task, WindowId};

use self::opening::Link;
pub(crate) use self::opening::Opening;
pub(crate) use self::saved_frame::MIN_SIZE as MIN_WINDOW_SIZE;
use self::saved_frame::{FrameFile, SavedFrame};
use self::tray::{TrayEvent, TrayItem};
use crate::calculator::main_window;
use crate::menus;
use crate::settings::Settings;

/// What the app's threads and the platform ask of the UI.
#[derive(Debug)]
pub(crate) enum Event {
    /// Show the window, or hide it if it is in front: the tray's click and
    /// the global shortcut.
    Toggle,
    /// Open these, and show the window: a second launch, the OS.
    Open(Vec<Opening>),
    Tray(TrayEvent),
}

/// What a launch found before the app starts.
#[derive(Debug)]
pub(crate) struct Launch {
    openings: Vec<Opening>,
    /// The autostart entry started it, at login.
    autostarted: bool,
    instance: Option<instance::Instance>,
    events: Sender<Event>,
    received: Receiver<Event>,
}

/// Reads what the command line asks to open and hands it to the Numero
/// already running, if one is. `None` means one is, and this launch should
/// end.
pub(crate) fn launch() -> Option<Launch> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let openings = opening::from_args(args.iter().cloned(), &cwd);
    match instance::claim(&openings) {
        instance::Claim::Forwarded => {
            tracing::info!("Numero is running already; showed it");
            None
        }
        instance::Claim::First(instance) => {
            login::upgrade();
            let (events, received) = async_channel::unbounded();
            Some(Launch {
                openings,
                autostarted: login::autostarted(&args),
                instance,
                events,
                received,
            })
        }
    }
}

impl Launch {
    /// Hooks up what the platform hands the app before it runs: files and
    /// links opened from Finder or the browser, and a click on the Dock
    /// icon, which shows the window (macOS).
    pub(crate) fn listen(&self, application: &Application) {
        let events = self.events.clone();
        application.on_open_urls(move |urls| {
            let openings: Vec<Opening> = urls
                .iter()
                .filter_map(|url| opening::from_platform(url))
                .collect();
            if !openings.is_empty() {
                let _gone = events.try_send(Event::Open(openings));
            }
        });
        application.on_reopen(windows::show);
    }
}

/// The settings this module applied last.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Applied {
    menu_bar_mode: bool,
    always_on_top: bool,
    hotkey: Option<String>,
}

/// The module's state, for as long as the app runs.
struct Desktop {
    events: Sender<Event>,
    applied: Option<Applied>,
    tray: Option<tray::Tray>,
    /// Whether the desktop shows the tray item now.
    tray_shown: bool,
    /// The app started in the tray and the item isn't up yet: if it can't
    /// be shown, the window opens instead.
    waiting_for_tray: bool,
    hotkey: Option<hotkey::Binding>,
    /// The calculator's window, while it is open.
    main_window: Option<WindowId>,
    /// The window is being closed to hide it, not by the user.
    hiding: bool,
    frame_file: Option<FrameFile>,
    /// The frame seen last, not yet saved.
    unsaved_frame: Option<SavedFrame>,
    frame_save: Option<Task<()>>,
    /// The single instance's socket, removed on quitting.
    socket: Option<PathBuf>,
    _subscriptions: Vec<Subscription>,
}

impl Global for Desktop {}

/// Starts it all: listens for second launches, applies `settings`, opens
/// the window, unless the launch starts in the tray, and what the launch
/// asked to open. Call it while the app finishes launching.
pub(crate) fn start(launch: Launch, settings: &Settings, cx: &mut App) {
    let Launch {
        openings,
        autostarted,
        instance,
        events,
        received,
    } = launch;
    // First, while macOS's launch event is still the current one.
    let at_login = autostarted || login::launched_as_login_item();
    let socket = instance.map(|instance| instance.serve(events.clone()));
    let subscriptions = vec![
        cx.on_window_closed(windows::closed),
        // Every window the app opens, the preferences' too, as Numi does.
        cx.observe_new(|_: &mut gpui::base::Root, window, cx| {
            if let Some(window) = window
                && cx
                    .global::<Desktop>()
                    .applied
                    .as_ref()
                    .is_some_and(|applied| applied.always_on_top)
            {
                windows::keep_on_top_once_shown(window);
            }
        }),
        cx.on_app_quit(|cx| {
            flush_frame(cx);
            if let Some(socket) = &cx.global::<Desktop>().socket {
                instance::release(socket);
            }
            async {}
        }),
    ];
    cx.set_global(Desktop {
        events,
        applied: None,
        tray: None,
        tray_shown: false,
        waiting_for_tray: false,
        hotkey: None,
        main_window: None,
        hiding: false,
        frame_file: FrameFile::from_env(),
        unsaved_frame: None,
        frame_save: None,
        socket,
        _subscriptions: subscriptions,
    });
    cx.spawn(async move |cx| {
        while let Ok(event) = received.recv().await {
            cx.update(|cx| handle(event, cx));
        }
    })
    .detach();
    apply(settings, cx);
    if starts_in_tray(at_login, settings.menu_bar_mode, &openings) {
        tracing::info!("opened at login in menu-bar mode; starting in the tray");
        cx.global_mut::<Desktop>().waiting_for_tray = true;
    } else {
        windows::show(cx);
    }
    if !openings.is_empty() {
        open(openings, cx);
    }
}

/// Whether a launch leaves the window closed and waits in the tray: one
/// the login started, in menu-bar mode, with nothing to open.
const fn starts_in_tray(at_login: bool, menu_bar_mode: bool, openings: &[Opening]) -> bool {
    at_login && menu_bar_mode && openings.is_empty()
}

fn handle(event: Event, cx: &mut App) {
    match event {
        Event::Toggle | Event::Tray(TrayEvent::Toggle) => windows::toggle(cx),
        Event::Open(openings) => open(openings, cx),
        Event::Tray(TrayEvent::Shown) => {
            let desktop = cx.global_mut::<Desktop>();
            desktop.tray_shown = true;
            desktop.waiting_for_tray = false;
        }
        Event::Tray(TrayEvent::Unavailable(reason)) => {
            tracing::info!(%reason, "the tray item isn't shown");
            let desktop = cx.global_mut::<Desktop>();
            desktop.tray_shown = false;
            if std::mem::take(&mut desktop.waiting_for_tray) && desktop.main_window.is_none() {
                tracing::info!("no tray to start in; opening the window");
                windows::show(cx);
            }
        }
        Event::Tray(TrayEvent::Chose(item)) => choose(item, cx),
    }
}

/// Apply the parts of the settings this module owns (menu-bar mode, always
/// on top, the global shortcut) after they changed.
pub(crate) fn settings_changed(_old: &Settings, new: &Settings, cx: &mut App) {
    if cx.has_global::<Desktop>() {
        apply(new, cx);
    }
}

/// Applies what changed since the settings applied last, or everything the
/// first time.
fn apply(settings: &Settings, cx: &mut App) {
    let wanted = Applied {
        menu_bar_mode: settings.menu_bar_mode,
        always_on_top: settings.always_on_top,
        hotkey: settings.hotkey.clone(),
    };
    let before = cx.global::<Desktop>().applied.clone();
    if before.as_ref().map(|applied| applied.menu_bar_mode) != Some(wanted.menu_bar_mode) {
        set_menu_bar_mode(wanted.menu_bar_mode, cx);
    }
    if before.as_ref().map(|applied| applied.always_on_top) != Some(wanted.always_on_top) {
        windows::set_always_on_top(wanted.always_on_top, cx);
    }
    if before.as_ref().map(|applied| &applied.hotkey) != Some(&wanted.hotkey) {
        set_hotkey(wanted.hotkey.as_deref(), cx);
    }
    cx.global_mut::<Desktop>().applied = Some(wanted);
}

/// Puts the tray item up or takes it down, and on macOS hides the Dock icon
/// while it is up, as Numi switches to the accessory activation policy.
fn set_menu_bar_mode(on: bool, cx: &mut App) {
    let desktop = cx.global_mut::<Desktop>();
    desktop.tray = None;
    desktop.tray_shown = false;
    if on {
        let events = desktop.events.clone();
        let tray = tray::Tray::start(tray::menu(offered), events, cx);
        cx.global_mut::<Desktop>().tray = Some(tray);
    }
    #[cfg(target_os = "macos")]
    {
        use gpui::ActivationPolicy;
        if on {
            cx.set_activation_policy(ActivationPolicy::Accessory);
        } else {
            cx.set_activation_policy(ActivationPolicy::Regular);
            if cx.global::<Desktop>().main_window.is_some() {
                cx.activate(true);
            }
        }
    }
    tracing::info!(on, "menu-bar mode");
}

/// Whether the tray offers `item`: everything the app has a command for.
const fn offered(item: TrayItem) -> bool {
    // Numero has no printing yet.
    !matches!(item, TrayItem::Print)
}

/// Binds the global shortcut, or unbinds it.
fn set_hotkey(keystroke: Option<&str>, cx: &mut App) {
    // The old binding goes first: on macOS it unregisters its handler.
    let events = {
        let desktop = cx.global_mut::<Desktop>();
        desktop.hotkey = None;
        desktop.events.clone()
    };
    let Some(keystroke) = keystroke else {
        tracing::info!("no global shortcut");
        return;
    };
    match hotkey::Hotkey::parse(keystroke) {
        Ok(parsed) => {
            let binding = hotkey::Binding::bind(parsed, events, cx);
            cx.global_mut::<Desktop>().hotkey = Some(binding);
        }
        Err(reason) => tracing::warn!(%reason, keystroke, "can't bind the global shortcut"),
    }
}

/// Runs the command `item` stands for: the same actions the menus run.
fn choose(item: TrayItem, cx: &mut App) {
    match item {
        TrayItem::Quit => cx.dispatch_action(&crate::app::Quit),
        TrayItem::Preferences => {
            cx.activate(true);
            cx.dispatch_action(&menus::OpenPreferences);
        }
        // Each acts on the calculator, which needs its window.
        TrayItem::Import => on_calculator(&menus::ImportFile, cx),
        TrayItem::Export => on_calculator(&menus::ExportFile, cx),
        TrayItem::CopyToClipboard => on_calculator(&menus::CopyToClipboard, cx),
        TrayItem::Docs => on_calculator(&menus::OpenDocs, cx),
        TrayItem::Print => tracing::warn!("Numero can't print yet"),
    }
}

fn on_calculator(action: &dyn gpui::Action, cx: &mut App) {
    windows::show(cx);
    cx.dispatch_action(action);
}

/// Opens what a launch, a second launch or the OS asked for, and shows the
/// window.
fn open(openings: Vec<Opening>, cx: &mut App) {
    windows::show(cx);
    for opening in openings {
        match opening {
            Opening::File(path) => match opening::read_calculation(&path) {
                Ok(text) => {
                    tracing::info!(path = %path.display(), "opening a file as a new calculation");
                    open_new_with(text, cx);
                }
                Err(error) => tracing::warn!(%error, "couldn't open the file"),
            },
            Opening::Link(Link::New { text }) => open_new_with(text, cx),
            Opening::Link(Link::Open { name }) => open_named(name, cx),
            Opening::Link(Link::Show) => {}
        }
    }
}

fn open_new_with(text: String, cx: &mut App) {
    main_window::with_calculator(cx, move |calculator, window, cx| {
        calculator.open_new_with(&text, window, cx);
    });
}

/// Opens the calculation whose file is called `name`, with or without its
/// `.txt`; failing that, the one whose name matches ignoring case.
fn open_named(name: String, cx: &mut App) {
    main_window::with_calculator(cx, move |calculator, window, cx| {
        calculator.refresh_entries(cx);
        let ids = calculator.entries.iter().map(|entry| &entry.id);
        let exact = ids
            .clone()
            .find(|id| id.as_str() == name || id.stem() == name);
        let found = exact
            .or_else(|| {
                ids.clone().find(|id| {
                    id.as_str().eq_ignore_ascii_case(&name) || id.stem().eq_ignore_ascii_case(&name)
                })
            })
            .cloned();
        match found {
            Some(id) => calculator.open_calculation(&id, window, cx),
            None => tracing::warn!(name, "numero://open: no calculation of that name"),
        }
    });
}

/// Notes the window's frame after it moved or changed size, and saves it a
/// moment later.
pub(crate) fn frame_changed(window: &gpui::Window, cx: &mut App) {
    let frame = windows::frame_of(window, cx);
    let desktop = cx.global_mut::<Desktop>();
    if desktop.unsaved_frame == Some(frame) {
        return;
    }
    desktop.unsaved_frame = Some(frame);
    let Some(file) = desktop.frame_file.clone() else {
        return;
    };
    let save = cx.spawn(async move |cx| {
        cx.background_executor()
            .timer(windows::SAVE_FRAME_AFTER)
            .await;
        let Some(frame) = cx.update(|cx| cx.global_mut::<Desktop>().unsaved_frame.take()) else {
            return;
        };
        let saved = cx
            .background_executor()
            .spawn(async move { file.save(&frame) })
            .await;
        if let Err(error) = saved {
            tracing::warn!(%error, "couldn't save the window's frame");
        }
    });
    // Replacing the task drops, and so cancels, the one before.
    cx.global_mut::<Desktop>().frame_save = Some(save);
}

/// Saves the frame not yet saved, at once: on quitting.
fn flush_frame(cx: &mut App) {
    let desktop = cx.global_mut::<Desktop>();
    desktop.frame_save = None;
    if let (Some(frame), Some(file)) = (desktop.unsaved_frame.take(), &desktop.frame_file)
        && let Err(error) = file.save(&frame)
    {
        tracing::warn!(%error, "couldn't save the window's frame");
    }
}

/// Whether Numero opens when the user logs in.
pub(crate) fn launch_at_login() -> bool {
    login::is_on()
}

/// Add Numero to, or remove it from, the programs that open at login.
pub(crate) fn set_launch_at_login(enabled: bool) -> Result<(), String> {
    login::set(enabled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_launch_at_login_in_menu_bar_mode_starts_in_the_tray() {
        assert!(starts_in_tray(true, true, &[]));
        assert!(!starts_in_tray(true, false, &[]));
        assert!(!starts_in_tray(false, true, &[]));
        // What the launch asks to open needs the window.
        assert!(!starts_in_tray(true, true, &[Opening::Link(Link::Show)]));
    }
}
