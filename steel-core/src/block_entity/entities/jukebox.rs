//! Jukebox block entity.

use std::mem;
use std::sync::{Arc, Weak};

use simdnbt::ToNbtTag as _;
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtTag};
use steel_registry::data_components::vanilla_components::JUKEBOX_PLAYABLE;
use steel_registry::item_stack::ItemStack;
use steel_registry::{
    REGISTRY, RegistryExt as _, RegistryHolder, level_events, vanilla_block_entity_types,
};
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, locks::SyncMutex};

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::world::World;

/// Ticks per second, used to convert a song's length into a tick budget.
const TICKS_PER_SECOND: f32 = 20.0;

struct JukeboxState {
    /// The record currently in the jukebox.
    record: ItemStack,
    /// Ticks since the current song started, or `None` when nothing is playing.
    ticks_since_song_started: Option<i64>,
}

/// Vanilla `JukeboxBlockEntity`, including its `JukeboxSongPlayer`.
pub struct JukeboxBlockEntity {
    base: BlockEntityBase,
    state: SyncMutex<JukeboxState>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `JukeboxBlockEntity`.
unsafe impl DowncastType for JukeboxBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/jukebox");
}

impl JukeboxBlockEntity {
    /// Creates an empty jukebox block entity.
    #[must_use]
    pub fn new(world: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self {
            base: BlockEntityBase::new(&vanilla_block_entity_types::JUKEBOX, world, pos, state),
            state: SyncMutex::new(JukeboxState {
                record: ItemStack::empty(),
                ticks_since_song_started: None,
            }),
        }
    }

    /// Returns the record currently in the jukebox.
    #[must_use]
    pub fn record(&self) -> ItemStack {
        self.state.lock().record.clone()
    }

    /// Returns whether a song is currently playing.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        self.state.lock().ticks_since_song_started.is_some()
    }

    /// Vanilla `JukeboxSongPlayer.play`: inserts a record and starts its song.
    pub fn play(&self, world: &Arc<World>, record: ItemStack) {
        let song_id = Self::song_id(&record);
        {
            let mut state = self.state.lock();
            state.record = record;
            state.ticks_since_song_started = song_id.map(|_| 0);
        }

        if let Some(song_id) = song_id {
            world.level_event(
                level_events::SOUND_PLAY_JUKEBOX_SONG,
                self.get_block_pos(),
                song_id,
                None,
            );
        }
        self.set_changed();
    }

    /// Vanilla `JukeboxBlockEntity.popOutTheItem`: takes the record back out.
    #[must_use]
    pub fn take_record(&self, world: &Arc<World>) -> ItemStack {
        let record = {
            let mut state = self.state.lock();
            state.ticks_since_song_started = None;
            mem::take(&mut state.record)
        };

        world.level_event(
            level_events::SOUND_STOP_JUKEBOX_SONG,
            self.get_block_pos(),
            0,
            None,
        );
        self.set_changed();
        record
    }

    /// Vanilla `JukeboxBlockEntity.getComparatorOutput`.
    #[must_use]
    pub fn comparator_output(&self) -> i32 {
        self.state
            .lock()
            .record
            .get(JUKEBOX_PLAYABLE)
            .map_or(0, |playable| playable.song().value().comparator_output)
    }

    /// The registry id the client uses to pick which song to play.
    fn song_id(record: &ItemStack) -> Option<i32> {
        let playable = record.get(JUKEBOX_PLAYABLE)?;
        let key = match playable.song() {
            RegistryHolder::Reference(song) => &song.key,
            // A directly-encoded song has no registry id to send to the client.
            RegistryHolder::Direct(_) => return None,
        };
        REGISTRY
            .jukebox_songs
            .id_from_key(key)
            .and_then(|id| i32::try_from(id).ok())
    }

    /// Ticks the song budget down, returning whether the song just finished.
    fn advance_song(&self) -> bool {
        let mut state = self.state.lock();
        let Some(ticks) = state.ticks_since_song_started.as_mut() else {
            return false;
        };
        *ticks += 1;

        let Some(playable) = state.record.get(JUKEBOX_PLAYABLE) else {
            state.ticks_since_song_started = None;
            return true;
        };

        #[expect(
            clippy::cast_possible_truncation,
            reason = "song lengths are a few hundred seconds at most"
        )]
        let length_ticks = (playable.song().value().length_in_seconds * TICKS_PER_SECOND) as i64;
        if state
            .ticks_since_song_started
            .is_some_and(|t| t >= length_ticks)
        {
            state.ticks_since_song_started = None;
            return true;
        }

        false
    }
}

impl BlockEntity for JukeboxBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    /// Vanilla `JukeboxSongPlayer.tick`: stops the song once it has run its length.
    fn tick(&self, world: &Arc<World>) {
        if self.advance_song() {
            world.level_event(
                level_events::SOUND_STOP_JUKEBOX_SONG,
                self.get_block_pos(),
                0,
                None,
            );
            self.set_changed();
        }
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let nbt_view: NbtCompoundView<'_, '_> = nbt.into();
        let mut state = self.state.lock();

        state.record = nbt_view
            .compound("RecordItem")
            .and_then(|compound| ItemStack::from_borrowed_compound(&compound))
            .unwrap_or_else(ItemStack::empty);
        state.ticks_since_song_started = nbt_view.long("ticks_since_song_started");
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        let state = self.state.lock();

        if !state.record.is_empty()
            && let NbtTag::Compound(record_nbt) = state.record.clone().to_nbt_tag()
        {
            nbt.insert("RecordItem", record_nbt);
        }

        if let Some(ticks) = state.ticks_since_song_started {
            nbt.insert("ticks_since_song_started", ticks);
        }
    }

    fn get_update_tag(&self) -> Option<NbtCompound> {
        None
    }
}
