use std::io::{self, Write};

use glam::{DVec3, IVec3};
use steel_macros::{ClientPacket, WriteTo};
use steel_registry::packets::play::{C_SOUND, C_STOP_SOUND};
use steel_registry::sound_event::SoundEventRef;
use steel_utils::{Identifier, codec::VarInt};

/// A sound event sent either by registry id or written inline.
///
/// Mirrors vanilla's `Holder<SoundEvent>` encoding: id `0` introduces a direct event whose
/// identifier and optional fixed range follow, and any registered event is written as
/// `registry_id + 1`. `/playsound` needs the direct form, because vanilla accepts an arbitrary
/// identifier there rather than requiring a registered sound.
#[derive(Clone, Debug, PartialEq)]
pub enum SoundHolder {
    /// A registered sound event, already encoded as `registry_id + 1`.
    Registered(i32),
    /// An inline sound event, as vanilla's `SoundEvent.DIRECT_STREAM_CODEC` writes it.
    Direct {
        /// The sound's identifier.
        key: Identifier,
        /// An explicit audible range; `None` derives it from the volume.
        fixed_range: Option<f32>,
    },
}

impl steel_utils::serial::WriteTo for SoundHolder {
    fn write(&self, writer: &mut impl Write) -> io::Result<()> {
        match self {
            Self::Registered(id) => VarInt(*id).write(writer),
            Self::Direct { key, fixed_range } => {
                VarInt(0).write(writer)?;
                key.write(writer)?;
                fixed_range.write(writer)
            }
        }
    }
}

/// Sound source categories (matches vanilla `SoundSource` enum order).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SoundSource {
    Master = 0,
    Music = 1,
    Records = 2,
    Weather = 3,
    Blocks = 4,
    Hostile = 5,
    Neutral = 6,
    Players = 7,
    Ambient = 8,
    Voice = 9,
    Ui = 10,
}

impl SoundSource {
    /// Returns the `VarInt` value for the enum.
    #[must_use]
    pub const fn as_varint(self) -> i32 {
        self as i32
    }
}

/// Sent to stop currently playing sounds on the client.
///
/// The flag byte says which of the two optional fields follow: `1` for the source category,
/// `2` for the sound identifier. Omitting both stops every sound.
#[derive(ClientPacket, Clone, Debug)]
#[packet_id(Play = C_STOP_SOUND)]
pub struct CStopSound {
    /// The category to silence, or `None` for every category.
    pub source: Option<SoundSource>,
    /// The sound to silence, or `None` for every sound.
    pub sound: Option<Identifier>,
}

impl steel_utils::serial::WriteTo for CStopSound {
    fn write(&self, writer: &mut impl Write) -> io::Result<()> {
        let flags = u8::from(self.source.is_some()) | (u8::from(self.sound.is_some()) << 1);
        flags.write(writer)?;
        if let Some(source) = self.source {
            VarInt(source.as_varint()).write(writer)?;
        }
        if let Some(sound) = self.sound.as_ref() {
            sound.write(writer)?;
        }
        Ok(())
    }
}

/// Sent to play a sound effect at a specific position.
///
/// The position is encoded at 8x precision (divide by 8 to get actual block coordinates).
/// This allows sub-block positioning for more accurate sound placement.
#[derive(WriteTo, ClientPacket, Clone, Debug)]
#[packet_id(Play = C_SOUND)]
pub struct CSound {
    /// The sound event, either by registry id or written inline.
    pub sound: SoundHolder,
    /// The sound source category (`VarInt`).
    #[write(as = VarInt)]
    pub source: i32,
    /// X position multiplied by 8 (fixed-point).
    pub pos: IVec3,
    /// Volume (1.0 = normal).
    pub volume: f32,
    /// Pitch (1.0 = normal).
    pub pitch: f32,
    /// Random seed for sound variations.
    pub seed: i64,
}

impl CSound {
    /// Creates a new sound packet.
    ///
    /// # Arguments
    /// * `sound` - Sound event to play
    /// * `source` - Sound source category
    /// * `x`, `y`, `z` - Position in block coordinates (will be scaled by 8)
    /// * `volume` - Volume multiplier (1.0 = normal)
    /// * `pitch` - Pitch multiplier (1.0 = normal)
    /// * `seed` - Random seed for sound variations
    #[must_use]
    pub fn new(
        sound: SoundEventRef,
        source: SoundSource,
        pos: DVec3,
        volume: f32,
        pitch: f32,
        seed: i64,
    ) -> Self {
        Self {
            sound: SoundHolder::Registered(sound.packet_holder_id()),
            source: source.as_varint(),
            pos: IVec3::new(
                (pos.x * 8.0) as i32,
                (pos.y * 8.0) as i32,
                (pos.z * 8.0) as i32,
            ),
            volume,
            pitch,
            seed,
        }
    }

    /// Creates a block sound packet at the center of a block position.
    ///
    /// # Arguments
    /// * `sound` - Sound event to play
    /// * `pos` - Block position (will be centered at +0.5)
    /// * `volume` - Volume multiplier
    /// * `pitch` - Pitch multiplier
    /// * `seed` - Random seed
    #[must_use]
    pub fn block_sound(
        sound: SoundEventRef,
        pos: steel_utils::BlockPos,
        volume: f32,
        pitch: f32,
        seed: i64,
    ) -> Self {
        Self::new(
            sound,
            SoundSource::Blocks,
            pos.0.as_dvec3().map(|v| v + 0.5),
            volume,
            pitch,
            seed,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Once;

    use steel_registry::{REGISTRY, Registry, RegistryEntry, sound_events};
    use steel_utils::BlockPos;

    use super::{CSound, CStopSound, SoundHolder, SoundSource};
    use steel_utils::{Identifier, serial::WriteTo as _};

    static INIT_REGISTRY: Once = Once::new();

    fn init_registry() {
        INIT_REGISTRY.call_once(|| {
            let mut registry = Registry::new_vanilla();
            registry.freeze();
            let _ = REGISTRY.init(registry);
        });
    }

    #[test]
    fn registered_sound_packet_uses_holder_id() {
        init_registry();

        let packet = CSound::block_sound(
            &sound_events::BLOCK_WOODEN_BUTTON_CLICK_ON,
            BlockPos::ZERO,
            1.0,
            1.0,
            0,
        );

        let expected_holder_id = sound_events::BLOCK_WOODEN_BUTTON_CLICK_ON.id() as i32 + 1;
        assert_eq!(
            sound_events::BLOCK_WOODEN_BUTTON_CLICK_ON.packet_holder_id(),
            expected_holder_id
        );
        assert_eq!(packet.sound, SoundHolder::Registered(expected_holder_id));
    }

    /// The flag byte is a bitmask: 1 for the category, 2 for the sound.
    #[test]
    fn stop_sound_flags_say_which_fields_follow() {
        for (source, sound, expected_flags) in [
            (None, None, 0_u8),
            (Some(SoundSource::Music), None, 1),
            (None, Some(Identifier::vanilla_static("custom")), 2),
            (
                Some(SoundSource::Music),
                Some(Identifier::vanilla_static("custom")),
                3,
            ),
        ] {
            let mut encoded = Vec::new();
            assert!(
                CStopSound {
                    source,
                    sound: sound.clone()
                }
                .write(&mut encoded)
                .is_ok()
            );
            assert_eq!(
                encoded[0], expected_flags,
                "source={source:?} sound={sound:?}"
            );
            if expected_flags == 0 {
                assert_eq!(encoded.len(), 1, "no fields follow an empty mask");
            }
        }
    }

    /// A direct sound event writes holder id 0, then its identifier and optional range.
    #[test]
    fn direct_sound_holder_writes_the_inline_form() {
        let mut encoded = Vec::new();
        assert!(
            SoundHolder::Direct {
                key: Identifier::vanilla_static("custom"),
                fixed_range: None,
            }
            .write(&mut encoded)
            .is_ok()
        );
        // 0, then the length-prefixed identifier, then a false Option tag.
        assert_eq!(encoded[0], 0);
        assert_eq!(*encoded.last().expect("a trailing option tag"), 0);
    }
}
