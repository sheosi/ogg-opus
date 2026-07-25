use audiopus::SampleRate;

// We use this to check whether a file is ogg opus or not inside the client
pub(crate) const OGG_OPUS_SPS: u32 = 48000;
pub(crate) const MAX_NUM_CHANNELS: u8 = 2;
pub(crate) const OPUS_MAGIC_HEADER: [u8; 8] = [b'O', b'p', b'u', b's', b'H', b'e', b'a', b'd'];
pub(crate) const MAX_FRAME_SAMPLES: usize = 5760; // According to opus_decode docs
pub(crate) const MAX_FRAME_SIZE: usize = MAX_FRAME_SAMPLES * (MAX_NUM_CHANNELS as usize); // Our buffer will be i16 so, don't convert to bytes
pub(crate) const FRAME_TIME_MS: u32 = 20;
pub(crate) const MAX_PACKET: usize = 4000; // Maximum theorical recommended by Opus
// Opus only accepts frames of 2.5/5/10/20/40/60 ms, so frame lengths are kept
// in tenths of a millisecond to stay integral.
pub(crate) const MIN_FRAME_TENTHS_MS: u32 = 25;
// Balance between quality, speed and size, especially for Telegram.
pub(crate) const BITRATE_BPS: u32 = 24000;
pub(crate) const VENDOR_STR: &str = concat!("ogg-opus", " ", std::env!("CARGO_PKG_VERSION"));
pub(crate) const VENDOR_STR_LEN: [u8; 4] = (VENDOR_STR.len() as u32).to_le_bytes();
pub(crate) const VENDOR_STR_BYTES: &[u8] = VENDOR_STR.as_bytes();

pub(crate) const OPUS_TAGS: [u8; 30] = [
    b'O',
    b'p',
    b'u',
    b's',
    b'T',
    b'a',
    b'g',
    b's',
    VENDOR_STR_LEN[0],
    VENDOR_STR_LEN[1],
    VENDOR_STR_LEN[2],
    VENDOR_STR_LEN[3],
    VENDOR_STR_BYTES[0],
    VENDOR_STR_BYTES[1],
    VENDOR_STR_BYTES[2],
    VENDOR_STR_BYTES[3],
    VENDOR_STR_BYTES[4],
    VENDOR_STR_BYTES[5],
    VENDOR_STR_BYTES[6],
    VENDOR_STR_BYTES[7],
    VENDOR_STR_BYTES[8],
    VENDOR_STR_BYTES[9],
    VENDOR_STR_BYTES[10],
    VENDOR_STR_BYTES[11],
    VENDOR_STR_BYTES[12],
    VENDOR_STR_BYTES[13],
    0,
    0,
    0,
    0,
];

pub(crate) const fn calc_sr(val: u16, org_sr: u32, dest_sr: u32) -> u16 {
    ((val as u32 * dest_sr) / org_sr) as u16
}
pub(crate) const fn calc_sr_u64(val: u64, org_sr: u32, dest_sr: u32) -> u64 {
    (val * dest_sr as u64) / (org_sr as u64)
}

pub(crate) const fn s_ps_to_audiopus(s_ps: u32) -> Option<SampleRate> {
    Some(match s_ps {
        8000 => SampleRate::Hz8000,
        12000 => SampleRate::Hz12000,
        16000 => SampleRate::Hz16000,
        24000 => SampleRate::Hz24000,
        48000 => SampleRate::Hz48000,
        _ => return None,
    })
}
