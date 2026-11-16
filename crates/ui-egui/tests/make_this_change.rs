//! "Make this change" on a Just ask answer: a real click fills the prompt box, switches it to
//! "Change it" and leaves the cursor in the box, ready to be read, edited and sent.
use craft_anything_engine::Mode;
use craft_anything_engine::questions::AskedQuestion;
use craft_anything_ui_egui::{CraftApp, demo, just_ask::Asked};
use egui_kittest::{Harness, kittest::Queryable};

#[test]
fn a_click_on_make_this_change_leaves_the_cursor_in_the_filled_box() {
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_eframe(|cc| {
        CraftApp::setup_context(&cc.egui_ctx);
        let mut app = demo::app(demo::engine(Mode::Beginner, true));
        demo::fill_project(&mut app);
        app.state.feed.retain(|i| !matches!(i, craft_anything_ui_egui::state::FeedItem::Ask { .. }));
        app.state.just_ask = true;
        let after = app.state.feed.len();
        if let Some(root) = app.workspace.as_ref().map(|w| w.root.clone()) {
            let mut asked: Asked =
                AskedQuestion::new("why is it cramped?".into(), "The card is too wide.".into(), true, "Claude Code".into(), None, after).into();
            asked.collapsed = false;
            app.asked.insert(root, vec![asked]);
        }
        app
    });
    h.run_steps(24);
    // A real pointer click: it also counts as a click outside the prompt box, which must not take
    // the cursor away from it.
    let at = h.get_by_label("Make this change").rect().center();
    h.input_mut().events.push(egui::Event::PointerMoved(at));
    h.step();
    for pressed in [true, false] {
        h.input_mut().events.push(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() });
        h.step();
    }
    h.run_steps(3);
    assert_eq!(h.state().state.project_input, "Make only the change you suggested for: \"why is it cramped?\"");
    assert!(!h.state().state.just_ask, "switched to Change it");
    assert!(h.get_by_label("Ask for a change").is_focused(), "the cursor is in the prompt box");
}
