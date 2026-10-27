use crate::support::*;
use crossterm::event::KeyCode;

use docker_tui::{
    app::App,
    docker::{
        fs::{ChangeKind, FileKind, FilePreview, FsChange},
        models::ContainerState,
    },
    msg::Msg,
};

fn app_on_files() -> App {
    let mut app = App::new();
    app.update(Msg::ContainersLoaded(Ok(vec![container(
        "c1",
        "web",
        ContainerState::Running,
    )])));
    app.update(key(KeyCode::Enter));
    app.update(key(KeyCode::Left));
    let mut link = entry("bin", FileKind::Symlink);
    link.link_target = Some("usr/bin".into());
    let mut file = entry("hosts", FileKind::File);
    file.size = 1_500;
    app.update(Msg::DirListed {
        id: "c1".into(),
        path: "/".into(),
        result: Ok(listing(vec![entry("etc", FileKind::Dir), link, file])),
    });
    app
}

#[test]
fn renders_directory_listing() {
    insta::assert_snapshot!(render_to_string(&app_on_files(), 70, 10));
}

#[test]
fn renders_archive_listing_note() {
    let mut app = App::new();
    app.update(Msg::ContainersLoaded(Ok(vec![container(
        "c1",
        "web",
        ContainerState::Exited,
    )])));
    app.update(key(KeyCode::Enter));
    app.update(key(KeyCode::Left));
    let mut l = listing(vec![entry("etc", FileKind::Dir)]);
    l.via_archive = true;
    l.partial = true;
    app.update(Msg::DirListed {
        id: "c1".into(),
        path: "/".into(),
        result: Ok(l),
    });
    insta::assert_snapshot!(render_to_string(&app, 80, 7));
}

#[test]
fn renders_file_preview() {
    let mut app = app_on_files();
    app.update(key(KeyCode::Char('G')));
    app.update(key(KeyCode::Enter));
    app.update(Msg::FileRead {
        id: "c1".into(),
        path: "/hosts".into(),
        result: Ok(FilePreview {
            content: "127.0.0.1 localhost\n::1 localhost\n".into(),
            binary: false,
            size: 1_500,
            truncated: true,
        }),
    });
    insta::assert_snapshot!(render_to_string(&app, 70, 9));
}

#[test]
fn renders_changes() {
    let mut app = app_on_files();
    app.update(key(KeyCode::Char('D')));
    app.update(Msg::DiffLoaded {
        id: "c1".into(),
        result: Ok(vec![
            FsChange {
                path: "/tmp/x".into(),
                kind: ChangeKind::Added,
            },
            FsChange {
                path: "/etc/hosts".into(),
                kind: ChangeKind::Modified,
            },
            FsChange {
                path: "/old".into(),
                kind: ChangeKind::Deleted,
            },
        ]),
    });
    insta::assert_snapshot!(render_to_string(&app, 60, 9));
}
