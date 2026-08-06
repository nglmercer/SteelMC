use std::io::{self, Cursor, Write};

use crate::{
    codec::VarInt,
    serial::{ReadFrom, WriteTo},
};

/// A raw block state id. Using the registry this id can be derived into a block and it's current properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct BlockStateId(pub u16);

impl WriteTo for BlockStateId {
    fn write(&self, writer: &mut impl Write) -> io::Result<()> {
        VarInt(i32::from(self.0)).write(writer)
    }
}

impl ReadFrom for BlockStateId {
    fn read(data: &mut Cursor<&[u8]>) -> io::Result<Self> {
        let id = VarInt::read(data)?.0;
        #[expect(
            clippy::cast_sign_loss,
            reason = "VarInt is validated upstream; block state IDs are non-negative"
        )]
        Ok(Self(id as u16))
    }
}
