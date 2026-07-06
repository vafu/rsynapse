use relm4::prelude::*;
use shell_core::{
    gtk::{self, prelude::*},
    source::{self, Observable, SourceError, rx::Observable as _},
};

pub(super) fn source_error_count() -> Observable<u64> {
    source::error_count()
}

pub(super) fn source_error_items() -> Observable<Vec<SourceError>> {
    source::errors()
        .map(|errors| errors.recent)
        .distinct_until_changed()
        .box_it()
}

#[derive(Debug)]
pub(super) struct SourceErrorRow {
    error: SourceError,
}

#[relm4::component(pub(crate))]
impl SimpleComponent for SourceErrorRow {
    type Init = SourceError;
    type Input = ();
    type Output = ();

    view! {
        #[root]
        gtk::Box {
            add_css_class: "source-error-row",
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 2,
            set_tooltip_text: Some(model.error.message.as_str()),

            gtk::Label {
                add_css_class: "source-error-row-title",
                set_halign: gtk::Align::Start,
                set_ellipsize: gtk::pango::EllipsizeMode::End,
                set_label: source_error_title(&model.error).as_str(),
            },

            gtk::Label {
                add_css_class: "source-error-row-path",
                set_halign: gtk::Align::Start,
                set_ellipsize: gtk::pango::EllipsizeMode::Middle,
                set_label: model.error.path.display().to_string().as_str(),
            },

            gtk::Label {
                add_css_class: "source-error-row-message",
                set_halign: gtk::Align::Start,
                set_wrap: true,
                set_wrap_mode: gtk::pango::WrapMode::WordChar,
                set_label: model.error.message.as_str(),
            }
        }
    }

    fn init(
        init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = SourceErrorRow { error: init };
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }
}

fn source_error_title(error: &SourceError) -> String {
    format!("#{} {}", error.id, error.source)
}
