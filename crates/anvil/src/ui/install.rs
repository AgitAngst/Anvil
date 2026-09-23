//! Вкладка «Установка»: установленные копии бинарников проекта, их версии и откат.

use std::path::PathBuf;

use anvil_ui::widgets as w;
use anvil_ui::{Icon, Kind, Palette, Tone, semibold};
use anvil_update::Version;
use eframe::egui::{self, RichText, Ui};

use crate::app::App;
use crate::github::{Release, Remote};
use crate::i18n::{self, t};
use crate::installs::{self, Installed};
use crate::worker::Project;

/// Что попросили на вкладке.
pub enum Action {
    /// Собрать release и поставить.
    Local(String),
    /// Скачать выпуск с GitHub и поставить.
    Release(String, Box<Release>),
    Activate(String, String),
    Launch(String),
    Folder(PathBuf),
    Uninstall(String),
}

/// Выпуск на GitHub, который можно поставить для бинарника: самый новый с архивом по соглашению.
pub fn release_for<'a>(remote: Option<&'a Remote>, bin: &str, prerelease: bool) -> Option<(&'a Release, Version)> {
    remote?
        .releases
        .iter()
        .filter(|r| !r.draft && (prerelease || !r.prerelease))
        .filter_map(|r| Version::parse(&r.tag).map(|v| (r, v)))
        .filter(|(r, v)| r.assets.iter().any(|a| a.name == anvil_update::asset_name(bin, v)))
        .filter(|(r, _)| r.assets.iter().any(|a| a.name == "SHA256SUMS"))
        .max_by(|(_, a), (_, b)| a.cmp(b))
}

/// Установленная версия без хвоста локальной сборки: `0.1.0-3f89301` → `0.1.0`.
fn core(label: &str) -> Option<Version> {
    let v = Version::parse(label)?;
    Some(Version { pre: None, ..v })
}

/// Есть ли на GitHub версия новее установленной.
pub fn newer<'a>(installed: Option<&Installed>, release: Option<&'a (&'a Release, Version)>) -> Option<&'a Version> {
    let current = core(installed?.current.as_deref()?)?;
    let (_, version) = release?;
    (Version { pre: None, ..version.clone() } > current).then_some(version)
}

pub fn tab(app: &App, ui: &mut Ui, project: &Project, remote: Option<&Remote>) -> Vec<Action> {
    let mut actions = Vec::new();
    let Some(meta) = project.meta() else {
        w::card(ui, |ui| w::empty_state(ui, Icon::Package, t("Читаю Cargo.toml…"), ""));
        return actions;
    };
    if meta.bins.is_empty() {
        w::card(ui, |ui| w::empty_state(ui, Icon::Package, t("Ставить нечего"), t("Библиотека: запускать нечего.")));
        return actions;
    }
    let p = Palette::of(ui);
    let git = project.git();
    let local = installs::local_label(
        meta.version.as_deref().unwrap_or("0.0.0"),
        git.and_then(|g| g.commits.first()).map(|c| c.hash.as_str()),
        git.is_some_and(|g| g.dirty()),
    );
    let prerelease = app.config.common.prerelease;

    for bin in &meta.bins {
        let installed = app.installs.get(&bin.name).and_then(Option::as_ref);
        let release = release_for(remote, &bin.name, prerelease);
        let root = installs::root(&bin.name);
        let running: Vec<u32> = app
            .running(&bin.name)
            .iter()
            .filter(|r| r.path.as_ref().is_some_and(|p| installs::inside(p, &root)))
            .map(|r| r.pid)
            .collect();
        w::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(installs::display_name(&bin.name)).font(semibold(15.0)).color(p.text));
                w::mono(ui, &bin.name, None);
                match installed.and_then(|i| i.current.as_deref()) {
                    Some(current) => {
                        w::badge(ui, current, Tone::Accent).on_hover_text(t("Активная версия"));
                    }
                    None => {
                        w::badge(ui, t("не установлена"), Tone::Neutral);
                    }
                }
                if let Some(version) = newer(installed, release.as_ref()) {
                    w::badge(ui, &format!("{} v{version}", t("есть")), Tone::Success);
                }
                if !running.is_empty() {
                    w::note(ui, format!("· {} (PID {})", t("запущена из установки"), running[0]));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if installed.is_some() {
                        if w::button(ui, Kind::Danger, Some(Icon::Trash), t("Удалить…")).clicked() {
                            actions.push(Action::Uninstall(bin.name.clone()));
                        }
                        if w::icon_button(ui, Icon::Folder, t("Открыть папку установки")).clicked()
                        {
                            actions.push(Action::Folder(root.clone()));
                        }
                        if w::button(ui, Kind::Secondary, Some(Icon::Play), t("Запустить")).clicked() {
                            actions.push(Action::Launch(bin.name.clone()));
                        }
                    }
                });
            });
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                let hint = format!("cargo build --release --bin {} → versions\\{local}", bin.name);
                let text = format!("{} · {local}", t("Из сборки"));
                if w::button(ui, Kind::Secondary, Some(Icon::Hammer), &text).on_hover_text(hint).clicked() {
                    actions.push(Action::Local(bin.name.clone()));
                }
                match &release {
                    Some((release, version)) => {
                        let text = format!("{} · v{version}", t("С GitHub"));
                        let asset = anvil_update::asset_name(&bin.name, version);
                        if w::button(ui, Kind::Secondary, Some(Icon::Download), &text).on_hover_text(asset).clicked() {
                            actions.push(Action::Release(bin.name.clone(), Box::new((*release).clone())));
                        }
                    }
                    None => {
                        ui.add_enabled_ui(false, |ui| {
                            w::button(ui, Kind::Secondary, Some(Icon::Download), t("С GitHub"))
                        })
                        .response
                        .on_disabled_hover_text(t(
                            "На GitHub нет выпуска по соглашению: архива <бинарник>-X.Y.Z-windows-x64.zip и SHA256SUMS",
                        ));
                    }
                }
            });

            if let Some(installed) = installed
                && !installed.versions.is_empty()
            {
                ui.add_space(10.0);
                w::section_label(ui, t("Версии"));
                ui.add_space(2.0);
                for (label, when) in &installed.versions {
                    let current = installed.current.as_deref() == Some(label.as_str());
                    ui.horizontal(|ui| {
                        ui.set_min_height(28.0);
                        w::dot(ui, if current { Tone::Accent } else { Tone::Neutral });
                        w::mono(ui, label, Some(p.text));
                        w::note(ui, format!("{} {}", t("поставлена"), i18n::ago(*when)));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if current {
                                w::badge(ui, t("текущая"), Tone::Accent);
                            } else if w::button(ui, Kind::Ghost, Some(Icon::ArrowUp), t("Сделать текущей")).clicked()
                            {
                                actions.push(Action::Activate(bin.name.clone(), label.clone()));
                            }
                        });
                    });
                }
            }
        });
        ui.add_space(10.0);
    }
    w::note(
        ui,
        t(
            "Установка — в папке Programs внутри %LOCALAPPDATA%, ярлык — в «Пуске». Данные программ живут в %APPDATA%, их Anvil не трогает.",
        ),
    );
    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str) -> Release {
        Release {
            tag: tag.into(),
            name: tag.into(),
            url: String::new(),
            published: 0,
            prerelease: false,
            draft: false,
            assets: Vec::new(),
        }
    }

    fn installed(current: &str) -> Installed {
        Installed { root: PathBuf::new(), current: Some(current.into()), versions: Vec::new() }
    }

    #[test]
    fn newer_compares_release_with_installed_core_version() {
        let r = release("v0.1.0");
        let pair = (&r, Version::parse("0.1.0").unwrap());
        assert!(newer(Some(&installed("0.0.9")), Some(&pair)).is_some());
        // Локальная сборка той же версии — не старше выпуска: её собрали из коммитов после тега.
        assert!(newer(Some(&installed("0.1.0-3f89301-dirty")), Some(&pair)).is_none());
        assert!(newer(Some(&installed("0.1.0")), Some(&pair)).is_none());
        assert!(newer(None, Some(&pair)).is_none());
    }
}
