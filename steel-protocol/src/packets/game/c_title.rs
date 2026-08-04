use steel_macros::{ClientPacket, WriteTo};
use steel_registry::packets::play::{
    C_CLEAR_TITLES, C_SET_ACTION_BAR_TEXT, C_SET_SUBTITLE_TEXT, C_SET_TITLE_TEXT,
    C_SET_TITLES_ANIMATION,
};
use text_components::{TextComponent, resolving::TextResolutor};

/// Sets the large title line, which is shown until cleared or replaced.
#[derive(ClientPacket, WriteTo, Clone, Debug)]
#[packet_id(Play = C_SET_TITLE_TEXT)]
pub struct CSetTitleText {
    /// The title text.
    pub title: TextComponent,
}

impl CSetTitleText {
    /// Creates the packet with `title` resolved for one viewer.
    pub fn new<T: TextResolutor>(title: &TextComponent, player: &T) -> Self {
        Self {
            title: title.resolve(player),
        }
    }
}

/// Sets the smaller line beneath the title.
///
/// The client holds this until a title is shown, so it may be sent first.
#[derive(ClientPacket, WriteTo, Clone, Debug)]
#[packet_id(Play = C_SET_SUBTITLE_TEXT)]
pub struct CSetSubtitleText {
    /// The subtitle text.
    pub subtitle: TextComponent,
}

impl CSetSubtitleText {
    /// Creates the packet with `subtitle` resolved for one viewer.
    pub fn new<T: TextResolutor>(subtitle: &TextComponent, player: &T) -> Self {
        Self {
            subtitle: subtitle.resolve(player),
        }
    }
}

/// Shows a message above the hotbar.
#[derive(ClientPacket, WriteTo, Clone, Debug)]
#[packet_id(Play = C_SET_ACTION_BAR_TEXT)]
pub struct CSetActionBarText {
    /// The action bar text.
    pub text: TextComponent,
}

impl CSetActionBarText {
    /// Creates the packet with `text` resolved for one viewer.
    pub fn new<T: TextResolutor>(text: &TextComponent, player: &T) -> Self {
        Self {
            text: text.resolve(player),
        }
    }
}

/// Sets how long a title fades in, stays, and fades out, all in ticks.
#[derive(ClientPacket, WriteTo, Clone, Debug)]
#[packet_id(Play = C_SET_TITLES_ANIMATION)]
pub struct CSetTitlesAnimation {
    /// Ticks spent fading the title in. Vanilla writes these as plain 4-byte ints.
    pub fade_in: i32,
    /// Ticks the title stays fully visible.
    pub stay: i32,
    /// Ticks spent fading the title out.
    pub fade_out: i32,
}

/// Hides any showing title, optionally restoring the default timings.
#[derive(ClientPacket, WriteTo, Clone, Debug)]
#[packet_id(Play = C_CLEAR_TITLES)]
pub struct CClearTitles {
    /// Whether to also restore the default fade-in, stay and fade-out times.
    pub reset_times: bool,
}
