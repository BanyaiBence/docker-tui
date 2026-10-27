use docker_tui::{
    app::detail::Loadable,
    docker::fs::{ChangeKind, FileKind, FilePreview, FsChange},
    msg::Command,
};

use crate::support::{entry, listing};
use crossterm::event::KeyCode;

use docker_tui::app::files::*;

fn loaded_root() -> FilesState {
    let mut f = FilesState::default();
    f.ensure_loaded("c1");
    f.on_listed(
        "/",
        Ok(listing(vec![
            entry("etc", FileKind::Dir),
            entry("bin", FileKind::Symlink),
            entry(".env", FileKind::File),
        ])),
    );
    f
}

#[test]
fn first_open_lists_root_once() {
    let mut f = FilesState::default();
    assert_eq!(
        f.ensure_loaded("c1"),
        vec![Command::ListDir {
            id: "c1".into(),
            path: "/".into()
        }]
    );
    assert_eq!(f.listing, Some(Loadable::Loading));
    assert!(f.ensure_loaded("c1").is_empty());
}

#[test]
fn enter_on_directory_navigates() {
    let mut f = loaded_root();
    let cmds = f.on_key("c1", KeyCode::Enter);
    assert_eq!(
        cmds,
        vec![Command::ListDir {
            id: "c1".into(),
            path: "/etc".into()
        }]
    );
    assert_eq!(f.path, "/etc");
    assert_eq!(f.listing, Some(Loadable::Loading));
}

#[test]
fn enter_on_symlink_tries_to_navigate() {
    let mut f = loaded_root();
    f.on_key("c1", KeyCode::Char('j'));
    assert_eq!(
        f.on_key("c1", KeyCode::Enter),
        vec![Command::ListDir {
            id: "c1".into(),
            path: "/bin".into()
        }]
    );
}

#[test]
fn enter_on_file_opens_preview() {
    let mut f = loaded_root();
    f.on_key("c1", KeyCode::Char('G'));
    let cmds = f.on_key("c1", KeyCode::Enter);
    assert_eq!(
        cmds,
        vec![Command::ReadFile {
            id: "c1".into(),
            path: "/.env".into()
        }]
    );
    assert_eq!(f.preview.as_ref().unwrap().content, Loadable::Loading);
    assert!(f.has_overlay());
    f.on_file_read(
        "/.env",
        Ok(FilePreview {
            content: "A=1".into(),
            binary: false,
            size: 3,
            truncated: false,
        }),
    );
    assert!(matches!(
        f.preview.as_ref().unwrap().content,
        Loadable::Loaded(_)
    ));
}

#[test]
fn esc_closes_preview() {
    let mut f = loaded_root();
    f.on_key("c1", KeyCode::Char('G'));
    f.on_key("c1", KeyCode::Enter);
    f.on_key("c1", KeyCode::Esc);
    assert!(f.preview.is_none());
}

#[test]
fn backspace_goes_up_and_reselects_previous_dir() {
    let mut f = loaded_root();
    f.on_key("c1", KeyCode::Enter);
    f.on_listed("/etc", Ok(listing(vec![entry("ssl", FileKind::Dir)])));
    let cmds = f.on_key("c1", KeyCode::Backspace);
    assert_eq!(
        cmds,
        vec![Command::ListDir {
            id: "c1".into(),
            path: "/".into()
        }]
    );
    f.on_listed(
        "/",
        Ok(listing(vec![
            entry("bin", FileKind::Dir),
            entry("etc", FileKind::Dir),
        ])),
    );
    assert_eq!(f.selection.index, 1);
}

#[test]
fn backspace_at_root_does_nothing() {
    let mut f = loaded_root();
    assert!(f.on_key("c1", KeyCode::Backspace).is_empty());
}

#[test]
fn stale_listings_are_ignored() {
    let mut f = loaded_root();
    f.on_key("c1", KeyCode::Enter);
    f.on_listed("/", Ok(listing(vec![])));
    assert_eq!(f.listing, Some(Loadable::Loading));
}

#[test]
fn listing_error_is_kept() {
    let mut f = loaded_root();
    f.on_key("c1", KeyCode::Enter);
    f.on_listed("/etc", Err("permission denied".into()));
    assert_eq!(
        f.listing,
        Some(Loadable::Failed("permission denied".into()))
    );
}

#[test]
fn shift_d_shows_changes() {
    let mut f = loaded_root();
    assert_eq!(
        f.on_key("c1", KeyCode::Char('D')),
        vec![Command::Diff("c1".into())]
    );
    f.on_diff(Ok(vec![FsChange {
        path: "/tmp/x".into(),
        kind: ChangeKind::Added,
    }]));
    let changes = f.changes.as_ref().unwrap();
    assert!(matches!(&changes.items, Loadable::Loaded(v) if v.len() == 1));
    f.on_key("c1", KeyCode::Esc);
    assert!(f.changes.is_none());
}

#[test]
fn enter_on_change_previews_unless_deleted() {
    let mut f = loaded_root();
    f.on_key("c1", KeyCode::Char('D'));
    f.on_diff(Ok(vec![
        FsChange {
            path: "/tmp/x".into(),
            kind: ChangeKind::Added,
        },
        FsChange {
            path: "/gone".into(),
            kind: ChangeKind::Deleted,
        },
    ]));
    assert_eq!(
        f.on_key("c1", KeyCode::Enter),
        vec![Command::ReadFile {
            id: "c1".into(),
            path: "/tmp/x".into()
        }]
    );
    f.on_key("c1", KeyCode::Esc);
    f.on_key("c1", KeyCode::Char('j'));
    assert!(f.on_key("c1", KeyCode::Enter).is_empty());
}

#[test]
fn r_reloads_current_directory() {
    let mut f = loaded_root();
    assert_eq!(
        f.on_key("c1", KeyCode::Char('r')),
        vec![Command::ListDir {
            id: "c1".into(),
            path: "/".into()
        }]
    );
}
