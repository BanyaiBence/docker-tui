use bollard::models::{ChangeType, FilesystemChange};
use docker_tui::docker::fs::*;

const BUSYBOX: &str = include_str!("../../fixtures/ls_busybox.txt");
const BUSYBOX_DEV: &str = include_str!("../../fixtures/ls_busybox_dev.txt");
const GNU: &str = include_str!("../../fixtures/ls_gnu.txt");
const GNU_DEV: &str = include_str!("../../fixtures/ls_gnu_dev.txt");

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
fn tar_listing_formats_modification_time_in_utc() {
    let mut builder = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Regular);
    header.set_size(0);
    header.set_mode(0o644);
    header.set_mtime(1_700_000_000);
    // The archive root comes first, as Docker sends it.
    builder
        .append_data(&mut tar_dir_header(), "etc/", &[][..])
        .unwrap();
    builder
        .append_data(&mut header, "etc/hosts", &[][..])
        .unwrap();
    let entries = list_tar(&builder.into_inner().unwrap()).unwrap();
    assert_eq!(entries[0].modified, "2023-11-14 22:13");
}

fn tar_dir_header() -> tar::Header {
    let mut header = tar::Header::new_gnu();
    header.set_entry_type(tar::EntryType::Directory);
    header.set_size(0);
    header.set_mode(0o755);
    header
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
