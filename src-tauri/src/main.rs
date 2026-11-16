#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod hud;
mod instance;
mod metrics;
mod overlay;
mod plugins;
mod scheduler;
mod sink;
mod source;
mod taskbar;
mod tray;
mod updater;
mod windows;

fn main() {
    if !instance::acquire() {
        // 已有实例：通知它把面板显示出来，然后退出
        std::process::exit(0);
    }
    app::run();
}
