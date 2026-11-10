use super::{CssPriority, SassConfig, StylesheetError, StylesheetSource, watcher};

#[derive(Debug)]
pub(crate) struct Stylesheet {
    source: StylesheetSource,
    priority: CssPriority,
    provider: gtk::CssProvider,
    sass_config: SassConfig,
}

impl Stylesheet {
    pub(crate) fn new(
        source: StylesheetSource,
        priority: CssPriority,
        sass_config: SassConfig,
    ) -> Self {
        Self {
            source,
            priority,
            provider: gtk::CssProvider::new(),
            sass_config,
        }
    }

    pub(crate) fn load(&mut self) -> Result<(), StylesheetError> {
        let css = self.source.load(&self.sass_config)?;
        self.provider.load_from_data(&css);
        Ok(())
    }

    pub(crate) fn install(&self) {
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &self.provider,
                self.priority.gtk_priority(),
            );
        }
    }

    pub(crate) fn watch(self) -> Result<StylesheetWatcher, StylesheetError> {
        let source = self.source.clone();
        let provider = self.provider;
        let sass_config = self.sass_config.clone();
        let (reload_sender, reload_receiver) = async_channel::bounded(1);

        gtk::glib::MainContext::default().spawn_local(async move {
            while reload_receiver.recv().await.is_ok() {
                match source.load(&sass_config) {
                    Ok(css) => {
                        provider.load_from_data(&css);
                    }
                    Err(error) => {
                        eprintln!("{error}");
                    }
                }
            }
        });

        watcher::watch_stylesheet(&self.source, &self.sass_config, reload_sender)
    }
}

pub(crate) use watcher::StylesheetWatcher;
