//! Binary entrypoint for the Crypto Volume Viewer desktop GUI.

// Prevents an additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    cryptovol_gui_lib::run();
}
