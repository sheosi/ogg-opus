use std::process;

use crate::common::*;
use crate::Error;

use audiopus::{
    coder::{Encoder as OpusEnc, GenericCtl},
    Bitrate,
};
use ogg::{PacketWriteEndInfo, PacketWriter};

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

//--- Code ---------------------------------------------------------------------

// Samples *per channel* held by `ms` milliseconds of audio.
const fn to_samples<const S_PS: u32>(ms: u32) -> usize {
    ((S_PS * ms) / 1000) as usize
}

// Interleaved samples held by a frame of `tenths_ms` tenths of a millisecond.
const fn frame_len(tenths_ms: u32, channels: u8, sps: u32) -> usize {
    ((sps * tenths_ms * channels as u32) / 10_000) as usize
}

// Determine opus channels at compile-time if possible
const fn opus_channels(val: u8) -> audiopus::Channels {
    match val {
        1 => audiopus::Channels::Mono,
        2 => audiopus::Channels::Stereo,
        _ => panic!("Invalid number of channels. Use 1 or 2 instead."),
    }
}

const fn is_end_of_stream(pos: usize, max: usize) -> PacketWriteEndInfo {
    if pos == max {
        PacketWriteEndInfo::EndStream
    } else {
        PacketWriteEndInfo::NormalPacket
    }
}

// Granule positions are counted per channel and always expressed at 48 kHz.
const fn granule<const S_PS: u32>(samples_per_channel: usize) -> u64 {
    calc_sr_u64(samples_per_channel as u64, S_PS, OGG_OPUS_SPS)
}

// Largest frame we ever hand to Opus, in interleaved samples: 20 ms of 48 kHz
// stereo. Used to size the stack buffer that pads frames with silence.
const MAX_FRAME_LEN: usize = frame_len(FRAME_TIME_MS * 10, MAX_NUM_CHANNELS, OGG_OPUS_SPS);
// Same, for the shortest frame Opus accepts (2.5 ms).
const MAX_SHORT_FRAME_LEN: usize = frame_len(MIN_FRAME_TENTHS_MS, MAX_NUM_CHANNELS, OGG_OPUS_SPS);

pub fn encode<const S_PS: u32, const NUM_CHANNELS: u8>(audio: &[i16]) -> Result<Vec<u8>, Error> {
    let opus_sr = const {
        match s_ps_to_audiopus(S_PS) {
            Some(v) => v,
            None => panic!("Wrong SampleRate"),
        }
    };

    // More frame time, sligtly less overhead more problematic packet loses,
    // a frame time of 20ms is considered good enough for most applications
    let channels = const { NUM_CHANNELS as usize };
    let frame = const { to_samples::<S_PS>(FRAME_TIME_MS) * NUM_CHANNELS as usize };

    let mut opus_encoder = OpusEnc::new(
        opus_sr,
        const { opus_channels(NUM_CHANNELS) },
        audiopus::Application::Audio,
    )?;
    // This gives a bitrate of 24 Kb/s, exactly what IBM recommends
    opus_encoder.set_bitrate(Bitrate::BitsPerSecond(BITRATE_BPS as i32))?;

    // Opus reports its lookahead per channel; every position below indexes the
    // interleaved stream, so keep the pre-skip in that same unit.
    let lookahead = opus_encoder.lookahead()? as u16;
    let pre_skip = lookahead as usize * channels;
    let pre_skip_48 = calc_sr(lookahead, S_PS, OGG_OPUS_SPS);

    let mut encoder = InnerEncoder::new(opus_encoder);

    // What the decoder will see: the silence Opus needs to prime itself,
    // followed by the caller's audio.
    let total = audio.len() + pre_skip;
    let full_frames = total / frame;

    // At a fixed bitrate the output size is known up front within a few
    // percent, so one allocation covers the whole stream. The extra eighth
    // absorbs the Ogg page headers and the encoder's VBR wobble.
    let payload = (audio.len() as u64 * BITRATE_BPS as u64) / (8 * S_PS as u64 * channels as u64);
    let mut buffer = Vec::with_capacity((payload + payload / 8) as usize + 512);

    // Everything but the pre-skip is known at compile time.
    let mut opus_head = const {
        let mut head = [0u8; 19];
        let mut i = 0;
        while i < OPUS_MAGIC_HEADER.len() {
            head[i] = OPUS_MAGIC_HEADER[i];
            i += 1;
        }
        head[8] = 1; // Version number, always 1
        head[9] = NUM_CHANNELS;
        // 10..12 is the pre-skip, patched in below
        let sps = S_PS.to_le_bytes(); // Original Hz (informational)
        head[12] = sps[0];
        head[13] = sps[1];
        head[14] = sps[2];
        head[15] = sps[3];
        // 16..18 output gain and 18 channel map family stay zero. If the
        // channel map were != 0 a channel mapping table would follow.
        head
    };
    opus_head[10..12].copy_from_slice(&pre_skip_48.to_le_bytes());

    // Generate the serial which is nothing but a value to identify a stream, we
    // will also use the process id so that two programs don't use
    // the same serial even if getting one at the same time
    let serial = rand::random::<u32>() ^ process::id();

    let mut packet_writer = PacketWriter::new(&mut buffer);

    packet_writer.write_packet(&opus_head[..], serial, PacketWriteEndInfo::EndPage, 0)?;
    packet_writer.write_packet(&OPUS_TAGS[..], serial, PacketWriteEndInfo::EndPage, 0)?;

    for counter in 0..full_frames {
        let pos_a = counter * frame;
        let pos_b = pos_a + frame;

        let packet = encoder.encode_padded(audio, pos_a, pos_b, pre_skip)?;

        packet_writer.write_packet(
            packet,
            serial,
            is_end_of_stream(pos_b, total),
            granule::<S_PS>(pos_b / channels),
        )?;
    }

    // Whatever did not fill a 20 ms frame goes out as the longest frames that
    // still fit, down to 2.5 ms.
    let tail_frames = const {
        [
            frame_len(MIN_FRAME_TENTHS_MS, NUM_CHANNELS, S_PS),
            frame_len(50, NUM_CHANNELS, S_PS),
            frame_len(100, NUM_CHANNELS, S_PS),
            frame_len(200, NUM_CHANNELS, S_PS),
        ]
    };

    let mut last_sample = full_frames * frame;

    while last_sample < total {
        let rem_samples = total - last_sample;
        let last_audio_s = last_sample.saturating_sub(pre_skip);

        if let Some(&size) = tail_frames.iter().rev().find(|&&size| size <= rem_samples) {
            let packet = if last_sample >= pre_skip {
                encoder.encode_frame(&audio[last_audio_s..last_audio_s + size])?
            } else {
                encoder.encode_padded(audio, last_sample, last_sample + size, pre_skip)?
            };
            last_sample += size;
            packet_writer.write_packet(
                packet,
                serial,
                is_end_of_stream(last_sample, total),
                granule::<S_PS>(last_sample / channels),
            )?;
        } else {
            // Less than 2.5 ms left: pad it with silence and end the stream
            // there. The granule position still marks the true end of the
            // audio, so the decoder trims the padding back off.
            let mut in_buffer = [0i16; MAX_SHORT_FRAME_LEN];
            let rem_skip = pre_skip.saturating_sub(last_sample);
            in_buffer[rem_skip..rem_samples].copy_from_slice(&audio[last_audio_s..]);

            last_sample = total; // We end this here

            let packet = encoder.encode_frame(&in_buffer[..tail_frames[0]])?;
            packet_writer.write_packet(
                packet,
                serial,
                PacketWriteEndInfo::EndStream,
                granule::<S_PS>(total / channels),
            )?;
        }
    }

    if cfg!(test) {
        set_final_range(encoder.encoder.final_range().unwrap())
    }

    Ok(buffer)
}

struct InnerEncoder {
    encoder: OpusEnc,
    // Reused by every packet: Opus never writes more than `MAX_PACKET` bytes,
    // but a 20 ms frame at 24 Kb/s only fills about 60 of them.
    scratch: [u8; MAX_PACKET],
}

impl InnerEncoder {
    fn new(encoder: OpusEnc) -> Self {
        Self {
            encoder,
            scratch: [0; MAX_PACKET],
        }
    }

    // Encodes one frame, sized to exactly what Opus produced. `PacketWriter`
    // holds on to every packet until it flushes a page (up to 255 of them), so
    // handing it right-sized buffers keeps that page buffer small.
    fn encode_frame(&mut self, audio: &[i16]) -> Result<Vec<u8>, Error> {
        let len = self.encoder.encode(audio, &mut self.scratch)?;
        Ok(self.scratch[..len].to_vec())
    }

    // Same, for a frame that may overlap the leading silence: `pos_a`/`pos_b`
    // index the padded stream, `audio` starts at `pre_skip`.
    fn encode_padded(
        &mut self,
        audio: &[i16],
        pos_a: usize,
        pos_b: usize,
        pre_skip: usize,
    ) -> Result<Vec<u8>, Error> {
        if pos_a >= pre_skip {
            return self.encode_frame(&audio[pos_a - pre_skip..pos_b - pre_skip]);
        }

        let mut padded = [0i16; MAX_FRAME_LEN];
        let frame = &mut padded[..pos_b - pos_a];
        if pos_b > pre_skip {
            frame[pre_skip - pos_a..].copy_from_slice(&audio[..pos_b - pre_skip]);
        }
        self.encode_frame(frame)
    }
}
