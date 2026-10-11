//! `cargo run --release --example demo -- 1234 warbound fast`   (seed, preset, optional pace)

use bevy::prelude::*;
use verdandi::composer::STEPS_PER_LOOP;
use verdandi::prelude::{
    Layer, LayerAction, MusicConfig, MusicController, MusicPace, MusicPreset, MusicTrack,
    ProceduralMusicPlugin,
};

const LAYER_KEYS: [(KeyCode, Layer); 8] = [
    (KeyCode::Digit1, Layer::Drums),
    (KeyCode::Digit2, Layer::Bass),
    (KeyCode::Digit3, Layer::Pad),
    (KeyCode::Digit4, Layer::Arp),
    (KeyCode::Digit5, Layer::Lead),
    (KeyCode::Digit6, Layer::RhythmGuitar),
    (KeyCode::Digit7, Layer::Strings),
    (KeyCode::Digit8, Layer::Choir),
];

#[derive(Component)]
struct InfoText;

fn main() {
    let seed = std::env::args()
        .nth(1)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(42);
    let preset = match std::env::args().nth(2).as_deref() {
        Some("contemplative") | Some("classical-ambient") | Some("classical") => MusicPreset::ClassicalAmbient,
        Some("warbound") | Some("heavy-metal") | Some("metal") => MusicPreset::HeavyMetal,
        Some("orchestral") | Some("legendary-metal") | Some("legendary") => MusicPreset::LegendaryMetal,
        _ => MusicPreset::MythicAmbient,
    };
    let pace = if matches!(
        preset,
        MusicPreset::HeavyMetal | MusicPreset::LegendaryMetal
    ) && std::env::args().nth(3).as_deref() == Some("fast")
    {
        MusicPace::Fast
    } else {
        MusicPace::Standard
    };

    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(ProceduralMusicPlugin::new(MusicConfig {
            seed,
            preset,
            pace,
            ..default()
        }))
        .add_systems(Startup, setup_ui)
        .add_systems(Update, (handle_input, update_info))
        .run();
}

fn setup_ui(mut commands: Commands) {
    commands.spawn(Camera2d);

    commands.spawn((
        Text::new(""),
        Node {
            position_type: PositionType::Absolute,
            top: px(12),
            left: px(12),
            ..default()
        },
        InfoText,
    ));

    commands.spawn((
        Text::new(
            "Left/Right: seed -/+   N: random seed   (applies at next loop)\n\
             1-8: toggle layers   Up/Down: intensity   A: auto intensity   Space: pause",
        ),
        Node {
            position_type: PositionType::Absolute,
            bottom: px(12),
            left: px(12),
            ..default()
        },
    ));
}

fn time_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1)
        % 1_000_000
}

fn handle_input(
    keys: Res<ButtonInput<KeyCode>>,
    music: Res<MusicController>,
    sink: Query<&AudioSink, With<MusicTrack>>,
) {
    if keys.just_pressed(KeyCode::ArrowRight) {
        music.set_seed(music.seed().wrapping_add(1));
    }
    if keys.just_pressed(KeyCode::ArrowLeft) {
        music.set_seed(music.seed().wrapping_sub(1));
    }
    if keys.just_pressed(KeyCode::KeyN) {
        music.set_seed(time_seed());
    }

    for (key, layer) in LAYER_KEYS {
        if keys.just_pressed(key) {
            music.toggle_layer(layer);
        }
    }

    if keys.just_pressed(KeyCode::KeyA) {
        music.set_auto_intensity(!music.auto_intensity());
    }
    let nudge = if keys.just_pressed(KeyCode::ArrowUp) {
        0.1
    } else if keys.just_pressed(KeyCode::ArrowDown) {
        -0.1
    } else {
        0.0
    };
    if nudge != 0.0 {
        let current = music.status().intensity; // start from what you are hearing
        music.set_auto_intensity(false);
        music.set_intensity(current + nudge);
    }

    if keys.just_pressed(KeyCode::Space) {
        if let Ok(sink) = sink.single() {
            sink.toggle_playback();
        }
    }
}

fn update_info(music: Res<MusicController>, mut text: Single<&mut Text, With<InfoText>>) {
    let st = music.status();
    let mut s = format!(
        "seed {} (next: {})   {} ({})   {} {}   {:.0} bpm\nloop {}   step {:02}/{}   intensity {:.2} ({})\n\n",
        st.seed,
        music.seed(),
        st.preset.name(),
        st.pace.name(),
        st.key_name(),
        st.scale.name(),
        st.bpm,
        st.loop_index,
        st.step,
        STEPS_PER_LOOP,
        st.intensity,
        if music.auto_intensity() { "auto" } else { "manual" },
    );

    for layer in Layer::ALL {
        let i = layer.index();
        let state = if !st.enabled[i] {
            "OFF"
        } else if !st.audible[i] {
            "waiting"
        } else {
            "on"
        };
        let action = match st.actions[i] {
            LayerAction::Regenerate => "new pattern",
            LayerAction::Repeat => "repeated",
            LayerAction::Mutate => "mutated",
        };
        s.push_str(&format!(
            "[{}] {:<6} {:<8} {}\n",
            i + 1,
            layer.name(),
            state,
            action
        ));
    }
    text.0 = s;
}
