use super::*;
use crate::ui_runtime::presentation::publish::capture_hud_frame;

#[test]
fn unknown_worn_item_keeps_protection_unresolved_in_the_hud() {
    let mut player = player_state::PlayerState::new(1);
    let mut runtime = UiRuntime::new(1);
    let mut presentation = UiPresentationRuntime::new(fixture_font()).unwrap();
    let unknown = protocol::NetworkItemStack {
        network_id: 10_001,
        count: 1,
        ..protocol::NetworkItemStack::empty()
    };
    runtime
        .enqueue_inventory_event(
            &mut player,
            1,
            1,
            protocol::InventoryEvent::Content(protocol::InventoryContentEvent {
                container: protocol::ContainerIdentity::window(protocol::ARMOR_WINDOW_ID),
                slots: vec![
                    protocol::NetworkItemStack::empty(),
                    unknown,
                    protocol::NetworkItemStack::empty(),
                    protocol::NetworkItemStack::empty(),
                ]
                .into(),
                storage_item: protocol::NetworkItemStack::empty(),
            }),
        )
        .unwrap();
    runtime.drain_pending_inventory(&mut player);
    assert!(!runtime.local_armor(&player).chestplate.is_empty());
    capture_hud_frame(
        &player,
        &mut runtime,
        &mut presentation,
        None,
        semantic_input::PerspectiveMode::FirstPerson,
        0,
        Default::default(),
    );
    assert_eq!(runtime.hud().armor(), None);
}
