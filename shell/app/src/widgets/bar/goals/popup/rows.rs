use super::*;

pub(super) struct RenderedRow {
    pub item: adw::ActionRow,
    pub disclosure: Option<gtk::Button>,
}

impl GoalPopup {
    pub(super) fn row(&self, row: &GoalRow) -> RenderedRow {
        let goal = &row.goal;
        let item = adw::ActionRow::new();
        let status = if row.completed && goal.status != "completed" {
            "Subgoals complete"
        } else {
            match goal.status.as_str() {
                "in-progress" => "In progress",
                "completed" => "Completed",
                "deferred" => "Deferred",
                _ => "To do",
            }
        };
        let mut subtitle = format!(
            "{status} · {} priority",
            if goal.priority.is_empty() {
                "medium"
            } else {
                &goal.priority
            }
        );
        if row.child_count > 0 {
            subtitle.push_str(&format!(
                " · {}/{} subgoals",
                row.completed_children, row.child_count
            ));
        }
        if !goal.description.is_empty() {
            subtitle.push_str(&format!(" · {}", goal.description));
        }
        item.set_use_markup(row.completed);
        if row.completed {
            item.set_title(&format!(
                "<s>{}</s>",
                gtk::glib::markup_escape_text(&goal.title)
            ));
            item.set_subtitle(&gtk::glib::markup_escape_text(&subtitle));
        } else {
            item.set_title(&goal.title);
            item.set_subtitle(&subtitle);
        }
        item.set_title_lines(2);
        item.set_subtitle_lines(2);
        item.set_margin_start((row.depth.min(10) * 16) as i32);
        let disclosure = if row.child_count > 0 {
            let button = gtk::Button::from_icon_name("pan-down-symbolic");
            button.add_css_class("flat");
            button.add_css_class("goal-disclosure");
            button.set_valign(gtk::Align::Center);
            button.update_property(&[gtk::accessible::Property::Label(&format!(
                "Toggle subgoals for {}",
                goal.title
            ))]);
            let current = self.current.clone();
            let folds = self.folds.clone();
            let rendered = Rc::downgrade(&self.rendered);
            let instance = row.instance_id.clone();
            button.connect_clicked(move |_| {
                let Some(rendered) = rendered.upgrade() else {
                    return;
                };
                let vm = current.borrow();
                let Some(row) = vm.rows.iter().find(|r| r.instance_id == instance) else {
                    return;
                };
                folds.borrow_mut().toggle(row);
                apply_folds(&vm, &folds.borrow(), &rendered.borrow());
            });
            item.add_suffix(&button);
            Some(button)
        } else {
            None
        };
        let check = gtk::CheckButton::new();
        check.set_valign(gtk::Align::Center);
        check.set_active(goal.status == "completed");
        check.set_inconsistent(row.completed && goal.status != "completed");
        check.update_property(&[gtk::accessible::Property::Label(&format!(
            "Complete {}",
            goal.title
        ))]);
        let key = goal.key();
        let overlay = self.overlay.downgrade();
        check.connect_toggled(move |check| {
            if !check.is_sensitive() {
                return;
            }
            let Some(overlay) = overlay.upgrade() else {
                return;
            };
            let completed = check.is_active();
            check.set_sensitive(false);
            let weak = check.downgrade();
            actions::run(
                Action::Status {
                    key: key.clone(),
                    status: if completed { "completed" } else { "planned" }.into(),
                },
                &overlay,
                move |ok| {
                    if let Some(check) = weak.upgrade() {
                        if !ok {
                            check.set_active(!completed);
                        }
                        check.set_sensitive(true);
                    }
                },
            );
        });
        item.add_prefix(&check);
        item.set_activatable(true);
        let created = if row.created_at_ms == 0 {
            "Unknown (predates recording)".into()
        } else {
            gtk::glib::DateTime::from_unix_local((row.created_at_ms / 1000) as i64)
                .and_then(|d| d.format("%F %R"))
                .map(|s| s.to_string())
                .unwrap_or_else(|_| "Unknown".into())
        };
        item.set_tooltip_text(Some(&format!(
            "Click the title to open its Markdown note in $EDITOR\nCreated: {created}"
        )));
        let note_path = goal.note_path.clone();
        let overlay = self.overlay.downgrade();
        let popup = self.popover.downgrade();
        item.connect_activated(move |_| {
            if let Some(p) = popup.upgrade() {
                p.popdown();
            }
            if let Some(overlay) = overlay.upgrade() {
                actions::open_note(note_path.clone(), &overlay);
            }
        });
        RenderedRow { item, disclosure }
    }
}
