use adw::prelude::*;
use adw::subclass::prelude::*;
use glib::clone;
use gtk::glib::once_cell::sync::Lazy;
use gtk::{gdk, gio, glib, graphene, gsk};

static PARTICLE_TEXTURE: Lazy<gdk::Texture> = Lazy::new(|| {
    let bytes = glib::Bytes::from_static(include_bytes!("res/turbulence_2x.png"));
    gdk::Texture::from_bytes(&bytes).unwrap()
});

const BLUR_SHADER: &[u8] = r#"
// src: https://www.shadertoy.com/view/Xltfzj

precision highp float;
precision highp sampler2D;
uniform bool dark;
uniform sampler2D u_texture1;

void mainImage(out vec4 fragColor,
    in vec2 fragCoord,
    in vec2 resolution,
    in vec2 uv) {
    float Pi = 6.28318530718; // Pi*2
    
    // GAUSSIAN BLUR SETTINGS {{{
    float Directions = 32.0; // BLUR DIRECTIONS (Default 16.0 - More is better but slower)
    float Quality = 16.0; // BLUR QUALITY (Default 4.0 - More is better but slower)
    float Size = 128.0; // BLUR SIZE (Radius)
    // GAUSSIAN BLUR SETTINGS }}}
   
    vec2 Radius = Size/resolution.xy;
    
    // Pixel colour
    vec4 Color = texture(u_texture1, uv);
    
    // Blur calculations
    for( float d=0.0; d<Pi; d+=Pi/Directions)
    {
		for(float i=1.0/Quality; i<=1.0; i+=1.0/Quality)
        {
            vec2 coord = uv+vec2(cos(d),sin(d))*Radius*i;
			Color += texture(u_texture1, clamp(coord, vec2(0), resolution));		
        }
    }
    
    // Output to screen
    Color /= Quality * Directions - 15.0;
    fragColor =  Color;
}
"#
.as_bytes();

mod imp {
    use gtk::glib::once_cell::unsync::OnceCell;

    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::SpoilerParticles)]
    pub struct SpoilerParticles {
        pub(super) start_time: Cell<i64>,
        pub(super) reveal_progress: Cell<f32>,
        pub(super) click_point: Cell<(f32, f32)>,

        pub(super) shader: RefCell<Option<gsk::GLShader>>,
        pub(super) blurred_texture_cache: RefCell<Option<(gdk::Texture, (f32, f32))>>,

        #[property(get)]
        pub(super) animation: OnceCell<adw::TimedAnimation>,

        #[property(get, set = Self::set_hidden)]
        pub(super) hidden: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SpoilerParticles {
        const NAME: &'static str = "ComponentsSpoilerParticles";
        type Type = super::SpoilerParticles;
        type ParentType = adw::Bin;
    }

    impl ObjectImpl for SpoilerParticles {
        fn properties() -> &'static [glib::ParamSpec] {
            Self::derived_properties()
        }

        fn property(&self, id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            self.derived_property(id, pspec)
        }

        fn set_property(&self, id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            self.derived_set_property(id, value, pspec)
        }

        fn constructed(&self) {
            let widget = self.obj();

            self.reveal_progress.set(1.0);

            widget.connect_child_notify(|widget| {
                widget.imp().blurred_texture_cache.take();
            });

            self.parent_constructed();
            self.obj().connect_visible_notify(|widget| {
                if widget.is_visible() {
                    widget.imp().start_time.set(widget.time());
                    widget.add_tick_callback(|widget, _clock| {
                        widget.queue_draw();
                        Continue(widget.is_visible())
                    });
                }
            });

            let target =
                adw::CallbackAnimationTarget::new(clone!(@weak widget => move |progress| {
                    widget.imp().reveal_progress.set(progress as f32);
                    widget.queue_draw();
                }));

            let animation = adw::TimedAnimation::builder()
                .widget(&*widget)
                .value_from(0.0)
                .value_to(1.0)
                .duration(1000)
                .easing(adw::Easing::EaseInOutCubic)
                .target(&target)
                .repeat_count(1)
                .build();

            self.animation.set(animation).unwrap();

            let controller = gtk::GestureClick::builder().button(1).build();

            controller.connect_pressed(clone!(@weak widget => move |_, _button, x, y| {
                if widget.hidden() {
                    widget.imp().click_point.set((x as f32, y as f32));
                    widget.set_hidden(false);
                }
            }));

            self.obj().add_controller(controller);
        }
    }

    impl WidgetImpl for SpoilerParticles {
        fn realize(&self) {
            self.parent_realize();
            self.ensure_shader();
            self.obj().notify("visible");
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            self.parent_snapshot(snapshot);

            let widget = self.obj();
            let texture = &*PARTICLE_TEXTURE;

            let width = widget.width() as f32;
            let height = widget.height() as f32;

            let bounds = graphene::Rect::new(0.0, 0.0, width, height);

            let (x, y) = if self.hidden.get() {
                (width * 0.5, height * 0.5)
            } else {
                self.click_point.get()
            };
            let center = &graphene::Point::new(x, y);

            let max_corner_length = [
                bounds.top_left(),
                bounds.top_right(),
                bounds.bottom_left(),
                bounds.bottom_right(),
            ]
            .into_iter()
            .map(|v| (v.distance(center)).0)
            .max_by(|this, other| this.partial_cmp(other).unwrap())
            .unwrap();

            let progress = self.reveal_progress.get();
            let radius = max_corner_length * progress;

            if radius > 0.0 {
                snapshot.push_mask(gsk::MaskMode::InvertedAlpha);

                snapshot.append_radial_gradient(
                    &bounds,
                    &graphene::Point::new(x, y),
                    radius,
                    radius,
                    0.0,
                    1.0,
                    &[
                        gsk::ColorStop::new(0.0, gdk::RGBA::BLACK),
                        gsk::ColorStop::new(progress, gdk::RGBA::BLACK),
                        gsk::ColorStop::new((progress + 0.5).min(1.0), gdk::RGBA::TRANSPARENT),
                        gsk::ColorStop::new(1.0, gdk::RGBA::TRANSPARENT),
                    ],
                );
                snapshot.pop();
            }

            let texture_bounds = {
                // Texture have 2x size, so we divide it to scale correctly;
                let width = texture.width() as f32 / 2.0;
                let heigth = texture.height() as f32 / 2.0;

                graphene::Rect::new(0.0, 0.0, width, heigth)
            };

            self.render_blur_texture(snapshot, &bounds);

            let time = widget.time() - self.start_time.get();
            let time = time as f32 / 50000.0;

            let layers = [
                (0.468, 0.287),
                (0.305, 0.1967),
                (0.316, 0.3239),
                (-0.0239, 0.7745),
                (-0.0736, 0.2023),
                (0.5138, -0.15),
                (0.5603, -0.8172),
                (-0.8098, -0.8822),
            ];

            for (x, y) in layers {
                let x = x * time;
                let y = y * time;

                snapshot.push_repeat(&bounds, None);
                snapshot.translate(&graphene::Point::new(x, y));
                snapshot.append_texture(texture, &texture_bounds);
                snapshot.translate(&graphene::Point::new(-x, -y));
                snapshot.pop();
            }

            if radius > 0.0 {
                snapshot.pop();
            }
        }
    }

    impl BinImpl for SpoilerParticles {}

    impl SpoilerParticles {
        fn set_hidden(&self, hidden: bool) {
            let animation = self.animation.get().unwrap();
            animation.set_reverse(hidden);
            animation.play();
            self.hidden.set(hidden);
        }

        fn ensure_shader(&self) {
            let widget = self.obj();
            if self.shader.borrow().is_none() {
                let renderer = widget.native().unwrap().renderer();

                let shader = gsk::GLShader::from_bytes(&BLUR_SHADER.into());
                match shader.compile(&renderer) {
                    Err(e) => {
                        if !e.matches(gio::IOErrorEnum::NotSupported) {
                            log::error!("can't compile the blur shader {e}");
                        }
                    }
                    Ok(_) => {
                        self.shader.replace(Some(shader));
                    }
                }
            };
        }

        fn render_blur_texture(&self, snapshot: &gtk::Snapshot, bounds: &graphene::Rect) {
            let current_size = (bounds.width(), bounds.height());

            match self.blurred_texture_cache.borrow().as_ref() {
                Some((texture, size)) if *size == current_size => {
                    snapshot.append_texture(texture, bounds);
                    return;
                }
                _ => {}
            };

            if let Some(shader) = &*self.shader.borrow() {
                let texture = {
                    let snapshot = gtk::Snapshot::new();

                    let args = gsk::ShaderArgsBuilder::new(shader, None);
                    snapshot.push_gl_shader(shader, bounds, args.to_args());
                    snapshot.append_color(&gdk::RGBA::new(0.3, 0.3, 0.3, 1.0), bounds);
                    self.parent_snapshot(&snapshot);
                    snapshot.gl_shader_pop_texture();
                    snapshot.pop();

                    let renderer = self.obj().native().unwrap().renderer();
                    renderer.render_texture(snapshot.to_node().unwrap(), Some(bounds))
                };

                snapshot.append_texture(&texture, bounds);

                self.blurred_texture_cache
                    .replace(Some((texture, current_size)));
            } else {
                snapshot.append_color(&gdk::RGBA::new(0.3, 0.3, 0.3, 1.0), bounds);
            }
        }
    }
}

glib::wrapper! {
    pub struct SpoilerParticles(ObjectSubclass<imp::SpoilerParticles>)
        @extends adw::Bin, gtk::Widget;
}

impl SpoilerParticles {
    pub(crate) fn drop_cache(&self) {
        self.imp().blurred_texture_cache.take();
    }

    fn time(&self) -> i64 {
        self.frame_clock()
            .and_then(|clk| clk.current_timings())
            .map(|t| t.frame_time())
            .unwrap_or_default()
    }
}
