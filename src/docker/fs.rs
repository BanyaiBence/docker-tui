//! Container filesystem helpers: `ls` parsing, tar listing, previews, diffs.

use bollard::models::{ChangeType, FilesystemChange};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Dir,
    File,
    Symlink,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsEntry {
    pub name: String,
    pub kind: FileKind,
    pub size: u64,
    /// Permission string such as `drwxr-xr-x`.
    pub mode: String,
    /// Modification time as printed by `ls`, or derived from tar headers.
    pub modified: String,
    pub link_target: Option<String>,
}

/// Parse `ls -la` output (GNU coreutils or BusyBox). Skips `.` and `..`;
/// directories sort first, then by name.
pub fn parse_ls(output: &str) -> Vec<FsEntry> {
    let mut entries: Vec<FsEntry> = output.lines().filter_map(parse_ls_line).collect();
    sort_entries(&mut entries);
    entries
}

fn sort_entries(entries: &mut [FsEntry]) {
    entries.sort_by(|a, b| {
        (a.kind != FileKind::Dir, &a.name).cmp(&(b.kind != FileKind::Dir, &b.name))
    });
}

fn kind_from_char(c: char) -> FileKind {
    match c {
        'd' => FileKind::Dir,
        '-' => FileKind::File,
        'l' => FileKind::Symlink,
        _ => FileKind::Other,
    }
}

/// Byte ranges of whitespace-separated tokens.
fn token_spans(line: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start = None;
    for (i, c) in line.char_indices() {
        match (c.is_whitespace(), start) {
            (false, None) => start = Some(i),
            (true, Some(s)) => {
                spans.push((s, i));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        spans.push((s, line.len()));
    }
    spans
}

fn parse_ls_line(line: &str) -> Option<FsEntry> {
    let spans = token_spans(line);
    let tok = |i: usize| spans.get(i).map(|&(s, e)| &line[s..e]);
    let mode = tok(0)?;
    let kind_char = mode.chars().next()?;
    if mode.len() < 10 || !"-dlcbps".contains(kind_char) {
        return None;
    }
    // mode, links, owner, group, then size — or "major, minor" for devices.
    let mut i = 4;
    let size = if tok(i)?.ends_with(',') {
        i += 2;
        0
    } else {
        i += 1;
        tok(i - 1)?.parse().ok()?
    };
    // Month, day, and time-or-year.
    let (date_start, _) = *spans.get(i)?;
    let (_, date_end) = *spans.get(i + 2)?;
    let name_start = spans.get(i + 3)?.0;
    let modified = line[date_start..date_end].to_string();
    let rest = &line[(date_end + 1).min(name_start)..];

    let kind = kind_from_char(kind_char);
    let (name, link_target) = match (kind, rest.split_once(" -> ")) {
        (FileKind::Symlink, Some((n, t))) => (n, Some(t.to_string())),
        _ => (rest, None),
    };
    if name == "." || name == ".." {
        return None;
    }
    Some(FsEntry {
        name: name.to_string(),
        kind,
        size,
        mode: mode.to_string(),
        modified,
        link_target,
    })
}

/// List the direct children of the archive's root directory, as returned
/// by `GET /containers/{id}/archive?path=<dir>`.
pub fn list_tar(bytes: &[u8]) -> Result<Vec<FsEntry>, String> {
    let mut archive = tar::Archive::new(bytes);
    let mut entries = Vec::new();
    let mut root: Option<String> = None;
    for entry in archive.entries().map_err(|e| e.to_string())? {
        // A capped download ends mid-archive; keep what was read.
        let Ok(entry) = entry else { break };
        let Ok(path) = entry.path() else { continue };
        let path = path.to_string_lossy().trim_end_matches('/').to_string();
        let root = root.get_or_insert_with(|| path.clone());
        let Some(rel) = path
            .strip_prefix(root.as_str())
            .and_then(|r| r.strip_prefix('/'))
        else {
            continue;
        };
        if rel.is_empty() || rel.contains('/') {
            continue;
        }
        let header = entry.header();
        let kind = match header.entry_type() {
            tar::EntryType::Directory => FileKind::Dir,
            tar::EntryType::Regular | tar::EntryType::Continuous => FileKind::File,
            tar::EntryType::Symlink => FileKind::Symlink,
            _ => FileKind::Other,
        };
        let kind_char = match kind {
            FileKind::Dir => 'd',
            FileKind::File => '-',
            FileKind::Symlink => 'l',
            FileKind::Other => '?',
        };
        entries.push(FsEntry {
            name: rel.to_string(),
            kind,
            size: header.size().unwrap_or(0),
            mode: format!(
                "{kind_char}{}",
                permission_string(header.mode().unwrap_or(0))
            ),
            modified: header.mtime().map(format_unix).unwrap_or_default(),
            link_target: entry
                .link_name()
                .ok()
                .flatten()
                .map(|l| l.to_string_lossy().into_owned()),
        });
    }
    sort_entries(&mut entries);
    Ok(entries)
}

fn permission_string(mode: u32) -> String {
    (0..9)
        .map(|i| {
            let bit = 1 << (8 - i);
            if mode & bit == 0 {
                '-'
            } else {
                ['r', 'w', 'x'][i % 3]
            }
        })
        .collect()
}

/// `YYYY-MM-DD HH:MM` (UTC) for a unix timestamp.
fn format_unix(ts: u64) -> String {
    let days = (ts / 86_400) as i64;
    let secs = ts % 86_400;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        secs / 3600,
        secs % 3600 / 60
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePreview {
    /// UTF-8 text, or a hex dump for binary files.
    pub content: String,
    pub binary: bool,
    /// Full file size according to the tar header.
    pub size: u64,
    /// Whether `content` covers only the start of the file.
    pub truncated: bool,
}

/// Preview the single file in a (possibly truncated) archive, reading at most
/// `max` bytes of content.
pub fn preview_tar(bytes: &[u8], max: usize) -> Result<FilePreview, String> {
    use std::io::Read;

    let mut archive = tar::Archive::new(bytes);
    let mut entry = archive
        .entries()
        .map_err(|e| e.to_string())?
        .next()
        .ok_or("empty archive")?
        .map_err(|e| e.to_string())?;
    if entry.header().entry_type() == tar::EntryType::Directory {
        return Err("is a directory".into());
    }
    let size = entry.header().size().unwrap_or(0);
    let mut data = vec![0; max];
    let mut read = 0;
    while read < max {
        match entry.read(&mut data[read..]) {
            Ok(0) | Err(_) => break,
            Ok(n) => read += n,
        }
    }
    data.truncate(read);
    let truncated = (read as u64) < size;

    let text = match std::str::from_utf8(&data) {
        Ok(s) => Some(s),
        // A multi-byte character cut off by the cap is still text.
        Err(e) if e.error_len().is_none() => std::str::from_utf8(&data[..e.valid_up_to()]).ok(),
        Err(_) => None,
    };
    let binary = data.contains(&0) || text.is_none();
    let content = match text {
        Some(t) if !binary => t.to_string(),
        _ => hexdump(&data[..data.len().min(4096)]),
    };
    Ok(FilePreview {
        content,
        binary,
        size,
        truncated,
    })
}

/// Classic `hexdump -C` style output.
pub fn hexdump(bytes: &[u8]) -> String {
    let mut out = String::new();
    for (row, chunk) in bytes.chunks(16).enumerate() {
        out.push_str(&format!("{:08x}  ", row * 16));
        for i in 0..16 {
            match chunk.get(i) {
                Some(b) => out.push_str(&format!("{b:02x} ")),
                None => out.push_str("   "),
            }
            if i == 7 {
                out.push(' ');
            }
        }
        let ascii: String = chunk
            .iter()
            .map(|&b| {
                if b.is_ascii_graphic() || b == b' ' {
                    b as char
                } else {
                    '.'
                }
            })
            .collect();
        out.push_str(&format!(" |{ascii}|\n"));
    }
    out
}

/// Join a directory and a child name into an absolute path.
pub fn join(dir: &str, name: &str) -> String {
    format!("{}/{name}", dir.trim_end_matches('/'))
}

/// Parent directory of an absolute path; `/` is its own parent.
pub fn parent(path: &str) -> String {
    match path.trim_end_matches('/').rsplit_once('/') {
        Some(("", _)) | None => "/".into(),
        Some((p, _)) => p.into(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsChange {
    pub path: String,
    pub kind: ChangeKind,
}

impl From<FilesystemChange> for FsChange {
    fn from(c: FilesystemChange) -> Self {
        let kind = match c.kind {
            ChangeType::_0 => ChangeKind::Modified,
            ChangeType::_1 => ChangeKind::Added,
            ChangeType::_2 => ChangeKind::Deleted,
        };
        FsChange { path: c.path, kind }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUSYBOX: &str = include_str!("../../tests/fixtures/ls_busybox.txt");
    const BUSYBOX_DEV: &str = include_str!("../../tests/fixtures/ls_busybox_dev.txt");
    const GNU: &str = include_str!("../../tests/fixtures/ls_gnu.txt");
    const GNU_DEV: &str = include_str!("../../tests/fixtures/ls_gnu_dev.txt");

    fn find<'a>(entries: &'a [FsEntry], name: &str) -> &'a FsEntry {
        entries
            .iter()
            .find(|e| e.name == name)
            .unwrap_or_else(|| panic!("{name} missing"))
    }

    #[test]
    fn parses_busybox_root() {
        let entries = parse_ls(BUSYBOX);
        assert!(entries.iter().all(|e| e.name != "." && e.name != ".."));
        let etc = find(&entries, "etc");
        assert_eq!(etc.kind, FileKind::Dir);
        assert_eq!(etc.mode, "drwxr-xr-x");
        assert_eq!(etc.size, 56);
        let env = find(&entries, ".dockerenv");
        assert_eq!(env.kind, FileKind::File);
        assert_eq!(find(&entries, "bin").modified, "Apr 15  2026");
    }

    #[test]
    fn parses_gnu_root_with_symlinks_and_selinux_marker() {
        let entries = parse_ls(GNU);
        let bin = find(&entries, "bin");
        assert_eq!(bin.kind, FileKind::Symlink);
        assert_eq!(bin.link_target.as_deref(), Some("usr/bin"));
        assert_eq!(bin.mode, "lrwxrwxrwx.");
        assert_eq!(find(&entries, "boot").kind, FileKind::Dir);
    }

    #[test]
    fn parses_device_files() {
        for out in [BUSYBOX_DEV, GNU_DEV] {
            let entries = parse_ls(out);
            let full = find(&entries, "full");
            assert_eq!(full.kind, FileKind::Other);
            assert_eq!(full.size, 0);
            assert_eq!(
                find(&entries, "fd").link_target.as_deref(),
                Some("/proc/self/fd")
            );
        }
    }

    #[test]
    fn keeps_spaces_in_names_and_sorts_dirs_first() {
        let out = "total 8\n\
            -rw-r--r--    1 root     root            5 Oct 27 22:28 my  file.txt\n\
            drwxr-xr-x    2 root     root           40 Oct 27 22:28 zdir\n\
            -rw-r--r--    1 root     root            5 Oct 27 22:28 a\n";
        let names: Vec<_> = parse_ls(out).into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["zdir", "a", "my  file.txt"]);
    }

    #[test]
    fn ignores_garbage_lines() {
        assert!(parse_ls("ls: /nope: No such file or directory\n").is_empty());
    }

    fn tar_of(entries: &[(&str, Option<&[u8]>)]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (path, content) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_mtime(0);
            match content {
                Some(data) => {
                    header.set_entry_type(tar::EntryType::Regular);
                    header.set_size(data.len() as u64);
                    header.set_mode(0o644);
                    builder.append_data(&mut header, path, *data).unwrap();
                }
                None => {
                    header.set_entry_type(tar::EntryType::Directory);
                    header.set_size(0);
                    header.set_mode(0o755);
                    builder.append_data(&mut header, path, &[][..]).unwrap();
                }
            }
        }
        builder.into_inner().unwrap()
    }

    #[test]
    fn lists_direct_children_of_tar_root() {
        let bytes = tar_of(&[
            ("etc/", None),
            ("etc/passwd", Some(b"root:x:0:0")),
            ("etc/ssl/", None),
            ("etc/ssl/cert.pem", Some(b"pem")),
        ]);
        let entries = list_tar(&bytes).unwrap();
        let names: Vec<_> = entries.iter().map(|e| (e.name.as_str(), e.kind)).collect();
        assert_eq!(names, [("ssl", FileKind::Dir), ("passwd", FileKind::File)]);
        assert_eq!(entries[1].size, 10);
        assert_eq!(entries[1].mode, "-rw-r--r--");
        assert_eq!(entries[0].mode, "drwxr-xr-x");
        assert_eq!(entries[1].modified, "1970-01-01 00:00");
    }

    #[test]
    fn previews_text_file() {
        let bytes = tar_of(&[("hosts", Some(b"127.0.0.1 localhost\n"))]);
        let p = preview_tar(&bytes, 1024).unwrap();
        assert_eq!(p.content, "127.0.0.1 localhost\n");
        assert!(!p.binary && !p.truncated);
        assert_eq!(p.size, 20);
    }

    #[test]
    fn preview_is_capped_and_tolerates_truncated_archive() {
        let data = vec![b'a'; 4000];
        let bytes = tar_of(&[("big", Some(&data))]);
        let p = preview_tar(&bytes[..1500], 100).unwrap();
        assert_eq!(p.content.len(), 100);
        assert!(p.truncated);
        assert_eq!(p.size, 4000);
    }

    #[test]
    fn binary_files_become_hexdumps() {
        let bytes = tar_of(&[("bin", Some(&[0x7f, b'E', b'L', b'F', 0, 1]))]);
        let p = preview_tar(&bytes, 1024).unwrap();
        assert!(p.binary);
        assert!(p.content.starts_with("00000000  7f 45 4c 46 00 01"));
    }

    #[test]
    fn hexdump_format() {
        let dump = hexdump(b"ABCDEFGHIJKLMNOPQ");
        assert_eq!(
            dump,
            "00000000  41 42 43 44 45 46 47 48  49 4a 4b 4c 4d 4e 4f 50  |ABCDEFGHIJKLMNOP|\n\
             00000010  51                                                |Q|\n"
        );
    }

    #[test]
    fn formats_unix_time() {
        assert_eq!(format_unix(1_700_000_000), "2023-11-14 22:13");
    }

    #[test]
    fn path_helpers() {
        assert_eq!(join("/", "etc"), "/etc");
        assert_eq!(join("/etc", "ssl"), "/etc/ssl");
        assert_eq!(join("/etc/", "ssl"), "/etc/ssl");
        assert_eq!(parent("/etc/ssl"), "/etc");
        assert_eq!(parent("/etc"), "/");
        assert_eq!(parent("/"), "/");
    }

    #[test]
    fn change_kinds() {
        let c = |kind| {
            FsChange::from(FilesystemChange {
                path: "/x".into(),
                kind,
            })
        };
        assert_eq!(c(ChangeType::_0).kind, ChangeKind::Modified);
        assert_eq!(c(ChangeType::_1).kind, ChangeKind::Added);
        assert_eq!(c(ChangeType::_2).kind, ChangeKind::Deleted);
    }
}
