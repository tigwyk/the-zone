//! Three cues, and nothing that has to be plumbed through the game to fire them.
//!
//! `ponytail:` PLAN §4 pencilled in `bevy_kira_audio`. Bevy's own `AudioPlayer` plays
//! a one-shot at a volume, which is the whole requirement — take the dependency if we
//! ever need a mixed ambience bed under it. Registered from `main` only, so the
//! headless play-through tests never need an asset server.

use bevy::audio::Volume;
use bevy::prelude::*;

use crate::run::RunState;

/// Quiet. The move cue fires on nearly every keypress.
const CUE_VOLUME: f32 = 0.5;

#[derive(Resource)]
pub(crate) struct Cues {
    move_: Handle<AudioSource>,
    confirm: Handle<AudioSource>,
    hurt: Handle<AudioSource>,
    /// HP as of the last frame, to notice a hit without anyone reporting one.
    last_hp: i32,
}

pub(crate) fn load_cues(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(Cues {
        move_: assets.load("audio/move.wav"),
        confirm: assets.load("audio/confirm.wav"),
        hurt: assets.load("audio/hurt.wav"),
        last_hp: 0,
    });
}

fn play(commands: &mut Commands, sound: &Handle<AudioSource>) {
    commands.spawn((
        AudioPlayer::new(sound.clone()),
        PlaybackSettings::DESPAWN.with_volume(Volume::Linear(CUE_VOLUME)),
    ));
}

/// Reads the keyboard and the stalker's health directly. Every screen in the game
/// already moves on arrows and commits on Enter, so that is the whole vocabulary,
/// and losing HP is the one event worth hearing about wherever it comes from.
pub(crate) fn play_cues(
    mut commands: Commands,
    mut cues: ResMut<Cues>,
    keys: Res<ButtonInput<KeyCode>>,
    run: Res<RunState>,
) {
    if keys.any_just_pressed([KeyCode::ArrowUp, KeyCode::ArrowDown, KeyCode::ArrowLeft, KeyCode::ArrowRight]) {
        let sound = cues.move_.clone();
        play(&mut commands, &sound);
    }
    if keys.just_pressed(KeyCode::Enter) {
        let sound = cues.confirm.clone();
        play(&mut commands, &sound);
    }

    if run.hp < cues.last_hp {
        let sound = cues.hurt.clone();
        play(&mut commands, &sound);
    }
    cues.last_hp = run.hp;
}
