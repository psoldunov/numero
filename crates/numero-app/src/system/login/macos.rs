//! macOS: the app's own login item, `SMAppService.mainApp`, as concierge's
//! (`crates/concierge-app/src/app_service/macos.rs`). Only the app started
//! from its bundle has one; the binary `cargo run` builds says so instead.

use objc2::rc::Retained;
use objc2_foundation::{NSBundle, NSError};
use objc2_service_management::{SMAppService, SMAppServiceStatus, kSMErrorAlreadyRegistered};

/// Why a bare binary can't open at login.
const UNBUNDLED: &str =
    "Open at login needs Numero.app: this copy of Numero runs outside an app bundle";

/// Whether this process is an app started from its bundle, not a binary
/// cargo built, which has no bundle id.
fn from_bundle() -> bool {
    NSBundle::mainBundle().bundleIdentifier().is_some()
}

#[expect(
    unsafe_code,
    reason = "objc2 marks every SMAppService method unsafe; none of them is"
)]
fn main_app() -> Retained<SMAppService> {
    // SAFETY: a class method that takes nothing and returns a retained
    // object; it reads no memory of ours.
    unsafe { SMAppService::mainAppService() }
}

/// Whether the login item is registered and allowed to run. One the user
/// turned off in System Settings reads as off.
#[expect(
    unsafe_code,
    reason = "objc2 marks every SMAppService method unsafe; none of them is"
)]
pub(in crate::system) fn is_on() -> bool {
    if !from_bundle() {
        return false;
    }
    // SAFETY: a live SMAppService; the call takes nothing.
    let status = unsafe { main_app().status() };
    status == SMAppServiceStatus::Enabled
}

/// Registers the login item, or unregisters it.
#[expect(
    unsafe_code,
    reason = "objc2 marks every SMAppService method unsafe; none of them is"
)]
pub(in crate::system) fn set(on: bool) -> Result<(), String> {
    if !from_bundle() {
        return Err(UNBUNDLED.to_owned());
    }
    let service = main_app();
    let done = if on {
        // SAFETY: a live SMAppService; the call takes nothing.
        match unsafe { service.registerAndReturnError() } {
            Err(error) if u32::try_from(error.code()) == Ok(kSMErrorAlreadyRegistered) => Ok(()),
            other => other,
        }
    } else {
        // SAFETY: as above.
        unsafe { service.unregisterAndReturnError() }
    };
    done.map_err(|error| describe(&error))?;
    tracing::info!(on, "changed Numero's login item");
    Ok(())
}

/// What `error` says, as the user reads it.
fn describe(error: &NSError) -> String {
    error.localizedDescription().to_string()
}
