use crate::common::*;
use crate::Error;
use audiopus::coder::{Decoder as OpusDec, GenericCtl};
use ogg::{Packet, PacketReader};
use std::convert::TryFrom;
use std::io::{Read, Seek, SeekFrom};

//--- Final range  things ------------------------------------------------------

#[cfg(test)]
use std::cell::Cell;

#[cfg(test)]
thread_local! {
    static LAST_FINAL_RANGE: Cell<u32> = const { Cell::new(0) };
}

#[cfg(test)]
fn set_final_range(r: u32) {
    LAST_FINAL_RANGE.with(|f| f.set(r));
}

// Just here so that it can be used in the function
#[cfg(not(test))]
fn set_final_range(_: u32) {}

#[cfg(test)]
pub(crate) fn get_final_range() -> u32 {
    LAST_FINAL_RANGE.with(|f| f.get())
}

//--- Sizing the output up front -----------------------------------------------

// An Ogg page carries at most 255 segments of 255 bytes, after a 27 byte header
// and the segment table, so the last page always starts within this many bytes
// of the end of the stream.
const MAX_OGG_PAGE: u64 = 27 + 255 + 255 * 255;

// Ceiling on what a size hint may ask us to reserve, so that a corrupt or
// hostile granule position cannot turn into a huge allocation. Past this the
// buffer just grows the usual way, where doubling is amortised anyway.
const MAX_PREALLOC_SAMPLES: u64 = 32 * 1024 * 1024; // 64 MiB of i16

/// Granule position of the last Ogg page, which by RFC 7845 counts every sample
/// the stream decodes to (at 48 kHz, per channel, pre-skip included).
///
/// This is only ever a hint: every failure path returns `None`, the caller
/// clamps whatever comes back, and a wrong answer costs at most a reallocation.
/// That is why it can get away with scanning for the capture pattern rather
/// than walking the page chain.
fn last_page_granule<T: Read + Seek>(data: &mut T) -> Option<u64> {
    let start = data.stream_position().ok()?;
    let end = data.seek(SeekFrom::End(0)).ok()?;

    let window = MAX_OGG_PAGE.min(end);
    let mut tail = vec![0u8; window as usize];
    let read = data
        .seek(SeekFrom::Start(end - window))
        .and_then(|_| data.read_exact(&mut tail));

    // Put the stream back where we found it before reporting any failure.
    data.seek(SeekFrom::Start(start)).ok()?;
    read.ok()?;

    tail.windows(27)
        .rev()
        .find(|w| w.starts_with(b"OggS") && w[4] == 0)
        .map(|w| u64::from_le_bytes(w[6..14].try_into().unwrap()))
        // -1 marks a page on which no packet finishes, so it says nothing about
        // the length of the stream.
        .filter(|&granule| granule != u64::MAX)
}

//--- Code ---------------------------------------------------------------------

pub struct PlayData {
    pub channels: u16,
}

struct DecodeData {
    pre_skip: u16,
    gain: i32,
}

/**Reads audio from Ogg Opus, note: it only can read from the ones produced
by itself, this is not ready for anything more, third return is final range just
available while testing, otherwise it is a 0*/
pub fn decode<T: Read + Seek, const TARGET_SPS: u32>(
    mut data: T,
) -> Result<(Vec<i16>, PlayData), Error> {
    let opus_sr = const {
        match s_ps_to_audiopus(TARGET_SPS) {
            Some(v) => v,
            None => panic!("Wrong SampleRate"),
        }
    };

    // Read while the stream is still ours to seek around in: the demuxer below
    // consumes it front to back.
    let granule_hint = last_page_granule(&mut data);

    // Data
    let mut reader = PacketReader::new(data);

    let fp = reader
        .read_packet_expected()
        .map_err(|_| Error::MalformedAudio)?;
    let (play_data, dec_data) = check_fp::<TARGET_SPS>(&fp)?;

    let chans = match play_data.channels {
        1 => audiopus::Channels::Mono,
        2 => audiopus::Channels::Stereo,
        _ => return Err(Error::MalformedAudio),
    };

    // According to RFC7845 if a device supports 48Khz, decode at this rate
    let mut decoder = OpusDec::new(opus_sr, chans)?;
    decoder.set_gain(dec_data.gain)?;

    // Vendor and other tags, do a basic check
    let sp = reader
        .read_packet_expected()
        .map_err(|_| Error::MalformedAudio)?;

    check_sp(&sp)?;

    let chans = play_data.channels as usize;

    // The last granule counts every sample the stream decodes to, including the
    // pre-skip that gets trimmed away again, so dropping it gives the exact
    // output length. Anything unexpected in the stream just costs a regrow.
    let capacity = granule_hint
        .map(|granule| calc_sr_u64(granule, OGG_OPUS_SPS, TARGET_SPS))
        .map(|samples| samples.saturating_sub(dec_data.pre_skip as u64) * chans as u64)
        .unwrap_or(0)
        .min(MAX_PREALLOC_SAMPLES);

    let mut buffer = Vec::with_capacity(capacity as usize);
    // Counted per channel, like everything Opus reports; `temp_buffer` is
    // interleaved, so indexing it means scaling by the channel count.
    let mut rem_skip = dec_data.pre_skip as usize;
    let mut dec_absgsp = 0;
    // We don't need to reallocate temp_buffer because:
    // 1) We dont borrow
    // 2) Decoder fully rewrites temp_buffer
    let mut temp_buffer = [0; MAX_FRAME_SIZE];

    while let Some(packet) = reader.read_packet()? {
        let inner_packet = audiopus::packet::Packet::try_from(&packet.data)?;
        let again_buffer = audiopus::MutSignals::try_from(&mut temp_buffer[..])?;

        let out_size = decoder.decode(Some(inner_packet), again_buffer, false)?;

        dec_absgsp += out_size;

        // out_size == num of samples *per channel*
        if rem_skip < out_size {
            let mut trimmed_end = out_size * chans;
            if packet.last_in_stream() {
                let absgsp = calc_sr_u64(packet.absgp_page(), OGG_OPUS_SPS, TARGET_SPS) as usize;

                if dec_absgsp > absgsp {
                    trimmed_end -= (dec_absgsp - absgsp) * chans;
                }
            }

            buffer.extend_from_slice(&temp_buffer[rem_skip * chans..trimmed_end]);
            rem_skip = 0;
        } else {
            rem_skip -= out_size;
        }
    }

    if cfg!(test) {
        set_final_range(decoder.final_range().unwrap())
    };

    Ok((buffer, play_data))
}

fn check_sp(sp: &Packet) -> Result<(), Error> {
    if sp.data.len() < 12 {
        return Err(Error::MalformedAudio);
    }

    if !sp.data.starts_with(b"OpusTags") {
        return Err(Error::MalformedAudio);
    }

    Ok(())
}

// Analyze first page, where all the metadata we need is contained
fn check_fp<const TARGET_SPS: u32>(fp: &Packet) -> Result<(PlayData, DecodeData), Error> {
    // Check size
    if fp.data.len() < 19 {
        return Err(Error::MalformedAudio);
    }

    // Read magic header
    if fp.data[0..8] != OPUS_MAGIC_HEADER {
        return Err(Error::MalformedAudio);
    }

    // Read version
    if fp.data[8] != 1 {
        return Err(Error::MalformedAudio);
    }

    Ok((
        PlayData {
            channels: fp.data[9] as u16, // Number of channels
        },
        DecodeData {
            pre_skip: calc_sr(
                u16::from_le_bytes([fp.data[10], fp.data[11]]),
                OGG_OPUS_SPS,
                TARGET_SPS,
            ),
            gain: i16::from_le_bytes([fp.data[16], fp.data[17]]) as i32,
        },
    ))
}
