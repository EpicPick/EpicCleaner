//! Native UI adapter for the disk engine. Scans produce bounded display rows and
//! typed actions. Neither search text nor any displayed path is evaluated as a command.
use disk::{
    Job, apps, artifacts,
    clean::{self, CleanReport},
    duplicates, empty, explorer, hidden, history, installers, junk, locks, organize,
    paths::{Guard, Origin},
    rules::{self, Risk, TargetRule},
    settings::{self, Settings},
    shell, startup, steam, system,
};
use epic_disk_core as disk;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::SystemTime,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Caches,
    Map,
    Large,
    Duplicates,
    Empty,
    Projects,
    Downloads,
    Installers,
    Models,
    Assistants,
    Games,
    Apps,
    Startup,
    Windows,
    Settings,
}

pub const TOOLS: &[(Tool, &str)] = &[
    (Tool::Caches, "Caches et résidus"),
    (Tool::Map, "Carte du disque"),
    (Tool::Large, "Gros fichiers"),
    (Tool::Duplicates, "Doublons"),
    (Tool::Empty, "Dossiers vides"),
    (Tool::Projects, "Développeurs"),
    (Tool::Downloads, "Téléchargements"),
    (Tool::Installers, "Installateurs"),
    (Tool::Models, "Modèles IA"),
    (Tool::Assistants, "Assistants IA"),
    (Tool::Games, "Jeux Steam"),
    (Tool::Apps, "Applications"),
    (Tool::Startup, "Démarrage"),
    (Tool::Windows, "Outils Windows"),
    (Tool::Settings, "Réglages"),
];

impl Tool {
    pub fn label(self) -> &'static str {
        TOOLS.iter().find(|t| t.0 == self).unwrap().1
    }
    pub fn needs_root(self) -> bool {
        matches!(
            self,
            Self::Map | Self::Large | Self::Duplicates | Self::Empty
        )
    }
}

#[derive(Clone)]
pub enum Action {
    Rule(Box<TargetRule>),
    AdminRule(String, String),
    Path(PathBuf, Origin, Fingerprint),
    Duplicate(PathBuf, Fingerprint, PathBuf, Fingerprint),
    Ollama(String),
    Empty(PathBuf),
    Inspect(PathBuf),
    Open(PathBuf),
    Uninstall(String),
    Broken(String),
    Startup(startup::StartupSource, String, bool),
    Close(PathBuf, u32, u64),
    Windows(SystemAction),
    ExplorerMenu(bool),
    Shortcut,
    StartShortcut,
    JumpList,
    Export(Vec<String>),
    ShellPage(&'static str),
    ClearHistory,
}

#[derive(Clone)]
pub struct Fingerprint {
    len: u64,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
    directory: bool,
}

impl Fingerprint {
    fn read(path: &Path) -> Option<Self> {
        let m = std::fs::symlink_metadata(path).ok()?;
        if disk::fsmeta::is_link(&m.file_type()) || disk::fsmeta::is_cloud_only(&m) {
            return None;
        }
        Some(Self {
            len: m.len(),
            modified: m.modified().ok(),
            created: m.created().ok(),
            directory: m.is_dir(),
        })
    }
    fn matches(&self, path: &Path) -> bool {
        Self::read(path).is_some_and(|other| {
            self.len == other.len
                && self.modified == other.modified
                && self.created == other.created
                && self.directory == other.directory
        })
    }
}

#[derive(Clone, Copy)]
pub enum SystemAction {
    Bin,
    Dism,
    Delivery,
    HibernateOff,
    HibernateOn,
    Previous,
}

impl Action {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Rule(_) | Self::Path(..) | Self::Duplicate(..) => "Nettoyer",
            Self::AdminRule(..) => "Nettoyer avec UAC",
            Self::Ollama(_) => "Retirer avec Ollama",
            Self::Empty(_) => "Retirer le dossier vide",
            Self::Inspect(_) => "Utilisé par ?",
            Self::Open(_) | Self::ShellPage(_) => "Ouvrir",
            Self::Uninstall(_) => "Désinstaller",
            Self::Broken(_) => "Sauvegarder et retirer",
            Self::Startup(_, _, true) => "Activer",
            Self::Startup(..) => "Désactiver",
            Self::Close(..) => "Demander la fermeture",
            Self::Windows(SystemAction::Bin) => "Vider la Corbeille",
            Self::Windows(_) => "Exécuter",
            Self::ExplorerMenu(true) => "Activer le menu",
            Self::ExplorerMenu(false) => "Retirer le menu",
            Self::Shortcut => "Créer le raccourci",
            Self::StartShortcut => "Ajouter au menu Démarrer",
            Self::JumpList => "Ajouter à la barre des tâches",
            Self::Export(_) => "Exporter le plan PowerShell",
            Self::ClearHistory => "Effacer l’historique",
        }
    }
    pub fn confirm(&self) -> bool {
        !matches!(
            self,
            Self::Inspect(_) | Self::Open(_) | Self::ShellPage(_) | Self::Export(_)
        )
    }
    pub fn selectable(&self) -> bool {
        matches!(
            self,
            Self::Rule(_) | Self::Path(..) | Self::Duplicate(..) | Self::Empty(_)
        )
    }
    pub fn rule_id(&self) -> Option<&str> {
        match self {
            Self::Rule(rule) => Some(&rule.id),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct Entry {
    pub title: String,
    pub detail: String,
    pub path: Option<PathBuf>,
    pub bytes: Option<u64>,
    pub risk: Option<Risk>,
    pub action: Option<Action>,
}

impl Entry {
    fn new(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            detail: detail.into(),
            path: None,
            bytes: None,
            risk: None,
            action: None,
        }
    }
    fn at(mut self, path: impl Into<PathBuf>, bytes: u64) -> Self {
        self.path = Some(path.into());
        self.bytes = Some(bytes);
        self
    }
    fn do_(mut self, action: Action) -> Self {
        self.action = Some(action);
        self
    }
    fn risk(mut self, risk: Risk) -> Self {
        self.risk = Some(risk);
        self
    }
    fn picked(mut self, guard: &Guard, origin: Origin) -> Self {
        if let Some(path) = &self.path
            && guard.check(path, origin).is_ok()
            && let Some(mark) = Fingerprint::read(path)
        {
            self.action = Some(Action::Path(path.clone(), origin, mark));
        }
        self
    }
}

#[derive(Default)]
pub struct Output {
    pub note: String,
    pub rows: Vec<Entry>,
    pub tree: Option<explorer::Tree>,
    pub gap: Option<hidden::HiddenSpace>,
    pub images: HashMap<usize, Vec<u8>>,
    pub preselected: Vec<usize>,
}

pub fn guard(settings: &Settings, base: &Path) -> Guard {
    Guard::new(&settings.exclusions, Some(base))
}
fn name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map_or_else(|| path.into(), |p| p.to_string_lossy().into_owned())
}
fn date(t: i64) -> String {
    chrono::DateTime::from_timestamp(t, 0)
        .filter(|_| t > 0)
        .map_or_else(
            || "date inconnue".into(),
            |t| t.format("%d/%m/%Y").to_string(),
        )
}

type Result<T> = std::result::Result<T, String>;
fn msg(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn matches_duplicate(
    path: &Path,
    mark: &Fingerprint,
    keep: &Path,
    kept: &Fingerprint,
    job: &Job,
) -> bool {
    fn digest(path: &Path, job: &Job) -> Option<blake3::Hash> {
        use std::io::Read;
        let mut file = std::fs::File::open(path).ok()?;
        let mut hash = blake3::Hasher::new();
        let mut buffer = [0u8; 128 * 1024];
        while !job.is_cancelled() {
            let n = file.read(&mut buffer).ok()?;
            if n == 0 {
                return Some(hash.finalize());
            }
            hash.update(&buffer[..n]);
        }
        None
    }
    if crate::engine::checked_path(path).is_err() || crate::engine::checked_path(keep).is_err() {
        return false;
    }
    mark.matches(path)
        && kept.matches(keep)
        && digest(keep, job).is_some_and(|expected| digest(path, job) == Some(expected))
        && mark.matches(path)
        && kept.matches(keep)
}

pub fn scan(
    tool: Tool,
    root: Option<PathBuf>,
    settings: &Settings,
    base: &Path,
    job: &Job,
) -> Result<Output> {
    let guard = guard(settings, base);
    if let Some(given) = &root {
        let resolved = crate::engine::checked_path(given).map_err(msg)?;
        if !resolved.is_dir() {
            return Err("La sélection doit être un dossier local ordinaire.".into());
        }
    }
    let required = || {
        root.as_deref()
            .filter(|p| p.is_dir())
            .ok_or_else(|| "Choisissez un dossier ou un lecteur.".to_string())
    };
    let mut out = Output::default();
    out.rows = match tool {
        Tool::Caches => {
            let (custom, warnings) = rules::load_user_targets(&base.join("rules"));
            let mut seen = HashSet::new();
            let all: Vec<_> = rules::builtin_targets()
                .iter()
                .cloned()
                .chain(custom)
                .filter(|r| settings.show_caution || r.risk != Risk::Caution)
                .filter(|r| seen.insert(r.id.clone()))
                .collect();
            let by_id: HashMap<_, _> = all.iter().map(|rule| (rule.id.as_str(), rule)).collect();
            out.note = format!(
                "{} règles actives. Les applications ouvertes bloquent le nettoyage de leurs caches. {} règle(s) personnalisée(s) ignorée(s).",
                all.len(),
                warnings.len()
            );
            junk::scan_targets(&all, job)
                .into_iter()
                .filter_map(|hit| {
                    let rule = *by_id.get(hit.id.as_str())?;
                    if hit.found.is_empty() && hit.bytes == 0 && !hit.command_available {
                        return None;
                    }
                    let why = if !hit.running.is_empty() {
                        format!(" · Fermer : {}", hit.running.join(", "))
                    } else if rule.admin {
                        " · Administrateur requis".into()
                    } else if rule.mode == rules::Mode::Report {
                        " · Mesure seule".into()
                    } else {
                        String::new()
                    };
                    let where_ = hit
                        .found
                        .iter()
                        .map(|p| p.path.as_str())
                        .collect::<Vec<_>>()
                        .join(" | ");
                    let mut entry = Entry::new(
                        &rule.name,
                        format!(
                            "{:?} · {} fichiers{}{}\n{}",
                            rule.group,
                            hit.files,
                            why,
                            if hit.partial { " · partiel" } else { "" },
                            where_
                        ),
                    )
                    .risk(rule.risk);
                    entry.bytes = Some(hit.bytes);
                    if hit.files > 0
                        && hit.running.is_empty()
                        && !rule.admin
                        && rule.mode != rules::Mode::Report
                    {
                        let mut approved = rule.clone();
                        approved.paths = hit
                            .found
                            .iter()
                            .map(|p| p.path.clone())
                            .filter(|p| !p.contains(['*', '?', '%', '\u{fffd}']))
                            .collect();
                        if !approved.paths.is_empty() {
                            entry.action = Some(Action::Rule(Box::new(approved)));
                        }
                    } else if hit.files > 0
                        && hit.running.is_empty()
                        && rule.admin
                        && !rule.user
                        && rule.mode != rules::Mode::Report
                    {
                        entry.action = Some(Action::AdminRule(rule.id.clone(), rule.name.clone()));
                    } else if !rule.user
                        && rule.mode == rules::Mode::Report
                        && hit.command_available
                    {
                        entry.action = match rule.id.as_str() {
                            "recycle-bin" => Some(Action::Windows(SystemAction::Bin)),
                            "component-store-cleanup" => Some(Action::Windows(SystemAction::Dism)),
                            "delivery-optimization" => {
                                Some(Action::Windows(SystemAction::Delivery))
                            }
                            "windows-old" | "windows-upgrade-temp" => {
                                Some(Action::Windows(SystemAction::Previous))
                            }
                            _ => None,
                        };
                    }
                    Some(entry)
                })
                .collect()
        }
        Tool::Map | Tool::Large => {
            let root = required()?;
            let mut tree = explorer::Tree::scan(root, job);
            out.note = format!(
                "{} dossiers · {} fichiers · {} accès refusés · {} fichiers cloud ignorés",
                tree.summary.dirs,
                tree.summary.files,
                tree.summary.denied,
                tree.summary.cloud_files
            );
            if tool == Tool::Map {
                let root_key = disk::paths::key(root);
                let rules = rules::builtin_targets()
                    .iter()
                    .filter(|r| {
                        !r.admin && r.mode != rules::Mode::Report && r.risk != Risk::Caution
                    })
                    .filter_map(|r| {
                        if job.is_cancelled() {
                            return None;
                        }
                        let mut scoped = r.clone();
                        scoped.paths = r
                            .paths
                            .iter()
                            .flat_map(|p| disk::paths::expand_pattern(p).unwrap_or_default())
                            .filter(|p| disk::paths::is_within(&disk::paths::key(p), &root_key))
                            .filter_map(|p| p.into_os_string().into_string().ok())
                            .filter(|p| !p.contains(['*', '?', '%']))
                            .collect();
                        (!scoped.paths.is_empty()).then_some(scoped)
                    })
                    .collect::<Vec<_>>();
                let annotations = junk::scan_targets(&rules, job)
                    .into_iter()
                    .zip(rules)
                    .filter(|(hit, _)| hit.running.is_empty())
                    .flat_map(|(hit, rule)| {
                        hit.found
                            .into_iter()
                            .map(move |p| (PathBuf::from(p.path), rule.risk, p.bytes))
                    })
                    .filter(|(p, _, _)| guard.check(p, Origin::Rule).is_ok())
                    .collect::<Vec<_>>();
                tree.mark(&annotations);
                if let Some(drive) = system::drives().into_iter().find(|d| {
                    disk::paths::key(root).starts_with(&disk::paths::key(Path::new(&d.root)))
                }) {
                    let shadows = hidden::shadow_storage().ok();
                    out.gap = hidden::hidden_space(
                        Path::new(&drive.root),
                        Some(&tree),
                        shadows.as_deref(),
                    )
                    .ok();
                }
                out.tree = Some(tree);
                Vec::new()
            } else {
                tree.large_files(
                    settings.large_min_mb.saturating_mul(1024 * 1024),
                    2000,
                    &guard,
                )
                .into_iter()
                .map(|f| {
                    Entry::new(f.name, format!("{:?} · {}", f.category, date(f.mtime)))
                        .at(f.path, f.bytes)
                        .picked(&guard, Origin::User)
                })
                .collect()
            }
        }
        Tool::Duplicates => {
            let minimum = settings.duplicate_min_kb.saturating_mul(1024).max(1);
            let groups = duplicates::find_duplicates_in_same_folder(
                &[required()?.to_owned()],
                minimum,
                &guard,
                job,
            );
            out.note = format!(
                "{} groupes · seuil {} Kio. Seuls les fichiers du même dossier sont comparés, et une copie est réservée dans chaque groupe.",
                groups.len(),
                settings.duplicate_min_kb
            );
            groups
                .into_iter()
                .enumerate()
                .flat_map(|(i, g)| {
                    let keeper = g
                        .files
                        .first()
                        .map(|f| PathBuf::from(&f.path))
                        .and_then(|p| Fingerprint::read(&p).map(|stamp| (p, stamp)));
                    g.files.into_iter().enumerate().map(move |(j, f)| {
                        let entry = Entry::new(
                            name(&f.path),
                            format!(
                                "Groupe {} · {}{}",
                                i + 1,
                                date(f.mtime),
                                if j == 0 { " · copie réservée" } else { "" }
                            ),
                        )
                        .at(f.path, g.bytes);
                        (entry, (j > 0).then(|| keeper.clone()).flatten())
                    })
                })
                .map(|(mut entry, keeper)| {
                    if let Some((keep, keep_mark)) = keeper
                        && let Some(path) = &entry.path
                        && guard.check(path, Origin::User).is_ok()
                        && let Some(mark) = Fingerprint::read(path)
                    {
                        entry.action = Some(Action::Duplicate(path.clone(), mark, keep, keep_mark));
                    }
                    entry
                })
                .collect()
        }
        Tool::Empty => empty::find_empty(&[required()?.to_owned()], &guard, job)
            .into_iter()
            .map(|f| {
                let p = PathBuf::from(&f.path);
                Entry::new(
                    name(&f.path),
                    format!("{} sous-dossiers vides · {}", f.nested, date(f.mtime)),
                )
                .at(&p, 0)
                .do_(Action::Empty(p))
            })
            .collect(),
        Tool::Projects => {
            let roots: Vec<_> = root
                .into_iter()
                .chain(settings.project_roots.iter().map(PathBuf::from))
                .collect();
            let roots = if roots.is_empty() {
                artifacts::suggested_roots()
                    .into_iter()
                    .map(PathBuf::from)
                    .collect()
            } else {
                roots
            };
            out.note = roots
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(" · ");
            artifacts::scan_projects(&roots, rules::builtin_artifacts(), job)
                .into_iter()
                .flat_map(|p| {
                    let idle = disk::fsmeta::now_unix().saturating_sub(p.last_activity)
                        >= i64::from(settings.idle_days) * 86400;
                    p.artifacts
                        .into_iter()
                        .filter(|a| settings.show_caution || a.risk != Risk::Caution)
                        .map(move |a| {
                            Entry::new(
                                format!("{} / {}", p.name, a.dir_name),
                                format!(
                                    "{} fichiers · projet actif le {}{}{}",
                                    a.files,
                                    date(p.last_activity),
                                    if p.has_git { " · Git" } else { "" },
                                    if idle { " · projet inactif" } else { "" }
                                ),
                            )
                            .at(a.path, a.bytes)
                            .risk(a.risk)
                        })
                })
                .map(|e| e.picked(&guard, Origin::User))
                .collect()
        }
        Tool::Downloads => {
            let p = root
                .or_else(|| system::library_folders().downloads.map(PathBuf::from))
                .ok_or("Dossier Téléchargements introuvable.")?;
            let report = organize::downloads(&p, job);
            if report.truncated {
                out.note = "Inventaire tronqué.".into();
            }
            report
                .files
                .into_iter()
                .map(|f| {
                    Entry::new(
                        f.name,
                        format!(
                            "{} · {}{}",
                            f.ext,
                            date(f.mtime),
                            if f.copy {
                                " · retéléchargement probable"
                            } else {
                                ""
                            }
                        ),
                    )
                    .at(f.path, f.bytes)
                    .picked(&guard, Origin::User)
                })
                .collect()
        }
        Tool::Installers => {
            let p = root
                .or_else(|| system::library_folders().downloads.map(PathBuf::from))
                .ok_or("Dossier Téléchargements introuvable.")?;
            installers::find_installers(&[p], settings.installer_age_days)
                .into_iter()
                .map(|f| {
                    Entry::new(name(&f.path), format!("Installateur · {}", date(f.mtime)))
                        .at(f.path, f.bytes)
                        .picked(&guard, Origin::User)
                })
                .collect()
        }
        Tool::Models => organize::ai_models(&root.into_iter().collect::<Vec<_>>(), job)
            .into_iter()
            .map(|m| {
                let mut entry = Entry::new(
                    m.name,
                    format!(
                        "{} · {} · {} · {} fichiers{}",
                        m.source,
                        m.kind,
                        m.formats.join(", "),
                        m.files,
                        if m.same_as.is_empty() {
                            String::new()
                        } else {
                            format!(
                                " · {} autre(s) outil(s) avec les mêmes poids",
                                m.same_as.len()
                            )
                        }
                    ),
                )
                .at(m.path, m.bytes)
                .risk(Risk::Review);
                if m.source == "ollama" && m.command.is_some() {
                    entry.action = Some(Action::Ollama(m.id));
                } else if let [path] = m.remove.as_slice() {
                    entry.path = Some(PathBuf::from(path));
                    entry = entry.picked(&guard, Origin::User);
                }
                entry
            })
            .collect(),
        Tool::Assistants => organize::assistant_data(job)
            .into_iter()
            .filter(|d| settings.show_caution || d.risk != Risk::Caution)
            .flat_map(|d| {
                let can_select_parent = d.items.is_empty();
                let mut list = vec![
                    Entry::new(
                        format!("{} · {}", d.tool, d.id),
                        format!("{} fichiers · {}", d.files, date(d.mtime)),
                    )
                    .at(d.path, d.bytes)
                    .risk(d.risk),
                ];
                list.extend(d.items.into_iter().map(|i| {
                    Entry::new(i.name, format!("{} fichiers · {}", i.files, date(i.mtime)))
                        .at(i.path, i.bytes)
                        .risk(d.risk)
                }));
                list.into_iter()
                    .enumerate()
                    .map(move |(i, e)| (e, can_select_parent || i != 0))
            })
            .map(|(e, allowed)| {
                if allowed {
                    e.picked(&guard, Origin::User)
                } else {
                    e
                }
            })
            .collect(),
        Tool::Games => {
            let games = steam::report(job);
            if games.running {
                out.note = "Steam est ouvert. Fermez-le avant de nettoyer ses caches.".into();
            }
            let running = games.running;
            let mut rows = Vec::new();
            for library in games.libraries {
                rows.push(
                    Entry::new(
                        "Bibliothèque Steam",
                        format!("{} jeux", library.games.len()),
                    )
                    .at(&library.path, library.bytes)
                    .do_(Action::Open(PathBuf::from(&library.path))),
                );
                rows.extend(library.games.into_iter().map(|game| Entry {
                    bytes: Some(game.bytes),
                    ..Entry::new(game.name, format!("Steam App {}", game.app_id))
                }));
            }
            for cache in games.caches {
                let mut e = Entry::new(
                    cache
                        .name
                        .unwrap_or_else(|| format!("Steam App {}", cache.app_id)),
                    if cache.installed {
                        "Cache de shaders"
                    } else {
                        "Cache de shaders · jeu absent"
                    },
                )
                .at(&cache.path, cache.bytes)
                .risk(cache.risk);
                if !running && let Some(rule) = rules::find_builtin("steam-shadercache") {
                    let mut rule = rule.clone();
                    rule.mode = rules::Mode::Folder;
                    rule.paths = vec![cache.path];
                    e.action = Some(Action::Rule(Box::new(rule)));
                }
                rows.push(e);
            }
            rows
        }
        Tool::Apps => {
            let mut apps = apps::desktop_apps();
            match apps::store_apps() {
                Ok(store) => apps.extend(store),
                Err(e) => out.note = format!("Catalogue Microsoft Store indisponible : {e}"),
            }
            apps.sort_by_key(|a| a.name.to_lowercase());
            let files = apps
                .iter()
                .take(160)
                .enumerate()
                .filter_map(|(i, a)| {
                    let p = a.icon.as_deref().and_then(disk::icons::icon_file)?;
                    Some((i, p))
                })
                .collect::<Vec<_>>();
            out.images = thumbnails(files, disk::icons::ImageKind::Icon, 32);
            apps.into_iter().map(app_row).collect()
        }
        Tool::Startup => startup::list()
            .into_iter()
            .map(|i| {
                let description = format!(
                    "{} · {:?}{}\n{}",
                    if i.enabled { "Activé" } else { "Désactivé" },
                    i.source,
                    if i.needs_admin {
                        " · administrateur requis"
                    } else {
                        ""
                    },
                    i.command
                );
                let row = Entry::new(&i.name, description);
                if i.needs_admin {
                    row
                } else {
                    row.do_(Action::Startup(i.source, i.name, !i.enabled))
                }
            })
            .collect(),
        Tool::Windows => windows_rows(),
        Tool::Settings => settings_rows(base),
    };
    if tool == Tool::Downloads && !job.is_cancelled() {
        let eligible = out
            .rows
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                let p = e.path.as_ref()?;
                let ext = p.extension()?.to_str()?.to_ascii_lowercase();
                matches!(
                    ext.as_str(),
                    "jpg" | "jpeg" | "png" | "webp" | "gif" | "mp4" | "mov" | "pdf"
                )
                .then(|| (i, p.to_string_lossy().into_owned()))
            })
            .take(120)
            .collect();
        out.images = thumbnails(eligible, disk::icons::ImageKind::Thumb, 64);
    }
    if job.is_cancelled() {
        return Err("Analyse annulée.".into());
    }
    Ok(out)
}

fn thumbnails(
    paths: Vec<(usize, String)>,
    kind: disk::icons::ImageKind,
    size: u32,
) -> HashMap<usize, Vec<u8>> {
    use base64::Engine;
    let paths = paths
        .into_iter()
        .filter(|(_, file)| {
            let p = Path::new(file);
            p.is_absolute()
                && !file.starts_with(r"\\")
                && p.ancestors().filter(|p| p.has_root()).all(|p| {
                    std::fs::symlink_metadata(p).is_ok_and(|m| {
                        !disk::fsmeta::is_link(&m.file_type()) && !disk::fsmeta::is_cloud_only(&m)
                    })
                })
        })
        .collect::<Vec<_>>();
    let names = paths.iter().map(|(_, p)| p.clone()).collect::<Vec<_>>();
    let found = disk::icons::images(&names, size, kind);
    paths
        .into_iter()
        .filter_map(|(i, p)| {
            let (_, bytes) = found.get(&p)?.as_deref()?.split_once(";base64,")?;
            if bytes.len() > 1_000_000 {
                return None;
            }
            Some((
                i,
                base64::engine::general_purpose::STANDARD
                    .decode(bytes)
                    .ok()?,
            ))
        })
        .collect()
}

fn app_row(a: apps::InstalledApp) -> Entry {
    let id = a.id.clone();
    let broken = a.broken;
    let mut row = Entry::new(
        a.name,
        format!(
            "{} · {} · {}",
            a.publisher,
            a.version,
            if id.starts_with("store:") {
                "Microsoft Store"
            } else {
                "Bureau"
            }
        ),
    );
    row.path = a.location.map(PathBuf::from);
    row.bytes = a.bytes;
    if broken && !id.starts_with("store:") {
        row.do_(Action::Broken(id))
    } else if a.uninstall.is_some() || id.starts_with("store:") {
        row.do_(Action::Uninstall(id))
    } else {
        row
    }
}

fn windows_rows() -> Vec<Entry> {
    use SystemAction::*;
    let bin = system::recycle_bin_info();
    let mut rows = vec![Entry {
        bytes: Some(bin.bytes),
        ..Entry::new(
            "Corbeille",
            format!("{} éléments · suppression définitive", bin.items),
        )
        .do_(Action::Windows(Bin))
    }];
    for (name, detail, action) in [
        (
            "Magasin de composants",
            "Nettoyage DISM officiel · UAC",
            Dism,
        ),
        (
            "Optimisation de la distribution",
            "Nettoyer le cache Windows Update · UAC",
            Delivery,
        ),
        (
            "Désactiver la veille prolongée",
            "Retire hiberfil.sys et peut affecter le démarrage rapide · UAC",
            HibernateOff,
        ),
        (
            "Activer la veille prolongée",
            "Rétablit la fonctionnalité Windows · UAC",
            HibernateOn,
        ),
        (
            "Ancienne installation Windows",
            "Ouvre le Nettoyage de disque de Windows",
            Previous,
        ),
    ] {
        rows.push(Entry::new(name, detail).do_(Action::Windows(action)));
    }
    rows.extend(
        system::system_files()
            .into_iter()
            .map(|f| Entry::new(f.name, "Géré par Windows").at(f.path, f.bytes)),
    );
    rows.push(
        Entry::new(
            "Paramètres de stockage",
            "Assistant Stockage et fichiers temporaires",
        )
        .do_(Action::ShellPage("ms-settings:storagesense")),
    );
    rows
}

fn settings_rows(base: &Path) -> Vec<Entry> {
    let active = std::env::current_exe()
        .ok()
        .is_some_and(|p| shell::status(&p).enabled);
    vec![
        Entry::new(
            "Menu contextuel de l’Explorateur",
            if active {
                "Activé pour ce compte"
            } else {
                "Désactivé"
            },
        )
        .do_(Action::ExplorerMenu(!active)),
        Entry::new(
            "Raccourci Nettoyage rapide",
            "Crée sur le Bureau un raccourci qui ouvre l’aperçu.",
        )
        .do_(Action::Shortcut),
        Entry::new(
            "Menu Démarrer",
            "Ajoute un accès au nettoyage rapide pour ce compte.",
        )
        .do_(Action::StartShortcut),
        Entry::new(
            "Accès depuis la barre des tâches",
            "Ajoute le nettoyage rapide à la liste de raccourcis Windows.",
        )
        .do_(Action::JumpList),
        Entry::new(
            "Données locales",
            if settings::is_portable() {
                "Mode portable"
            } else {
                "Profil de l’utilisateur"
            },
        )
        .do_(Action::Open(base.to_owned())),
        Entry::new(
            "Historique de nettoyage",
            "Efface uniquement les entrées locales EpicCleaner.",
        )
        .do_(Action::ClearHistory),
    ]
}

pub struct Outcome {
    pub note: String,
    pub output: Option<Output>,
    pub refresh: bool,
}

pub fn action_preview(action: &Action) -> (String, String) {
    let text = match action {
        Action::Rule(rule) => return (rule.name.clone(), rule.paths.join(" · ")),
        Action::AdminRule(id, name) => {
            return (
                name.clone(),
                format!("Règle système {id}. Windows demandera une confirmation UAC."),
            );
        }
        Action::Path(p, ..)
        | Action::Duplicate(p, ..)
        | Action::Empty(p)
        | Action::Open(p)
        | Action::Inspect(p) => p.display().to_string(),
        Action::Uninstall(id) | Action::Broken(id) | Action::Ollama(id) => id.clone(),
        Action::Startup(_, name, _) => name.clone(),
        Action::Close(p, pid, _) => format!("PID {pid} · {}", p.display()),
        Action::Windows(action) => {
            return (
                match action {
                    SystemAction::Bin => "Supprimer tous les éléments de la Corbeille",
                    SystemAction::Dism => "Nettoyer le magasin de composants avec DISM",
                    SystemAction::Delivery => "Nettoyer le cache d’optimisation de la distribution",
                    SystemAction::HibernateOff => {
                        "Désactiver la veille prolongée et libérer hiberfil.sys"
                    }
                    SystemAction::HibernateOn => "Réactiver la veille prolongée",
                    SystemAction::Previous => "Ouvrir le Nettoyage de disque Windows",
                }
                .into(),
                "Action Windows".into(),
            );
        }
        _ => action.label().into(),
    };
    (text.clone(), text)
}

pub fn perform(
    tool: Tool,
    actions: Vec<Action>,
    settings: &Settings,
    base: &Path,
    job: &Job,
) -> Result<Outcome> {
    let guard = guard(settings, base);
    let mut outcome = Outcome {
        note: String::new(),
        output: None,
        refresh: false,
    };
    if !actions.is_empty() && actions.iter().all(Action::selectable) {
        let method = if tool == Tool::Apps {
            disk::clean::DeleteMethod::Recycle
        } else if matches!(tool, Tool::Caches | Tool::Projects | Tool::Games) {
            settings.cache_delete
        } else {
            settings.files_delete
        };
        let mut report = CleanReport::default();
        for action in actions {
            if job.is_cancelled() {
                report.cancelled = true;
                break;
            }
            match action {
                Action::Rule(rule) => {
                    report.absorb(clean::clean_targets(&[&rule], method, &guard, job, false))
                }
                Action::Path(path, origin, mark) if mark.matches(&path) => {
                    report.absorb(clean::clean_picked(&[(path, origin)], method, &guard, job))
                }
                Action::Duplicate(path, mark, keep, keep_mark)
                    if guard.check(&path, Origin::User).is_ok()
                        && guard.check(&keep, Origin::User).is_ok()
                        && matches_duplicate(&path, &mark, &keep, &keep_mark, job) =>
                {
                    report.absorb(clean::clean_picked(
                        &[(path, Origin::User)],
                        method,
                        &guard,
                        job,
                    ));
                }
                Action::Duplicate(path, ..) => {
                    report.failed += 1;
                    report.failures.push(clean::Failure {
                        path: path.display().to_string(),
                        reason: "la copie ou son original a changé depuis l’aperçu".into(),
                    });
                }
                Action::Path(path, ..) => {
                    report.failed += 1;
                    report.failures.push(clean::Failure {
                        path: path.display().to_string(),
                        reason: "modifié depuis l’aperçu".into(),
                    });
                }
                Action::Empty(path) => {
                    let (removed, result) = remove_empty(&path, &guard, job);
                    report.dirs += removed;
                    match result {
                        Ok(()) => {
                            report.items.push(path.display().to_string());
                        }
                        Err(reason) => {
                            report.failed += 1;
                            report.failures.push(clean::Failure {
                                path: path.display().to_string(),
                                reason,
                            });
                        }
                    }
                }
                _ => unreachable!(),
            }
        }
        let journal = history::append(
            base,
            &history::HistoryEntry::from_report(tool.label(), method, &report, false),
        );
        outcome.note = format!(
            "{} fichiers · {} dossiers · {} octets retirés · {} octets recyclés · {} échecs{}{}",
            report.files,
            report.dirs,
            report.freed,
            report.recycled,
            report.failed,
            if report.cancelled {
                " · opération interrompue"
            } else {
                ""
            },
            if journal.is_err() {
                " · historique indisponible"
            } else {
                ""
            }
        );
        let rows = report
            .failures
            .into_iter()
            .map(|f| {
                Entry::new(name(&f.path), f.reason)
                    .at(&f.path, 0)
                    .do_(Action::Inspect(PathBuf::from(f.path)))
            })
            .collect();
        outcome.output = Some(Output {
            rows,
            note: "Rapport d’opération. Relancez l’analyse pour actualiser la liste.".into(),
            ..Output::default()
        });
        return Ok(outcome);
    }
    let [action] = actions.as_slice() else {
        return Err("Action invalide.".into());
    };
    match action {
        Action::Inspect(path) => {
            let report = locks::find_lockers(path).map_err(msg)?;
            let rows = report
                .lockers
                .into_iter()
                .map(|p| {
                    let mut row = Entry::new(
                        p.name,
                        format!(
                            "PID {} · {:?}{}",
                            p.pid,
                            p.kind,
                            p.service
                                .as_deref()
                                .map_or(String::new(), |s| format!(" · {s}"))
                        ),
                    );
                    if matches!(p.kind, locks::LockerKind::App)
                        && p.pid != std::process::id()
                        && p.started != 0
                    {
                        row.action = Some(Action::Close(path.clone(), p.pid, p.started));
                    }
                    row
                })
                .collect();
            outcome.output = Some(Output {
                rows,
                note: format!(
                    "{} · {} fichiers contrôlés · {} non vérifiés{}",
                    path.display(),
                    report.checked,
                    report.unchecked,
                    if report.truncated {
                        " · résultat partiel"
                    } else {
                        ""
                    }
                ),
                ..Output::default()
            });
        }
        Action::Close(path, pid, started) => {
            let report = locks::find_lockers(path).map_err(msg)?;
            if !report
                .lockers
                .iter()
                .any(|p| p.pid == *pid && p.started == *started)
            {
                return Err("Le processus ne verrouille plus ce chemin.".into());
            }
            outcome.note = format!(
                "Fermeture : {:?}",
                locks::close_gracefully(*pid, *started, 1500)
            );
        }
        Action::Open(path) => {
            let path = if path.is_dir() {
                path.as_path()
            } else {
                path.parent()
                    .filter(|p| p.is_dir())
                    .ok_or("Dossier introuvable.")?
            };
            #[cfg(windows)]
            std::process::Command::new("explorer.exe")
                .arg(path)
                .spawn()
                .map_err(msg)?;
            #[cfg(not(windows))]
            std::process::Command::new("xdg-open")
                .arg(path)
                .spawn()
                .map_err(msg)?;
        }
        Action::Ollama(id) => {
            let model = organize::ai_models(&[], job)
                .into_iter()
                .find(|m| m.id == *id && m.source == "ollama")
                .ok_or("Modèle Ollama introuvable.")?;
            if guard.is_excluded(&disk::paths::key(Path::new(&model.path))) {
                return Err("Ce modèle est exclu du nettoyage.".into());
            }
            organize::ollama_remove(id).map_err(msg)?;
            outcome.refresh = true;
        }
        Action::ShellPage(page) => system::shell_open(page).map_err(msg)?,
        Action::AdminRule(id, _) => {
            let sid = system::current_user_sid().ok_or("Identité Windows introuvable.")?;
            approved_admin_rule(id)?;
            if !valid_sid(&sid) {
                return Err("Identité Windows invalide.".into());
            }
            let exe = std::env::current_exe().map_err(msg)?;
            let args = format!("--admin-rule {id} {sid}");
            outcome.note = match system::run_and_wait(&exe.to_string_lossy(), &args, true, false)
                .map_err(msg)?
            {
                system::LaunchOutcome::Exited(0) => "Nettoyage administrateur terminé.".into(),
                system::LaunchOutcome::Exited(3) => {
                    "Nettoyage administrateur partiel ou refusé ; consultez l’historique.".into()
                }
                system::LaunchOutcome::Exited(_) => {
                    return Err("Nettoyage administrateur refusé.".into());
                }
                system::LaunchOutcome::Cancelled => "Demande UAC annulée.".into(),
            };
            outcome.refresh = true;
        }
        Action::Startup(source, name, enabled) => {
            if source.needs_admin() {
                return Err("Ce réglage nécessite les droits administrateur.".into());
            }
            startup::set_enabled(*source, name, *enabled).map_err(msg)?;
            outcome.refresh = true;
        }
        Action::Broken(id) => {
            if !apps::desktop_apps().iter().any(|a| a.id == *id && a.broken) {
                return Err("L’entrée n’est plus marquée comme orpheline.".into());
            }
            outcome.note = format!(
                "Sauvegarde : {}",
                apps::remove_broken_entry(id, &base.join("backups")).map_err(msg)?
            );
            outcome.refresh = true;
        }
        Action::Uninstall(id) => {
            let installed = || {
                if id.starts_with("store:") {
                    apps::store_apps().map_err(msg)
                } else {
                    Ok(apps::desktop_apps())
                }
            };
            let app = installed()?
                .into_iter()
                .find(|a| a.id == *id)
                .ok_or("Application introuvable.")?;
            let launch = apps::uninstall(&app).map_err(msg)?;
            if matches!(launch, system::LaunchOutcome::Cancelled) {
                outcome.note = "Désinstallation annulée. Aucun résidu n’a été touché.".into();
                return Ok(outcome);
            }
            outcome.note = format!("Désinstallation : {launch:?}");
            let mut remaining = installed()?;
            if !remaining.iter().any(|a| a.id == *id) {
                if id.starts_with("store:") {
                    remaining.extend(apps::desktop_apps());
                } else if let Ok(store) = apps::store_apps() {
                    remaining.extend(store);
                }
                let mut rows = Vec::new();
                let mut preselected = Vec::new();
                for f in disk::leftovers::find(&app, &remaining, &guard) {
                    use disk::leftovers::{Confidence, Reason};
                    let level = if f.confidence == Confidence::High {
                        "Lien probable avec l’application"
                    } else {
                        "Attribution incertaine"
                    };
                    let why = match f.reason {
                        Reason::Install => "Dossier d’installation déclaré",
                        Reason::Name => "Nom exact",
                        Reason::PublisherName => "Dossier dédié chez l’éditeur",
                        Reason::Publisher => "Dossier potentiellement partagé",
                        Reason::Similar => "Nom similaire",
                    };
                    let mut entry = Entry::new(
                        name(&f.path),
                        format!(
                            "{level} · {why} · {} fichier(s){}",
                            f.files,
                            if f.admin {
                                " · droits Windows possibles"
                            } else {
                                ""
                            }
                        ),
                    )
                    .at(f.path, f.bytes)
                    .risk(Risk::Review);
                    if f.reason != Reason::Publisher
                        && (!f.admin || f.confidence == Confidence::High)
                    {
                        entry = entry.picked(&guard, Origin::Leftover);
                    }
                    if f.confidence == Confidence::High && entry.action.is_some() {
                        preselected.push(rows.len());
                    }
                    rows.push(entry);
                }
                outcome.output = Some(Output {
                    note: format!(
                        "Restes de {} : les dossiers les plus probablement liés sont présélectionnés. Vérifie la liste avant l’envoi à la Corbeille ; elle peut contenir des réglages ou sauvegardes. Les emplacements protégés restent soumis aux droits Windows.",
                        app.name
                    ),
                    rows,
                    preselected,
                    ..Output::default()
                });
            } else {
                outcome.note.push_str(" · L’application est encore installée. Aucun résidu n’a été nettoyé ; redémarre si le désinstalleur le demande.");
            }
        }
        Action::Windows(action) => {
            let before = matches!(action, SystemAction::Bin).then(system::recycle_bin_info);
            outcome.note = run_windows(*action)?;
            if !outcome.note.ends_with("annulée.") {
                let report = CleanReport {
                    files: before.as_ref().map_or(0, |b| b.items),
                    freed: before.as_ref().map_or(0, |b| b.bytes),
                    items: vec![action_preview(&Action::Windows(*action)).0],
                    ..CleanReport::default()
                };
                if history::append(
                    base,
                    &history::HistoryEntry::from_report(
                        "Outils Windows",
                        disk::clean::DeleteMethod::Permanent,
                        &report,
                        before.is_none(),
                    ),
                )
                .is_err()
                {
                    outcome.note.push_str(" · historique indisponible");
                }
            }
            outcome.refresh = true;
        }
        Action::ExplorerMenu(enabled) => {
            if *enabled {
                shell::register(&std::env::current_exe().map_err(msg)?, "fr").map_err(msg)?;
            } else {
                shell::unregister().map_err(msg)?;
            }
            outcome.refresh = true;
        }
        Action::Shortcut => {
            let p = disk::launch::create_desktop_shortcut(
                &std::env::current_exe().map_err(msg)?,
                "EpicCleaner — Nettoyage rapide",
                "Ouvrir l’aperçu du nettoyage",
            )
            .map_err(msg)?;
            outcome.note = format!("Raccourci créé : {}", p.display());
        }
        Action::StartShortcut => {
            #[cfg(windows)]
            {
                let appdata =
                    disk::paths::env_path("APPDATA").ok_or("Dossier AppData introuvable.")?;
                let menu = appdata
                    .join(r"Microsoft\Windows\Start Menu\Programs")
                    .join(disk::launch::SHORTCUT_NAME);
                disk::launch::write_shortcut(
                    &menu,
                    &std::env::current_exe().map_err(msg)?,
                    "EpicCleaner — Nettoyage rapide",
                    "Ouvrir l’aperçu EpicCleaner",
                )
                .map_err(msg)?;
                outcome.note = "Accès rapide ajouté au menu Démarrer.".into();
            }
            #[cfg(not(windows))]
            return Err("Menu Démarrer disponible sous Windows.".into());
        }
        Action::JumpList => {
            disk::launch::set_jump_list(
                &std::env::current_exe().map_err(msg)?,
                "Nettoyage rapide",
                "Ouvrir l’aperçu EpicCleaner",
            )
            .map_err(msg)?;
            outcome.note = "Accès rapide ajouté à la barre des tâches.".into();
        }
        Action::Export(ids) => {
            #[cfg(windows)]
            {
                let Some(file) = rfd::FileDialog::new()
                    .add_filter("Script PowerShell", &["ps1"])
                    .set_file_name("EpicCleaner-plan.ps1")
                    .save_file()
                else {
                    return Ok(outcome);
                };
                let exe = std::env::current_exe().map_err(msg)?;
                let mut script = format!(
                    "# EpicCleaner : rescanne les regles et ouvre une confirmation.\r\n& '{}' '--review-cache'",
                    exe.to_string_lossy().replace('\'', "''")
                );
                for id in ids {
                    script.push_str(&format!(" '{}'", id.replace('\'', "''")));
                }
                std::fs::write(&file, format!("{script}\r\n")).map_err(msg)?;
                outcome.note = format!("Plan exporté : {}", file.display());
            }
            #[cfg(not(windows))]
            {
                let _ = ids;
                return Err("Export PowerShell disponible sous Windows.".into());
            }
        }
        Action::ClearHistory => {
            history::clear(base).map_err(msg)?;
            outcome.note = "Historique effacé.".into();
        }
        _ => return Err("Action invalide.".into()),
    }
    Ok(outcome)
}

fn remove_empty(path: &Path, guard: &Guard, job: &Job) -> (u64, Result<()>) {
    let mut removed = 0;
    let mut work = || -> Result<()> {
        guard.check(path, Origin::User).map_err(msg)?;
        let root = crate::engine::checked_path(path).map_err(msg)?;
        let mut dirs = Vec::new();
        for entry in walkdir::WalkDir::new(&root)
            .follow_links(false)
            .follow_root_links(false)
        {
            if job.is_cancelled() {
                return Err("Opération interrompue.".into());
            }
            let entry = entry.map_err(msg)?;
            let meta = entry.metadata().map_err(msg)?;
            if !meta.is_dir()
                || disk::fsmeta::is_link(&meta.file_type())
                || disk::fsmeta::is_cloud_only(&meta)
            {
                return Err("Le dossier n’est plus vide ou contient un lien.".into());
            }
            let safe = crate::engine::checked_path(entry.path()).map_err(msg)?;
            guard.check(&safe, Origin::User).map_err(msg)?;
            dirs.push(safe);
        }
        for dir in dirs.into_iter().rev() {
            if job.is_cancelled() {
                return Err("Opération interrompue.".into());
            }
            let safe = crate::engine::checked_path(&dir).map_err(msg)?;
            guard.check(&safe, Origin::User).map_err(msg)?;
            // Never call remove_dir_all: if a file appears here, the OS refuses.
            std::fs::remove_dir(&safe).map_err(msg)?;
            removed += 1;
        }
        Ok(())
    };
    let result = work();
    (removed, result)
}

fn valid_sid(sid: &str) -> bool {
    sid.starts_with("S-1-")
        && sid.len() < 185
        && sid[2..].bytes().all(|c| c == b'-' || c.is_ascii_digit())
}

fn approved_admin_rule(id: &str) -> Result<&'static TargetRule> {
    if id.is_empty()
        || id.len() > 80
        || !id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
    {
        return Err("Identifiant de règle invalide.".into());
    }
    rules::find_builtin(id)
        .filter(|r| r.admin && !r.user && r.mode != rules::Mode::Report)
        .ok_or_else(|| "Règle administrateur inconnue.".into())
}

pub fn admin_entry(id: &str, sid: &str) -> i32 {
    let same = valid_sid(sid) && system::current_user_sid().is_some_and(|actual| actual == sid);
    if !same || !system::is_elevated() {
        return 2;
    }
    let Ok(rule) = approved_admin_rule(id) else {
        return 2;
    };
    let base = settings::data_dir();
    let Ok(config) = Settings::load(&base) else {
        return 2;
    };
    if rule.risk == Risk::Caution && !config.show_caution {
        return 2;
    }
    let report = clean::clean_targets(
        &[rule],
        config.cache_delete,
        &guard(&config, &base),
        &Job::new(),
        true,
    );
    let written = history::append(
        &base,
        &history::HistoryEntry::from_report(
            "Caches administrateur",
            config.cache_delete,
            &report,
            true,
        ),
    )
    .is_ok();
    if report.failed == 0 && written { 0 } else { 3 }
}

fn run_windows(action: SystemAction) -> Result<String> {
    use SystemAction::*;
    if matches!(action, Bin) {
        system::empty_recycle_bin().map_err(msg)?;
        return Ok("Corbeille vidée.".into());
    }
    let sys = PathBuf::from(std::env::var_os("SystemRoot").ok_or("Dossier Windows inconnu.")?)
        .join("System32");
    let (exe, args, visible) = match action {
        Dism => (
            "Dism.exe",
            "/Online /Cleanup-Image /StartComponentCleanup",
            true,
        ),
        Delivery => (
            r"WindowsPowerShell\v1.0\powershell.exe",
            "-NoProfile -Command Delete-DeliveryOptimizationCache -Force",
            true,
        ),
        HibernateOff => ("powercfg.exe", "/hibernate off", false),
        HibernateOn => ("powercfg.exe", "/hibernate on", false),
        Previous => ("cleanmgr.exe", "", true),
        Bin => unreachable!(),
    };
    let result =
        system::run_and_wait(&sys.join(exe).to_string_lossy(), args, true, visible).map_err(msg)?;
    match result {
        system::LaunchOutcome::Exited(0) => Ok("Action Windows terminée.".into()),
        system::LaunchOutcome::Exited(code) => Err(format!("Windows a retourné le code {code}.")),
        system::LaunchOutcome::Cancelled => Ok("Action Windows annulée.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advanced_disk_map_scans_real_fixture() {
        let root = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("Media")).unwrap();
        std::fs::write(root.path().join("Media").join("test.bin"), vec![0; 65_536]).unwrap();
        let report = scan(
            Tool::Map,
            Some(root.path().into()),
            &Settings::default(),
            data.path(),
            &Job::new(),
        )
        .unwrap();
        let view = report.tree.unwrap().view(0, 2, 16).unwrap();
        assert_eq!(view.files, 1);
        assert!(view.children.iter().any(|n| n.name == "Media"));
    }

    #[test]
    fn duplicate_cleanup_refuses_to_remove_last_copy_when_keeper_disappears() {
        let root = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let first = root.path().join("a.bin");
        let second = root.path().join("b.bin");
        std::fs::write(&first, [9; 8192]).unwrap();
        std::fs::write(&second, [9; 8192]).unwrap();
        let settings = Settings {
            duplicate_min_kb: 1,
            files_delete: disk::clean::DeleteMethod::Permanent,
            ..Settings::default()
        };
        let found = scan(
            Tool::Duplicates,
            Some(root.path().into()),
            &settings,
            data.path(),
            &Job::new(),
        )
        .unwrap();
        let saved = found.rows.iter().find_map(|r| r.action.clone()).unwrap();
        let Action::Duplicate(ref final_copy, _, ref original, _) = saved else {
            panic!("expected duplicate")
        };
        let final_copy = final_copy.clone();
        std::fs::remove_file(original).unwrap();
        let result = perform(
            Tool::Duplicates,
            vec![saved],
            &settings,
            data.path(),
            &Job::new(),
        )
        .unwrap();
        assert!(final_copy.is_file());
        assert_eq!(result.output.unwrap().rows.len(), 1);
        assert_eq!(history::read(data.path(), 1)[0].failed, 1);
    }

    #[test]
    fn admin_helper_only_accepts_builtin_admin_rules() {
        assert!(approved_admin_rule(r"C:\Users").is_err());
        assert!(approved_admin_rule("--inject").is_err());
        for rule in rules::builtin_targets()
            .iter()
            .filter(|r| !r.admin || r.mode == rules::Mode::Report)
        {
            assert!(approved_admin_rule(&rule.id).is_err());
        }
    }
}
