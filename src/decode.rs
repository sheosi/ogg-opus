use std::io::{Read, Seek};

use crate::Error;
use crate::common::*;
use byteorder::{LittleEndian, ByteOrder};
use ogg::{Packet, PacketReader};
use audiopus::{coder::{Decoder as OpusDec, GenericCtl}};

//--- Final range  things ------------------------------------------------------

#[cfg(test)]
use std::cell::RefCell;

#[cfg(test)]
thread_local! {
    static LAST_FINAL_RANGE: RefCell<u32> = RefCell::new(0);
}

#[cfg(test)]
fn set_final_range(r:u32) {
    LAST_FINAL_RANGE.with(|f|*f.borrow_mut() = r);
}

// Just here so that it can be used in the function
#[cfg(not(test))]
fn set_final_range(_:u32) {}

#[cfg(test)]
pub(crate) fn get_final_range() -> u32 {
    LAST_FINAL_RANGE.with(|f|*f.borrow())
}

//--- Code ---------------------------------------------------------------------

pub struct PlayData {
    pub channels: u16
}

pub(crate) const OPUS_MAGIC_HEADER:[u8;8] = [b'O', b'p', b'u', b's', b'H', b'e', b'a', b'd'];

/**Reads audio from Ogg Opus, note: it only can read from the ones produced 
by itself, this is not ready for anything more, third return is final range just
available while testing, otherwise it is a 0*/
pub fn decode<T: Read + Seek, const TARGET_SPS: u32>(data: T) -> Result<(Vec<i16>, PlayData), Error> {
    // Data
    const MAX_FRAME_SAMPLES: usize = 5760; // According to opus_decode docs
    const MAX_FRAME_SIZE: usize = MAX_FRAME_SAMPLES * (MAX_NUM_CHANNELS as usize); // Our buffer will be i16 so, don't convert to bytes

    let mut reader = PacketReader::new(data);
    let fp = reader.read_packet_expected().map_err(|_| Error::MalformedAudio)?; // Header

    struct DecodeData {
        pre_skip: u16,
        gain: i32
    }

    // Analyze first page, where all the metadata we need is contained
    fn check_fp<const TARGET_SPS: u32>(fp: &Packet) -> Result<(PlayData, DecodeData), Error> {

        // Check size
        if fp.data.len() < 19 {
            return Err(Error::MalformedAudio)
        }

        // Read magic header
        if fp.data[0..8] != OPUS_MAGIC_HEADER {
            return Err(Error::MalformedAudio)
        }

        // Read version
        if fp.data[8] != 1 {
            return Err(Error::MalformedAudio)
        }
        

        Ok((
            PlayData{
                channels: fp.data[9] as u16, // Number of channels
            },
            DecodeData {
                pre_skip: calc_sr(LittleEndian::read_u16(&fp.data[10..12]), OGG_OPUS_SPS, TARGET_SPS),
                gain: LittleEndian::read_i16(&fp.data[16..18]) as i32
            }
        ))
    }

    let (play_data, dec_data) = check_fp::<TARGET_SPS>(&fp)?;

    let chans = match play_data.channels {
        1 => Ok(audiopus::Channels::Mono),
        2 => Ok(audiopus::Channels::Stereo),
        _ => Err(Error::MalformedAudio)
    }?;

    let opus_sr = s_ps_to_audiopus(TARGET_SPS)?;

    // According to RFC7845 if a device supports 48Khz, decode at this rate
    let mut decoder =  OpusDec::new(opus_sr, chans)?;
    decoder.set_gain(dec_data.gain)?;

    // Vendor and other tags, do a basic check
    let sp = reader.read_packet_expected().map_err(|_| Error::MalformedAudio)?; // Tags
    fn check_sp(sp: &Packet) -> Result<(), Error> {
        if sp.data.len() < 12 {
            return Err(Error::MalformedAudio)
        }

        let head = std::str::from_utf8(&sp.data[0..8]).or_else(|_| Err(Error::MalformedAudio))?;
        if head != "OpusTags" {
            return Err(Error::MalformedAudio)
        }
        
        Ok(())
    }
        
    check_sp(&sp)?;

    let mut buffer: Vec<i16> = Vec::new();
    let mut rem_skip = dec_data.pre_skip as usize;
    let mut dec_absgsp = 0;
    while let Some(packet) = reader.read_packet()? {
        let mut temp_buffer = [0i16; MAX_FRAME_SIZE];
        let out_size = decoder.decode(Some(&packet.data), &mut temp_buffer[..], false)?;
        let absgsp = calc_sr_u64(packet.absgp_page(),OGG_OPUS_SPS, TARGET_SPS) as usize;
        dec_absgsp += out_size;
        let trimmed_end = if packet.last_in_stream() && dec_absgsp > absgsp {
            (out_size as usize * play_data.channels as usize) - (dec_absgsp - absgsp)
        }
        else {
            // out_size == num of samples *per channel*
            out_size as usize * play_data.channels as usize
        } as usize;

        if rem_skip < out_size {
            buffer.extend_from_slice(&temp_buffer[rem_skip..trimmed_end]);
            rem_skip = 0;
        }
        else {
            rem_skip -= out_size;
        }

    }

    if cfg!(test) {set_final_range(decoder.final_range().unwrap())};

    Ok( (buffer, play_data))
}
