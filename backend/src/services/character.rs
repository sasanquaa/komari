use std::fmt::Debug;

use crate::{Character, PotionMode, player::PlayerContext};

/// A service to handle character-related incoming requests.
pub trait CharacterService: Debug {
    /// Gets the currently in use [`Character`].
    fn character(&self) -> Option<&Character>;

    /// Sets a new `character` to be used.
    fn update_character(&mut self, character: Option<Character>);

    /// Updates `player_context` with information from the currently in use `[Character]`.
    fn apply_character(&self, player_context: &mut PlayerContext);
}

#[derive(Debug, Default)]
pub struct DefaultCharacterService {
    character: Option<Character>,
}

impl CharacterService for DefaultCharacterService {
    fn character(&self) -> Option<&Character> {
        self.character.as_ref()
    }

    fn update_character(&mut self, character: Option<Character>) {
        self.character = character;
    }

    fn apply_character(&self, player_context: &mut PlayerContext) {
        player_context.reset();
        if let Some(character) = self.character.as_ref() {
            player_context.config.link_key_timing_millis = character.link_key_timing_millis;
            player_context.config.has_extended_teleport_range =
                character.has_extended_teleport_range;
            player_context.config.disable_double_jumping = character.disable_double_jumping;
            player_context.config.disable_adjusting = character.disable_adjusting;
            player_context.config.disable_teleport_on_fall = character.disable_teleport_on_fall;
            player_context.config.disable_grapple_on_double_jumping =
                character.disable_grapple_on_double_jumping;
            player_context.config.up_jump_is_flight = character.up_jump_is_flight;
            player_context.config.up_jump_specific_key_should_jump =
                character.up_jump_specific_key_should_jump;
            player_context.config.interact_key = character.interact_key.key.into();
            player_context.config.grappling_key = character.ropelift_key.map(|key| key.key.into());
            player_context.config.teleport_key = character.teleport_key.map(|key| key.key.into());
            player_context.config.jump_key = character.jump_key.key.into();
            player_context.config.up_jump_key = character.up_jump_key.map(|key| key.key.into());
            player_context.config.cash_shop_key = character.cash_shop_key.map(|key| key.key.into());
            player_context.config.familiar_key =
                character.familiar_menu_key.map(|key| key.key.into());
            player_context.config.to_town_key = character.to_town_key.map(|key| key.key.into());
            player_context.config.change_channel_key =
                character.change_channel_key.map(|key| key.key.into());
            player_context.config.potion_key = character.potion_key.key.into();
            player_context.config.use_potion_below_percent =
                match (character.potion_key.enabled, character.potion_mode) {
                    (false, _) | (_, PotionMode::EveryMillis(_)) => None,
                    (_, PotionMode::Percentage(percent)) => Some(percent / 100.0),
                };
            player_context.config.update_health_millis = Some(character.health_update_millis);
            player_context.config.generic_booster_key = character.generic_booster_key.key.into();
            player_context.config.hexa_booster_key = character.hexa_booster_key.key.into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{KeyBinding, KeyBindingConfiguration, bridge::KeyKind, player::PlayerContext};

    fn mock_character() -> Character {
        Character {
            link_key_timing_millis: 30,
            has_extended_teleport_range: true,
            disable_double_jumping: true,
            disable_adjusting: true,
            disable_teleport_on_fall: true,
            up_jump_is_flight: true,
            up_jump_specific_key_should_jump: true,
            interact_key: KeyBindingConfiguration {
                key: KeyBinding::Z,
                ..Default::default()
            },
            ropelift_key: Some(KeyBindingConfiguration {
                key: KeyBinding::V,
                ..Default::default()
            }),
            teleport_key: Some(KeyBindingConfiguration {
                key: KeyBinding::X,
                ..Default::default()
            }),
            jump_key: KeyBindingConfiguration {
                key: KeyBinding::C,
                ..Default::default()
            },
            up_jump_key: Some(KeyBindingConfiguration {
                key: KeyBinding::A,
                ..Default::default()
            }),
            cash_shop_key: Some(KeyBindingConfiguration {
                key: KeyBinding::B,
                ..Default::default()
            }),
            familiar_menu_key: Some(KeyBindingConfiguration {
                key: KeyBinding::N,
                ..Default::default()
            }),
            to_town_key: Some(KeyBindingConfiguration {
                key: KeyBinding::M,
                ..Default::default()
            }),
            change_channel_key: Some(KeyBindingConfiguration {
                key: KeyBinding::L,
                ..Default::default()
            }),
            potion_key: KeyBindingConfiguration {
                key: KeyBinding::P,
                enabled: true,
            },
            potion_mode: PotionMode::Percentage(50.0),
            health_update_millis: 3000,
            ..Default::default()
        }
    }

    #[test]
    fn update_and_current() {
        let mut service = DefaultCharacterService::default();
        assert!(service.character().is_none());

        let character = mock_character();
        service.update_character(Some(character));
        let current = service.character().unwrap();

        assert_eq!(current, &mock_character());
    }

    #[test]
    fn update_from_character_none() {
        let service = DefaultCharacterService::default();
        let mut state = PlayerContext::default();
        state.config.link_key_timing_millis = 11;

        service.apply_character(&mut state);
        assert_eq!(state.config.link_key_timing_millis, 11);
    }

    #[test]
    fn update_from_character_some() {
        let mut service = DefaultCharacterService::default();
        let character = mock_character();
        service.update_character(Some(character.clone()));

        let mut state = PlayerContext::default();
        service.apply_character(&mut state);

        assert_eq!(
            state.config.link_key_timing_millis,
            character.link_key_timing_millis
        );
        assert_eq!(
            state.config.has_extended_teleport_range,
            character.has_extended_teleport_range
        );
        assert_eq!(
            state.config.disable_double_jumping,
            character.disable_double_jumping
        );
        assert_eq!(state.config.disable_adjusting, character.disable_adjusting);
        assert_eq!(
            state.config.disable_teleport_on_fall,
            character.disable_teleport_on_fall
        );
        assert_eq!(state.config.up_jump_is_flight, character.up_jump_is_flight);
        assert_eq!(
            state.config.up_jump_specific_key_should_jump,
            character.up_jump_specific_key_should_jump
        );
        assert_eq!(state.config.interact_key, KeyKind::Z);
        assert_eq!(state.config.grappling_key, Some(KeyKind::V));
        assert_eq!(state.config.teleport_key, Some(KeyKind::X));
        assert_eq!(state.config.jump_key, KeyKind::C);
        assert_eq!(state.config.up_jump_key, Some(KeyKind::A));
        assert_eq!(state.config.cash_shop_key, Some(KeyKind::B));
        assert_eq!(state.config.familiar_key, Some(KeyKind::N));
        assert_eq!(state.config.to_town_key, Some(KeyKind::M));
        assert_eq!(state.config.change_channel_key, Some(KeyKind::L));
        assert_eq!(state.config.potion_key, KeyKind::P);
        assert_eq!(state.config.use_potion_below_percent, Some(0.5));
        assert_eq!(state.config.update_health_millis, Some(3000));
    }
}
