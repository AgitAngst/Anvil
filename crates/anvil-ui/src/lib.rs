//! Единый вид всех программ: темы, акценты, значки и виджеты на egui.
//!
//! ```ignore
//! anvil_ui::install(&cc.egui_ctx, anvil_ui::Accent::VIOLET, anvil_ui::ThemeChoice::System);
//! ```

pub mod appicon;
pub mod chrome;
pub mod family;
pub mod icons;
pub mod lang;
pub mod motion;
pub mod taskbar;
pub mod theme;
pub mod wallpaper;
pub mod widgets;

pub use chrome::{AppInfo, CommonSettings};
pub use family::Mark;
pub use icons::Icon;
pub use lang::{Lang, tr};
pub use theme::{Accent, Palette, ThemeChoice, choose, install, semibold, set_accent};
pub use widgets::{Kind, Tone};
