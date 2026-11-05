use gtk::{self, prelude::*};
use relm4::prelude::*;

use super::{ComponentListBoxExt, ComponentListUpdate};

struct TestRow;

#[relm4::component]
impl SimpleComponent for TestRow {
    type Init = usize;
    type Input = ();
    type Output = ();

    view! {
        gtk::Label {},
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        root.set_widget_name(&init.to_string());
        let model = TestRow;
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, _message: Self::Input, _sender: ComponentSender<Self>) {}
}

fn child_names(container: &gtk::Box) -> Vec<String> {
    let mut names = Vec::new();
    let mut child = container.first_child();
    while let Some(widget) = child {
        names.push(widget.widget_name().to_string());
        child = widget.next_sibling();
    }
    names
}

#[test]
fn component_list_reconciles_insertions_removals_and_moves() {
    gtk::init().unwrap();
    let container = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    container.set_visible(false);

    let sequences = [
        vec![],
        vec![0],
        vec![0, 1, 2],
        vec![2, 0, 1],
        vec![0, 2],
        vec![3, 1, 0, 2],
        vec![2, 3, 1],
        vec![],
        vec![4, 3, 2, 1, 0],
    ];

    for items in sequences {
        container.set_component_list(ComponentListUpdate::<TestRow>::new(&items));
        let expected = items.iter().map(usize::to_string).collect::<Vec<_>>();
        assert_eq!(child_names(&container), expected, "items={items:?}");
    }

    let mut items = Vec::<usize>::new();
    let mut next_id = 0usize;
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;

    for step in 0..2000 {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let action = (seed >> 33) % 4;
        match action {
            0 => {
                let index = ((seed >> 1) as usize) % (items.len() + 1);
                items.insert(index, next_id);
                next_id += 1;
            }
            1 if !items.is_empty() => {
                let index = ((seed >> 1) as usize) % items.len();
                items.remove(index);
            }
            2 if items.len() > 1 => {
                let from = ((seed >> 1) as usize) % items.len();
                let to = ((seed >> 17) as usize) % items.len();
                let item = items.remove(from);
                items.insert(to, item);
            }
            _ => {}
        }

        container.set_component_list(ComponentListUpdate::<TestRow>::new(&items));
        let expected = items.iter().map(usize::to_string).collect::<Vec<_>>();
        assert_eq!(
            child_names(&container),
            expected,
            "step={step} items={items:?}"
        );
    }
}
