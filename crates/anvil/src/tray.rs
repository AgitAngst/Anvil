//! Значок в трее (§7.9): меню «Запущено» и «Закреплено», «Быстрый запуск», «Открыть Anvil»,
//! «Выход»; левый щелчок — быстрый запуск, двойной — окно. Красная точка на значке — что-то упало
//! или не удалось, пока окна не видно.
//!
//! Значок живёт в главном потоке (там же, где окно): его сообщения разбирает цикл окна, а
//! обработчики отправляют просьбы в канал и будят окно.

use std::sync::mpsc::Sender;

use eframe::egui;

/// Что попросили в трее.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Show,
    Quick,
    Quit,
    /// Главное действие предмета (ключ Пульта): к окну, запустить.
    Main(String),
    /// Страница предмета.
    Page(String),
    /// Остановить предмет (своё из кода — сразу, остальное — с вопросом в окне).
    Stop(String),
}

#[cfg_attr(not(windows), allow(dead_code))]
impl Request {
    fn id(&self) -> String {
        match self {
            Request::Show => "show".to_owned(),
            Request::Quick => "quick".to_owned(),
            Request::Quit => "quit".to_owned(),
            Request::Main(key) => format!("main:{key}"),
            Request::Page(key) => format!("page:{key}"),
            Request::Stop(key) => format!("stop:{key}"),
        }
    }

    /// Обратно из id пункта меню.
    pub fn parse(id: &str) -> Option<Request> {
        match id {
            "show" => return Some(Request::Show),
            "quick" => return Some(Request::Quick),
            "quit" => return Some(Request::Quit),
            _ => {}
        }
        let (kind, key) = id.split_once(':')?;
        let key = key.to_owned();
        match kind {
            "main" => Some(Request::Main(key)),
            "page" => Some(Request::Page(key)),
            "stop" => Some(Request::Stop(key)),
            _ => None,
        }
    }
}

/// Сколько Windows ждёт второго щелчка, чтобы счесть его двойным.
#[cfg(windows)]
pub fn double_click() -> std::time::Duration {
    // SAFETY: вызов без аргументов.
    let ms = unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime() };
    std::time::Duration::from_millis(u64::from(ms.clamp(200, 900)))
}

#[cfg(not(windows))]
pub fn double_click() -> std::time::Duration {
    std::time::Duration::from_millis(500)
}

/// Открыто ли сейчас всплывающее меню (меню трея): пересобрать его — значит закрыть под курсором.
#[cfg(windows)]
fn menu_open() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GUI_POPUPMENUMODE, GUITHREADINFO, GetGUIThreadInfo};
    // SAFETY: структура на стеке, размер задан перед вызовом; 0 — поток, который сейчас впереди.
    unsafe {
        let mut info: GUITHREADINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
        GetGUIThreadInfo(0, &mut info) != 0 && info.flags & GUI_POPUPMENUMODE != 0
    }
}

/// Строка меню трея.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// Заголовок группы: серый, не нажимается.
    Header(String),
    /// Пункт; клавиши справа (`Alt+1`) — только подсказка.
    Item(String, Option<String>, Request),
    /// Подменю: подпись и пункты.
    Sub(String, Vec<(String, Request)>),
    Separator,
}

#[cfg(windows)]
pub struct Tray {
    icon: tray_icon::TrayIcon,
    plain: tray_icon::Icon,
    dot: tray_icon::Icon,
    alert: bool,
    menu: Vec<Entry>,
}

#[cfg(windows)]
impl Tray {
    /// Значок с меню; просьбы — в `tx`, после каждой окно будится.
    pub fn new(tx: Sender<Request>, ctx: egui::Context, menu: Vec<Entry>) -> Result<Tray, String> {
        use tray_icon::menu::MenuEvent;
        use tray_icon::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

        const SIZE: u32 = 32;
        let base = anvil_ui::appicon::rgba(crate::app::ACCENT, anvil_ui::Icon::Hammer, SIZE);
        let plain = tray_icon::Icon::from_rgba(base.clone(), SIZE, SIZE).map_err(|e| e.to_string())?;
        let dot = tray_icon::Icon::from_rgba(with_dot(base, SIZE), SIZE, SIZE).map_err(|e| e.to_string())?;
        let icon = TrayIconBuilder::new()
            .with_icon(plain.clone())
            .with_tooltip("Anvil")
            .with_menu(Box::new(build(&menu)?))
            // Левый щелчок — быстрый запуск, меню — по правому.
            .with_menu_on_left_click(false)
            .build()
            .map_err(|e| e.to_string())?;

        // Обработчики ставятся один раз на процесс (второй вызов молча не действует): пункты
        // различаются по id, поэтому меню можно пересобирать.
        let (menu_tx, menu_ctx) = (tx.clone(), ctx.clone());
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if let Some(request) = Request::parse(&event.id.0) {
                let _ = menu_tx.send(request);
                menu_ctx.request_repaint_of(egui::ViewportId::ROOT);
            }
        }));
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            let request = match event {
                TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } => {
                    Request::Quick
                }
                TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } => Request::Show,
                _ => return,
            };
            let _ = tx.send(request);
            ctx.request_repaint_of(egui::ViewportId::ROOT);
        }));
        Ok(Tray { icon, plain, dot, alert: false, menu })
    }

    /// Красная точка на значке.
    pub fn set_alert(&mut self, on: bool) {
        if on != self.alert {
            self.alert = on;
            let icon = if on { self.dot.clone() } else { self.plain.clone() };
            let _ = self.icon.set_icon(Some(icon));
        }
    }

    /// Пересобрать меню, если оно изменилось.
    pub fn set_menu(&mut self, entries: Vec<Entry>) {
        // Меню открыто — не трогать: пересобранное закрыло бы его под курсором. Через секунду снова.
        if entries == self.menu || menu_open() {
            return;
        }
        if let Ok(menu) = build(&entries) {
            self.icon.set_menu(Some(Box::new(menu)));
            self.menu = entries;
        }
    }
}

#[cfg(windows)]
fn build(entries: &[Entry]) -> Result<tray_icon::menu::Menu, String> {
    use tray_icon::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};

    let err = |e: tray_icon::menu::Error| e.to_string();
    let menu = Menu::new();
    for (n, entry) in entries.iter().enumerate() {
        match entry {
            Entry::Header(text) => {
                menu.append(&MenuItem::with_id(format!("header:{n}"), text, false, None)).map_err(err)?;
            }
            Entry::Item(text, keys, request) => {
                // Клавиши — после табуляции: так Windows ставит их справа, как у сочетаний.
                let text = match keys {
                    Some(keys) => format!("{text}\t{keys}"),
                    None => text.clone(),
                };
                menu.append(&MenuItem::with_id(request.id(), text, true, None)).map_err(err)?;
            }
            Entry::Sub(text, items) => {
                let sub = Submenu::with_id(format!("sub:{n}"), text, true);
                for (label, request) in items {
                    sub.append(&MenuItem::with_id(request.id(), label, true, None)).map_err(err)?;
                }
                menu.append(&sub).map_err(err)?;
            }
            Entry::Separator => menu.append(&PredefinedMenuItem::separator()).map_err(err)?,
        }
    }
    Ok(menu)
}

/// Красная точка Ø6 из 16 в правом нижнем углу, с прозрачным кольцом (§7.9): видна на любой
/// панели задач.
#[cfg(windows)]
fn with_dot(mut px: Vec<u8>, size: u32) -> Vec<u8> {
    let [dr, dg, db] = [0xF2_u8, 0x67, 0x6B];
    let s = size as f32;
    let (r, ring) = (s * 3.0 / 16.0, s / 16.0);
    let c = s - r - ring;
    for y in 0..size {
        for x in 0..size {
            let d = ((x as f32 + 0.5 - c).powi(2) + (y as f32 + 0.5 - c).powi(2)).sqrt();
            let i = ((y * size + x) * 4) as usize;
            let hole = (r + ring + 0.5 - d).clamp(0.0, 1.0);
            let a = f32::from(px[i + 3]) / 255.0 * (1.0 - hole);
            let cover = (r + 0.5 - d).clamp(0.0, 1.0);
            let out = cover + a * (1.0 - cover);
            if out > 0.0 {
                for (k, dc) in [dr, dg, db].into_iter().enumerate() {
                    let v = (f32::from(dc) * cover + f32::from(px[i + k]) * a * (1.0 - cover)) / out;
                    px[i + k] = v.round() as u8;
                }
            }
            px[i + 3] = (out * 255.0).round() as u8;
        }
    }
    px
}

/// Вне Windows трея нет: крестик закрывает Anvil.
#[cfg(not(windows))]
pub struct Tray;

#[cfg(not(windows))]
impl Tray {
    pub fn new(_tx: Sender<Request>, _ctx: egui::Context, _menu: Vec<Entry>) -> Result<Tray, String> {
        Err("no tray".to_owned())
    }

    pub fn set_alert(&mut self, _on: bool) {}

    pub fn set_menu(&mut self, _entries: Vec<Entry>) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_round_trip_through_menu_ids() {
        for request in [
            Request::Show,
            Request::Quick,
            Request::Quit,
            Request::Main(r"d:\dev\amber|amber-desktop".into()),
            Request::Page("k|x".into()),
            Request::Stop("k|y".into()),
        ] {
            assert_eq!(Request::parse(&request.id()), Some(request));
        }
        assert_eq!(Request::parse("header:3"), None);
    }
}
