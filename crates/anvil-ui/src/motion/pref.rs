//! «Меньше движения»: системная настройка Windows «Показывать анимацию» и ручное «всегда меньше».
//!
//! Флаг — `SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION)`. Об изменении Windows сообщает окну
//! `WM_SETTINGCHANGE`, но оконную процедуру держит winit и наружу это сообщение не отдаёт, а
//! подменять её (subclass) ради одного флага — хрупко. Поэтому флаг перечитывается дёшево: при
//! получении окном фокуса (настройку меняют в другом окне — «Параметрах») и не чаще раза в
//! [`RECHECK`] на кадрах, которые окно и так рисует. Своих перерисовок модуль не просит: спрятанное
//! или спокойное окно не тратит ничего, а новый флаг увидит на первом же кадре.

use std::time::{Duration, Instant};

use eframe::egui;

/// Как часто перечитывать флаг на идущих кадрах.
pub const RECHECK: Duration = Duration::from_secs(3);

/// Включена ли анимация в Windows. Не спросить — включена.
#[cfg(windows)]
pub fn animations_enabled() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW};
    let mut on: i32 = 1;
    // SAFETY: для SPI_GETCLIENTAREAANIMATION `pvparam` — указатель на BOOL (i32) на стеке.
    let asked = unsafe { SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, (&mut on as *mut i32).cast(), 0) };
    asked == 0 || on != 0
}

/// Вне Windows просьбу о меньшем движении пока не читаем.
#[cfg(not(windows))]
pub fn animations_enabled() -> bool {
    true
}

/// Флаг «анимация включена» с дешёвым перечитыванием. Держит окно; [`MotionPref::enabled`] —
/// каждый кадр. Обычно им пользуется [`super::tick`] — программе самой его создавать не нужно.
#[derive(Debug, Clone)]
pub struct MotionPref {
    enabled: bool,
    checked_at: Instant,
    focused: bool,
}

impl MotionPref {
    pub fn new() -> MotionPref {
        MotionPref { enabled: animations_enabled(), checked_at: Instant::now(), focused: true }
    }

    /// Флаг на этот кадр.
    pub fn enabled(&mut self, ctx: &egui::Context) -> bool {
        let focused = ctx.input(|i| i.focused);
        let gained = focused && !self.focused;
        self.focused = focused;
        if recheck(gained, self.checked_at.elapsed()) {
            self.enabled = animations_enabled();
            self.checked_at = Instant::now();
        }
        self.enabled
    }
}

impl Default for MotionPref {
    fn default() -> Self {
        MotionPref::new()
    }
}

/// Перечитать ли флаг: окно только что получило фокус или давно не смотрели.
fn recheck(focus_gained: bool, since: Duration) -> bool {
    focus_gained || since >= RECHECK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rechecks_on_focus_and_now_and_then() {
        assert!(recheck(true, Duration::ZERO), "вернулись в окно — могли поменять в «Параметрах»");
        assert!(recheck(false, RECHECK));
        assert!(!recheck(false, Duration::from_millis(100)), "не на каждом кадре");
    }

    #[test]
    fn flag_is_readable() {
        // На машине разработчика — как в Windows; главное, что вызов не падает.
        let _ = animations_enabled();
        let ctx = egui::Context::default();
        let mut pref = MotionPref::new();
        assert_eq!(pref.enabled(&ctx), pref.enabled);
    }
}
