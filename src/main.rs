#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
#![deny(unsafe_op_in_unsafe_fn)]

mod engine;
mod tools;
mod ui;

use engine::{Item, Mode, Progress, Receipt, Report};
use epic_disk_core as disk_core;
use rune::{AppSpec, Chord, KeyEvent, TextEdit, Tx, Waker};
use std::{
    collections::BTreeSet,
    io,
    path::PathBuf,
    sync::{Arc, atomic::Ordering::Relaxed, mpsc},
    time::Duration,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Home,
    Space,
    Duplicates,
    Cleanup,
    History,
    Tools,
}

impl Page {
    fn mode(self) -> Option<Mode> {
        Some(match self {
            Self::Space => Mode::Space,
            Self::Duplicates => Mode::Duplicates,
            Self::Cleanup => Mode::Cleanup,
            _ => return None,
        })
    }
}

enum Event {
    Picked(Option<PathBuf>),
    Scanned(io::Result<(Report, Option<(u64, u64)>)>),
    Recycled(Receipt),
    ToolScan(Result<tools::Output, String>),
    ToolAction(Result<tools::Outcome, String>),
}

struct Job {
    result: mpsc::Receiver<Event>,
    label: &'static str,
    progress: Option<Arc<Progress>>,
    core: Option<Arc<disk_core::Job>>,
}

struct State {
    overview_scan: Option<(usize, u64)>,
    page: Page,
    waker: Option<Waker>,
    job: Option<Job>,
    status: String,
    error: bool,
    path: Option<PathBuf>,
    report: Option<Report>,
    disk: Option<(u64, u64)>,
    selected: BTreeSet<usize>,
    filter: TextEdit,
    receipts: Vec<Receipt>,
    confirm: bool,
    demo: bool,
    motion: bool,
    scan_progress: (u64, u64, bool),
    tool: tools::Tool,
    tool_output: Option<tools::Output>,
    images: Option<rune::ImageCache>,
    tool_images: std::collections::HashMap<usize, rune::Image>,
    tool_pending: Vec<tools::Action>,
    tool_selection: BTreeSet<usize>,
    tool_page: usize,
    sort_size: Option<bool>,
    tool_node: u32,
    tool_gap: bool,
    map_cleanup: bool,
    map_detail: Option<u32>,
    map_catalog: bool,
    map_return: Option<(tools::Output, usize)>,
    quick: bool,
    review_ids: BTreeSet<String>,
    base: PathBuf,
    config: disk_core::settings::Settings,
    config_valid: bool,
    journal: Vec<disk_core::history::HistoryEntry>,
    exclusions: TextEdit,
    project_roots: TextEdit,
    drive_list: Vec<disk_core::system::Drive>,
    launch: Option<Launch>,
}

enum Launch {
    Quick,
    Analyze(PathBuf),
    Review(BTreeSet<String>),
}

impl Default for State {
    fn default() -> Self {
        Self {
            overview_scan: None,
            page: Page::Home,
            waker: None,
            job: None,
            status: String::new(),
            error: false,
            path: None,
            report: None,
            disk: None,
            selected: BTreeSet::new(),
            filter: TextEdit::default(),
            receipts: Vec::new(),
            confirm: false,
            demo: false,
            motion: true,
            scan_progress: (0, 0, false),
            tool: tools::Tool::Caches,
            tool_output: None,
            images: None,
            tool_images: Default::default(),
            tool_pending: Vec::new(),
            tool_selection: BTreeSet::new(),
            tool_page: 0,
            sort_size: None,
            tool_node: 0,
            tool_gap: false,
            map_cleanup: false,
            map_detail: None,
            map_catalog: false,
            map_return: None,
            quick: false,
            review_ids: BTreeSet::new(),
            base: disk_core::settings::data_dir(),
            config: disk_core::settings::Settings::default(),
            config_valid: true,
            journal: Vec::new(),
            exclusions: TextEdit::default(),
            project_roots: TextEdit::default(),
            drive_list: Vec::new(),
            launch: None,
        }
    }
}

impl State {
    fn navigate(&mut self, page: Page) {
        if self.job.is_some() || self.page == page {
            return;
        }
        self.page = page;
        self.clear_report();
        self.tool_output = None;
        self.map_return = None;
        self.tool_selection.clear();
        self.quick = false;
        self.review_ids.clear();
        self.path = None;
        self.status.clear();
        self.error = false;
    }

    fn clear_report(&mut self) {
        self.report = None;
        self.selected.clear();
        self.filter = TextEdit::default();
        self.disk = None;
    }

    fn spawn(
        &mut self,
        label: &'static str,
        progress: Option<Arc<Progress>>,
        run: impl FnOnce() -> Event + Send + 'static,
    ) -> bool {
        if self.job.is_some() || self.demo {
            return false;
        }
        let (send, result) = mpsc::channel();
        let waker = self.waker.clone();
        match std::thread::Builder::new()
            .name(label.into())
            .spawn(move || {
                let event = std::panic::catch_unwind(std::panic::AssertUnwindSafe(run));
                if let Ok(event) = event {
                    let _ = send.send(event);
                }
                drop(send);
                if let Some(waker) = waker {
                    waker();
                }
            }) {
            Ok(_) => {
                self.job = Some(Job {
                    result,
                    label,
                    progress,
                    core: None,
                });
                self.scan_progress = (0, 0, false);
                self.status.clear();
                self.error = false;
                true
            }
            Err(err) => {
                self.set_error(format!("Impossible de démarrer : {err}"));
                false
            }
        }
    }

    fn set_error(&mut self, message: String) {
        self.status = message;
        self.error = true;
    }

    fn pick(&mut self) {
        if self.page == Page::Home {
            self.tool_select(tools::Tool::Map);
        }
        if self.page != Page::Tools && self.page.mode().is_none_or(|m| m == Mode::Cleanup) {
            return;
        }
        #[cfg(windows)]
        self.spawn("Sélection du dossier", None, || {
            Event::Picked(
                rfd::FileDialog::new()
                    .set_title("Choisir un dossier à analyser")
                    .pick_folder(),
            )
        });
        #[cfg(not(windows))]
        self.set_error("Glissez un dossier dans la fenêtre.".into());
    }

    fn start(&mut self, path: PathBuf) {
        if self.page == Page::Tools {
            self.run_tool(Some(path));
            return;
        }
        let Some(mode) = self.page.mode() else { return };
        let progress = Arc::new(Progress::default());
        let worker = Arc::clone(&progress);
        let root = path.clone();
        if self.spawn("Analyse en cours", Some(progress), move || {
            Event::Scanned(engine::scan(&root, mode, &worker).map(|report| {
                let disk = fs2::statvfs(&root)
                    .ok()
                    .map(|s| (s.available_space(), s.total_space()));
                (report, disk)
            }))
        }) {
            self.clear_report();
            self.path = Some(path);
        }
    }

    fn run_temp(&mut self) {
        match engine::temp_root() {
            Some(root) => self.start(root),
            None => self.set_error("Dossier temporaire standard du profil introuvable.".into()),
        }
    }

    fn open_confirm(&mut self) {
        if self.page == Page::Cleanup
            && self.job.is_none()
            && !self.selected.is_empty()
            && !self.demo
        {
            self.confirm = true;
        }
    }

    fn recycle(&mut self) {
        if !self.confirm || self.page != Page::Cleanup || self.demo || self.job.is_some() {
            return;
        }
        self.confirm = false;
        if !self.config_valid {
            self.set_error("Réglages illisibles : nettoyage bloqué.".into());
            return;
        }
        let guard = tools::guard(&self.config, &self.base);
        let items: Vec<_> = self
            .report
            .iter()
            .flat_map(|report| {
                self.selected
                    .iter()
                    .filter_map(|index| report.rows.get(*index))
                    .filter(|item| item.selectable())
                    .filter(|item| {
                        guard
                            .check(&item.path, disk_core::paths::Origin::User)
                            .is_ok()
                    })
                    .cloned()
            })
            .collect();
        if !items.is_empty()
            && self.spawn("Envoi à la Corbeille", None, move || {
                Event::Recycled(engine::recycle(items))
            })
        {
            self.clear_report();
        }
    }

    fn tool_select(&mut self, tool: tools::Tool) {
        if self.job.is_some() || (self.page == Page::Tools && self.tool == tool && !self.quick) {
            return;
        }
        self.navigate(Page::Tools);
        self.tool = tool;
        self.path = None;
        self.tool_output = None;
        self.map_return = None;
        self.tool_images.clear();
        self.tool_pending.clear();
        self.tool_selection.clear();
        self.tool_page = 0;
        self.tool_node = 0;
        self.tool_gap = false;
        self.map_cleanup = false;
        self.map_detail = None;
        self.map_catalog = false;
        self.quick = false;
        self.review_ids.clear();
        self.filter = TextEdit::default();
        self.status.clear();
        self.error = false;
        if matches!(tool, tools::Tool::Settings | tools::Tool::Windows) {
            self.run_tool(None);
        }
    }

    fn quick_clean(&mut self) {
        self.tool_select(tools::Tool::Caches);
        self.quick = true;
        self.run_tool(None);
    }

    fn run_tool(&mut self, root: Option<PathBuf>) {
        if self.tool.needs_root() && root.is_none() {
            self.pick();
            return;
        }
        let (tool, base, settings) = (self.tool, self.base.clone(), self.config.clone());
        let chosen = root.clone();
        let core = Arc::new(disk_core::Job::new());
        let work = Arc::clone(&core);
        if self.spawn("Analyse en cours", None, move || {
            Event::ToolScan(tools::scan(tool, root, &settings, &base, &work))
        }) {
            self.job.as_mut().unwrap().core = Some(core);
            self.path = chosen;
            self.tool_output = None;
            self.tool_images.clear();
            self.tool_selection.clear();
            self.filter = TextEdit::default();
            self.tool_page = 0;
            self.tool_node = 0;
            self.tool_gap = false;
            self.map_cleanup = false;
            self.map_detail = None;
            self.map_catalog = false;
            self.map_return = None;
        }
    }

    fn tool_request(&mut self, actions: Vec<tools::Action>) {
        if self.demo || self.job.is_some() || actions.is_empty() {
            return;
        }
        if !self.config_valid && actions.iter().any(tools::Action::confirm) {
            self.set_error(
                "Réglages illisibles : corrigez le fichier settings.json avant toute modification."
                    .into(),
            );
        } else if actions.iter().any(tools::Action::confirm) {
            self.tool_pending = actions;
        } else {
            self.tool_run_actions(actions);
        }
    }

    fn map_enter(&mut self, id: u32) {
        self.tool_node = id;
        self.tool_page = 0;
        self.map_detail = None;
    }

    fn set_size_sort(&mut self, ascending: bool) {
        self.sort_size = Some(ascending);
        self.tool_page = 0;
        self.map_detail = None;
    }

    fn return_to_map(&mut self) {
        if let Some((output, page)) = self.map_return.take() {
            self.store_output(output);
            self.tool_page = page;
            self.status.clear();
            self.error = false;
        }
    }

    fn tool_run_actions(&mut self, actions: Vec<tools::Action>) {
        if self.demo || self.job.is_some() || actions.is_empty() {
            return;
        }
        let (tool, base, settings) = (self.tool, self.base.clone(), self.config.clone());
        let core = Arc::new(disk_core::Job::new());
        let work = Arc::clone(&core);
        if self.spawn("Opération en cours", None, move || {
            Event::ToolAction(tools::perform(tool, actions, &settings, &base, &work))
        }) {
            self.job.as_mut().unwrap().core = Some(core);
        }
    }

    fn tool_confirm(&mut self) {
        let actions = std::mem::take(&mut self.tool_pending);
        self.tool_run_actions(actions);
    }

    fn persist_settings(&mut self) {
        if self.demo || !self.config_valid {
            return;
        }
        self.config.exclusions = self
            .exclusions
            .text
            .lines()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        self.config.project_roots = self
            .project_roots
            .text
            .lines()
            .map(str::trim)
            .filter(|p| !p.is_empty() && PathBuf::from(p).is_absolute())
            .map(str::to_owned)
            .collect();
        match self.config.save(&self.base) {
            Ok(()) => {
                self.status = "Réglages enregistrés.".into();
                self.error = false;
            }
            Err(e) => self.set_error(format!("Enregistrement impossible : {e}")),
        }
    }

    fn store_output(&mut self, mut output: tools::Output) {
        self.tool_images.clear();
        if let Some(cache) = &self.images {
            self.tool_images.extend(
                output
                    .images
                    .drain()
                    .map(|(i, bytes)| (i, cache.load(bytes))),
            );
        }
        self.tool_output = Some(output);
    }
}

fn poll(s: &mut State, _: f64, tx: &mut Tx) -> bool {
    if let Some(launch) = s.launch.take() {
        match launch {
            Launch::Quick => s.quick_clean(),
            Launch::Review(ids) => {
                s.tool_select(tools::Tool::Caches);
                s.quick = true;
                s.review_ids = ids;
                s.run_tool(None);
            }
            Launch::Analyze(path) => {
                s.tool_select(tools::Tool::Map);
                s.run_tool(Some(path));
            }
        }
        return true;
    }
    let Some(job) = &s.job else { return false };
    match job.result.try_recv() {
        Ok(event) => {
            s.job = None;
            match event {
                Event::Picked(Some(path)) => s.start(path),
                Event::Picked(None) => {}
                Event::Scanned(Ok((report, disk))) => {
                    s.report = Some(report);
                    s.disk = disk;
                }
                Event::Scanned(Err(err)) if err.kind() == io::ErrorKind::Interrupted => {
                    s.status = "Analyse annulée.".into()
                }
                Event::Scanned(Err(err)) => s.set_error(format!("Analyse impossible : {err}")),
                Event::Recycled(receipt) => {
                    s.status = format!(
                        "{} fichier(s) envoyé(s) à la Corbeille · {} échec(s).",
                        receipt.moved,
                        receipt.failures.len()
                    );
                    s.error = !receipt.failures.is_empty();
                    s.receipts.push(receipt);
                }
                Event::ToolScan(Ok(output)) => {
                    if s.tool == tools::Tool::Caches {
                        let items = output.rows.iter().filter(|entry| {
                            entry.action.as_ref().is_some_and(tools::Action::selectable)
                        });
                        s.overview_scan = Some(items.fold((0, 0u64), |(count, size), entry| {
                            (count + 1, size.saturating_add(entry.bytes.unwrap_or(0)))
                        }));
                    }
                    if s.quick {
                        s.tool_selection = output
                            .rows
                            .iter()
                            .enumerate()
                            .filter(|(_, r)| {
                                r.action.as_ref().is_some_and(|action| {
                                    action.rule_id().is_some_and(|id| {
                                        if s.review_ids.is_empty() {
                                            r.risk == Some(disk_core::rules::Risk::Safe)
                                        } else {
                                            s.review_ids.contains(id)
                                        }
                                    })
                                })
                            })
                            .map(|(i, _)| i)
                            .collect();
                    }
                    s.store_output(output);
                }
                Event::ToolAction(Ok(outcome)) => {
                    if s.tool == tools::Tool::Caches
                        && (outcome.output.is_some() || outcome.refresh)
                    {
                        s.overview_scan = None;
                    }
                    s.journal = disk_core::history::read(&s.base, 100);
                    if let Some(output) = outcome.output {
                        if s.tool_output.as_ref().is_some_and(|o| o.tree.is_some())
                            && let Some(map) = s.tool_output.take()
                        {
                            s.map_return = Some((map, s.tool_page));
                        }
                        s.tool_selection = output.preselected.iter().copied().collect();
                        s.tool_page = 0;
                        s.filter = TextEdit::default();
                        s.store_output(output);
                    }
                    if outcome.refresh {
                        s.run_tool(s.path.clone());
                    }
                    s.status = outcome.note;
                }
                Event::ToolScan(Err(err)) | Event::ToolAction(Err(err)) => s.set_error(err),
            }
            if s.job.is_some() {
                tx.wake_after(Duration::from_millis(125));
            }
            true
        }
        Err(mpsc::TryRecvError::Disconnected) => {
            s.job = None;
            s.set_error("L’opération s’est interrompue de manière inattendue.".into());
            true
        }
        Err(mpsc::TryRecvError::Empty) => {
            tx.wake_after(Duration::from_millis(125));
            if let Some(progress) = &job.progress {
                let new = (
                    progress.files.load(Relaxed),
                    progress.bytes.load(Relaxed),
                    progress.hashing.load(Relaxed),
                );
                return std::mem::replace(&mut s.scan_progress, new) != new;
            }
            if let Some(job) = &job.core {
                let new = (job.files.load(Relaxed), job.bytes.load(Relaxed), false);
                return std::mem::replace(&mut s.scan_progress, new) != new;
            }
            false
        }
    }
}

fn on_key(s: &mut State, event: &KeyEvent, tx: &mut Tx) -> bool {
    if !s.confirm
        && s.tool_pending.is_empty()
        && s.job.is_none()
        && Chord::parse("mod+o").is_ok_and(|key| key.matches_tx(event, tx))
    {
        s.pick();
        true
    } else {
        false
    }
}

fn demo_report(s: &mut State) {
    s.demo = true;
    s.page = Page::Space;
    s.path = Some(PathBuf::from(r"C:\Exemple\Studio"));
    const GB: u64 = 1024 * 1024 * 1024;
    s.disk = Some((178 * GB, 512 * GB));
    s.report = Some(Report {
        files: 24_816,
        bytes: 84 * GB,
        skipped: 6,
        entries: 3,
        folders: vec![
            ("Projets".into(), 39 * GB),
            ("Médias".into(), 22 * GB),
            ("Archives".into(), 15 * GB),
            ("Exports".into(), 8 * GB),
        ],
        rows: vec![
            Item::example(
                PathBuf::from(r"C:\Exemple\Studio\Archives\studio-2026.zip"),
                13 * GB,
                "Fichier",
            ),
            Item::example(
                PathBuf::from(r"C:\Exemple\Studio\Médias\master-video.mov"),
                9 * GB,
                "Fichier",
            ),
            Item::example(
                PathBuf::from(r"C:\Exemple\Studio\Projets\textures.pack"),
                3 * GB,
                "Fichier",
            ),
        ],
        ..Report::default()
    });
}

fn demo_cleanup(s: &mut State) {
    s.page = Page::Cleanup;
    s.demo = true;
    s.path = Some(PathBuf::from(r"C:\Exemple\AppData\Local\Temp"));
    let rows = [
        ("installation.log", 18_721_881),
        ("preview-cache.tmp", 7_325_696),
    ]
    .into_iter()
    .map(|(name, size)| {
        Item::example(
            s.path.as_ref().unwrap().join(name),
            size,
            "Temporaire · plus de 7 jours",
        )
    })
    .collect::<Vec<_>>();
    s.report = Some(Report {
        entries: rows.len(),
        matched_bytes: rows.iter().map(|item| item.size).sum(),
        rows,
        ..Report::default()
    });
}

fn init() -> State {
    let mut s = State::default();
    match disk_core::settings::Settings::load(&s.base) {
        Ok(config) => {
            s.exclusions.text = config.exclusions.join("\n");
            s.project_roots.text = config.project_roots.join("\n");
            s.config = config;
        }
        Err(e) => {
            s.config_valid = false;
            s.set_error(format!("Réglages illisibles : {e}"));
        }
    }
    s.journal = disk_core::history::read(&s.base, 100);
    s.drive_list = disk_core::system::drives();
    let args = std::env::args().collect::<Vec<_>>();
    if !args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--snap" | "--scene" | "--tree" | "--script"))
    {
        s.launch = if args.get(1).is_some_and(|a| a == "--review-cache") && args.len() > 2 {
            Some(Launch::Review(
                args.iter().skip(2).take(500).cloned().collect(),
            ))
        } else if disk_core::launch::quick_clean_requested(&args) {
            Some(Launch::Quick)
        } else {
            disk_core::shell::analyze_args(&args)
                .into_iter()
                .next()
                .map(Launch::Analyze)
        };
    }
    s
}

fn demo_tools(s: &mut State) {
    s.page = Page::Tools;
    s.tool = tools::Tool::Caches;
    s.demo = true;
    s.config.theme = "light".into();
    let entries = [
        (
            "Cache du navigateur",
            "Navigateurs · 380 fichiers",
            444_000_000,
            disk_core::rules::Risk::Safe,
        ),
        (
            "Cache des téléchargements Cargo",
            "Développeurs · 190 fichiers",
            270_000_000,
            disk_core::rules::Risk::Review,
        ),
        (
            "Miniatures Windows",
            "Système · 55 fichiers",
            18_000_000,
            disk_core::rules::Risk::Safe,
        ),
    ];
    s.tool_output = Some(tools::Output {
        note: "Données fictives : aucune opération réelle ne sera lancée.".into(),
        rows: entries
            .into_iter()
            .map(|(title, detail, bytes, risk)| tools::Entry {
                title: title.into(),
                detail: detail.into(),
                path: None,
                bytes: Some(bytes),
                risk: Some(risk),
                action: None,
            })
            .collect(),
        ..tools::Output::default()
    });
}

fn demo_map_start(s: &mut State) {
    s.page = Page::Tools;
    s.tool = tools::Tool::Map;
    s.demo = true;
    s.config.theme = "light".into();
    const GIB: u64 = 1024 * 1024 * 1024;
    s.drive_list = [
        (r"C:\", "Windows", 512, 72, true),
        (r"D:\", "Projets et médias", 1024, 430, false),
        (r"E:\", "Bibliothèque de jeux", 512, 39, false),
    ]
    .into_iter()
    .map(
        |(root, label, total, free, system)| disk_core::system::Drive {
            root: root.into(),
            label: label.into(),
            file_system: "NTFS".into(),
            total: total * GIB,
            free: free * GIB,
            system,
            removable: false,
        },
    )
    .collect();
}

fn demo_home(s: &mut State) {
    demo_map_start(s);
    s.page = Page::Home;
    s.overview_scan = Some((19, 38_700_000_000));
}

fn demo_map(s: &mut State) {
    use disk_core::{
        categories::Category,
        explorer::{Node, NodeKind, ScanSummary, Tree},
    };
    const GIB: u64 = 1024 * 1024 * 1024;
    demo_map_start(s);
    let root = r"D:\Jeux\Exemple";
    let node = |name: &str, parent, kind, bytes, files, children| Node {
        name: name.into(),
        parent,
        kind,
        bytes,
        files,
        children,
        mtime: 0,
        category: Category::Apps,
        denied: false,
        reclaim_safe: 0,
        reclaim_review: 0,
        marked: None,
        removed: false,
        mount: None,
    };
    s.tool_output = Some(tools::Output {
        gap: Some(disk_core::hidden::HiddenSpace {
            root: r"D:\".into(),
            total: 1024 * GIB,
            free: 430 * GIB,
            used: 594 * GIB,
            scanned: None,
            parts: Vec::new(),
            unexplained: None,
            denied_paths: Vec::new(),
        }),
        tree: Some(Tree {
            root_path: root.into(),
            extensions: Vec::new(),
            summary: ScanSummary {
                root: root.into(),
                bytes: 23 * GIB,
                files: 522,
                dirs: 3,
                denied: 3,
                millis: 0,
                scanned_at: 0,
                cancelled: false,
                cloud_bytes: 0,
                cloud_files: 0,
                selection: Vec::new(),
            },
            nodes: vec![
                node(
                    "Exemple",
                    u32::MAX,
                    NodeKind::Dir,
                    23 * GIB,
                    522,
                    vec![1, 2, 3, 4],
                ),
                node("Game", 0, NodeKind::Dir, 18 * GIB, 370, vec![5]),
                node("Plugins", 0, NodeKind::Dir, 4 * GIB + GIB / 2, 92, vec![]),
                node("libcef.dll", 0, NodeKind::File, GIB / 4, 1, vec![]),
                node("", 0, NodeKind::Small, GIB / 4, 59, vec![]),
                node("Assets.pack", 1, NodeKind::File, 18 * GIB, 1, vec![]),
            ],
        }),
        ..tools::Output::default()
    });
}

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if args.get(1).is_some_and(|a| a == "--admin-rule") {
        std::process::exit(if let [_, _, id, sid] = args.as_slice() {
            tools::admin_entry(id, sid)
        } else {
            2
        });
    }
    rune::run(AppSpec {
        min_size: Some((910.0, 635.0)),
        setup: Some(|s: &mut State, waker| {
            s.images = Some(rune::ImageCache::new(waker.clone()));
            s.waker = Some(waker);
        }),
        poll: Some(poll),
        on_key: Some(on_key),
        scenes: &[
            ("analyse", demo_report),
            ("nettoyage", demo_cleanup),
            ("outils", demo_tools),
            ("carte", demo_map),
            ("carte-depart", demo_map_start),
            ("accueil-editorial", demo_home),
        ],
        ..AppSpec::new("EpicCleaner", (1180.0, 790.0), init, ui::view)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Runtime};

    #[test]
    fn disk_map_start_prioritizes_drives_and_keeps_the_tool_catalog_available() {
        let mut state = State::default();
        demo_map_start(&mut state);
        let mut rt = Runtime::new(state, ui::view, (910.0, 635.0), Env::new(false, 1.0));
        rt.frame(0.0);
        assert!(rt.find("tool-scan").is_some());
        assert!(rt.find(r"drive-D:\").is_some());
        assert!(rt.find("tool-tab-Caches").is_none());
        let (x, y) = rt.find("map-tools").unwrap();
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        assert!(rt.find("tool-tab-Caches").is_some());
    }

    #[test]
    fn disk_map_opens_folders_and_returns_to_the_parent() {
        let mut state = State::default();
        demo_map(&mut state);
        let mut rt = Runtime::new(state, ui::view, (910.0, 635.0), Env::new(false, 1.0));
        rt.frame(0.0);
        let click = |rt: &mut Runtime<State>, key| {
            let (x, y) = rt.find(key).unwrap();
            rt.pointer_down(x, y);
            rt.pointer_up(x, y);
            for _ in 0..30 {
                rt.frame(1.0 / 60.0);
            }
        };
        click(&mut rt, "map-tile-1");
        assert_eq!(rt.state.map_detail, Some(1));
        click(&mut rt, "map-inspector-enter");
        assert_eq!(rt.state.tool_node, 1);
        assert_eq!(rt.state.map_detail, None);
        assert!(rt.find("map-line-5").is_some());
        click(&mut rt, "map-parent");
        assert_eq!(rt.state.tool_node, 0);
        assert!(rt.find("map-line-2").is_some());
    }

    #[test]
    fn navigation_keeps_cleanup_in_preview_until_explicit_action() {
        let mut rt = Runtime::new(
            State::default(),
            ui::view,
            (1180.0, 790.0),
            Env::new(true, 1.0),
        );
        rt.frame(0.0);
        let (x, y) = rt.find("feature-Nettoyage").unwrap();
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.25);
        assert!(rt.state.page == Page::Tools);
        assert!(rt.state.tool == tools::Tool::Caches);
        assert!(rt.find("tool-scan").is_some());
        assert!(rt.state.job.is_none());
        assert!(rt.state.selected.is_empty());
        rt.state.tool_request(Vec::new());
        assert!(rt.state.tool_pending.is_empty());
    }

    #[test]
    fn confirmation_can_be_cancelled_with_escape() {
        let state = State {
            page: Page::Cleanup,
            confirm: true,
            selected: BTreeSet::from([0]),
            ..State::default()
        };
        let mut rt = Runtime::new(state, ui::view, (910.0, 635.0), Env::new(true, 1.0));
        rt.frame(0.0);
        assert!(rt.find("confirm-modal").is_some());
        rt.key(KeyEvent {
            key: rune::Key::Escape,
            mods: rune::Mods::default(),
        });
        rt.frame(0.25);
        assert!(!rt.state.confirm);
        assert!(rt.state.job.is_none());
    }

    #[test]
    fn native_system_action_needs_confirmation_and_can_be_cancelled() {
        let mut state = State {
            page: Page::Tools,
            tool: tools::Tool::Windows,
            ..State::default()
        };
        state.tool_request(vec![tools::Action::Windows(tools::SystemAction::Bin)]);
        assert_eq!(state.tool_pending.len(), 1);
        assert!(state.job.is_none());
        let mut rt = Runtime::new(state, ui::view, (910.0, 635.0), Env::new(true, 1.0));
        rt.frame(0.0);
        assert!(rt.find("tool-modal").is_some());
        rt.key(KeyEvent {
            key: rune::Key::Escape,
            mods: rune::Mods::default(),
        });
        rt.frame(0.25);
        assert!(rt.state.tool_pending.is_empty());
        assert!(rt.state.job.is_none());
    }
}
