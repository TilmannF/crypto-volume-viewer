//! Disables macOS App Nap for the lifetime of the `e2e-webdriver` process.
//!
//! App Nap throttles a background process's timers and I/O once macOS
//! decides it "hasn't recently updated content in the visible portion of a
//! window" -- which the e2e-webdriver harness's default headless build
//! (`visible: false`, see `apps/cryptovol-gui/e2e/wdio.conf.ts`) always
//! satisfies, since it has no visible window content at all. Confirmed by
//! this project's own testing: the WebDriver embedded server's `click`
//! command is implemented purely as a JS `el.click()` evaluated in the
//! WKWebView (see `tauri-plugin-wdio-webdriver`'s `click_element`), not any
//! native input event, so App Nap's throttling of the process (not any
//! native input-routing bug) is what intermittently delayed that JS
//! evaluation past WebdriverIO's script timeout -- reproduced as
//! intermittent `"Script execution timed out"` failures on
//! `apps/cryptovol-gui/e2e/specs/gui-smoke.e2e.ts`'s wrong-password spec,
//! correlated with concurrent foreground activity on the same machine. See
//! docs/gui-testing.md caveat 10 for the full investigation and confirmed
//! before/after results.
//!
//! This always runs whenever the `e2e-webdriver` feature is compiled in,
//! regardless of whether the window ends up headless or headed (see
//! `CRYPTOVOL_E2E_HEADED` in wdio.conf.ts) -- disabling App Nap is harmless
//! for a visible window (which is not a napping candidate anyway), so
//! there is no need to also thread that env var into this crate.

use objc2::rc::Retained;
use objc2::runtime::{NSObjectProtocol, ProtocolObject};
use objc2_foundation::{NSActivityOptions, NSProcessInfo, NSString};

/// Holds the process-wide activity assertion. Dropping it would let macOS
/// resume napping the process, so the caller must keep this alive for as
/// long as App Nap should stay disabled -- in practice, for the whole
/// program (see `run()` in `lib.rs`, which keeps this alive across the
/// blocking `tauri::App::run` call).
pub struct AppNapGuard(#[allow(dead_code)] Retained<ProtocolObject<dyn NSObjectProtocol>>);

/// Begins an `NSProcessInfo` activity that keeps the process from being
/// App Nap-throttled. `UserInitiated` communicates user-visible, latency-
/// sensitive work (matching what the WebDriver harness actually is from
/// the test runner's perspective, even though this app has no visible
/// window); `LatencyCritical` additionally discourages the timer/I-O
/// coalescing App Nap otherwise applies.
pub fn disable() -> AppNapGuard {
    let process_info = NSProcessInfo::processInfo();
    let reason = NSString::from_str("cryptovol-gui e2e-webdriver harness: avoid App Nap");
    let options = NSActivityOptions::UserInitiated | NSActivityOptions::LatencyCritical;
    let token = process_info.beginActivityWithOptions_reason(options, &reason);
    AppNapGuard(token)
}
