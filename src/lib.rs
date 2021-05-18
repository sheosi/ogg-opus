mod common;
mod decode;
mod encode;

use thiserror::Error;

pub use decode::decode;
pub use encode::encode;

use std::io::{Read, Seek, SeekFrom};
pub fn is_ogg_opus<T: Read + Seek>(mut d: T) -> bool {
    let mut buff = [0u8; 8];
    if let Ok(_) = d.seek(SeekFrom::Start(28)) {
        if let Ok(d) = d.read(&mut buff) {
            if d == 8 {
                return buff == common::OPUS_MAGIC_HEADER;
            }
        }
    }
    // If anything fails
    false
}
#[derive(Debug, Error)]
pub enum Error {
    #[error("Input audio was malformed")]
    MalformedAudio,

    #[error("Encoding error")]
    OpusError(#[from] magnum_opus::Error),

    #[error("Failed to decode ogg")]
    OggReadError(#[from] ogg::OggReadError),

    #[error("Faile to write in OGG")]
    OggWriteError(#[from]std::io::Error)
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
