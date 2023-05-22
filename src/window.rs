use adw::prelude::*;
use adw::subclass::prelude::*;
use glib::clone;
use gtk::{gdk, gio, glib};

use crate::SpoilerParticles;

mod imp {

    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(file = "src/window.blp")]
    pub struct SpoilerWindow {
        #[template_child]
        pub(super) spoiler: TemplateChild<SpoilerParticles>,
        #[template_child]
        pub(super) drop_target: TemplateChild<gtk::DropTarget>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SpoilerWindow {
        const NAME: &'static str = "SpoilerWindow";
        type Type = super::SpoilerWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for SpoilerWindow {
        fn constructed(&self) {
            self.parent_constructed();

            self.drop_target.connect_drop(
                clone!(@to-owned self as imp => @default-return false, move
                    |_, value, _, _ | {
                        let Ok(file_list) = value.get::<gdk::FileList>() else { return false; };

                        let files = file_list.files();

                        if let Some(picture) = imp.spoiler
                            .child()
                            .and_downcast::<gtk::Picture>() {
                                picture.set_file(files.first());
                                imp.spoiler.drop_cache();
                        }

                        true
                    }
                ),
            );
        }
    }

    impl WidgetImpl for SpoilerWindow {}
    impl WindowImpl for SpoilerWindow {}
    impl ApplicationWindowImpl for SpoilerWindow {}
    impl AdwApplicationWindowImpl for SpoilerWindow {}
}

glib::wrapper! {
    pub struct SpoilerWindow(ObjectSubclass<imp::SpoilerWindow>)
        @extends adw::ApplicationWindow, gtk::ApplicationWindow, gtk::Window, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Native, gtk::Root, gtk::ShortcutManager, gio::ActionGroup, gio::ActionMap;
}

impl SpoilerWindow {
    pub fn new(app: &impl IsA<gio::Application>) -> Self {
        glib::Object::builder().property("application", app).build()
    }
}
