use crate::{
    Page, Report, State,
    tools::{Action, Entry, Output, TOOLS, Tool},
};
use epic_disk_core::{clean::DeleteMethod, explorer, rules::Risk};
use rune::{
    Color, Cursor, Cx, El, Icon, ScrollSpec, col, el, icon, row, text, text_input, tok::TextStyle,
};
use std::{borrow::Cow, cell::Cell, path::Path, sync::atomic::Ordering::Relaxed};

const BG: &str = "#0c111b";
const SIDE: &str = "#101622";
const PANEL: &str = "#141d2b";
const EDGE: &str = "#273549";
const INK: &str = "#edf4fc";
const SOFT: &str = "#a2b2c7";
const MUTED: &str = "#8092aa";
const MINT: &str = "#64e8c7";
const AMBER: &str = "#ffcf87";
const WHITE: &str = "#fff8ee";
const BLACK: &str = "#151514";
const RED: &str = "#cb3c27";
const RED_TYPE: &str = "#bf3522";
const GREEN: &str = "#427455";
const SIDEMUTED: &str = "#bdb8ad";
const SIDEEDGE: &str = "#393732";
thread_local! { static LIGHT: Cell<bool> = const { Cell::new(false) }; }

macro_rules! glyph {
    ($id:ident, $path:literal) => {
        static $id: Icon = Icon {
            name: stringify!($id),
            view: 24.0,
            body: concat!(
                "<path fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.65\" stroke-linecap=\"round\" stroke-linejoin=\"round\" d=\"",
                $path,
                "\"/>"
            ),
        };
    };
}
glyph!(LOGO, "M4 20 20 4v16z");
glyph!(
    HOME,
    "M3.5 3.5h7v7h-7z M13.5 3.5h7v7h-7z M3.5 13.5h7v7h-7z M13.5 13.5h7v7h-7z"
);
glyph!(
    SPACE,
    "M4 14.5h16v5H4z M4 14.5l2.5-9h11l2.5 9 M16.5 17h.1 M13 17h.1"
);
glyph!(DUP, "M8 8h12v12H8z M5 16H4V4h12v1");
glyph!(
    CLEAN,
    "M4 7h16 M9 7V4h6v3 M6 7l1 13h10l1-13 M10 11v5 M14 11v5"
);
glyph!(HISTORY, "M4.5 8a8.5 8.5 0 1 1-.5 7 M4 4v5h5 M12 8v4l3 2");
glyph!(FOLDER, "M3 7V5h6l2 2h10v12H3z");
glyph!(ARROW, "M4 12h15 M13 6l6 6-6 6");
glyph!(
    SHIELD,
    "M12 3l8 3v5c0 5-3 8-8 10-5-2-8-5-8-10V6z M9 12l2 2 4-4"
);
glyph!(
    SEARCH,
    "M16.5 10.5a6 6 0 1 1-12 0 6 6 0 0 1 12 0z M15 15l5 5"
);
glyph!(FILE, "M6 3.5h8l4 4v13H6z M13.5 3.5v5H18 M9 13h6 M9 16h5");
glyph!(CHECK, "M5 12l4 4L19 6");
glyph!(COPY, "M8 8h12v12H8z M4 16V4h12");

fn c(value: &str) -> Color {
    let value = if LIGHT.get() {
        match value {
            BG => "#f6f4ed",
            SIDE => BLACK,
            PANEL => "#f6f4ed",
            EDGE => "#d3cec2",
            INK => BLACK,
            SOFT => "#4d4a43",
            MUTED => "#656157",
            AMBER => "#865608",
            MINT => RED_TYPE,
            "#1a2638" | "#101e2a" | "#101c2a" | "#152330" => "#ede9df",
            "#33465e" | "#263d49" | "#203b48" | "#2c555c" => "#8e8779",
            "#1b3540" | "#1c3540" => "#f3e4da",
            "#203748" => "#e9e4d9",
            "#9ebdff" | "#bdb2ff" => "#6a554c",
            "#243043" | "#4c607a" => "#d8d3c8",
            "#ff9ca6" => "#a92436",
            other => other,
        }
    } else {
        match value {
            BG => "#201f1c",
            SIDE => BLACK,
            PANEL => "#25231f",
            EDGE => "#48443b",
            INK => "#f6f3ea",
            SOFT => "#d1cbbf",
            MUTED => "#b6afa1",
            MINT | RED_TYPE => "#ff8067",
            GREEN => "#9ac9a7",
            "#1a2638" | "#203748" | "#101e2a" | "#101c2a" => "#302d26",
            "#33465e" | "#263d49" | "#203b48" | "#2c555c" => "#746d60",
            "#1b3540" | "#1c3540" => "#482a23",
            other => other,
        }
    };
    rune::color::hex(value).expect("valid static color")
}

fn readable_path(path: &Path) -> Cow<'_, str> {
    let value = path.to_string_lossy();
    if let Some(ordinary) = value.strip_prefix(r"\\?\") {
        return Cow::Owned(ordinary.to_owned());
    }
    value
}

fn label(value: impl Into<String>, size: f32, weight: f32, color: &str) -> El {
    text(value.into())
        .font(TextStyle::new(size, weight))
        .color(c(color))
}

fn overline(value: &str, color: &str) -> El {
    text(value)
        .font(TextStyle::new(10.0, 680.0).tracking(1.55))
        .color(c(color))
}

fn pic(glyph: &'static Icon, size: f32, color: &str) -> El {
    icon(glyph, size).color(c(color)).a11y_hidden()
}

fn spacer() -> El {
    el().grow(1.0).min_w(0.0).min_h(0.0)
}

fn card() -> El {
    col().fill(c(PANEL)).border(1.0, c(EDGE))
}

fn heavy_rule() -> El {
    el().h(2.0).w_full().fill(c(INK)).a11y_hidden()
}

fn display(value: &str, size: f32, color: &str) -> El {
    text(value)
        .font(
            TextStyle::new(size, 900.0)
                .tracking(-size * 0.068)
                .lh(size * 0.97),
        )
        .color(c(color))
}

fn rule() -> El {
    el().h(1.0).w_full().fill(c(EDGE)).a11y_hidden()
}

fn badge(value: &str, color: &str) -> El {
    row()
        .center_y()
        .h(26.0)
        .px(9.0)
        .fill(c(color).alpha(0.10))
        .child(overline(value, color))
}

#[derive(Clone, Copy)]
enum Button {
    Primary,
    Quiet,
}

fn button(
    key: &str,
    name: &str,
    glyph: Option<&'static Icon>,
    style: Button,
    active: bool,
    action: impl Fn(&mut State) + 'static,
) -> El {
    let (bg, fg, border) = match style {
        Button::Primary => (RED, WHITE, RED),
        Button::Quiet => (BG, INK, "#33465e"),
    };
    let mut node = row()
        .key(key)
        .role("button")
        .label(name)
        .disabled(!active)
        .h(40.0)
        .px(13.0)
        .gap(8.0)
        .center_y()
        .fill(c(bg))
        .border(1.0, c(border))
        .opacity(if active { 1.0 } else { 0.45 });
    if active {
        node = node
            .focusable(true)
            .focus_ring(2.0, 3.0, c(MINT))
            .cursor(Cursor::Pointer)
            .hover(Color::WHITE.alpha(0.12))
            .press(Color::BLACK.alpha(0.15))
            .press_scale(0.98)
            .on_press(action);
    }
    if let Some(glyph) = glyph {
        node = node.child(pic(glyph, 16.0, fg));
    }
    node.child(label(name, 13.0, 650.0, fg).a11y_hidden())
}

const NAV: [(Page, &str, &Icon); 6] = [
    (Page::Home, "Vue d’ensemble", &HOME),
    (Page::Space, "Espace disque", &SPACE),
    (Page::Duplicates, "Doublons", &DUP),
    (Page::Cleanup, "Nettoyage", &CLEAN),
    (Page::Tools, "Outils avancés", &SEARCH),
    (Page::History, "Historique", &HISTORY),
];

fn sidebar(s: &State) -> El {
    let logo = row()
        .gap(8.0)
        .center_y()
        .child(
            el().size(26.0, 26.0)
                .center()
                .fill(c(RED))
                .child(pic(&LOGO, 20.0, WHITE)),
        )
        .child(display("epicCleaner.", 22.0, WHITE));
    let mut nav = col()
        .gap(0.0)
        .child(overline("NAVIGATION / 01", SIDEMUTED).ml(8.0).mb(13.0));
    for (page, title, glyph) in NAV {
        let route = if s.page != Page::Tools {
            s.page
        } else {
            match s.tool {
                Tool::Map => Page::Space,
                Tool::Duplicates => Page::Duplicates,
                Tool::Caches => Page::Cleanup,
                Tool::Settings => Page::Home,
                _ => Page::Tools,
            }
        };
        let selected = page == route && !(s.page == Page::Tools && s.tool == Tool::Settings);
        let mut entry = row()
            .key(title)
            .h(42.0)
            .px(10.0)
            .gap(10.0)
            .center_y()
            .fill(if selected { c(RED) } else { Color::TRANSPARENT })
            .role("button")
            .label(title)
            .disabled(s.job.is_some());
        if s.job.is_none() {
            entry = entry
                .focusable(true)
                .focus_ring(2.0, 2.0, c(MINT))
                .cursor(Cursor::Pointer)
                .hover(Color::WHITE.alpha(0.07))
                .on_press(move |s: &mut State| match page {
                    Page::Space => s.tool_select(Tool::Map),
                    Page::Duplicates => s.tool_select(Tool::Duplicates),
                    Page::Cleanup => s.tool_select(Tool::Caches),
                    Page::Tools if s.page == Page::Tools => {
                        s.map_catalog = !s.map_catalog;
                    }
                    Page::Tools => {
                        s.navigate(page);
                        s.map_catalog = true;
                    }
                    _ => s.navigate(page),
                });
        }
        nav = nav
            .child(
                entry
                    .child(pic(glyph, 15.0, if selected { WHITE } else { SIDEMUTED }))
                    .child(
                        label(
                            title,
                            13.0,
                            if selected { 670.0 } else { 520.0 },
                            if selected { WHITE } else { SIDEMUTED },
                        )
                        .a11y_hidden(),
                    ),
            )
            .child(el().h(1.0).w_full().fill(c(SIDEEDGE)));
    }
    let settings = row()
        .key("nav-settings")
        .h(42.0)
        .px(10.0)
        .gap(10.0)
        .center_y()
        .fill(if s.page == Page::Tools && s.tool == Tool::Settings {
            c(RED)
        } else {
            Color::TRANSPARENT
        })
        .child(pic(&SHIELD, 15.0, SIDEMUTED))
        .child(label("Paramètres", 13.0, 550.0, WHITE))
        .disabled(s.job.is_some());
    nav = nav.child(if s.job.is_none() {
        settings
            .role("button")
            .label("Paramètres")
            .focusable(true)
            .cursor(Cursor::Pointer)
            .hover(Color::WHITE.alpha(0.07))
            .on_press(|s: &mut State| s.tool_select(Tool::Settings))
    } else {
        settings
    });
    col()
        .w(178.0)
        .h_full()
        .padding(13.0)
        .pt(22.0)
        .gap(30.0)
        .fill(c(SIDE))
        .child(logo)
        .child(nav)
        .child(spacer())
        .child(
            col()
                .padding(6.0)
                .gap(7.0)
                .child(el().w_full().h(1.0).fill(c(SIDEEDGE)))
                .child(
                    row()
                        .gap(7.0)
                        .center_y()
                        .child(pic(&SHIELD, 15.0, WHITE))
                        .child(overline("100 % LOCAL", WHITE)),
                )
                .child(
                    label(
                        "Les fichiers restent sur cet ordinateur.",
                        11.0,
                        460.0,
                        SIDEMUTED,
                    )
                    .wrap(),
                )
                .child(label("EpicCleaner / Windows", 10.0, 460.0, SIDEMUTED)),
        )
}

fn header(s: &State) -> El {
    let title = if s.page == Page::Tools {
        s.tool.label()
    } else {
        NAV.iter().find(|n| n.0 == s.page).map_or("", |n| n.1)
    };
    row()
        .h(43.0)
        .px(26.0)
        .w_full()
        .center_y()
        .gap(10.0)
        .child(overline("EPIC / CLEANER", INK))
        .child(label("/", 13.0, 470.0, MUTED))
        .child(label(title, 13.0, 620.0, INK))
        .child(spacer())
        .child(if s.demo {
            badge("MODE DÉMO", AMBER)
        } else {
            overline("●  HORS LIGNE", SOFT)
        })
}

fn home(s: &State) -> El {
    let ready = !s.demo && s.job.is_none();
    let pitch = col().grow(1.55).min_w(0.0).gap(10.0)
        .child(overline("PRENEZ LE CONTRÔLE DE VOTRE STOCKAGE", MINT))
        .child(col().gap(0.0)
            .child(display("FAITES", 56.0, INK))
            .child(row().gap(9.0).child(display("DE LA", 56.0, INK)).child(display("PLACE.", 56.0, MINT))))
        .child(label("Analysez vos disques, repérez les résidus et choisissez précisément ce que vous voulez retirer.", 13.0, 490.0, SOFT).wrap())
        .child(row().gap(8.0).mt(5.0)
            .child(button("home-cleanup", "Lancer une analyse", Some(&SEARCH), Button::Primary, ready, State::quick_clean))
            .child(button("home-scan", "Explorer les disques", Some(&ARROW), Button::Quiet, true, |s| s.tool_select(Tool::Map))));
    let summary = col().grow(1.0).min_w(0.0).gap(8.0)
        .child(overline("ESPACE RÉCUPÉRABLE ESTIMÉ", INK))
        .child(display(&s.overview_scan.map_or("—".into(), |(_, n)| bytes(n)), 49.0, MINT).truncate())
        .child(overline(&s.overview_scan.map_or("AUCUNE ANALYSE DE CACHES".into(), |(n, _)| format!("{} ÉLÉMENTS À EXAMINER", grouped(n as u64))), MUTED))
        .child(rule().mt(4.0))
        .child(row().gap(7.0).center_y().child(pic(&SHIELD, 15.0, GREEN))
            .child(label("Analyse non destructive", 12.0, 650.0, INK)))
        .child(label("Aucune suppression sans validation. Les estimations ne garantissent pas l’espace libéré.", 12.0, 480.0, SOFT).wrap());
    let mut drives = col().grow(1.0).min_w(0.0).gap(8.0).child(section_label(
        "01 / Vos disques",
        &format!("{:02} LECTEURS", s.drive_list.len()),
    ));
    for d in s.drive_list.iter().take(5) {
        drives = drives.child(editorial_drive(d, ready, "home"));
    }
    if s.drive_list.is_empty() {
        drives = drives.child(label("Aucun lecteur local détecté.", 12.0, 500.0, SOFT));
    }
    let total = s
        .drive_list
        .iter()
        .fold(0u64, |sum, drive| sum.saturating_add(drive.total));
    drives = drives.child(
        row()
            .center_y()
            .child(overline("CAPACITÉ TOTALE", MUTED))
            .child(spacer())
            .child(display(&bytes(total), 23.0, INK)),
    );
    let mut entries = col()
        .grow(1.0)
        .min_w(0.0)
        .gap(8.0)
        .child(section_label("02 / À examiner", "OUTILS D’ANALYSE"));
    for (tool, title, note, icon) in [
        (
            Tool::Caches,
            "Nettoyage",
            "Caches, fichiers temporaires et journaux",
            &CLEAN,
        ),
        (
            Tool::Duplicates,
            "Doublons",
            "Copies exactes dans un même dossier",
            &DUP,
        ),
        (
            Tool::Apps,
            "Applications",
            "Programmes installés et résidus",
            &FILE,
        ),
        (
            Tool::Downloads,
            "Téléchargements",
            "Fichiers anciens et retéléchargements",
            &FOLDER,
        ),
    ] {
        entries = entries.child(editorial_link(tool, title, note, icon, s.job.is_none()));
    }
    let freed = s
        .journal
        .iter()
        .fold(0u64, |sum, entry| sum.saturating_add(entry.freed));
    col()
        .gap(15.0)
        .w_full()
        .child(
            row()
                .center_y()
                .child(overline("01 — VUE D’ENSEMBLE / ATELIER DISQUE", INK))
                .child(spacer())
                .child(overline("WINDOWS / STOCKAGE LOCAL", MUTED)),
        )
        .child(heavy_rule())
        .child(
            row()
                .gap(19.0)
                .child(pitch)
                .child(el().w(1.0).fill(c(EDGE)))
                .child(summary),
        )
        .child(rule())
        .child(row().gap(19.0).child(drives).child(entries))
        .child(heavy_rule())
        .child(
            row()
                .center_y()
                .gap(9.0)
                .child(pic(&HISTORY, 17.0, MINT))
                .child(label("Historique des opérations", 14.0, 690.0, INK))
                .child(spacer())
                .child(label(
                    format!("{} libérés dans le journal local", bytes(freed)),
                    12.0,
                    550.0,
                    SOFT,
                ))
                .child(
                    button(
                        "home-history",
                        "Voir le journal",
                        Some(&ARROW),
                        Button::Quiet,
                        true,
                        |s| s.navigate(Page::History),
                    )
                    .h(33.0),
                ),
        )
}

fn section_label(title: &str, detail: &str) -> El {
    col()
        .gap(6.0)
        .child(
            row()
                .gap(6.0)
                .center_y()
                .child(display(title, 22.0, INK))
                .child(spacer())
                .child(overline(detail, MUTED)),
        )
        .child(heavy_rule())
}

fn editorial_link(
    tool: Tool,
    title: &str,
    subtitle: &str,
    glyph: &'static Icon,
    active: bool,
) -> El {
    let mut link = row()
        .key(format!("feature-{title}"))
        .role("button")
        .label(title)
        .disabled(!active)
        .min_h(47.0)
        .gap(9.0)
        .center_y()
        .child(pic(glyph, 16.0, INK))
        .child(
            col()
                .gap(2.0)
                .grow(1.0)
                .min_w(0.0)
                .child(label(title, 13.0, 720.0, INK))
                .child(label(subtitle, 11.0, 480.0, MUTED).truncate()),
        )
        .child(pic(&ARROW, 15.0, MINT));
    if active {
        link = link
            .focusable(true)
            .focus_ring(2.0, 2.0, c(MINT))
            .cursor(Cursor::Pointer)
            .hover(c(MINT).alpha(0.08))
            .on_press(move |s: &mut State| s.tool_select(tool));
    }
    col().child(link).child(rule())
}

fn editorial_drive(d: &epic_disk_core::system::Drive, active: bool, prefix: &str) -> El {
    let used = d.total.saturating_sub(d.free);
    let share = if d.total == 0 {
        0.0
    } else {
        used as f32 / d.total as f32
    };
    let path = std::path::PathBuf::from(&d.root);
    let name = if d.label.is_empty() {
        if d.system { "Windows" } else { "Disque local" }
    } else {
        &d.label
    };
    let mut drive = row()
        .key(format!("{prefix}-drive-{}", d.root))
        .min_h(56.0)
        .gap(10.0)
        .center_y()
        .role("button")
        .label(format!("Analyser le lecteur {}", d.root))
        .disabled(!active)
        .child(display(d.root.trim_end_matches('\\'), 22.0, INK).w(38.0))
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(4.0)
                .child(label(name, 13.0, 680.0, INK).truncate())
                .child(map_meter(
                    share,
                    5.0,
                    if d.system || share > 0.9 { MINT } else { INK },
                ))
                .child(
                    label(
                        format!("{} utilisés / {}", bytes(used), bytes(d.total)),
                        11.0,
                        480.0,
                        MUTED,
                    )
                    .truncate(),
                ),
        )
        .child(label(format!("{:.0} %", share * 100.0), 12.0, 630.0, SOFT).w(43.0))
        .child(pic(&ARROW, 14.0, MINT));
    if active {
        drive = drive
            .focusable(true)
            .focus_ring(2.0, 2.0, c(MINT))
            .cursor(Cursor::Pointer)
            .hover(c(MINT).alpha(0.07))
            .on_press(move |s: &mut State| {
                s.tool_select(Tool::Map);
                s.run_tool(Some(path.clone()));
            });
    }
    col().child(drive).child(rule())
}

fn intro(s: &State) -> El {
    let (section, title, detail) = match s.page {
        Page::Space => (
            "01 / EXPLORER",
            "Espace disque",
            "Mesurez un dossier ou un lecteur et identifiez les fichiers qui pèsent le plus.",
        ),
        Page::Duplicates => (
            "02 / COMPARER",
            "Doublons",
            "Identifiez les copies par empreinte BLAKE3 complète. Aucun fichier n’est modifié.",
        ),
        Page::Cleanup => (
            "03 / RÉVISER",
            "Nettoyage",
            "Fichiers temporaires du profil, modifiés il y a au moins 7 jours. Rien n’est présélectionné.",
        ),
        Page::History => (
            "04 / CONSULTER",
            "Historique",
            "Retrouvez les opérations de nettoyage enregistrées sur cet ordinateur.",
        ),
        Page::Home | Page::Tools => unreachable!(),
    };
    let actions = if s.page == Page::Cleanup {
        button(
            "preview",
            "Prévisualiser",
            Some(&SEARCH),
            Button::Primary,
            s.job.is_none() && !s.demo,
            State::run_temp,
        )
    } else if s.page != Page::History {
        button(
            "choose",
            "Choisir un dossier",
            Some(&FOLDER),
            Button::Primary,
            s.job.is_none() && !s.demo,
            State::pick,
        )
    } else {
        el()
    };
    col()
        .gap(9.0)
        .child(overline(section, MINT))
        .child(
            row()
                .center_y()
                .gap(12.0)
                .child(label(title, 34.0, 740.0, INK))
                .child(spacer())
                .child(actions),
        )
        .child(label(detail, 14.0, 450.0, SOFT).wrap())
}

fn bytes(value: u64) -> String {
    if value < 1024 {
        return format!("{value} o");
    }
    let mut n = value as f64;
    for unit in ["Kio", "Mio", "Gio", "Tio", "Pio", "Eio"] {
        n /= 1024.0;
        if n < 1024.0 || unit == "Eio" {
            return format!("{n:.1} {unit}").replace('.', ",");
        }
    }
    unreachable!()
}

fn stat(name: &str, value: String, note: &str, color: &str) -> El {
    card()
        .grow(1.0)
        .min_w(0.0)
        .padding(13.0)
        .gap(5.0)
        .child(overline(name, SOFT))
        .child(label(value, 23.0, 700.0, color).truncate())
        .child(label(note, 11.0, 470.0, MUTED).truncate())
}

fn map(report: &Report) -> El {
    let colors = [
        MINT, INK, "#9a7c6d", "#ad8e5b", GREEN, "#b37975", "#7b755e", "#a7a091",
    ];
    let mut bar = row().h(13.0).gap(2.0).fill(c("#243043"));
    let mut legend = row().gap(10.0);
    let mut sum = 0u64;
    for (i, (name, size)) in report.folders.iter().enumerate() {
        sum = sum.saturating_add(*size);
        if *size > 0 {
            bar = bar.child(
                el().grow((*size as f64 / report.bytes.max(1) as f64) as f32)
                    .h_full()
                    .radius(3.0)
                    .fill(c(colors[i]))
                    .help(format!("{name} · {}", bytes(*size))),
            );
        }
        if i < 4 {
            legend = legend.child(
                row()
                    .grow(1.0)
                    .min_w(0.0)
                    .center_y()
                    .gap(6.0)
                    .child(el().size(7.0, 7.0).radius(2.0).fill(c(colors[i])))
                    .child(
                        label(format!("{name} · {}", bytes(*size)), 11.0, 520.0, SOFT)
                            .min_w(0.0)
                            .truncate(),
                    ),
            );
        }
    }
    if report.bytes > sum {
        bar = bar.child(
            el().grow(((report.bytes - sum) as f64 / report.bytes as f64) as f32)
                .h_full()
                .radius(3.0)
                .fill(c("#4c607a"))
                .help("Autres dossiers"),
        );
    }
    card()
        .padding(13.0)
        .gap(10.0)
        .child(label("Répartition de l’espace", 15.0, 650.0, INK))
        .child(bar)
        .child(legend)
}

fn results(s: &State, report: &Report) -> El {
    let (a, b, note) = match s.page {
        Page::Duplicates => ("GROUPES", report.entries.to_string(), "Copies estimées"),
        Page::Cleanup => (
            "À VÉRIFIER",
            report.entries.to_string(),
            "Volume de l’aperçu",
        ),
        _ => ("FICHIERS", report.files.to_string(), "Volume logique"),
    };
    let volume = if s.page == Page::Space {
        report.bytes
    } else {
        report.matched_bytes
    };
    let mut result = col().gap(13.0).child(
        row()
            .gap(9.0)
            .child(stat("VOLUME", bytes(volume), note, MINT))
            .child(stat(a, b, "Dans le dossier analysé", INK))
            .child(if let Some((free, total)) = s.disk {
                stat(
                    "ESPACE LIBRE",
                    bytes(free),
                    &format!("Sur {}", bytes(total)),
                    INK,
                )
            } else {
                stat(
                    "IGNORÉS",
                    report.skipped.to_string(),
                    "Liens, exclusions ou accès refusé",
                    INK,
                )
            }),
    );
    if s.page == Page::Space && !report.folders.is_empty() {
        result = result.child(map(report));
    }
    if report.skipped > 0 || report.truncated {
        let mut notes = Vec::new();
        if report.skipped > 0 {
            notes.push(format!("{} élément(s) ignoré(s)", report.skipped));
        }
        if report.truncated {
            notes.push("limite d’analyse ou d’affichage atteinte".into());
        }
        result = result.child(label(notes.join(" · "), 12.0, 500.0, AMBER));
    }
    if s.page == Page::Cleanup && !report.rows.is_empty() {
        let selected = s
            .selected
            .iter()
            .filter_map(|i| report.rows.get(*i))
            .fold(0u64, |n, item| n.saturating_add(item.size));
        let active = !s.selected.is_empty() && !s.demo;
        result = result.child(
            row()
                .gap(8.0)
                .center_y()
                .child(label(
                    format!("{} sélectionné(s) · {}", s.selected.len(), bytes(selected)),
                    12.0,
                    620.0,
                    SOFT,
                ))
                .child(spacer())
                .child(button(
                    "clear",
                    "Tout désélectionner",
                    None,
                    Button::Quiet,
                    active,
                    |s| s.selected.clear(),
                ))
                .child(button(
                    "review",
                    "Vérifier la sélection",
                    Some(&CLEAN),
                    Button::Primary,
                    active,
                    State::open_confirm,
                )),
        );
    }
    result.child(table(s, report))
}

fn table(s: &State, report: &Report) -> El {
    let title = match s.page {
        Page::Space => "Fichiers les plus volumineux",
        Page::Duplicates => "Copies détectées",
        _ => "Fichiers temporaires détectés",
    };
    let filter = text_input(&s.filter, TextStyle::new(12.0, 490.0))
        .key("filter")
        .label("Filtrer les fichiers")
        .color(c(INK))
        .placeholder("Filtrer les fichiers…", c(MUTED))
        .single_line()
        .on_edit(|s: &mut State| &mut s.filter)
        .w(158.0)
        .h(28.0)
        .pt(6.0)
        .px(7.0);
    let mut table = card().padding(11.0).gap(6.0).child(
        row()
            .gap(9.0)
            .center_y()
            .child(label(title, 15.0, 680.0, INK))
            .child(spacer())
            .child(
                row()
                    .h(31.0)
                    .center_y()
                    .pl(8.0)
                    .radius(7.0)
                    .fill(c(BG))
                    .border(1.0, c(EDGE))
                    .child(pic(&SEARCH, 14.0, SOFT))
                    .child(filter),
            ),
    );
    let query = s.filter.text.trim().to_lowercase();
    let mut shown = 0;
    table = table.child(row().child(size_sort(s, false)));
    let mut items: Vec<_> = report.rows.iter().enumerate().collect();
    sort_by_size(&mut items, s.sort_size, |item| Some(item.1.size));
    for (index, item) in items {
        let path = readable_path(&item.path);
        if !query.is_empty()
            && !path.to_lowercase().contains(&query)
            && !item.detail.to_lowercase().contains(&query)
        {
            continue;
        }
        let name = item
            .path
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_else(|| item.path.to_string_lossy());
        let checked = s.selected.contains(&index);
        let selectable = s.page == Page::Cleanup && item.selectable() && !s.demo;
        let mut item_row = row()
            .key(format!("item-{index}"))
            .h(46.0)
            .gap(10.0)
            .px(7.0)
            .center_y()
            .radius(7.0)
            .fill(c(if checked { "#1c3540" } else { "#141d2b" }));
        if selectable {
            let mut checkbox = el()
                .size(18.0, 18.0)
                .radius(4.0)
                .center()
                .fill(if checked { c(MINT) } else { c(BG) })
                .border(1.0, c(if checked { MINT } else { SOFT }));
            if checked {
                checkbox = checkbox.child(pic(&CHECK, 14.0, "#082c27"));
            }
            item_row = item_row
                .role("checkbox")
                .label(path.as_ref())
                .checked(checked)
                .focusable(true)
                .focus_ring(2.0, 2.0, c(MINT))
                .cursor(Cursor::Pointer)
                .hover(Color::WHITE.alpha(0.06))
                .on_press(move |s: &mut State| {
                    if !s.selected.remove(&index) {
                        s.selected.insert(index);
                    }
                })
                .child(checkbox.a11y_hidden());
        } else {
            item_row = item_row.child(pic(&FILE, 17.0, "#9ebdff"));
        }
        let copy_path = path.into_owned();
        let path_help = copy_path.clone();
        let path_label = copy_path.clone();
        let clipboard = el()
            .key(format!("copy-{index}"))
            .size(27.0, 27.0)
            .center()
            .radius(6.0)
            .role("button")
            .label("Copier le chemin")
            .help("Copier le chemin")
            .focusable(true)
            .focus_ring(2.0, 2.0, c(MINT))
            .cursor(Cursor::Pointer)
            .hover(Color::WHITE.alpha(0.10))
            .on_press_tx(move |_: &mut State, tx| tx.copy(copy_path.clone()))
            .child(pic(&COPY, 14.0, SOFT));
        table = table.child(rule()).child(
            row()
                .center_y()
                .gap(5.0)
                .child(
                    item_row
                        .grow(1.0)
                        .min_w(0.0)
                        .child(
                            col()
                                .grow(1.0)
                                .min_w(0.0)
                                .gap(2.0)
                                .help(path_help)
                                .child(label(name.into_owned(), 13.0, 620.0, INK).truncate())
                                .child(label(path_label, 10.0, 450.0, MUTED).truncate()),
                        )
                        .child(label(&item.detail, 11.0, 500.0, SOFT).w(142.0).truncate())
                        .child(
                            label(bytes(item.size), 12.0, 680.0, MINT)
                                .w(77.0)
                                .truncate(),
                        ),
                )
                .child(clipboard),
        );
        shown += 1;
    }
    if shown == 0 {
        table = table.child(
            label("Aucun fichier correspondant.", 13.0, 500.0, SOFT)
                .mt(8.0)
                .mb(5.0),
        );
    }
    table
}

fn empty(page: Page) -> El {
    let (glyph, title, detail) = match page {
        Page::Space => (
            &SPACE,
            "Aucune analyse pour l’instant",
            "Choisissez ou glissez un dossier. Les résultats apparaîtront ici.",
        ),
        Page::Duplicates => (
            &DUP,
            "Prêt à comparer vos fichiers",
            "Choisissez un dossier. Les fichiers de moins de 4 Kio ne sont pas comparés.",
        ),
        Page::Cleanup => (
            &CLEAN,
            "Commencez par un aperçu",
            "Chaque fichier doit être sélectionné avant l’envoi à la Corbeille, qui ne libère pas immédiatement d’espace.",
        ),
        _ => (
            &HISTORY,
            "Aucune opération dans cette session",
            "Les détails des envois à la Corbeille s’afficheront ici.",
        ),
    };
    card()
        .padding(22.0)
        .gap(9.0)
        .child(
            el().size(43.0, 43.0)
                .center()
                .fill(c("#203748"))
                .radius(11.0)
                .child(pic(glyph, 23.0, MINT)),
        )
        .child(label(title, 18.0, 680.0, INK))
        .child(label(detail, 13.0, 460.0, SOFT).wrap())
}

fn history(s: &State) -> El {
    if s.receipts.is_empty() && s.journal.is_empty() {
        return empty(Page::History);
    }
    let mut content = col().gap(11.0);
    for (i, entry) in s.journal.iter().enumerate() {
        let detail = format!(
            "{} libérés · {} en Corbeille · {} fichiers · {} échecs{}",
            bytes(entry.freed),
            bytes(entry.recycled),
            entry.files,
            entry.failed,
            if entry.elevated { " · UAC" } else { "" }
        );
        let mut record = card()
            .key(format!("journal-{i}"))
            .padding(14.0)
            .gap(6.0)
            .child(overline(entry.time.get(..16).unwrap_or(&entry.time), MINT))
            .child(label(&entry.kind, 15.0, 680.0, INK).wrap())
            .child(label(detail, 12.0, 520.0, SOFT).wrap());
        if !entry.items.is_empty() {
            let lines = entry.items.join(" · ");
            record = record.child(
                label(&lines, 11.0, 480.0, MUTED)
                    .truncate()
                    .help(lines.clone()),
            );
        }
        content = content.child(record);
    }
    for (i, receipt) in s.receipts.iter().enumerate().rev() {
        let mut record = card()
            .padding(14.0)
            .gap(7.0)
            .child(overline(&format!("OPÉRATION {:02}", i + 1), MINT))
            .child(label(
                format!(
                    "{} fichier(s) envoyé(s) · {} échec(s)",
                    receipt.moved,
                    receipt.failures.len()
                ),
                15.0,
                670.0,
                INK,
            ));
        for (path, err) in &receipt.failures {
            record = record
                .child(label(format!("{} — {err}", path.display()), 12.0, 500.0, AMBER).wrap());
        }
        content = content.child(record);
    }
    content
}

fn activity(s: &State) -> El {
    let job = s.job.as_ref().expect("job present");
    let mut content = card()
        .padding(16.0)
        .gap(10.0)
        .child(label(job.label, 17.0, 670.0, INK));
    if let Some(progress) = &job.progress {
        let stopping = progress.cancel.load(Relaxed);
        let (files, size, hashing) = s.scan_progress;
        content = content
            .child(label(
                format!(
                    "{} · {files} fichier(s) parcouru(s) · {}",
                    if hashing {
                        "Comparaison BLAKE3"
                    } else {
                        "Parcours du dossier"
                    },
                    bytes(size)
                ),
                13.0,
                510.0,
                SOFT,
            ))
            .child(row().child(button(
                "cancel-scan",
                if stopping {
                    "Annulation…"
                } else {
                    "Annuler l’analyse"
                },
                None,
                Button::Quiet,
                !stopping,
                |s| {
                    if let Some(progress) = s.job.as_ref().and_then(|j| j.progress.as_ref()) {
                        progress.cancel.store(true, Relaxed);
                    }
                },
            )));
    } else if let Some(core) = &job.core {
        let p = core.snapshot();
        let stopping = core.is_cancelled();
        content = content
            .child(label(
                format!(
                    "{} fichiers · {} · {} erreurs{}",
                    p.files,
                    bytes(p.bytes),
                    p.errors,
                    if p.total > 0 {
                        format!(" · {}/{}", p.done, p.total)
                    } else {
                        String::new()
                    }
                ),
                13.0,
                520.0,
                SOFT,
            ))
            .child(
                row().child(
                    button(
                        "cancel-tool",
                        if stopping {
                            "Annulation…"
                        } else {
                            "Demander l’arrêt"
                        },
                        None,
                        Button::Quiet,
                        !stopping,
                        |s| {
                            if let Some(core) = s.job.as_ref().and_then(|j| j.core.as_ref()) {
                                core.cancel();
                            }
                        },
                    )
                    .h(31.0),
                ),
            );
    }
    content
}

fn workspace(s: &State) -> El {
    if s.page == Page::Home {
        return home(s);
    }
    if s.page == Page::Tools {
        return tool_workspace(s);
    }
    let mut content = col().gap(14.0).w_full().child(intro(s));
    if let Some(path) = &s.path {
        let caption = readable_path(path).into_owned();
        content = content.child(
            row()
                .gap(8.0)
                .center_y()
                .h(35.0)
                .px(9.0)
                .radius(8.0)
                .fill(c("#101c2a"))
                .border(1.0, c(EDGE))
                .child(pic(&FOLDER, 16.0, SOFT))
                .child(
                    label(caption.clone(), 12.0, 550.0, SOFT)
                        .min_w(0.0)
                        .truncate()
                        .help(caption),
                ),
        );
    }
    if !s.status.is_empty() {
        content =
            content.child(label(&s.status, 13.0, 560.0, if s.error { AMBER } else { MINT }).wrap());
    }
    if s.job.is_some() {
        content.child(activity(s))
    } else if s.page == Page::History {
        content.child(history(s))
    } else if let Some(report) = &s.report {
        content.child(results(s, report))
    } else {
        content.child(empty(s.page))
    }
}

fn tool_workspace(s: &State) -> El {
    let map = s.tool == Tool::Map;
    let map_start = map && s.job.is_none() && s.tool_output.is_none();
    let map_result = map
        && s.tool_output
            .as_ref()
            .is_some_and(|output| output.tree.is_some());
    let mut menu = col().gap(5.0);
    for (group, slice) in TOOLS.chunks(5).enumerate() {
        let mut line = row().gap(5.0);
        for &(tool, title) in slice {
            line = line.child(
                button(
                    &format!("tool-tab-{tool:?}"),
                    title,
                    None,
                    if s.tool == tool {
                        Button::Primary
                    } else {
                        Button::Quiet
                    },
                    s.job.is_none(),
                    move |s| s.tool_select(tool),
                )
                .grow(1.0)
                .min_w(0.0)
                .px(7.0)
                .h(31.0)
                .center(),
            );
        }
        menu = menu.child(line.key(format!("tool-group-{group}")));
    }
    let mut title = row()
        .gap(8.0)
        .center_y()
        .child(display(
            if s.quick {
                "Nettoyage rapide"
            } else {
                s.tool.label()
            },
            36.0,
            INK,
        ))
        .child(spacer());
    let ready = s.job.is_none() && !s.demo;
    let noun = if s.tool.needs_root() {
        "Choisir un dossier"
    } else if matches!(
        s.tool,
        Tool::Windows | Tool::Settings | Tool::Apps | Tool::Startup
    ) {
        "Actualiser"
    } else {
        "Analyser"
    };
    title = title.child(button(
        "tool-scan",
        noun,
        Some(&SEARCH),
        Button::Primary,
        ready,
        |s| {
            if s.tool.needs_root() {
                s.pick();
            } else {
                s.run_tool(s.path.clone());
            }
        },
    ));
    if matches!(
        s.tool,
        Tool::Projects | Tool::Downloads | Tool::Installers | Tool::Models
    ) {
        title = title.child(button(
            "tool-pick",
            "Dossier…",
            Some(&FOLDER),
            Button::Quiet,
            ready,
            State::pick,
        ));
    }
    let mut content = col().gap(12.0).w_full();
    if s.map_catalog {
        content = content
            .child(section_label("Index des outils", "EXPLORER / NETTOYER"))
            .child(menu);
    }
    if map {
        title = title.child(button(
            "map-tools",
            if s.map_catalog {
                "Masquer les outils"
            } else {
                "Autres outils"
            },
            None,
            Button::Quiet,
            true,
            |s| s.map_catalog = !s.map_catalog,
        ));
    }
    if map_result {
        let mut commands = row().gap(6.0).center_y().child(overline("LECTEURS", MUTED));
        for d in s.drive_list.iter().take(4) {
            let path = std::path::PathBuf::from(&d.root);
            commands = commands.child(
                button(
                    &format!("drive-{}", d.root),
                    &d.root,
                    None,
                    Button::Quiet,
                    ready,
                    move |s| s.run_tool(Some(path.clone())),
                )
                .h(30.0)
                .px(8.0),
            );
        }
        content = content.child(
            commands
                .child(spacer())
                .child(
                    button(
                        "tool-scan",
                        "Choisir un dossier",
                        Some(&FOLDER),
                        Button::Quiet,
                        ready,
                        State::pick,
                    )
                    .h(30.0),
                )
                .child(
                    button(
                        "map-tools",
                        if s.map_catalog {
                            "Masquer les outils"
                        } else {
                            "Autres outils"
                        },
                        None,
                        Button::Quiet,
                        true,
                        |s| s.map_catalog = !s.map_catalog,
                    )
                    .h(30.0),
                ),
        );
    } else if map_start {
        content = content.child(
            row()
                .center_y()
                .child(overline("STOCKAGE / EXPLORATEUR", MUTED))
                .child(spacer())
                .child(
                    button(
                        "map-tools",
                        if s.map_catalog {
                            "Masquer les outils"
                        } else {
                            "Autres outils"
                        },
                        None,
                        Button::Quiet,
                        true,
                        |s| s.map_catalog = !s.map_catalog,
                    )
                    .h(32.0),
                ),
        );
    } else {
        content = content
            .child(overline(
                if map {
                    "02 / EXPLORATEUR"
                } else if s.tool == Tool::Caches {
                    "03 / ANALYSE DU STOCKAGE"
                } else {
                    "ATELIER / ESPACE DE TRAVAIL"
                },
                MINT,
            ))
            .child(title)
            .child(heavy_rule());
    }
    if s.tool.needs_root() && !map_start && !map_result && !s.drive_list.is_empty() {
        let mut drives = row().gap(6.0).center_y().child(overline("LECTEURS", SOFT));
        for d in s.drive_list.iter().take(6) {
            let path = std::path::PathBuf::from(&d.root);
            drives = drives.child(
                button(
                    &format!("drive-{}", d.root),
                    &d.root,
                    Some(&SPACE),
                    Button::Quiet,
                    ready,
                    move |s| s.run_tool(Some(path.clone())),
                )
                .help(format!(
                    "{} · {} libres sur {}",
                    d.label,
                    bytes(d.free),
                    bytes(d.total)
                ))
                .h(31.0),
            );
        }
        content = content.child(drives);
    }
    if !s.status.is_empty() {
        content =
            content.child(label(&s.status, 13.0, 600.0, if s.error { AMBER } else { MINT }).wrap());
    }
    if s.job.is_some() {
        return content.child(activity(s));
    }
    if s.map_return.is_some() {
        content = content.child(
            row().child(
                button(
                    "map-return",
                    "Revenir à la carte",
                    None,
                    Button::Quiet,
                    true,
                    State::return_to_map,
                )
                .h(31.0),
            ),
        );
    }
    if s.tool == Tool::Settings {
        content = content.child(settings_form(s));
    }
    if let Some(output) = &s.tool_output {
        if !output.note.is_empty() && output.tree.is_none() {
            content = content.child(label(&output.note, 12.0, 520.0, SOFT).wrap());
        }
        if let Some(tree) = &output.tree {
            return content.child(tool_map(s, output, tree));
        }
        return content.child(tool_table(s, output));
    }
    if map_start {
        return content.child(map_landing(s, ready));
    }
    content.child(
        card()
            .padding(17.0)
            .gap(6.0)
            .child(label("Prêt à analyser", 17.0, 680.0, INK))
            .child(label(tool_hint(s.tool), 13.0, 500.0, SOFT).wrap()),
    )
}

const MAP_WHITE: &str = WHITE;
const MAP_MIST: &str = SIDEMUTED;
const MAP_AQUA: &str = "#ff9e8b";

fn map_panel() -> El {
    row().fill(c(BLACK))
}

fn map_landing(s: &State, ready: bool) -> El {
    let hero = map_panel().padding(24.0).gap(12.0).center_y()
        .child(col().grow(1.0).min_w(0.0).gap(9.0)
            .child(overline("CARTE DU DISQUE  /  ANALYSE LOCALE", MAP_AQUA))
            .child(display("Où va votre espace ?", 41.0, MAP_WHITE))
            .child(label("Analysez un lecteur entier ou un dossier. Repérez en un coup d’œil ce qui prend le plus de place.", 14.0, 480.0, MAP_MIST).wrap())
            .child(row().mt(7.0).child(button("tool-scan", "Choisir un dossier", Some(&FOLDER), Button::Primary, ready, State::pick))))
        .child(el().w(116.0).h(115.0).center().fill(c(RED)).child(pic(&SPACE, 63.0, WHITE)));
    let mut layout = col().gap(15.0).child(hero).child(
        row()
            .center_y()
            .child(label("Lecteurs disponibles", 19.0, 690.0, INK))
            .child(spacer())
            .child(
                row()
                    .gap(6.0)
                    .center_y()
                    .child(pic(&SHIELD, 15.0, MINT))
                    .child(label("Analyse en lecture seule", 12.0, 550.0, SOFT)),
            ),
    );
    if s.drive_list.is_empty() {
        return layout.child(
            card().padding(15.0).child(
                label(
                    "Aucun lecteur détecté. Vous pouvez choisir ou déposer un dossier.",
                    13.0,
                    500.0,
                    SOFT,
                )
                .wrap(),
            ),
        );
    }
    for drives in s.drive_list.chunks(3) {
        let mut line = row().gap(10.0);
        for drive in drives {
            let used = drive.total.saturating_sub(drive.free);
            let share = if drive.total > 0 {
                used as f32 / drive.total as f32
            } else {
                0.0
            };
            let path = std::path::PathBuf::from(&drive.root);
            let title = if drive.label.is_empty() {
                if drive.removable {
                    "Lecteur amovible"
                } else {
                    "Disque local"
                }
            } else {
                &drive.label
            };
            let mut tile = card()
                .key(format!("drive-{}", drive.root))
                .grow(1.0)
                .min_w(0.0)
                .padding(13.0)
                .gap(9.0)
                .role("button")
                .label(format!("Analyser le lecteur {}", drive.root))
                .disabled(!ready)
                .child(
                    row()
                        .gap(8.0)
                        .center_y()
                        .child(
                            el().size(32.0, 32.0)
                                .center()
                                .fill(c(MINT).alpha(0.12))
                                .child(pic(&SPACE, 18.0, MINT)),
                        )
                        .child(label(&drive.root, 18.0, 700.0, INK))
                        .child(spacer())
                        .child(pic(&ARROW, 15.0, MINT)),
                )
                .child(label(title, 13.0, 620.0, SOFT).truncate())
                .child(map_meter(
                    share,
                    6.0,
                    if share > 0.9 { AMBER } else { MINT },
                ))
                .child(
                    label(
                        format!("{} libres sur {}", bytes(drive.free), bytes(drive.total)),
                        11.0,
                        520.0,
                        MUTED,
                    )
                    .truncate(),
                );
            if ready {
                tile = tile
                    .cursor(Cursor::Pointer)
                    .focusable(true)
                    .focus_ring(2.0, 2.0, c(MINT))
                    .hover(c(MINT).alpha(0.08))
                    .press_scale(0.985)
                    .on_press(move |s: &mut State| s.run_tool(Some(path.clone())));
            }
            line = line.child(tile);
        }
        for _ in drives.len()..3 {
            line = line.child(spacer());
        }
        layout = layout.child(line);
    }
    layout
}

fn map_meter(share: f32, height: f32, color: &str) -> El {
    let share = share.clamp(0.0, 1.0);
    let mut meter = row().w_full().h(height).fill(c(EDGE));
    if share > 0.0 {
        meter = meter.child(el().grow(share).h_full().fill(c(color)));
        if share < 1.0 {
            meter = meter.child(el().grow(1.0 - share).h_full());
        }
    }
    meter
}

fn tool_hint(tool: Tool) -> &'static str {
    match tool {
        Tool::Caches => {
            "Parcourez les caches système, navigateurs, applications, outils de développement et jeux."
        }
        Tool::Map => {
            "Voyez ce qui prend de la place, du plus gros au plus petit. Cliquez sur un dossier pour l’explorer."
        }
        Tool::Large => {
            "Filtrez les fichiers volumineux par nom, extension, date ou type. Le seuil se règle dans Réglages."
        }
        Tool::Duplicates => {
            "Les sous-dossiers sont parcourus, mais seuls les fichiers du même dossier sont comparés. Le contenu est vérifié par BLAKE3."
        }
        Tool::Empty => {
            "Détecte les dossiers et sous-dossiers vides. Le retrait ne supprime jamais de fichier."
        }
        Tool::Projects => {
            "Détecte les artefacts de compilation associés à leurs projets : node_modules, target, .next et autres."
        }
        Tool::Downloads => {
            "Analyse le dossier connu de Windows et repère les retéléchargements probables."
        }
        Tool::Installers => {
            "Inventorie les anciens installateurs. Le seuil d’âge se règle dans Réglages."
        }
        Tool::Models => "Inventorie les modèles Hugging Face, LM Studio, Ollama et ComfyUI.",
        Tool::Assistants => "Inspecte les sessions et données de Claude Code, Codex et Gemini CLI.",
        Tool::Games => {
            "Détecte les bibliothèques Steam sur les autres lecteurs et associe les caches de shaders à leurs jeux."
        }
        _ => "Les modifications ne sont appliquées qu’après confirmation.",
    }
}

fn tool_table(s: &State, output: &Output) -> El {
    let query = s.filter.text.trim().to_lowercase();
    let mut matches: Vec<_> = output
        .rows
        .iter()
        .enumerate()
        .filter(|(_, entry)| {
            query.is_empty()
                || entry.title.to_lowercase().contains(&query)
                || entry.detail.to_lowercase().contains(&query)
                || entry
                    .path
                    .as_ref()
                    .is_some_and(|p| readable_path(p).to_lowercase().contains(&query))
        })
        .collect();
    sort_by_size(&mut matches, s.sort_size, |entry| entry.1.bytes);
    const PER_PAGE: usize = 32;
    let last = matches.len().saturating_sub(1) / PER_PAGE;
    let current = s.tool_page.min(last);
    let visible = &matches[current * PER_PAGE..matches.len().min((current + 1) * PER_PAGE)];
    let candidates = visible
        .iter()
        .filter(|(_, e)| e.action.as_ref().is_some_and(Action::selectable))
        .map(|(i, _)| *i)
        .collect::<Vec<_>>();
    let safe = matches
        .iter()
        .filter(|(_, e)| {
            e.risk == Some(Risk::Safe) && e.action.as_ref().is_some_and(Action::selectable)
        })
        .map(|(i, _)| *i)
        .collect::<Vec<_>>();
    let idle = if s.tool == Tool::Projects {
        matches
            .iter()
            .filter(|(_, e)| {
                e.detail.contains("projet inactif")
                    && e.risk == Some(Risk::Safe)
                    && e.action.as_ref().is_some_and(Action::selectable)
            })
            .map(|(i, _)| *i)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let selected = s.tool_selection.len();
    let search = text_input(&s.filter, TextStyle::new(12.0, 490.0))
        .key("tool-filter")
        .label("Filtrer les résultats")
        .placeholder("Rechercher…", c(MUTED))
        .color(c(INK))
        .single_line()
        .w(160.0)
        .h(30.0)
        .pt(7.0)
        .px(6.0)
        .on_edit(|s: &mut State| {
            s.tool_page = 0;
            &mut s.filter
        });
    let total = output
        .rows
        .iter()
        .filter_map(|r| r.bytes)
        .fold(0u64, u64::saturating_add);
    let mut table = col().gap(9.0);
    if s.tool == Tool::Caches && !output.rows.is_empty() {
        let selected_size = s
            .tool_selection
            .iter()
            .filter_map(|i| output.rows.get(*i)?.bytes)
            .fold(0u64, u64::saturating_add);
        table = table.child(
            row()
                .gap(14.0)
                .py(12.0)
                .child(
                    col()
                        .grow(1.5)
                        .min_w(0.0)
                        .gap(4.0)
                        .child(display("Moins de bruit.", 35.0, INK))
                        .child(display("Plus d’espace.", 35.0, MINT))
                        .child(
                            label(
                                "Évaluez chaque élément avant de confirmer.",
                                13.0,
                                510.0,
                                SOFT,
                            )
                            .wrap(),
                        ),
                )
                .child(el().w(1.0).fill(c(EDGE)))
                .child(
                    col()
                        .grow(1.0)
                        .min_w(0.0)
                        .gap(5.0)
                        .child(overline("SÉLECTION À VÉRIFIER", INK))
                        .child(display(&bytes(selected_size), 42.0, MINT).truncate())
                        .child(
                            label(
                                format!(
                                    "{} sélectionné(s) · {} détectés",
                                    grouped(selected as u64),
                                    bytes(total)
                                ),
                                12.0,
                                540.0,
                                SOFT,
                            )
                            .wrap(),
                        ),
                ),
        );
        let mut steps = row().gap(10.0);
        for (i, title) in ["Repérer", "Évaluer", "Prévisualiser", "Nettoyer"]
            .into_iter()
            .enumerate()
        {
            steps = steps.child(
                col()
                    .grow(1.0)
                    .min_w(0.0)
                    .gap(4.0)
                    .child(display(
                        &format!("{:02}", i + 1),
                        27.0,
                        match i {
                            0 => INK,
                            1 => MINT,
                            _ => MUTED,
                        },
                    ))
                    .child(label(title, 12.0, 650.0, INK).truncate()),
            );
        }
        table = table.child(rule()).child(steps).child(heavy_rule());
    }
    table = table
        .child(
            row()
                .gap(7.0)
                .center_y()
                .child(display(
                    format!(
                        "{} résultat(s){}",
                        matches.len(),
                        if s.tool == Tool::Caches {
                            format!(" · {}", bytes(total))
                        } else {
                            String::new()
                        }
                    )
                    .as_str(),
                    20.0,
                    INK,
                ))
                .child(spacer())
                .child(
                    row()
                        .center_y()
                        .pl(6.0)
                        .fill(c(BG))
                        .border(1.0, c(EDGE))
                        .child(pic(&SEARCH, 14.0, SOFT))
                        .child(search),
                ),
        )
        .child(if output.rows.iter().any(|entry| entry.bytes.is_some()) {
            row().child(size_sort(s, false))
        } else {
            el()
        })
        .child(heavy_rule());
    if selected > 0 || !candidates.is_empty() || !safe.is_empty() {
        let mut controls = row()
            .gap(6.0)
            .center_y()
            .child(label(
                format!("{selected} sélectionné(s)"),
                12.0,
                600.0,
                SOFT,
            ))
            .child(spacer());
        if !idle.is_empty() {
            controls = controls.child(
                button(
                    "select-idle",
                    "Projets inactifs",
                    None,
                    Button::Quiet,
                    !s.demo,
                    move |s| s.tool_selection.extend(idle.iter().copied()),
                )
                .h(31.0),
            );
        } else if !safe.is_empty() {
            controls = controls.child(
                button(
                    "select-safe",
                    "Sûrs",
                    None,
                    Button::Quiet,
                    !s.demo,
                    move |s| s.tool_selection.extend(safe.iter().copied()),
                )
                .h(31.0),
            );
        }
        if !candidates.is_empty() && s.tool != Tool::Projects {
            controls = controls.child(
                button(
                    "select-page",
                    "Cette page",
                    None,
                    Button::Quiet,
                    !s.demo,
                    move |s| s.tool_selection.extend(candidates.iter().copied()),
                )
                .h(31.0),
            );
        }
        if selected > 0 {
            if s.tool == Tool::Caches {
                let ids = s
                    .tool_selection
                    .iter()
                    .filter_map(|i| {
                        output
                            .rows
                            .get(*i)?
                            .action
                            .as_ref()?
                            .rule_id()
                            .map(str::to_owned)
                    })
                    .collect::<Vec<_>>();
                if !ids.is_empty() {
                    controls = controls.child(
                        button(
                            "export-plan",
                            "Exporter .ps1",
                            None,
                            Button::Quiet,
                            !s.demo,
                            move |s| s.tool_request(vec![Action::Export(ids.clone())]),
                        )
                        .h(31.0),
                    );
                }
            }
            controls = controls
                .child(
                    button(
                        "selection-clear",
                        "Désélect.",
                        None,
                        Button::Quiet,
                        !s.demo,
                        |s| s.tool_selection.clear(),
                    )
                    .h(31.0),
                )
                .child(
                    button(
                        "selection-review",
                        if s.tool == Tool::Apps {
                            "Revoir les résidus"
                        } else {
                            "Vérifier"
                        },
                        Some(&CLEAN),
                        Button::Primary,
                        !s.demo,
                        |s| {
                            let actions = s
                                .tool_output
                                .iter()
                                .flat_map(|o| {
                                    s.tool_selection
                                        .iter()
                                        .filter_map(|i| o.rows.get(*i))
                                        .filter_map(|e| e.action.clone())
                                        .filter(Action::selectable)
                                })
                                .collect();
                            s.tool_request(actions);
                        },
                    )
                    .h(31.0),
                );
        }
        table = table.child(controls);
    }
    if matches.is_empty() {
        return table.child(label("Aucun élément correspondant.", 13.0, 500.0, SOFT));
    }
    if last > 0 {
        table = table.child(
            row()
                .gap(7.0)
                .center_y()
                .child(label(
                    format!("Page {} / {}", current + 1, last + 1),
                    12.0,
                    550.0,
                    SOFT,
                ))
                .child(spacer())
                .child(
                    button(
                        "page-prev",
                        "Précédente",
                        None,
                        Button::Quiet,
                        current > 0,
                        move |s| s.tool_page = current - 1,
                    )
                    .h(30.0),
                )
                .child(
                    button(
                        "page-next",
                        "Suivante",
                        None,
                        Button::Quiet,
                        current < last,
                        move |s| s.tool_page = current + 1,
                    )
                    .h(30.0),
                ),
        );
    }
    for &(index, entry) in visible {
        table = table.child(rule()).child(tool_item(s, index, entry));
    }
    table
}

fn tool_item(s: &State, i: usize, entry: &Entry) -> El {
    let checked = s.tool_selection.contains(&i);
    let active = !s.demo && s.job.is_none();
    let toggle = entry.action.as_ref().is_some_and(Action::selectable);
    let name = entry.title.clone();
    let path = entry.path.as_ref().map(|p| readable_path(p).into_owned());
    let mut main = row()
        .key(format!("tool-item-{i}"))
        .min_h(48.0)
        .gap(9.0)
        .px(4.0)
        .center_y()
        .radius(6.0)
        .fill(if toggle && checked {
            c("#1b3540")
        } else {
            Color::TRANSPARENT
        });
    if toggle {
        main = main.child(
            button(
                &format!("tool-check-{i}"),
                if checked { "✓" } else { "–" },
                None,
                Button::Quiet,
                active,
                move |s| {
                    if !s.tool_selection.remove(&i) {
                        s.tool_selection.insert(i);
                    }
                },
            )
            .role("checkbox")
            .label(&name)
            .checked(checked)
            .w(27.0)
            .h(27.0)
            .px(0.0)
            .center(),
        );
    } else if !s.tool_images.get(&i).is_some_and(|img| img.is_ready()) {
        main = main.child(pic(&FILE, 16.0, SOFT));
    }
    if let Some(img) = s.tool_images.get(&i).filter(|img| img.is_ready()) {
        main = main.child(
            el().size(28.0, 28.0)
                .radius(5.0)
                .pixels(img.pixels())
                .a11y_hidden(),
        );
    }
    let detail = if let Some(path) = &path {
        format!("{} · {path}", entry.detail.replace('\n', " · "))
    } else {
        entry.detail.replace('\n', " · ")
    };
    main = main.child(
        col()
            .grow(1.0)
            .min_w(0.0)
            .gap(2.0)
            .child(label(name, 13.0, 660.0, INK).min_w(0.0).truncate())
            .child(
                label(&detail, 11.0, 460.0, SOFT)
                    .min_w(0.0)
                    .truncate()
                    .help(detail.clone()),
            ),
    );
    if let Some(risk) = entry.risk {
        let (word, color) = match risk {
            Risk::Safe => ("RECRÉABLE", GREEN),
            Risk::Review => ("À VÉRIFIER", AMBER),
            Risk::Caution => ("PRUDENCE", "#ff9ca6"),
        };
        main = main.child(badge(word, color));
    }
    if let Some(amount) = entry.bytes {
        main = main.child(label(bytes(amount), 12.0, 650.0, MINT).w(76.0).truncate());
    }
    if let Some(action) = &entry.action
        && (!toggle || matches!(action, Action::Empty(_)))
    {
        let action = action.clone();
        main = main.child(
            button(
                &format!("tool-action-{i}"),
                action.label(),
                None,
                Button::Quiet,
                active,
                move |s| s.tool_request(vec![action.clone()]),
            )
            .h(30.0),
        );
    }
    if let Some(path) = &entry.path {
        let p = path.clone();
        let inspect = path.clone();
        let copied = readable_path(path).into_owned();
        main = main
            .child(
                button(
                    &format!("tool-inspect-{i}"),
                    "?",
                    None,
                    Button::Quiet,
                    active,
                    move |s| s.tool_request(vec![Action::Inspect(inspect.clone())]),
                )
                .h(28.0)
                .px(8.0)
                .help("Quel processus utilise ce chemin ?"),
            )
            .child(
                button(
                    &format!("tool-open-{i}"),
                    "↗",
                    None,
                    Button::Quiet,
                    active,
                    move |s| s.tool_request(vec![Action::Open(p.clone())]),
                )
                .h(28.0)
                .px(8.0)
                .help("Afficher dans l’Explorateur"),
            )
            .child(
                el().key(format!("tool-copy-{i}"))
                    .size(26.0, 28.0)
                    .center()
                    .radius(5.0)
                    .role("button")
                    .label("Copier le chemin")
                    .help("Copier le chemin")
                    .focusable(true)
                    .cursor(Cursor::Pointer)
                    .hover(Color::WHITE.alpha(0.12))
                    .on_press_tx(move |_: &mut State, tx| tx.copy(copied.clone()))
                    .child(pic(&COPY, 13.0, SOFT)),
            );
    }
    main
}

fn tool_map(s: &State, output: &Output, tree: &explorer::Tree) -> El {
    let Some(node) = tree.view(s.tool_node, 0, 0) else {
        return el();
    };
    let ancestors = tree.ancestors(s.tool_node);
    let mut nav = row().gap(7.0).center_y();
    if let Some(&(parent, _)) = ancestors.iter().rev().nth(1) {
        nav = nav.child(
            button(
                "map-parent",
                "Remonter",
                None,
                Button::Quiet,
                true,
                move |s| s.map_enter(parent),
            )
            .h(30.0),
        );
        if parent != 0 {
            nav = nav.child(
                button(
                    "map-root",
                    "Racine du scan",
                    None,
                    Button::Quiet,
                    true,
                    |s| s.map_enter(0),
                )
                .h(30.0),
            );
        }
    }
    let path = readable_path(Path::new(&node.path)).into_owned();
    nav = nav.child(spacer());
    let switch = |key, title, active, select: fn(&mut State)| {
        button(
            key,
            title,
            None,
            if active {
                Button::Primary
            } else {
                Button::Quiet
            },
            true,
            select,
        )
        .h(32.0)
    };
    let mut tabs = row()
        .gap(6.0)
        .center_y()
        .child(switch(
            "map-all",
            "Par taille",
            !s.map_cleanup && !s.tool_gap,
            |s| {
                s.map_cleanup = false;
                s.tool_gap = false;
                s.map_enter(s.tool_node);
            },
        ))
        .child(switch(
            "map-cleanup",
            "Caches détectés",
            s.map_cleanup && !s.tool_gap,
            |s| {
                s.map_cleanup = true;
                s.tool_gap = false;
                s.map_enter(s.tool_node);
            },
        ));
    if output.gap.is_some() {
        tabs = tabs.child(switch("map-gap", "Bilan du lecteur", s.tool_gap, |s| {
            s.tool_gap = true;
        }));
    }
    let toolbar = nav.child(tabs);
    if s.tool_gap {
        let mut block = card().padding(16.0).gap(10.0);
        if let Some(gap) = &output.gap {
            block = block
                .child(label(
                    format!("Bilan du lecteur {}", gap.root),
                    20.0,
                    700.0,
                    INK,
                ))
                .child(
                    label(
                        "Ce bilan concerne le lecteur entier, pas le dossier ouvert.",
                        12.0,
                        520.0,
                        SOFT,
                    )
                    .wrap(),
                )
                .child(
                    row()
                        .gap(8.0)
                        .child(stat("UTILISÉ", bytes(gap.used), "Selon Windows", INK))
                        .child(stat("LIBRE", bytes(gap.free), "Selon Windows", MINT))
                        .child(stat(
                            "SCANNÉ",
                            gap.scanned.map_or("—".into(), bytes),
                            "Analyse du lecteur entier",
                            INK,
                        )),
                );
            if let Some(unexplained) = gap.unexplained {
                block = block.child(label(
                    format!(
                        "Écart restant : {}{}",
                        if unexplained < 0 { "−" } else { "" },
                        bytes(unexplained.unsigned_abs())
                    ),
                    13.0,
                    580.0,
                    SOFT,
                ));
            }
            for (i, part) in gap.parts.iter().enumerate() {
                let name = part
                    .label
                    .as_deref()
                    .unwrap_or_else(|| hidden_name(part.kind));
                block = block.child(
                    row()
                        .gap(7.0)
                        .center_y()
                        .child(label(name, 12.0, 600.0, SOFT).min_w(0.0).truncate())
                        .child(spacer())
                        .child(label(
                            part.bytes.map_or(
                                if part.needs_admin {
                                    "Droits admin requis".into()
                                } else {
                                    "Non mesuré".into()
                                },
                                bytes,
                            ),
                            12.0,
                            600.0,
                            MINT,
                        ))
                        .child(if let Some(page) = hidden_settings(part.kind) {
                            button(
                                &format!("hidden-{i}"),
                                "Gérer",
                                None,
                                Button::Quiet,
                                !s.demo,
                                move |s| s.tool_request(vec![Action::ShellPage(page)]),
                            )
                            .h(28.0)
                        } else {
                            el()
                        }),
                );
            }
        }
        return col().gap(12.0).child(toolbar).child(block);
    }
    const PER_PAGE: usize = 20;
    let offset = s.tool_page.saturating_mul(PER_PAGE);
    let ascending = s.sort_size == Some(true);
    let (count, children) =
        tree.ranked_children(s.tool_node, s.map_cleanup, ascending, offset, PER_PAGE);
    let total = map_amount(&node, s.map_cleanup);
    let hero = row()
        .py(10.0)
        .gap(14.0)
        .center_y()
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(5.0)
                .child(
                    row()
                        .gap(8.0)
                        .center_y()
                        .child(
                            el().size(27.0, 27.0)
                                .fill(c(MINT).alpha(0.10))
                                .center()
                                .child(pic(&FOLDER, 16.0, MINT)),
                        )
                        .child(overline(
                            if s.tool_node == 0 {
                                "RACINE DU SCAN"
                            } else {
                                "DOSSIER EN COURS"
                            },
                            MINT,
                        )),
                )
                .child(
                    display(&format!("Que contient {} ?", node.name), 35.0, INK)
                        .min_w(0.0)
                        .truncate(),
                )
                .child(
                    label(&path, 12.0, 480.0, SOFT)
                        .min_w(0.0)
                        .truncate()
                        .help(path.clone()),
                ),
        )
        .child(el().h(65.0).w(1.0).fill(c(EDGE)))
        .child(
            col()
                .gap(4.0)
                .w(187.0)
                .child(overline(
                    if s.map_cleanup {
                        "CACHES ESTIMÉS"
                    } else {
                        "TAILLE ANALYSÉE"
                    },
                    MUTED,
                ))
                .child(display(&bytes(total), 34.0, MINT).truncate())
                .child(label(
                    if s.map_cleanup {
                        "Aucune suppression ici".into()
                    } else {
                        format!("{} fichiers", grouped(node.files))
                    },
                    12.0,
                    500.0,
                    SOFT,
                )),
        );
    let layout = col()
        .gap(10.0)
        .child(toolbar)
        .child(hero)
        .child(heavy_rule());
    let layout = if total > 0 && offset == 0 {
        layout.child(
            row()
                .gap(12.0)
                .child(if ascending {
                    let (_, leaders) =
                        tree.ranked_children(s.tool_node, s.map_cleanup, false, 0, 5);
                    map_blocks(s, &leaders, total)
                } else {
                    map_blocks(s, &children, total)
                })
                .child(map_inspector(s, tree, &node, total)),
        )
    } else {
        layout
    };
    let mut block = col().gap(6.0).child(
        row()
            .center_y()
            .gap(8.0)
            .child(label(
                if s.map_cleanup {
                    "Caches par emplacement"
                } else {
                    "Contenu du dossier"
                },
                17.0,
                680.0,
                INK,
            ))
            .child(spacer())
            .child(label(
                format!("{} éléments", grouped(count as u64)),
                11.0,
                560.0,
                MUTED,
            )),
    );
    if tree.summary.denied > 0 || tree.summary.cloud_files > 0 {
        let mut notes = Vec::new();
        if tree.summary.denied > 0 {
            notes.push(format!("{} accès refusés", grouped(tree.summary.denied)));
        }
        if tree.summary.cloud_files > 0 {
            notes.push(format!(
                "{} fichiers cloud non téléchargés",
                grouped(tree.summary.cloud_files)
            ));
        }
        block = block.child(
            row()
                .gap(6.0)
                .center_y()
                .child(el().size(6.0, 6.0).radius(3.0).fill(c(AMBER)))
                .child(
                    label(
                        format!("Sur l’ensemble du scan : {}", notes.join(" · ")),
                        11.0,
                        530.0,
                        SOFT,
                    )
                    .wrap(),
                ),
        );
    }
    if count == 0 {
        return layout.child(block.child(label(
            if s.map_cleanup {
                "Aucun cache détecté dans ce dossier."
            } else {
                "Aucun élément à afficher dans ce dossier."
            },
            13.0,
            550.0,
            SOFT,
        )));
    }
    if count > PER_PAGE {
        block = block.child(
            row()
                .gap(6.0)
                .center_y()
                .child(label(
                    format!("{}–{} sur {count}", offset + 1, offset + children.len()),
                    12.0,
                    550.0,
                    SOFT,
                ))
                .child(spacer())
                .child(
                    button(
                        "map-prev",
                        "Précédent",
                        None,
                        Button::Quiet,
                        offset > 0,
                        |s| {
                            s.tool_page = s.tool_page.saturating_sub(1);
                            s.map_detail = None;
                        },
                    )
                    .h(29.0),
                )
                .child(
                    button(
                        "map-next",
                        "Suivant",
                        None,
                        Button::Quiet,
                        offset + children.len() < count,
                        |s| {
                            s.tool_page += 1;
                            s.map_detail = None;
                        },
                    )
                    .h(29.0),
                ),
        );
    }
    block = block.child(rule()).child(
        row()
            .gap(8.0)
            .center_y()
            .px(7.0)
            .child(overline("ÉLÉMENT", MUTED).grow(1.0).min_w(0.0))
            .child(
                overline(
                    if s.map_cleanup {
                        "PART DES CACHES"
                    } else {
                        "PART DU DOSSIER"
                    },
                    MUTED,
                )
                .w(155.0),
            )
            .child(size_sort(s, true).w(88.0).px(5.0))
            .child(el().w(34.0)),
    );
    for child in &children {
        block = block.child(map_line(s, child, total));
    }
    layout.child(block)
}

fn map_blocks(s: &State, entries: &[explorer::NodeView], total: u64) -> El {
    let mut nodes = entries
        .iter()
        .take(5)
        .filter(|e| map_amount(e, s.map_cleanup) as f64 / total as f64 >= 0.06);
    let Some(first) = nodes.next() else {
        return col()
            .grow(1.0)
            .min_w(0.0)
            .border(1.0, c(INK))
            .padding(13.0)
            .center()
            .child(
                label(
                    "L’espace est réparti entre de nombreux petits éléments.",
                    12.0,
                    570.0,
                    SOFT,
                )
                .wrap(),
            );
    };
    let rest = nodes.collect::<Vec<_>>();
    let first_size = map_amount(first, s.map_cleanup);
    let first_share = (first_size as f64 / total as f64).clamp(0.30, 0.74) as f32;
    let mut visual = row()
        .grow(1.0)
        .min_w(0.0)
        .h(168.0)
        .gap(3.0)
        .padding(3.0)
        .border(1.0, c(INK));
    let has_other =
        !rest.is_empty() || total.saturating_sub(first_size) as f64 / total as f64 >= 0.06;
    visual = visual.child(map_tile(first, first_size, 0).grow(if has_other {
        first_share
    } else {
        1.0
    }));
    if has_other {
        let mut column = col().grow(1.0 - first_share).min_h(0.0).min_w(0.0).gap(3.0);
        let represented = rest.iter().fold(first_size, |sum, n| {
            sum.saturating_add(map_amount(n, s.map_cleanup))
        });
        let remainder = total.saturating_sub(represented);
        let show_remainder = remainder as f64 / total as f64 >= 0.06 || rest.is_empty();
        for (i, pair) in rest.chunks(2).enumerate() {
            let pair_size = pair.iter().fold(0u64, |sum, n| {
                sum.saturating_add(map_amount(n, s.map_cleanup))
            });
            let rest_size = total.saturating_sub(first_size).max(1);
            let mut line = row()
                .grow((pair_size as f32 / rest_size as f32).max(0.25))
                .min_h(0.0)
                .gap(3.0);
            for (j, entry) in pair.iter().enumerate() {
                let size = map_amount(entry, s.map_cleanup);
                line = line.child(
                    map_tile(entry, size, i * 2 + j + 1)
                        .grow((size as f32 / pair_size.max(1) as f32).max(0.25)),
                );
            }
            column = column.child(line);
        }
        if show_remainder {
            column = column.child(
                col()
                    .grow(
                        (remainder as f32 / total.saturating_sub(first_size).max(1) as f32)
                            .max(0.25),
                    )
                    .min_h(0.0)
                    .min_w(0.0)
                    .padding(8.0)
                    .fill(c("#d8d3c8"))
                    .child(label("Autres éléments", 11.0, 740.0, BLACK).truncate())
                    .child(spacer())
                    .child(display(&bytes(remainder), 18.0, BLACK).truncate()),
            );
        }
        visual = visual.child(column);
    }
    visual
}

fn map_tile(entry: &explorer::NodeView, size: u64, index: usize) -> El {
    let id = entry.id;
    let title = if entry.kind == explorer::NodeKind::Small {
        "Petits fichiers"
    } else {
        &entry.name
    };
    let (fill, ink) = match index {
        0 => (RED, WHITE),
        1 => (BLACK, WHITE),
        _ => ("#d8d3c8", BLACK),
    };
    let mut tile = col()
        .key(format!("map-tile-{id}"))
        .min_w(0.0)
        .min_h(0.0)
        .padding(8.0)
        .fill(c(fill))
        .child(label(title, 11.0, 740.0, ink).truncate())
        .child(spacer())
        .child(display(&bytes(size), 20.0, ink).truncate());
    if entry.kind != explorer::NodeKind::Small {
        tile = tile
            .role("button")
            .label(format!("Inspecter {title}"))
            .focusable(true)
            .focus_ring(2.0, 1.0, c(MINT))
            .cursor(Cursor::Pointer)
            .hover(Color::WHITE.alpha(0.12))
            .on_press(move |s: &mut State| s.map_detail = Some(id));
    }
    tile
}

fn map_inspector(s: &State, tree: &explorer::Tree, node: &explorer::NodeView, total: u64) -> El {
    let selected = s.map_detail.and_then(|id| tree.view(id, 0, 0));
    let info = selected.as_ref().unwrap_or(node);
    let share = (map_amount(info, s.map_cleanup) as f64 / total as f64).clamp(0.0, 1.0) as f32;
    let mut panel = col()
        .w(186.0)
        .padding(11.0)
        .gap(6.0)
        .fill(c("#1a2638"))
        .child(overline("INSPECTEUR / SÉLECTION", MINT))
        .child(label(&info.name, 15.0, 720.0, INK).truncate())
        .child(display(&bytes(map_amount(info, s.map_cleanup)), 24.0, INK).truncate())
        .child(map_meter(share, 5.0, MINT))
        .child(
            label(
                format!(
                    "{} fichiers · {:.0} % du dossier",
                    grouped(info.files),
                    share * 100.0
                ),
                11.0,
                520.0,
                SOFT,
            )
            .truncate(),
        );
    if let Some(entry) = selected {
        let id = entry.id;
        if entry.kind == explorer::NodeKind::Dir && !entry.denied && entry.mount.is_none() {
            panel = panel.child(
                button(
                    "map-inspector-enter",
                    "Explorer",
                    Some(&ARROW),
                    Button::Primary,
                    true,
                    move |s| s.map_enter(id),
                )
                .h(29.0),
            );
        }
    } else {
        panel = panel.child(
            label(
                "Sélectionnez une zone pour l’inspecter.",
                11.0,
                500.0,
                MUTED,
            )
            .wrap(),
        );
    }
    panel
}

fn size_sort(s: &State, map: bool) -> El {
    let (title, ascending) = match s.sort_size {
        Some(true) => ("Taille ↑", false),
        Some(false) => ("Taille ↓", true),
        None if map => ("Taille ↓", true),
        None => ("Trier par taille", false),
    };
    button("size-sort", title, None, Button::Quiet, true, move |s| {
        s.set_size_sort(ascending)
    })
    .help(if ascending {
        "Afficher du plus petit au plus grand"
    } else {
        "Afficher du plus grand au plus petit"
    })
    .h(29.0)
}

fn sort_by_size<T>(rows: &mut [T], ascending: Option<bool>, size: impl Fn(&T) -> Option<u64>) {
    if let Some(ascending) = ascending {
        rows.sort_by(|a, b| match (size(a), size(b)) {
            (Some(a), Some(b)) => {
                if ascending {
                    a.cmp(&b)
                } else {
                    b.cmp(&a)
                }
            }
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });
    }
}

#[cfg(test)]
#[test]
fn sizes_sort_in_both_directions_and_unknown_stays_last() {
    let mut rows = [(0, None), (1, Some(600)), (2, Some(0)), (3, Some(600))];
    sort_by_size(&mut rows, Some(false), |row| row.1);
    assert_eq!(rows.map(|r| r.0), [1, 3, 2, 0]);
    sort_by_size(&mut rows, Some(true), |row| row.1);
    assert_eq!(rows.map(|r| r.0), [2, 1, 3, 0]);
}

fn grouped(n: u64) -> String {
    let n = n.to_string();
    let mut out = String::with_capacity(n.len() + n.len() / 3);
    for (i, digit) in n.chars().enumerate() {
        if i > 0 && (n.len() - i).is_multiple_of(3) {
            out.push('\u{202f}');
        }
        out.push(digit);
    }
    out
}

fn hidden_settings(kind: epic_disk_core::hidden::PartKind) -> Option<&'static str> {
    use epic_disk_core::hidden::PartKind::*;
    match kind {
        ShadowCopies => Some("SystemPropertiesProtection.exe"),
        PageFile | SwapFile => Some("SystemPropertiesAdvanced.exe"),
        ReservedStorage | HiberFile | RecycleBin => Some("ms-settings:storagesense"),
        _ => None,
    }
}

fn hidden_name(kind: epic_disk_core::hidden::PartKind) -> &'static str {
    use epic_disk_core::hidden::PartKind::*;
    match kind {
        ShadowCopies => "Points de restauration et instantanés",
        ReservedStorage => "Stockage réservé par Windows",
        PageFile => "Fichier d’échange",
        HiberFile => "Fichier de veille prolongée",
        SwapFile => "Fichier de pagination des applications",
        Mft => "Table de fichiers NTFS",
        RecycleBin => "Corbeille",
        VirtualDisk => "Disque virtuel",
        Unreadable => "Dossiers inaccessibles",
        CloudOnly => "Fichiers cloud non téléchargés",
    }
}

fn map_amount(node: &explorer::NodeView, cleanup: bool) -> u64 {
    if cleanup {
        node.reclaim_safe.saturating_add(node.reclaim_review)
    } else {
        node.bytes
    }
}

fn map_line(s: &State, n: &explorer::NodeView, total: u64) -> El {
    use explorer::NodeKind::*;
    let id = n.id;
    let details = s.map_detail == Some(id);
    let title = if n.kind == Small {
        "Petits fichiers regroupés"
    } else {
        &n.name
    };
    let kind = match n.kind {
        Dir if n.denied => "Accès refusé".into(),
        Dir if n.mount.is_some() => "Point de montage non parcouru".into(),
        Dir | Small => format!("{} fichier(s)", n.files),
        File => "Fichier".into(),
    };
    let amount = map_amount(n, s.map_cleanup);
    let share = if total == 0 {
        0.0
    } else {
        (amount as f64 / total as f64).clamp(0.0, 1.0)
    };
    let percent = if share > 0.0 && share < 0.001 {
        "< 0,1 %".into()
    } else {
        format!("{:.1} %", 100.0 * share).replace('.', ",")
    };
    let navigable = n.kind == Dir && !n.denied && n.mount.is_none();
    let mut identity = row()
        .key(format!("map-name-{id}"))
        .grow(1.0)
        .min_w(0.0)
        .h(46.0)
        .gap(7.0)
        .center_y()
        .radius(6.0)
        .child(pic(
            if n.kind == Dir { &FOLDER } else { &FILE },
            17.0,
            if n.kind == Dir { MINT } else { SOFT },
        ))
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(2.0)
                .child(label(title, 13.0, 660.0, INK).truncate().help(title))
                .child(label(kind, 11.0, 480.0, MUTED).truncate()),
        );
    if navigable {
        identity = identity
            .role("button")
            .label(format!("Explorer {title}"))
            .focusable(true)
            .focus_ring(2.0, 1.0, c(MINT))
            .cursor(Cursor::Pointer)
            .hover(c(MINT).alpha(0.09))
            .on_press(move |s: &mut State| s.map_enter(id))
            .child(pic(&ARROW, 14.0, MUTED));
    }
    let meter = el().size(95.0, 7.0).radius(3.5).fill(c(EDGE)).child(
        el().absolute(0.0, 0.0)
            .size(95.0 * share as f32, 7.0)
            .radius(3.5)
            .fill(c(MINT)),
    );
    let mut line = row()
        .gap(8.0)
        .px(7.0)
        .center_y()
        .child(identity)
        .child(
            row()
                .w(155.0)
                .gap(7.0)
                .center_y()
                .child(meter)
                .child(label(percent, 11.0, 550.0, SOFT)),
        )
        .child(label(bytes(amount), 12.0, 700.0, INK).w(88.0));
    if n.kind == Small {
        line = line.child(el().w(34.0));
    } else {
        line = line.child(
            button(
                &format!("map-detail-{id}"),
                if details { "×" } else { "…" },
                None,
                Button::Quiet,
                true,
                move |s| {
                    s.map_detail = if s.map_detail == Some(id) {
                        None
                    } else {
                        Some(id)
                    };
                },
            )
            .label(format!("Détails de {title}"))
            .help("Détails et actions")
            .w(34.0)
            .h(29.0)
            .px(0.0)
            .center(),
        );
    }
    let mut item = col()
        .key(format!("map-line-{id}"))
        .child(rule())
        .child(line);
    if details && n.kind != Small {
        let path = std::path::PathBuf::from(&n.path);
        let inspect = path.clone();
        let open = path.clone();
        let caption = readable_path(&path).into_owned();
        let copy = caption.clone();
        item = item.child(
            col()
                .padding(9.0)
                .gap(7.0)
                .fill(c(BG))
                .radius(7.0)
                .child(
                    label(&caption, 11.0, 520.0, SOFT)
                        .min_w(0.0)
                        .truncate()
                        .help(caption.clone()),
                )
                .child(
                    row()
                        .gap(6.0)
                        .child(
                            button(
                                "map-inspect",
                                "Utilisé par ?",
                                None,
                                Button::Quiet,
                                !s.demo && !n.unreadable_name,
                                move |s| s.tool_request(vec![Action::Inspect(inspect.clone())]),
                            )
                            .h(29.0),
                        )
                        .child(
                            button(
                                "map-open",
                                "Ouvrir dans l’Explorateur",
                                None,
                                Button::Quiet,
                                !s.demo && !n.unreadable_name,
                                move |s| s.tool_request(vec![Action::Open(open.clone())]),
                            )
                            .h(29.0),
                        )
                        .child(
                            button(
                                "map-copy",
                                "Copier le chemin",
                                None,
                                Button::Quiet,
                                true,
                                |_| {},
                            )
                            .h(29.0)
                            .on_press_tx(move |_: &mut State, tx| tx.copy(copy.clone())),
                        ),
                ),
        );
    }
    item
}

fn settings_form(s: &State) -> El {
    let format = |d| {
        if d == DeleteMethod::Recycle {
            "Corbeille"
        } else {
            "Définitif"
        }
    };
    let method = |d| {
        if d == DeleteMethod::Recycle {
            DeleteMethod::Permanent
        } else {
            DeleteMethod::Recycle
        }
    };
    card()
        .padding(12.0)
        .gap(9.0)
        .child(label("Préférences de nettoyage", 17.0, 670.0, INK))
        .child(
            row()
                .gap(7.0)
                .center_y()
                .child(
                    button(
                        "cfg-theme",
                        match s.config.theme.as_str() {
                            "light" => "Thème : clair",
                            "dark" => "Thème : sombre",
                            _ => "Thème : système",
                        },
                        None,
                        Button::Quiet,
                        s.config_valid && !s.demo,
                        |s| {
                            s.config.theme = match s.config.theme.as_str() {
                                "light" => "dark",
                                "dark" => "system",
                                _ => "light",
                            }
                            .into();
                            s.persist_settings();
                        },
                    )
                    .h(30.0),
                )
                .child(
                    button(
                        "motion",
                        if s.motion {
                            "Animations : oui"
                        } else {
                            "Animations : non"
                        },
                        None,
                        Button::Quiet,
                        !s.demo,
                        |s| s.motion = !s.motion,
                    )
                    .h(30.0),
                ),
        )
        .child(
            row()
                .gap(7.0)
                .child(
                    button(
                        "cfg-cache",
                        &format!("Caches : {}", format(s.config.cache_delete)),
                        None,
                        Button::Quiet,
                        !s.demo,
                        move |s| {
                            s.config.cache_delete = method(s.config.cache_delete);
                            s.persist_settings();
                        },
                    )
                    .h(30.0),
                )
                .child(
                    button(
                        "cfg-files",
                        &format!("Fichiers : {}", format(s.config.files_delete)),
                        None,
                        Button::Quiet,
                        !s.demo,
                        move |s| {
                            s.config.files_delete = method(s.config.files_delete);
                            s.persist_settings();
                        },
                    )
                    .h(30.0),
                )
                .child(
                    button(
                        "cfg-risk",
                        if s.config.show_caution {
                            "Prudence : visible"
                        } else {
                            "Prudence : masqué"
                        },
                        None,
                        Button::Quiet,
                        !s.demo,
                        |s| {
                            s.config.show_caution = !s.config.show_caution;
                            s.persist_settings();
                        },
                    )
                    .h(30.0),
                ),
        )
        .child(
            row()
                .gap(7.0)
                .child(
                    button(
                        "cfg-size",
                        &format!("Gros ≥ {} Mio", s.config.large_min_mb),
                        None,
                        Button::Quiet,
                        !s.demo,
                        |s| {
                            s.config.large_min_mb =
                                next_value(s.config.large_min_mb, &[10, 50, 100, 500, 1024]);
                            s.persist_settings();
                        },
                    )
                    .h(30.0),
                )
                .child(
                    button(
                        "cfg-dup",
                        &format!("Doublons ≥ {} Kio", s.config.duplicate_min_kb),
                        None,
                        Button::Quiet,
                        !s.demo,
                        |s| {
                            s.config.duplicate_min_kb =
                                next_value(s.config.duplicate_min_kb, &[4, 64, 256, 1024]);
                            s.persist_settings();
                        },
                    )
                    .h(30.0),
                )
                .child(
                    button(
                        "cfg-age",
                        &format!("Installateurs ≥ {} j", s.config.installer_age_days),
                        None,
                        Button::Quiet,
                        !s.demo,
                        |s| {
                            s.config.installer_age_days = next_value(
                                u64::from(s.config.installer_age_days),
                                &[7, 14, 30, 60, 90],
                            ) as u32;
                            s.persist_settings();
                        },
                    )
                    .h(30.0),
                ),
        )
        .child(label(
            "Exclusions · un chemin absolu par ligne",
            12.0,
            600.0,
            SOFT,
        ))
        .child(
            text_input(&s.exclusions, TextStyle::new(13.0, 480.0))
                .color(c(INK))
                .fill(c(BG))
                .border(1.0, c(EDGE))
                .label("Chemins exclus")
                .radius(7.0)
                .h(70.0)
                .padding(7.0)
                .on_edit(|s: &mut State| &mut s.exclusions),
        )
        .child(label(
            "Dossiers de projets · un chemin absolu par ligne",
            12.0,
            600.0,
            SOFT,
        ))
        .child(
            text_input(&s.project_roots, TextStyle::new(13.0, 480.0))
                .color(c(INK))
                .fill(c(BG))
                .border(1.0, c(EDGE))
                .label("Dossiers de projets")
                .radius(7.0)
                .h(52.0)
                .padding(7.0)
                .on_edit(|s: &mut State| &mut s.project_roots),
        )
        .child(
            row().child(
                button(
                    "cfg-save",
                    "Enregistrer les chemins",
                    Some(&CHECK),
                    Button::Primary,
                    s.config_valid && !s.demo,
                    State::persist_settings,
                )
                .h(31.0),
            ),
        )
}

fn next_value(n: u64, steps: &[u64]) -> u64 {
    steps.iter().copied().find(|x| *x > n).unwrap_or(steps[0])
}

fn tool_modal(s: &State) -> El {
    let planned = &s.tool_pending;
    let batch = planned.iter().all(Action::selectable);
    let mode = if s.tool == Tool::Apps {
        DeleteMethod::Recycle
    } else if matches!(s.tool, Tool::Caches | Tool::Projects | Tool::Games) {
        s.config.cache_delete
    } else {
        s.config.files_delete
    };
    let action = if batch {
        "Confirmer le nettoyage"
    } else {
        planned[0].label()
    };
    let explanation = if batch {
        let transfer = if mode == DeleteMethod::Recycle {
            "vers la Corbeille, si le volume le permet"
        } else {
            "de manière définitive pour les fichiers et caches concernés"
        };
        format!(
            "{} élément(s) sélectionné(s). Le moteur vérifie de nouveau les protections. Transfert {transfer}. Les répertoires vides sont retirés seulement s’ils sont encore vides.",
            planned.len()
        )
    } else if matches!(planned[0], Action::Uninstall(_)) {
        "EpicCleaner lance le désinstalleur officiel, puis recherche les résidus. Les dossiers attribués à l’application seront proposés à la Corbeille avec une confirmation séparée.".into()
    } else {
        "Cette opération modifie les données ou la configuration indiquées. Annulez si vous ne reconnaissez pas l’action.".into()
    };
    let mut dialog = card()
        .key("tool-modal")
        .w(480.0)
        .padding(20.0)
        .gap(11.0)
        .modal(false, |s: &mut State| s.tool_pending.clear())
        .child(row().child(badge("CONFIRMATION", AMBER)))
        .child(label(action, 21.0, 700.0, INK))
        .child(label(explanation, 13.0, 500.0, SOFT).wrap());
    if batch {
        for action in planned.iter().take(4) {
            let (title, detail) = crate::tools::action_preview(action);
            dialog = dialog.child(label(title, 13.0, 650.0, INK).truncate().help(detail));
        }
        if planned.len() > 4 {
            dialog = dialog.child(label(
                format!("… et {} autres sélection(s)", planned.len() - 4),
                11.0,
                550.0,
                SOFT,
            ));
        }
    } else {
        let (title, detail) = crate::tools::action_preview(&planned[0]);
        dialog = dialog.child(label(title, 13.0, 650.0, INK).wrap().help(detail));
    }
    el().key("tool-scrim")
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .center()
        .fill(Color::BLACK.alpha(0.68))
        .child(
            dialog.child(
                row()
                    .gap(8.0)
                    .child(spacer())
                    .child(
                        button("tool-dismiss", "Annuler", None, Button::Quiet, true, |s| {
                            s.tool_pending.clear()
                        })
                        .default_button(),
                    )
                    .child(button(
                        "tool-confirm",
                        "Confirmer",
                        None,
                        Button::Primary,
                        !s.demo,
                        State::tool_confirm,
                    )),
            ),
        )
}

fn confirm(s: &State) -> El {
    el().key("confirm-scrim").absolute(0.0, 0.0).w_full().h_full().center().fill(Color::BLACK.alpha(0.68))
        .child(card().key("confirm-modal").w(445.0).padding(21.0).gap(13.0).modal(false, |s: &mut State| s.confirm = false)
            .child(row().child(badge("CONFIRMATION", AMBER)))
            .child(label("Envoyer à la Corbeille ?", 23.0, 710.0, INK))
            .child(label(format!("Vous avez sélectionné {} fichier(s). Chaque fichier est revérifié avant le transfert.", s.selected.len()), 14.0, 500.0, SOFT).wrap())
            .child(label("La Corbeille permet de restaurer ces fichiers. L’espace ne sera pas libéré tant qu’elle ne sera pas vidée.", 13.0, 500.0, SOFT).wrap())
            .child(row().gap(9.0).child(spacer())
                .child(button("dismiss", "Annuler", None, Button::Quiet, true, |s| s.confirm = false).default_button())
                .child(button("recycle", "Envoyer à la Corbeille", Some(&CLEAN), Button::Primary, true, State::recycle))))
}

pub fn view(s: &State, cx: &mut Cx<State>) -> El {
    LIGHT.set(match s.config.theme.as_str() {
        "light" => true,
        "dark" => false,
        _ => !cx.system_dark() && !s.demo,
    });
    cx.reduced_motion(!s.motion);
    let body = col()
        .key(
            if s.page == Page::Tools
                && s.tool == Tool::Map
                && s.tool_output
                    .as_ref()
                    .is_some_and(|output| output.tree.is_some())
            {
                format!(
                    "workspace-map-{}-{}-{}-{}",
                    s.tool_node, s.tool_page, s.map_cleanup, s.tool_gap
                )
            } else if s.page == Page::Tools {
                format!("workspace-{:?}", s.tool)
            } else {
                format!("workspace-{}", s.page as u8)
            },
        )
        .grow(1.0)
        .min_h(0.0)
        .w_full()
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .scroll_edge_fade(10.0)
        .child(
            col()
                .w_full()
                .px(26.0)
                .pt(18.0)
                .pb(22.0)
                .child(workspace(s)),
        );
    let main = col()
        .grow(1.0)
        .min_w(0.0)
        .h_full()
        .child(header(s))
        .child(rule())
        .child(body)
        .child(rule())
        .child(
            row()
                .h(32.0)
                .px(26.0)
                .center_y()
                .gap(6.0)
                .child(el().size(5.0, 5.0).radius(3.0).fill(c(MINT)))
                .child(label(
                    if s.job.is_some() {
                        "Opération en cours"
                    } else {
                        "Prêt"
                    },
                    11.0,
                    550.0,
                    MUTED,
                ))
                .child(spacer())
                .child(label(
                    concat!("EpicCleaner  ", env!("CARGO_PKG_VERSION")),
                    11.0,
                    540.0,
                    MUTED,
                )),
        );
    let mut root = row()
        .key("epic-root")
        .w_full()
        .h_full()
        .fill(c(BG))
        .child(sidebar(s))
        .child(el().w(1.0).h_full().fill(c(EDGE)))
        .child(main);
    if !s.tool_pending.is_empty() {
        root = root.child(tool_modal(s));
    } else if s.confirm {
        root = root.child(confirm(s));
    } else if s.job.is_none()
        && !s.demo
        && (matches!(s.page, Page::Home | Page::Space | Page::Duplicates)
            || (s.page == Page::Tools && s.tool.needs_root()))
    {
        root = root.on_drop_files(|s: &mut State, paths, _| {
            if paths.len() != 1 {
                if s.page == Page::Home {
                    s.tool_select(Tool::Map);
                }
                s.set_error("Déposez un seul dossier à la fois.".into());
            } else if let Some(path) = paths.first() {
                if s.page == Page::Home {
                    s.tool_select(Tool::Map);
                }
                s.start(path.clone());
            }
        });
    }
    root
}
