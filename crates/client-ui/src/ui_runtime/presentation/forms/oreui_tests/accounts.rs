//! The standalone sign-in prompt through the menu adapter.

use super::super::super::UiPresentationRuntime;
use launcher::menu::auth::AuthState;
use launcher::menu::{MenuAction, MenuDialog, MenuScreen, MenuView};
use ui_engine::text_metrics::TextMetrics;

const SIZE: [f32; 2] = [1280.0, 720.0];

fn manager_view() -> MenuView {
    let mut view = MenuView::new(true, "First".into());
    view.auth_state = AuthState::Authenticated;
    view.dialog = Some(MenuDialog::Accounts);
    view.feeds.account_active_id = Some("first".into());
    view.feeds.accounts = ["first", "second"]
        .map(|id| launcher::accounts::AccountProfile {
            id: id.into(),
            gamertag: id.into(),
            picture_path: None,
        })
        .into();
    view
}

#[test]
fn device_prompt_captures_standalone_routes_and_their_focus() {
    let mut presentation =
        UiPresentationRuntime::new(ui_engine::test_support::fixture_font()).unwrap();
    let metrics = TextMetrics::for_viewport([1280, 720], ui::DpiScale::new(1.0).unwrap(), None);
    for screen in [MenuScreen::Home, MenuScreen::Profile, MenuScreen::Store] {
        let mut view = manager_view();
        view.dialog = None;
        view.screen = screen;
        view.auth_state = AuthState::AwaitingCode {
            uri: "https://example.invalid".into(),
            code: "TEST-CODE".into(),
        };
        let (mut nodes, mut next) = (Vec::new(), 1);
        let hits = presentation
            .append_oreui_screen(&view, &mut nodes, &mut next, metrics, SIZE, None, &|_| None)
            .unwrap()
            .expect("standalone sign-in prompt");
        assert!(
            hits.iter()
                .any(|(action, _)| *action == MenuAction::OpenSignInLink)
        );
        assert!(hits.iter().all(|(action, _)| matches!(
            action,
            MenuAction::OpenSignInLink | MenuAction::CancelSignIn | MenuAction::CloseSignIn
        )));
    }
}
