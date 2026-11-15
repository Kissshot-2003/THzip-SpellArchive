use thzip_core::error::{ArchiveError, FailReason, NameIssue};
use thzip_core::safety::{LimitHit, SanitizeError};

use super::Locale;

/// 把结构化失败原因翻译成界面文案。中文直接复用 core 的兜底文案，
/// 英文在此处翻译；参数（路径、方法名、库原始信息）不翻译。
pub fn reason(locale: Locale, reason: &FailReason) -> String {
    match locale {
        Locale::ZhCn => reason.zh(),
        Locale::EnUs => reason_en(reason),
    }
}

#[allow(dead_code)]
pub fn error(locale: Locale, error: &ArchiveError) -> String {
    reason(locale, &error.reason())
}

pub(crate) fn limit_en(hit: LimitHit) -> &'static str {
    match hit {
        LimitHit::TotalBytes => "total extracted size",
        LimitHit::EntryBytes => "single entry size",
        LimitHit::EntryCount => "entry count",
        LimitHit::Ratio => "compression ratio (possible zip bomb)",
        LimitHit::DictionarySize => "dictionary size",
        LimitHit::EntryPathDepth => "archive path depth",
        LimitHit::EntryNameLength => "archive entry name length",
        LimitHit::ScanDepth => "input path depth",
        LimitHit::ScanEntries => "input entry count",
        LimitHit::Nesting => "archive nesting depth",
        LimitHit::FreeSpace => "free space on the target drive",
    }
}

pub(crate) fn issue_en(issue: SanitizeError) -> String {
    match issue {
        SanitizeError::Empty => "entry name is empty".to_owned(),
        SanitizeError::Absolute => "absolute paths are rejected".to_owned(),
        SanitizeError::Parent => "paths containing `..` are rejected".to_owned(),
        SanitizeError::DepthExceeded => "path depth exceeds the limit".to_owned(),
        SanitizeError::PathTooLong => "path is too long".to_owned(),
        SanitizeError::NameTooLong => "file name is too long".to_owned(),
        SanitizeError::Reserved => "file name is a Windows reserved device name".to_owned(),
        SanitizeError::IllegalChar(ch) => {
            format!("file name contains an illegal character {ch:?}")
        }
    }
}

fn name_issue_en(issue: NameIssue) -> &'static str {
    match issue {
        NameIssue::Empty => "Archive entry name is empty",
        NameIssue::Absolute => "Archive entry name must not be an absolute path",
        NameIssue::Parent => "Archive entry name must not contain `..`",
        NameIssue::DriveOrAds => {
            "Archive entry name must not contain a drive letter or NTFS alternate data stream"
        }
        NameIssue::Nul => "Archive entry name must not contain NUL",
        NameIssue::Reserved => "Archive entry name is a Windows reserved device name",
        NameIssue::IllegalChar(_) => "Archive entry name contains an illegal character",
        NameIssue::NameTooLong => "Archive entry name exceeds the file system limit",
    }
}

fn reason_en(reason: &FailReason) -> String {
    match reason {
        FailReason::Raw(text) => text.clone(),
        FailReason::PasswordRequired => "Password required".to_owned(),
        FailReason::BadPassword => "Incorrect password".to_owned(),
        FailReason::OutOfSpace => "Not enough disk space".to_owned(),
        FailReason::InsufficientSpace {
            required,
            reserved,
            available,
        } => format!(
            "Not enough space on the target drive: need {required} bytes, {reserved} bytes must stay free, {available} bytes available"
        ),
        FailReason::Cancelled => "Cancelled".to_owned(),
        FailReason::Io { detail } => detail.clone(),
        FailReason::Limit(hit) => format!("Security limit reached: {}", limit_en(*hit)),
        FailReason::EntryCorrupt { entry, cause } => match entry {
            Some(id) => format!("Corrupt entry {id}: {}", reason_en(cause)),
            None => format!("Corrupt entry: {}", reason_en(cause)),
        },
        FailReason::BackendMissing => "This format's backend is not implemented yet".to_owned(),
        FailReason::BackendMissingWrite => "Writing this format is not implemented yet".to_owned(),
        FailReason::UnknownFormat => {
            "Unrecognized archive format (magic bytes mismatch)".to_owned()
        }
        FailReason::CreateUnsupported { format } => {
            format!("Creating {format} archives is not supported")
        }
        FailReason::AppendUnsupported { format } => {
            format!("Appending to {format} archives is not supported")
        }
        FailReason::RewriteUnsupported { format } => {
            format!("Modifying {format} archives is not supported")
        }
        FailReason::EncryptedRewrite => "Encrypted archives cannot be modified".to_owned(),
        FailReason::RewriteExtractFailed { count } => {
            format!("Failed to extract {count} retained entries; rewrite cancelled")
        }
        FailReason::RewriteUnsupportedEntries { count } => {
            format!(
                "{count} entries cannot be written back (symlinks or unsupported methods); rewrite cancelled"
            )
        }
        FailReason::EncryptUnsupported { format } => {
            format!("{format} does not support encryption")
        }
        FailReason::NotDecompressible { format } => {
            format!("Not a compressed stream: {format}")
        }
        FailReason::SignatureInvalid { format } => format!("Invalid {format} signature"),
        FailReason::CentralDirectoryMissing => "Entry not found in central directory".to_owned(),
        FailReason::UnsupportedMethod { format, method } => {
            format!("{format} does not support compression method {method}")
        }
        FailReason::EncryptedXzZipEntry => "Encrypted XZ zip entries are not supported".to_owned(),
        FailReason::EncryptedLzmaZipEntry => {
            "Encrypted LZMA zip entries are not supported".to_owned()
        }
        FailReason::ChecksumFailed { format } => {
            format!("{format} checksum verification failed")
        }
        FailReason::CorruptLzmaHeader => "Corrupt zip LZMA header".to_owned(),
        FailReason::WriterClosed { format } => format!("{format} writer is already closed"),
        FailReason::ArchiveClosed { format } => format!("{format} archive is already closed"),
        FailReason::EntryMissing { id } => format!("Entry {id} does not exist"),
        FailReason::RenameTargetMissing { id } => format!("Entry {id} has no target name"),
        FailReason::EntryCountMismatch { format } => {
            format!("{format} entry count does not match the listing")
        }
        FailReason::EntryReadFailed { detail } => format!("Failed to read entry: {detail}"),
        FailReason::EntryExtractFailed { detail } => format!("Failed to extract: {detail}"),
        FailReason::EntryVerifyFailed { detail } => format!("Verification failed: {detail}"),
        FailReason::EntryCrcFailed { detail } => {
            format!("CRC/data verification failed: {detail}")
        }
        FailReason::StreamReadFailed { detail } => {
            format!("Failed to read compressed stream: {detail}")
        }
        FailReason::StagedReadFailed { detail } => {
            format!("Failed to read RAR staging file: {detail}")
        }
        FailReason::RarFailed { detail } => format!("RAR processing failed: {detail}"),
        FailReason::VolumeNameInvalid => "Invalid volume file name".to_owned(),
        FailReason::VolumeNoParent => "Volume has no parent directory".to_owned(),
        FailReason::VolumeIncomplete { missing } => {
            format!("Incomplete volume set, missing: {missing}")
        }
        FailReason::MoveIntoSelf { name } => {
            format!("Cannot move directory {name} into itself")
        }
        FailReason::RenameDuplicate { name } => format!("Duplicate name after rename: {name}"),
        FailReason::DuplicateArchiveName { name } => {
            format!("Duplicate name in archive (case-insensitive): {name}")
        }
        FailReason::DuplicateInputName { name } => {
            format!("Duplicate input name (case-insensitive): {name}")
        }
        FailReason::NoInputs => "No files selected to pack".to_owned(),
        FailReason::InputNotRoot { path } => {
            format!("Cannot use {path} as an archive root")
        }
        FailReason::ReadFailed { path, detail } => {
            format!("Failed to read {path}: {detail}")
        }
        FailReason::ListFailed { path, detail } => {
            format!("Failed to list {path}: {detail}")
        }
        FailReason::NotRegularFile { path } => format!("Not a regular file: {path}"),
        FailReason::SymlinkNotAllowed { path } => {
            format!("Skipping symbolic link: {path}")
        }
        FailReason::UnsafeName { name, issue } => {
            format!("Unsafe name {name}: {}", issue_en(*issue))
        }
        FailReason::UnsafeTarget { issue } => {
            format!("Unsafe target path: {}", issue_en(*issue))
        }
        FailReason::TargetIsReparsePoint => {
            "Target exists and is a reparse point (junction/symlink)".to_owned()
        }
        FailReason::ReparsePointWrite { path } => {
            format!("{path} is a reparse point; refusing to write")
        }
        FailReason::ReparsePointOverwrite { path } => {
            format!("{path} exists and is a reparse point; refusing to overwrite")
        }
        FailReason::InvalidName(issue) => name_issue_en(*issue).to_owned(),
        FailReason::AppendPasswordUnsupported => {
            "Append mode does not support setting a new password".to_owned()
        }
        FailReason::AppendUnsupportedEntries { count } => format!(
            "The archive contains {count} entries that cannot be preserved; append cancelled"
        ),
        FailReason::ThemeManifestMissing => {
            "Invalid theme pack: expected <theme>/theme.toml".to_owned()
        }
        FailReason::ThemeTooManyEntries { count, limit } => {
            format!("Too many theme pack entries: {count} (limit {limit})")
        }
        FailReason::ThemeFileTooLarge { name, size } => {
            format!("Theme file {name} exceeds 8 MiB ({size} bytes)")
        }
        FailReason::ThemeTotalTooLarge => "Theme pack exceeds 64 MiB after extraction".to_owned(),
        FailReason::ThemeForbiddenFile { name } => {
            format!("Theme pack contains a forbidden file type: {name}")
        }
        FailReason::Panic { message } => format!("Task crashed: {message}"),
        FailReason::DecisionNotWrite { id } => {
            format!("Entry {id} decision is not write; backend must not call on_entry_begin")
        }
        FailReason::DirHasData { id } => {
            format!("Directory entry {id} must not carry data")
        }
        FailReason::EntryNotBegun { id } => {
            format!("Entry {id} did not call on_entry_begin")
        }
        FailReason::EntryNotStarted { id } => format!("Entry {id} has not started"),
        FailReason::EntryOrder => "Entry order mismatch".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::{error, reason};
    use crate::i18n::Locale;
    use thzip_core::error::{ArchiveError, FailReason, NameIssue};

    #[test]
    fn translates_core_errors_to_english() {
        let open = ArchiveError::Open(FailReason::UnknownFormat);
        assert_eq!(
            error(Locale::EnUs, &open),
            "Unrecognized archive format (magic bytes mismatch)"
        );
        assert_eq!(
            error(Locale::ZhCn, &open),
            "无法识别归档格式（magic bytes 不匹配）"
        );

        let missing = FailReason::EntryMissing { id: 7 };
        assert!(reason(Locale::EnUs, &missing).contains('7'));
        assert!(reason(Locale::ZhCn, &missing).contains('7'));
    }

    #[test]
    fn translates_password_related_flags() {
        assert!(FailReason::BadPassword.is_password_related());
        assert!(!FailReason::Cancelled.is_password_related());
        assert!(ArchiveError::PasswordRequired.is_password_related());
        assert!(
            !ArchiveError::Other(FailReason::InvalidName(NameIssue::Empty)).is_password_related()
        );
    }

    #[test]
    fn english_is_ascii_for_static_reasons() {
        let text = reason(Locale::EnUs, &FailReason::BackendMissing);
        assert!(text.is_ascii(), "{text}");
        assert!(!text.is_empty());
    }
}
