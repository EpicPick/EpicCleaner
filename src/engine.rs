use std::{
    cmp::Reverse,
    collections::{BTreeMap, BinaryHeap, HashMap, HashSet},
    fs::{self, File, Metadata},
    io::{self, Read},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed},
    time::{Duration, SystemTime},
};
use walkdir::WalkDir;

const MAX_ROWS: usize = 250;
const MAX_CANDIDATES: usize = 150_000;
const TEMP_AGE: Duration = Duration::from_secs(7 * 86_400);
const MIN_DUP_SIZE: u64 = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Space,
    Duplicates,
    Cleanup,
}

#[derive(Default)]
pub struct Progress {
    pub cancel: AtomicBool,
    pub files: AtomicU64,
    pub bytes: AtomicU64,
    pub hashing: AtomicBool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Stamp {
    size: u64,
    modified: SystemTime,
    created: Option<SystemTime>,
    identity: (u64, u64),
    links: u64,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub path: PathBuf,
    pub size: u64,
    pub detail: String,
    stamp: Option<Stamp>,
}

impl Item {
    pub fn example(path: PathBuf, size: u64, detail: &str) -> Self {
        Self {
            path,
            size,
            detail: detail.into(),
            stamp: None,
        }
    }

    pub fn selectable(&self) -> bool {
        self.stamp.is_some()
    }
}

#[derive(Default)]
pub struct Report {
    pub files: u64,
    pub bytes: u64,
    pub skipped: u64,
    pub folders: Vec<(String, u64)>,
    pub rows: Vec<Item>,
    pub entries: usize,
    pub matched_bytes: u64,
    pub truncated: bool,
}

pub fn temp_root() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let base = PathBuf::from(std::env::var_os("LOCALAPPDATA")?);
        // Only the standard per-user Temp folder, not a user-supplied scan path.
        let profile = PathBuf::from(std::env::var_os("USERPROFILE")?);
        if base != profile.join("AppData").join("Local") {
            return None;
        }
        Some(base.join("Temp"))
    }
    #[cfg(not(windows))]
    None
}

fn special(meta: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // Reparse point, offline, recall on open / data access.
        meta.file_attributes() & (0x400 | 0x1000 | 0x40000 | 0x400000) != 0
    }
    #[cfg(not(windows))]
    meta.file_type().is_symlink()
}

pub(crate) fn checked_path(path: &Path) -> io::Result<PathBuf> {
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(io::Error::other("Chemin local absolu requis."));
    }
    #[cfg(windows)]
    if !disk_path(path) {
        return Err(io::Error::other("Chemin hors disque local."));
    }
    for ancestor in path.ancestors().filter(|p| p.has_root()) {
        if special(&fs::symlink_metadata(ancestor)?) {
            return Err(io::Error::other("Lien, jonction ou fichier hors ligne."));
        }
    }
    let resolved = path.canonicalize()?;
    #[cfg(windows)]
    if !disk_path(&resolved) {
        return Err(io::Error::other("Chemin hors disque local."));
    }
    Ok(resolved)
}

#[cfg(windows)]
fn disk_path(path: &Path) -> bool {
    matches!(
        path.components().next(),
        Some(std::path::Component::Prefix(p))
            if matches!(p.kind(), std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_))
    )
}

fn opened(path: &Path) -> io::Result<File> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    options.open(path)
}

fn stamp(file: &File) -> io::Result<Stamp> {
    let meta = file.metadata()?;
    if !meta.is_file() || special(&meta) {
        return Err(io::Error::other("Fichier protégé ou non ordinaire."));
    }
    #[cfg(windows)]
    let (identity, links) = {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
        };
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        // A live, borrowed File handle and an initialized writable structure.
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            return Err(io::Error::last_os_error());
        }
        (
            (
                u64::from(info.dwVolumeSerialNumber),
                (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
            ),
            u64::from(info.nNumberOfLinks),
        )
    };
    #[cfg(unix)]
    let (identity, links) = {
        use std::os::unix::fs::MetadataExt;
        ((meta.dev(), meta.ino()), meta.nlink())
    };
    Ok(Stamp {
        size: meta.len(),
        modified: meta.modified()?,
        created: meta.created().ok(),
        identity,
        links,
    })
}

fn cancelled(progress: &Progress) -> io::Result<()> {
    if progress.cancel.load(Relaxed) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "Analyse annulée.",
        ))
    } else {
        Ok(())
    }
}

pub fn scan(path: &Path, mode: Mode, progress: &Progress) -> io::Result<Report> {
    let root = checked_path(path)?;
    if !root.is_dir() {
        return Err(io::Error::other("Choisissez un dossier."));
    }
    if mode == Mode::Cleanup
        && temp_root()
            .as_deref()
            .map(checked_path)
            .transpose()?
            .as_ref()
            != Some(&root)
    {
        return Err(io::Error::other(
            "Le nettoyage est limité au dossier temporaire du profil.",
        ));
    }

    let mut report = Report::default();
    let mut groups = HashMap::<String, u64>::new();
    let mut candidates = HashMap::<u64, Vec<PathBuf>>::new();
    let mut candidate_count = 0;
    let mut largest = BinaryHeap::<Reverse<(u64, PathBuf)>>::new();
    let mut walker = WalkDir::new(&root)
        .follow_links(false)
        .follow_root_links(false)
        .into_iter();
    while let Some(entry) = walker.next() {
        cancelled(progress)?;
        let (entry, meta) = match entry.and_then(|e| e.metadata().map(|m| (e, m))) {
            Ok(v) => v,
            Err(_) => {
                report.skipped += 1;
                continue;
            }
        };
        let path = entry.path();
        if special(&meta) || protected_name(path) {
            if meta.is_dir() {
                walker.skip_current_dir();
            }
            report.skipped += 1;
            continue;
        }
        if !meta.is_file() {
            continue;
        }
        let size = meta.len();
        report.files += 1;
        report.bytes = report.bytes.saturating_add(size);
        progress.files.store(report.files, Relaxed);
        progress.bytes.store(report.bytes, Relaxed);
        match mode {
            Mode::Space => {
                let group = if entry.depth() == 1 {
                    "Fichiers à la racine".into()
                } else {
                    path.strip_prefix(&root)
                        .unwrap_or(path)
                        .components()
                        .next()
                        .map(|part| part.as_os_str().to_string_lossy().into_owned())
                        .unwrap_or_default()
                };
                let total = groups.entry(group).or_default();
                *total = total.saturating_add(size);
                if largest.len() < MAX_ROWS || largest.peek().is_some_and(|r| r.0.0 < size) {
                    largest.push(Reverse((size, path.to_owned())));
                    if largest.len() > MAX_ROWS {
                        largest.pop();
                    }
                }
            }
            Mode::Duplicates if size >= MIN_DUP_SIZE => {
                if candidate_count < MAX_CANDIDATES {
                    candidates.entry(size).or_default().push(path.to_owned());
                    candidate_count += 1;
                } else {
                    report.truncated = true;
                }
            }
            Mode::Cleanup if size > 0 && old(meta.modified().ok()) => {
                if report.rows.len() >= MAX_ROWS {
                    report.truncated = true;
                    continue;
                }
                match inspect(path) {
                    Ok(stamp)
                        if stamp.size > 0 && stamp.links == 1 && old(Some(stamp.modified)) =>
                    {
                        report.matched_bytes = report.matched_bytes.saturating_add(stamp.size);
                        report.rows.push(Item {
                            path: path.into(),
                            size: stamp.size,
                            detail: "Temporaire · plus de 7 jours".into(),
                            stamp: Some(stamp),
                        });
                    }
                    _ => report.skipped += 1,
                }
            }
            _ => {}
        }
    }

    match mode {
        Mode::Space => {
            report.folders = groups.into_iter().collect();
            report
                .folders
                .sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            report.folders.truncate(8);
            report.rows = largest
                .into_iter()
                .map(|Reverse((size, path))| Item::example(path, size, "Fichier"))
                .collect();
            report.truncated = report.files > MAX_ROWS as u64;
        }
        Mode::Duplicates => {
            progress.hashing.store(true, Relaxed);
            find_duplicates(candidates, &mut report, progress)?;
        }
        Mode::Cleanup => {}
    }
    if mode != Mode::Duplicates {
        report
            .rows
            .sort_unstable_by(|a, b| b.size.cmp(&a.size).then(a.path.cmp(&b.path)));
        report.entries = report.rows.len();
    }
    cancelled(progress)?;
    Ok(report)
}

fn protected_name(path: &Path) -> bool {
    path.file_name().is_some_and(|name| {
        [".git", "$Recycle.Bin", "System Volume Information"]
            .iter()
            .any(|protected| name.eq_ignore_ascii_case(protected))
    })
}

fn old(modified: Option<SystemTime>) -> bool {
    modified
        .and_then(|m| m.elapsed().ok())
        .is_some_and(|age| age >= TEMP_AGE)
}

fn inspect(path: &Path) -> io::Result<Stamp> {
    stamp(&opened(&checked_path(path)?)?)
}

fn hash(path: &Path, size: u64, progress: &Progress) -> io::Result<(Stamp, blake3::Hash)> {
    let file = opened(&checked_path(path)?)?;
    let before = stamp(&file)?;
    if before.size != size {
        return Err(io::Error::other("Taille modifiée."));
    }
    let mut reader = (&file).take(size.saturating_add(1));
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0; 64 * 1024];
    let mut total = 0;
    loop {
        cancelled(progress)?;
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        hasher.update(&buffer[..n]);
    }
    if total != size || stamp(&file)? != before {
        return Err(io::Error::other("Fichier modifié pendant la lecture."));
    }
    Ok((before, hasher.finalize()))
}

fn find_duplicates(
    candidates: HashMap<u64, Vec<PathBuf>>,
    report: &mut Report,
    progress: &Progress,
) -> io::Result<()> {
    for (size, mut paths) in candidates
        .into_iter()
        .collect::<BTreeMap<_, _>>()
        .into_iter()
        .rev()
    {
        cancelled(progress)?;
        if paths.len() < 2 {
            continue;
        }
        paths.sort_unstable();
        let mut seen = HashSet::new();
        let mut matches = BTreeMap::<[u8; 32], Vec<PathBuf>>::new();
        for path in paths {
            cancelled(progress)?;
            match hash(&path, size, progress) {
                Ok((stamp, digest)) if seen.insert(stamp.identity) => {
                    matches.entry(*digest.as_bytes()).or_default().push(path);
                }
                Ok(_) => {}
                Err(err) if err.kind() == io::ErrorKind::Interrupted => return Err(err),
                Err(_) => report.skipped += 1,
            }
        }
        for paths in matches.into_values().filter(|p| p.len() > 1) {
            report.entries += 1;
            report.matched_bytes = report
                .matched_bytes
                .saturating_add(size.saturating_mul((paths.len() - 1) as u64));
            let number = report.entries;
            let room = MAX_ROWS - report.rows.len();
            if room < paths.len() {
                report.truncated = true;
            }
            if room >= 2 {
                report.rows.extend(
                    paths
                        .into_iter()
                        .take(room)
                        .map(|p| Item::example(p, size, &format!("Groupe {number} · BLAKE3"))),
                );
            }
        }
    }
    Ok(())
}

#[derive(Default)]
pub struct Receipt {
    pub moved: usize,
    pub failures: Vec<(PathBuf, String)>,
}

pub fn recycle(items: Vec<Item>) -> Receipt {
    let mut receipt = Receipt::default();
    for item in items {
        let outcome = (|| -> Result<(), String> {
            let root = temp_root().ok_or("Dossier temporaire du profil introuvable.")?;
            let root = checked_path(&root).map_err(|e| e.to_string())?;
            let file = checked_path(&item.path).map_err(|e| e.to_string())?;
            validate(&root, &file, item.stamp.as_ref()).map_err(|e| e.to_string())?;
            send_to_bin(&file)
        })();
        match outcome {
            Ok(()) => receipt.moved += 1,
            Err(e) => receipt.failures.push((item.path, e)),
        }
    }
    receipt
}

#[cfg(windows)]
fn send_to_bin(file: &Path) -> Result<(), String> {
    use std::{os::windows::ffi::OsStrExt, path::Component};
    use windows::{
        Win32::{
            System::Com::{
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
                CoUninitialize, IBindCtx,
            },
            UI::Shell::{
                FOF_ALLOWUNDO, FOF_NO_CONNECTED_ELEMENTS, FOF_NOERRORUI, FOF_NORECURSION,
                FOF_SILENT, FOF_WANTNUKEWARNING, FOFX_EARLYFAILURE, FOFX_RECYCLEONDELETE,
                FileOperation, IFileOperation, IFileOperationProgressSink, IShellItem,
                SHCreateItemFromParsingName,
            },
        },
        core::PCWSTR,
    };

    // The Shell expects ordinary Win32 paths. Refuse names whose interpretation
    // may change when the verbatim prefix is removed.
    for part in file.components() {
        if let Component::Normal(name) = part {
            let units: Vec<u16> = name.encode_wide().collect();
            if units.last().is_some_and(|ch| matches!(ch, 32 | 46)) || units.contains(&58) {
                return Err("Nom de fichier ambigu pour l’Explorateur Windows.".into());
            }
        }
    }
    let mut wide: Vec<u16> = file.as_os_str().encode_wide().collect();
    if wide.starts_with(&[92, 92, 63, 92]) {
        wide.drain(..4);
    }
    wide.push(0);
    // The dedicated worker thread owns the COM apartment. A successful initialization
    // (including S_FALSE) is paired with CoUninitialize after the interfaces are dropped.
    unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() }.map_err(|e| e.to_string())?;
    struct Com;
    impl Drop for Com {
        fn drop(&mut self) {
            // This value is only created on the worker after CoInitializeEx succeeds.
            unsafe { CoUninitialize() };
        }
    }
    let _com = Com;
    // `wide` is NUL-terminated and lives through the shell call. The COM interfaces are
    // created and used only in this apartment. There is no permanent-delete fallback.
    let result = unsafe {
        (|| -> windows::core::Result<bool> {
            let operation: IFileOperation =
                CoCreateInstance(&FileOperation, None, CLSCTX_INPROC_SERVER)?;
            operation.SetOperationFlags(
                FOFX_RECYCLEONDELETE
                    | FOF_ALLOWUNDO
                    | FOF_WANTNUKEWARNING
                    | FOF_NO_CONNECTED_ELEMENTS
                    | FOF_NORECURSION
                    | FOF_SILENT
                    | FOF_NOERRORUI
                    | FOFX_EARLYFAILURE,
            )?;
            let entry: IShellItem =
                SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None::<&IBindCtx>)?;
            operation.DeleteItem(&entry, None::<&IFileOperationProgressSink>)?;
            operation.PerformOperations()?;
            Ok(!operation.GetAnyOperationsAborted()?.as_bool())
        })()
    }
    .map_err(|e| e.to_string())?;
    if result && matches!(file.try_exists(), Ok(false)) {
        Ok(())
    } else {
        Err("Windows n’a pas confirmé le déplacement vers la Corbeille.".into())
    }
}

#[cfg(not(windows))]
fn send_to_bin(_: &Path) -> Result<(), String> {
    Err("Corbeille disponible uniquement sous Windows.".into())
}

fn validate(root: &Path, file: &Path, expected: Option<&Stamp>) -> io::Result<()> {
    let relative = file
        .strip_prefix(root)
        .map_err(|_| io::Error::other("Hors du dossier autorisé."))?;
    if relative.as_os_str().is_empty() || relative.ancestors().any(protected_name) {
        return Err(io::Error::other("Emplacement protégé."));
    }
    let current = inspect(file)?;
    if Some(&current) != expected || current.links != 1 || !old(Some(current.modified)) {
        return Err(io::Error::other(
            "Fichier non approuvé ou modifié depuis l’aperçu.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analysis_orders_sizes_and_aggregates() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("media")).unwrap();
        fs::write(dir.path().join("media/a"), [0; 24]).unwrap();
        fs::write(dir.path().join("small"), [0; 10]).unwrap();
        let report = scan(dir.path(), Mode::Space, &Progress::default()).unwrap();
        assert_eq!((report.files, report.bytes), (2, 34));
        assert_eq!(report.folders[0], ("media".into(), 24));
        assert_eq!(report.rows[0].size, 24);
    }

    #[test]
    fn full_hash_rejects_same_size_and_hardlinks_are_not_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a"), [0; 8192]).unwrap();
        fs::hard_link(dir.path().join("a"), dir.path().join("alias")).unwrap();
        fs::write(dir.path().join("b"), [0; 8192]).unwrap();
        fs::write(dir.path().join("different"), [1; 8192]).unwrap();
        let report = scan(dir.path(), Mode::Duplicates, &Progress::default()).unwrap();
        assert_eq!(
            (report.entries, report.rows.len(), report.matched_bytes),
            (1, 2, 8192)
        );
    }

    #[test]
    fn cancellation_and_unsafe_cleanup_scope_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let progress = Progress::default();
        progress.cancel.store(true, Relaxed);
        assert_eq!(
            scan(dir.path(), Mode::Space, &progress)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::Interrupted
        );
        assert!(scan(dir.path(), Mode::Cleanup, &Progress::default()).is_err());
    }

    #[test]
    fn cleanup_requires_unchanged_single_link_file_in_scope() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().canonicalize().unwrap().join("temp.bin");
        fs::write(&file, [0; 16]).unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&file)
            .unwrap()
            .set_modified(SystemTime::now() - 2 * TEMP_AGE)
            .unwrap();
        let approved = inspect(&file).unwrap();
        assert!(validate(file.parent().unwrap(), &file, Some(&approved)).is_ok());
        assert!(validate(&file.with_extension("x"), &file, Some(&approved)).is_err());
        assert!(validate(file.parent().unwrap(), &file, None).is_err());
        fs::write(&file, [0; 32]).unwrap();
        assert!(validate(file.parent().unwrap(), &file, Some(&approved)).is_err());
    }
}
