mod spoiler;
mod window;

use spoiler::SpoilerParticles;
use window::SpoilerWindow;

use adw::prelude::*;

fn main() {
    let app = adw::Application::builder()
        .application_id("com.github.yuraiz.Spoiler")
        .build();

    app.connect_activate(move |app| SpoilerWindow::new(app).present());

    app.run();
}
