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
    use std::fs::{File};
    use std::io::{Cursor};

        fn read_file_i16(path: &str) -> Vec<i16> {
            let mut f = File::open(path).expect("no file found");
            let (_, b) = wav::read(&mut f).unwrap();
            b.try_into_sixteen().unwrap()
        }

        #[test]
        fn dec_enc_empty() {
            let audio = Vec::new();
            let (opus,enc_fin_range) = crate::encode::<16000>(&audio).unwrap();
            let (audio2, _,dec_fin_range) = crate::decode::<_,16000>(Cursor::new(opus)).unwrap();
            assert_eq!(audio.len(), audio2.len()); // Should be the same, empty
            assert_eq!(enc_fin_range, dec_fin_range);
        }

        #[test]
        fn dec_enc_recording_big() {
            let audio = read_file_i16("test_assets/big.wav");
            let (opus,enc_fin_range) = crate::encode::<16000>(&audio).unwrap();
            let (a2,_,dec_fin_range) = crate::decode::<_,16000>(Cursor::new(opus)).unwrap();
            assert_eq!(dec_fin_range, enc_fin_range);
            assert_eq!(audio.len(), a2.len());
        }

        #[test]
        fn dec_enc_recording_small() {
            let audio = read_file_i16("test_assets/small.wav");
            let (opus, enc_fin_range) = crate::encode::<16000>(&audio).unwrap();
            let (a2, _, dec_fin_range) = crate::decode::<_, 16000>(Cursor::new(opus)).unwrap();
            assert_eq!(dec_fin_range, enc_fin_range);
            assert_eq!(audio.len(), a2.len());
        }

        #[test]
        // Record, encode, decode , encode and decode again, finally compare the
        // first and second decodes, to make sure nothing is lost (can't compare
        // raw audio as vorbis is lossy)
        fn dec_enc_recording_whole() {
            let audio = read_file_i16("test_assets/small.wav");
            let (opus, enc_fr1) = crate::encode::<16000>(&audio).unwrap();
            let (audio2, _, dec_fr1) = crate::decode::<_, 16000>(Cursor::new(opus)).unwrap();
            let (opus2, enc_fr2) = crate::encode::<16000>(&audio2).unwrap();
            let (audio3, _, dec_fr2) = crate::decode::<_, 16000>(Cursor::new(opus2)).unwrap();
            assert_eq!(audio2.len(), audio3.len());
            assert_eq!(enc_fr1, dec_fr1);
            assert_eq!(enc_fr2, dec_fr2);
        }
}
