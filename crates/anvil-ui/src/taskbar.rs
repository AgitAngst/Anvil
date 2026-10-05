//! Большой значок окна — тот, что Windows кладёт на панель задач и в Alt+Tab.
//!
//! `ViewportCommand::Icon` egui отдаёт winit `set_window_icon`, а он меняет только малый значок (`ICON_SMALL`:
//! заголовок окна). Поэтому анимированный значок программы (`motion::machine`) двигается в заголовке, но не на
//! панели задач и не в Alt+Tab. `TaskbarIcon` ставит большой значок сам: `WM_SETICON` с `ICON_BIG`. Кадра нет
//! (покой) — большой значок снова из exe программы. На других системах всё это — пустые вызовы.
//!
//! ```ignore
//! // при создании: HWND окна — один раз
//! let mut taskbar = anvil_ui::taskbar::TaskbarIcon::new(anvil_ui::taskbar::window_handle(cc));
//! // рядом с `ViewportCommand::Icon`:
//! match show.step(&scene, now, still) {
//!     Show::Frame(f) => { let icon = render(&f); taskbar.set(Some(&icon)); /* и ViewportCommand::Icon */ }
//!     Show::File => { taskbar.set(None); /* и ViewportCommand::Icon(файл) */ }
//!     Show::Keep => {}
//! }
//! ```
//!
//! Грабли Windows 10: кнопка закреплённой программы, а также сгруппированная с чужим окном (например консоли,
//! откуда программу запустили), показывает значок ярлыка или того окна, а не наш. В Alt+Tab значок наш всегда.

/// HWND окна программы (`None` вне Windows). Вызывать при создании: `cc` — из `eframe::run_native`.
#[cfg(windows)]
pub fn window_handle(cc: &eframe::CreationContext<'_>) -> Option<isize> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match cc.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
        _ => None,
    }
}

/// HWND окна программы (`None` вне Windows). Вызывать при создании: `cc` — из `eframe::run_native`.
#[cfg(not(windows))]
pub fn window_handle(_cc: &eframe::CreationContext<'_>) -> Option<isize> {
    None
}

/// Большой значок окна: свой `HICON` из кадра или значок exe.
#[cfg(windows)]
pub struct TaskbarIcon {
    hwnd: isize,
    /// Наш `HICON` в окне; 0 — свой не стоит, берётся значок exe.
    handle: isize,
}

#[cfg(windows)]
impl TaskbarIcon {
    /// `hwnd` — из [`window_handle`]; `None` — значок не трогаем.
    pub fn new(hwnd: Option<isize>) -> Self {
        Self { hwnd: hwnd.unwrap_or(0), handle: 0 }
    }

    /// `Some(кадр)` — поставить его большим значком окна; `None` — вернуть значок exe. Повторный `None` ничего
    /// не делает. Звать из потока окна (`update`/`logic`).
    pub fn set(&mut self, icon: Option<&eframe::egui::IconData>) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{CreateIcon, DestroyIcon, ICON_BIG, SendMessageW, WM_SETICON};
        if self.hwnd == 0 || (icon.is_none() && self.handle == 0) {
            return;
        }
        let new = icon.map_or(0, |icon| {
            // RGBA → BGRA; маска «И» пустая: прозрачность — в альфа-канале.
            let bgra: Vec<u8> = icon.rgba.as_chunks::<4>().0.iter().flat_map(|&[r, g, b, a]| [b, g, r, a]).collect();
            let mask = vec![0u8; (icon.width * icon.height) as usize];
            // SAFETY: буферы живут до конца вызова, `CreateIcon` их копирует.
            unsafe {
                CreateIcon(std::ptr::null_mut(), icon.width as i32, icon.height as i32, 1, 32, mask.as_ptr(), bgra.as_ptr()) as isize
            }
        });
        if icon.is_some() && new == 0 {
            return;
        }
        // SAFETY: `hwnd` — живое окно программы, сообщение уходит из его же потока; прежний значок
        // уничтожаем после того, как окно получило новый (или значок exe).
        unsafe {
            SendMessageW(self.hwnd as _, WM_SETICON, ICON_BIG as usize, new);
            if self.handle != 0 {
                DestroyIcon(self.handle as _);
            }
        }
        self.handle = new;
    }
}

/// Большой значок окна — вне Windows ничего не делает.
#[cfg(not(windows))]
pub struct TaskbarIcon;

#[cfg(not(windows))]
impl TaskbarIcon {
    pub fn new(_hwnd: Option<isize>) -> Self {
        Self
    }

    pub fn set(&mut self, _icon: Option<&eframe::egui::IconData>) {}
}
