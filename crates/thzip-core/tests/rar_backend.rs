use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use thzip_core::backend;
use thzip_core::limits::Limits;
use thzip_core::plan::{DecisionTable, OverwritePolicy};
use thzip_core::safety::{ExtractDirSink, VerifySink};
use thzip_core::secret::SecretString;
use thzip_core::task::CancelToken;
use thzip_core::types::{EntryId, Method};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "thzip-spellarchive-rar-{tag}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn extract_all(
    archive: &Path,
    target: &Path,
    password: Option<&SecretString>,
) -> Result<thzip_core::ExtractReport, thzip_core::ArchiveError> {
    let limits = Limits::default();
    let mut reader = backend::open(archive, password)?;
    let ids: Vec<EntryId> = (0..reader.entries().len() as EntryId).collect();
    let plan = reader.plan(&ids, target, &limits)?;
    let decisions = DecisionTable {
        default: OverwritePolicy::Overwrite,
        ..Default::default()
    };
    let cancel = CancelToken::new();
    let mut sink = ExtractDirSink::new(&plan, &decisions, limits);
    reader.extract(&plan, &decisions, &cancel, &mut sink)
}

#[test]
fn lists_and_extracts_simple_rar() {
    let archive = fixture("version.rar");
    let reader = backend::open(&archive, None).unwrap();
    assert_eq!(reader.kind(), thzip_core::ArchiveKind::Rar);
    assert_eq!(reader.entries().len(), 1);
    assert_eq!(reader.entries()[0].display.as_ref(), "VERSION");
    assert_eq!(reader.entries()[0].size, 11);
    assert_eq!(reader.entries()[0].method, Method::Rar);

    let dir = TempDir::new("simple");
    let target = dir.path().join("out");
    let report = extract_all(&archive, &target, None).unwrap();
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(
        std::fs::read_to_string(target.join("VERSION")).unwrap(),
        "unrar-0.4.0"
    );

    let mut reader = backend::open(&archive, None).unwrap();
    let cancel = CancelToken::new();
    let mut sink = VerifySink::with_limits(Limits::default());
    let report = reader.test(&cancel, &mut sink).unwrap();
    assert_eq!(report.failed.len(), 0);
    assert_eq!(report.crc_mismatch.len(), 0);
}

#[test]
fn encrypted_rar_requires_password() {
    let archive = fixture("crypted.rar");
    let dir = TempDir::new("crypted");

    let reader = backend::open(&archive, None).unwrap();
    let ids: Vec<EntryId> = (0..reader.entries().len() as EntryId).collect();
    let plan = reader
        .plan(&ids, &dir.path().join("out"), &Limits::default())
        .unwrap();
    assert!(plan.needs_password);

    let target = dir.path().join("no-password");
    let err = extract_all(&archive, &target, None).unwrap_err();
    assert!(
        matches!(err, thzip_core::ArchiveError::PasswordRequired),
        "{err:?}"
    );

    let password = SecretString::new("unrar".to_owned());
    let target = dir.path().join("with-password");
    let report = extract_all(&archive, &target, Some(&password)).unwrap();
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert_eq!(
        std::fs::read_to_string(target.join(".gitignore")).unwrap(),
        "target\nCargo.lock\n"
    );
}

#[test]
fn solid_rar_extracts() {
    let archive = fixture("solid.rar");
    let reader = backend::open(&archive, None).unwrap();
    assert!(!reader.entries().is_empty());

    let dir = TempDir::new("solid");
    let target = dir.path().join("out");
    let report = extract_all(&archive, &target, None).unwrap();
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    assert!(report.succeeded.len() >= reader.entries().len());
}
