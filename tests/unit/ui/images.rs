use crate::support::*;
use docker_tui::{
    app::{App, Tab},
    msg::Msg,
};

#[test]
fn renders_image_table() {
    let mut app = App::new();
    app.tab = Tab::Images;
    app.now = 10 * 86_400;
    let mut nginx = image("abcdef0123456789", "nginx:latest");
    nginx.size = 187_000_000;
    nginx.created = 9 * 86_400;
    nginx.containers = 1;
    app.update(Msg::ImagesLoaded(Ok(vec![
        nginx,
        image("0011223344556677", "alpine:3.20"),
    ])));
    insta::assert_snapshot!(render_to_string(&app, 80, 7));
}
