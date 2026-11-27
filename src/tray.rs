//! The menu-bar (macOS) / system-tray (Windows, Linux) icon: a click opens the System monitor panel, a right-click shows a small menu. Claybroom
//! keeps running there after its window is closed.

use eframe::egui;
use std::sync::mpsc::{channel, Receiver};

pub enum TrayMsg {
    /// Show or hide the panel; the icon's spot on screen in physical pixels (x, y, width, height).
    Toggle(Option<[f64; 4]>),
    Open,
    QuickClean,
    Quit,
}

pub struct Tray {
    #[cfg(not(target_os = "linux"))]
    _icon: tray_icon::TrayIcon,
    pub rx: Receiver<TrayMsg>,
}

/// The menu-bar picture: the app icon's cream broom as a black silhouette (macOS tints it to fit
/// the menu bar), or the coloured app icon on Windows and Linux.
fn icon_image() -> tray_icon::Icon {
    let size = 44;
    let mut px = crate::kit::app_icon(size);
    if cfg!(target_os = "macos") {
        // Keep only the cream broom, as opaque black on transparent.
        for p in px.chunks_mut(4) {
            let white = p[0].min(p[1]).min(p[2]) as u32;
            let a = (white.saturating_sub(120) * 255 / 135).min(255) as u8;
            p.copy_from_slice(&[0, 0, 0, if p[3] > 0 { a } else { 0 }]);
        }
    }
    tray_icon::Icon::from_rgba(px, size, size).expect("tray icon")
}

pub fn start(ctx: &egui::Context) -> Option<Tray> {
    let (tx, rx) = channel();
    // Linux tray icons live on a GTK thread of their own (and only show the menu); the menu and
    // icon must be made on that thread.
    #[cfg(target_os = "linux")]
    {
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            if gtk::init().is_ok() {
                let _icon = build(tx, ctx);
                gtk::main();
            }
        });
        Some(Tray { rx })
    }
    #[cfg(not(target_os = "linux"))]
    {
        Some(Tray { _icon: build(tx, ctx.clone())?, rx })
    }
}

fn build(tx: std::sync::mpsc::Sender<TrayMsg>, ctx: egui::Context) -> Option<tray_icon::TrayIcon> {
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};

    let open = MenuItem::new("Open Claybroom", true, None);
    let panel = MenuItem::new("System monitor", true, None);
    let quick = MenuItem::new("Quick Clean", true, None);
    let quit = MenuItem::new("Quit Claybroom", true, None);
    let ids = (open.id().clone(), panel.id().clone(), quick.id().clone(), quit.id().clone());
    let menu = Menu::new();
    menu.append_items(&[&open, &panel, &quick, &PredefinedMenuItem::separator(), &quit]).ok()?;

    let (t, c) = (tx.clone(), ctx.clone());
    MenuEvent::set_event_handler(Some(std::sync::Arc::new(move |e: MenuEvent| {
        let msg = if e.id == ids.0 {
            TrayMsg::Open
        } else if e.id == ids.1 {
            TrayMsg::Toggle(None)
        } else if e.id == ids.2 {
            TrayMsg::QuickClean
        } else if e.id == ids.3 {
            TrayMsg::Quit
        } else {
            return;
        };
        let _ = t.send(msg);
        c.request_repaint();
    })));
    let (t, c) = (tx, ctx);
    TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
        if let TrayIconEvent::Click { rect, button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
            let _ = t.send(TrayMsg::Toggle(Some([rect.position.x, rect.position.y, rect.size.width as f64, rect.size.height as f64])));
            c.request_repaint();
        }
    }));

    let builder = tray_icon::TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .with_tooltip("Claybroom");
    #[cfg(target_os = "macos")]
    let builder = builder.with_icon_templated(icon_image());
    #[cfg(not(target_os = "macos"))]
    let builder = builder.with_icon(icon_image());
    builder.build().ok()
}

/// Where the panel goes (logical points): under the icon when the bar is at the top (macOS,
/// most Linux desktops), above it when the bar is at the bottom (Windows).
pub fn panel_pos(icon: Option<[f64; 4]>, ppp: f32, panel: egui::Vec2, monitor: Option<egui::Vec2>) -> egui::Pos2 {
    let screen = monitor.unwrap_or(egui::vec2(1440.0, 900.0));
    let Some([x, y, w, h]) = icon else {
        return egui::pos2(screen.x - panel.x - 12.0, 32.0);
    };
    let (x, y, w, h) = (x as f32 / ppp, y as f32 / ppp, w as f32 / ppp, h as f32 / ppp);
    let left = (x + w / 2.0 - panel.x / 2.0).clamp(8.0, (screen.x - panel.x - 8.0).max(8.0));
    let top = if y > screen.y / 2.0 { y - panel.y - 6.0 } else { y + h + 6.0 };
    egui::pos2(left, top.max(0.0))
}

#[cfg(test)]
mod tests {
    #[test]
    fn panel_sits_under_a_top_bar_and_above_a_bottom_one() {
        use eframe::egui::vec2;
        let panel = vec2(368.0, 700.0);
        let screen = Some(vec2(1440.0, 900.0));
        // macOS: icon at the top, 2x screen.
        let p = super::panel_pos(Some([2400.0, 0.0, 44.0, 48.0]), 2.0, panel, screen);
        assert!(p.y >= 24.0 && p.y < 40.0 && (p.x + panel.x / 2.0 - 1211.0).abs() < 1.0);
        // Windows: icon at the bottom, 1x screen.
        let p = super::panel_pos(Some([1300.0, 860.0, 24.0, 40.0]), 1.0, panel, screen);
        assert!(p.y + panel.y <= 860.0 && p.x + panel.x <= 1440.0);
    }
}
