//! Глобальное сочетание быстрого запуска (`Ctrl+Alt+Space`): `RegisterHotKey` в своём потоке с
//! очередью сообщений. Сочетание занято другой программой — это видимое состояние, а не тишина.

use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};

/// Что с сочетанием сейчас.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// Ещё регистрируется.
    Pending,
    /// Работает.
    #[cfg_attr(not(windows), allow(dead_code))]
    Ready,
    /// Занято другой программой — надо выбрать другое.
    Busy,
    /// Не разобрать (`Ctrl+Alt+?`) или не Windows.
    Invalid,
}

/// Сочетание: модификаторы и клавиша, как их понимает `RegisterHotKey`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Combo {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    /// Код виртуальной клавиши (`VK_SPACE` = 0x20, буквы и цифры — их ASCII в верхнем регистре).
    pub vk: u32,
}

impl Combo {
    /// «Ctrl+Alt+Space», «Win+Shift+K», «Alt+F2». Нужен хотя бы один модификатор.
    pub fn parse(text: &str) -> Option<Combo> {
        let mut combo = Combo { ctrl: false, alt: false, shift: false, win: false, vk: 0 };
        for part in text.split('+').map(str::trim) {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => combo.ctrl = true,
                "alt" => combo.alt = true,
                "shift" => combo.shift = true,
                "win" => combo.win = true,
                key if combo.vk == 0 => combo.vk = vk(key)?,
                _ => return None,
            }
        }
        (combo.vk != 0 && (combo.ctrl || combo.alt || combo.win)).then_some(combo)
    }

    /// Клавиши для подсказок: `["Ctrl", "Alt", "Space"]`.
    pub fn keys(&self) -> Vec<String> {
        let mut keys = Vec::new();
        for (on, name) in [(self.ctrl, "Ctrl"), (self.alt, "Alt"), (self.shift, "Shift"), (self.win, "Win")] {
            if on {
                keys.push(name.to_owned());
            }
        }
        keys.push(key_name(self.vk));
        keys
    }
}

/// Код клавиши по имени: буква, цифра, `Space`, `F1`…`F24`.
fn vk(key: &str) -> Option<u32> {
    let key = key.to_ascii_uppercase();
    match key.as_str() {
        "SPACE" => Some(0x20),
        k if k.len() == 1 && k.chars().all(|c| c.is_ascii_alphanumeric()) => Some(u32::from(k.as_bytes()[0])),
        k if k.starts_with('F') => k[1..].parse::<u32>().ok().filter(|n| (1..=24).contains(n)).map(|n| 0x6F + n),
        _ => None,
    }
}

fn key_name(vk: u32) -> String {
    match vk {
        0x20 => "Space".to_owned(),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        v => char::from_u32(v).map(String::from).unwrap_or_default(),
    }
}

/// Поток сочетания: нажатия приходят в `pressed`, состояние — в `state()`.
pub struct Hotkey {
    pub pressed: Receiver<()>,
    state: Arc<Mutex<State>>,
    combo: Option<Combo>,
}

impl Hotkey {
    /// Запустить поток. `wake` зовётся после каждого нажатия — разбудить окно (оно может быть
    /// спрятано в трей).
    pub fn spawn(combo: Option<Combo>, wake: impl Fn() + Send + 'static) -> Hotkey {
        let (tx, pressed) = std::sync::mpsc::channel();
        let state = Arc::new(Mutex::new(if combo.is_some() { State::Pending } else { State::Invalid }));
        #[cfg(windows)]
        {
            let s = state.clone();
            let _ = std::thread::Builder::new()
                .name("anvil-hotkey".into())
                .spawn(move || imp::run(tx, s, combo, Box::new(wake)));
            Hotkey { pressed, state, combo }
        }
        #[cfg(not(windows))]
        {
            let _ = (tx, wake);
            *state.lock().unwrap_or_else(|e| e.into_inner()) = State::Invalid;
            Hotkey { pressed, state, combo }
        }
    }

    pub fn state(&self) -> State {
        self.state.lock().map(|s| s.clone()).unwrap_or(State::Invalid)
    }

    pub fn combo(&self) -> Option<Combo> {
        self.combo
    }
}

/// Отправка нажатия: канал живёт столько же, сколько окно.
#[cfg(windows)]
type Pressed = std::sync::mpsc::Sender<()>;

#[cfg(windows)]
mod imp {
    use super::{Combo, Pressed, State};
    use std::sync::{Arc, Mutex};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey, UnregisterHotKey,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, WM_HOTKEY};

    const ID: i32 = 1;

    pub fn run(tx: Pressed, state: Arc<Mutex<State>>, combo: Option<Combo>, wake: Box<dyn Fn() + Send>) {
        // SAFETY: у потока своя очередь сообщений; регистрация и снятие — в этом же потоке.
        unsafe {
            let register = |state: &Arc<Mutex<State>>| {
                let result = match combo {
                    Some(c) => {
                        let mut mods = MOD_NOREPEAT;
                        for (on, flag) in
                            [(c.ctrl, MOD_CONTROL), (c.alt, MOD_ALT), (c.shift, MOD_SHIFT), (c.win, MOD_WIN)]
                        {
                            if on {
                                mods |= flag;
                            }
                        }
                        if RegisterHotKey(std::ptr::null_mut(), ID, mods, c.vk) != 0 {
                            State::Ready
                        } else {
                            State::Busy
                        }
                    }
                    None => State::Invalid,
                };
                if let Ok(mut s) = state.lock() {
                    *s = result;
                }
            };
            register(&state);
            wake();
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                if msg.message == WM_HOTKEY && msg.wParam == ID as usize {
                    if tx.send(()).is_err() {
                        break;
                    }
                    wake();
                }
            }
            UnregisterHotKey(std::ptr::null_mut(), ID);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combos_parse_and_print() {
        let c = Combo::parse("Ctrl+Alt+Space").unwrap();
        assert!(c.ctrl && c.alt && !c.shift && !c.win);
        assert_eq!(c.vk, 0x20);
        assert_eq!(c.keys(), ["Ctrl", "Alt", "Space"]);
        assert_eq!(Combo::parse("win + shift + k").unwrap().keys(), ["Shift", "Win", "K"]);
        assert_eq!(Combo::parse("Alt+F2").unwrap().vk, 0x71);
        // Без модификатора сочетание съело бы клавишу во всех программах.
        assert_eq!(Combo::parse("Space"), None);
        assert_eq!(Combo::parse("Ctrl+Alt+Space+K"), None);
        assert_eq!(Combo::parse("Ctrl+Enter"), None);
    }
}
