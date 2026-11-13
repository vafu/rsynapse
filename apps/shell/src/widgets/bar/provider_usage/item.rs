use nerd_icon_picker::NerdIconPicker;
use relm4::prelude::*;
use shell_core::gtk::{self, prelude::*};

use super::source::{ProviderUsageVm, provider_usage};
use crate::widgets::nerd_icon::NerdIconLabelExt;
use crate::widgets::{
    bar::bar_item,
    level_indicator::{self, LevelRenderStyle, LevelStage, LineStyle},
};

const STYLE: LevelRenderStyle = LevelRenderStyle::Line(LineStyle::vertical(3.0));
const STAGES: &[LevelStage] = &[
    LevelStage {
        level: 0.0,
        class: "normal",
    },
    LevelStage {
        level: 80.0,
        class: "warn",
    },
    LevelStage {
        level: 90.0,
        class: "danger",
    },
    LevelStage {
        level: 95.0,
        class: "critical",
    },
];

#[derive(Debug)]
pub(crate) enum ProviderUsageInput {
    Source(Box<provider_usage_item_sources::Msg>),
    Refresh,
}

impl From<provider_usage_item_sources::Msg> for ProviderUsageInput {
    fn from(message: provider_usage_item_sources::Msg) -> Self {
        Self::Source(Box::new(message))
    }
}

#[derive(Debug)]
#[shell_macros::model(module = provider_usage_item_sources)]
pub(crate) struct ProviderUsageIndicator {
    id: String,
    icon_picker: NerdIconPicker,

    #[source(provider_usage(id.clone()))]
    vm: Option<ProviderUsageVm>,
}

#[shell_macros::component(module = provider_usage_item_sources, model = ProviderUsageIndicator)]
#[relm4::component(pub(crate))]
impl SimpleComponent for ProviderUsageIndicator {
    type Init = String;
    type Input = ProviderUsageInput;
    type Output = ();

    view! {
        #[root]
        gtk::MenuButton {
            #[watch]
            set_visible: model.vm.is_some(),
            set_css_classes: &bar_item::action_classes(&["provider-usage-button"]),
            set_always_show_arrow: false,
            set_has_frame: false,
            #[watch]
            set_tooltip_text: model.vm.as_ref().map(|vm| vm.usage.tooltip()).as_deref(),

            #[wrap(Some)]
            set_child = &gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::Center,
                set_spacing: 3,

                gtk::Label {
                    set_css_classes: &["nerdicon", bar_item::ICON_CLASS],
                    #[watch]
                    set_nerd_icon: icon(model.vm.as_ref()),
                },

                gtk::Overlay {
                    #[watch]
                    set_css_classes: &root_classes(),
                    #[watch]
                    set_visible: model.vm.as_ref().and_then(|vm| vm.percent).is_some(),
                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::Center,

                    add_overlay = &gtk::DrawingArea {
                        set_css_classes: level_indicator::TRACK_CLASSES,
                        set_can_target: false,
                        set_vexpand: true,
                        set_draw_func: track_draw(),
                    },

                    add_overlay = &gtk::DrawingArea {
                        #[watch]
                        set_css_classes: &level_classes(model.vm.as_ref()),
                        set_can_target: false,
                        set_vexpand: true,
                        #[watch]
                        set_draw_func: level_draw(model.vm.as_ref()),
                    }
                }
            },

            #[wrap(Some)]
            set_popover = &gtk::Popover {
                add_css_class: "provider-usage-popover",
                set_position: gtk::PositionType::Bottom,

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 8,
                    set_margin_all: 12,
                    set_width_request: 340,

                    gtk::Label {
                        add_css_class: "heading",
                        set_xalign: 0.0,
                        #[watch]
                        set_label: &title(model.vm.as_ref()),
                    },

                    #[local_ref]
                    icon_picker_root -> gtk::Box {},

                    gtk::Separator::new(gtk::Orientation::Horizontal),

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 3,
                        #[watch]
                        set_visible: model.vm.is_some(),

                        gtk::Label { set_xalign: 0.0, #[watch] set_label: &metadata(model.vm.as_ref()) },
                    },

                    gtk::Button {
                        set_css_classes: &["flat", "provider-refresh-button"],
                        set_halign: gtk::Align::Start,
                        set_label: "Refresh",
                        connect_clicked => ProviderUsageInput::Refresh,
                    },
                },
            },
        }
    }

    fn init(
        id: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let icon_picker = NerdIconPicker::new();
        let icon_picker_root = icon_picker.widget().clone();
        let model = ProviderUsageIndicator {
            __shell: Default::default(),
            id: id.clone(),
            icon_picker: icon_picker.clone(),
            vm: None,
        };
        let select_id = id.clone();
        icon_picker.connect_icon_selected(move |icon| {
            data_sources::provider_usage::set_provider_icon(
                select_id.clone(),
                icon.glyph().to_owned(),
                String::new(),
            );
        });
        let reset_id = id.clone();
        icon_picker.connect_reset(move || {
            data_sources::provider_usage::clear_provider_icon(reset_id.clone());
        });
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Self::Input, _sender: ComponentSender<Self>) {
        match input {
            ProviderUsageInput::Source(message) => {
                ProviderUsageIndicator::update(self, *message);
                sync_picker(self);
            }
            ProviderUsageInput::Refresh => {
                data_sources::provider_usage::refresh(self.id.clone());
            }
        }
    }
}

fn sync_picker(model: &ProviderUsageIndicator) {
    let Some(vm) = model.vm.as_ref() else {
        return;
    };
    let icons = vm
        .icon
        .candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            nerd_icon_picker::NerdIcon::specific(
                format!("Suggested icon {}", index + 1),
                candidate.glyph.clone(),
            )
        })
        .collect();
    model.icon_picker.set_specific_icons(icons);
    model
        .icon_picker
        .set_selected_glyph(Some(vm.icon.selected.glyph.as_str()));
    model.icon_picker.set_reset_visible(vm.icon.overridden);
}

fn icon(vm: Option<&ProviderUsageVm>) -> crate::widgets::nerd_icon::NerdIcon {
    vm.map(|vm| vm.icon.selected_nerd_icon())
        .unwrap_or_else(|| {
            crate::widgets::nerd_icon::NerdIcon::new(crate::widgets::nerd_icon::md::MD_SERVER)
        })
}

fn title(vm: Option<&ProviderUsageVm>) -> String {
    vm.map(|vm| {
        let name = if vm.usage.name.is_empty() {
            &vm.usage.id
        } else {
            &vm.usage.name
        };
        format!("{name}\n{}", vm.usage.id)
    })
    .unwrap_or_default()
}

fn metadata(vm: Option<&ProviderUsageVm>) -> String {
    let Some(vm) = vm else { return String::new() };
    let usage = &vm.usage;
    let mut lines = vec![
        format!(
            "state: {} ({})",
            usage.state,
            if usage.stale { "stale" } else { "live" }
        ),
        format!("label: {}", usage.label),
        format!("all-time: {}", money(usage.usage_usd)),
        format!("daily: {}", money(usage.usage_daily_usd)),
        format!("weekly: {}", money(usage.usage_weekly_usd)),
        format!("monthly: {}", money(usage.usage_monthly_usd)),
        format!("BYOK: {}", money(usage.byok_usage_usd)),
        format!("BYOK daily: {}", money(usage.byok_usage_daily_usd)),
        format!("BYOK weekly: {}", money(usage.byok_usage_weekly_usd)),
        format!("BYOK monthly: {}", money(usage.byok_usage_monthly_usd)),
        format!("include BYOK in limit: {}", usage.include_byok_in_limit),
        format!("last attempt: {}", timestamp(usage.last_attempt_unix)),
        format!("last success: {}", timestamp(usage.last_success_unix)),
        format!("HTTP status: {}", usage.last_http_status),
    ];
    if usage.has_limit {
        lines.push(format!(
            "limit: {} ({}, {})",
            money(usage.limit_usd),
            money(usage.limit_remaining_usd),
            if usage.limit_reset.is_empty() {
                "no reset"
            } else {
                &usage.limit_reset
            },
        ));
    } else {
        lines.push("limit: none".to_owned());
    }
    if usage.is_free_tier {
        lines.push(format!(
            "free models: {}/{} ({} remaining)",
            usage.free_model_daily_used,
            usage.free_model_daily_limit,
            usage.free_model_daily_remaining
        ));
    }
    if !usage.refresh_error.is_empty() {
        lines.push(format!("error: {}", usage.refresh_error));
    }
    lines.join("\n")
}

fn money(value: f64) -> String {
    format!("${value:.2}")
}

fn timestamp(value: u64) -> String {
    if value == 0 {
        "never".to_owned()
    } else {
        value.to_string()
    }
}

fn root_classes() -> Vec<&'static str> {
    level_indicator::root_classes(["line", "agent-context-level"])
}

fn level_classes(vm: Option<&ProviderUsageVm>) -> Vec<&'static str> {
    let percent = vm.and_then(|vm| vm.percent).unwrap_or_default();
    level_indicator::level_classes(f64::from(percent), 0.0, STAGES)
}

fn track_draw() -> impl Fn(&gtk::DrawingArea, &gtk::cairo::Context, i32, i32) + 'static {
    level_indicator::track_draw_func(STYLE)
}

fn level_draw(
    vm: Option<&ProviderUsageVm>,
) -> impl Fn(&gtk::DrawingArea, &gtk::cairo::Context, i32, i32) + 'static {
    let percent = vm.and_then(|vm| vm.percent).unwrap_or_default();
    level_indicator::level_draw_func(f64::from(percent), 0.0, 100.0, STYLE)
}
