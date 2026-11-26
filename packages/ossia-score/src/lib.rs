#![doc = include_str!("../README.md")]
mod index;
mod instruments;
mod layout;
mod lexer;
mod midi;
mod midi_range;
mod pitch;
mod position;
mod selection_source;
pub use selection_source::*;
mod selection;
pub use selection::*;

pub use index::*;
mod identity;
pub use identity::*;
mod crop;
pub use crop::*;
pub use instruments::*;
pub use layout::*;
pub use lexer::{serialize, tokenize, Mode, Token, TokenKind};
pub use midi::*;
pub use midi_range::*;
pub use ossia_types::{
    AppError, ContentHash, ErrorCode, LocatedRegion, LocationCapability, Locator, MeasureRange,
    SourceSpan,
};
pub use pitch::*;
pub use position::*;

mod analysis;
pub use analysis::*;

mod playback;
pub use playback::*;

mod verification;
pub use verification::*;

/// Whether the first source line selects Ossia's numbered notation layout.
pub fn is_jianpu_source(source: &str) -> bool {
    source
        .trim_start_matches('\u{feff}')
        .lines()
        .next()
        .is_some_and(|line| line.trim_end() == "% ossia:notation=jianpu")
}
