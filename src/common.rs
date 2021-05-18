use crate::Error;

use audiopus::SampleRate;

// We use this to check whether a file is ogg opus or not inside the client
pub(crate) const OGG_OPUS_SPS: u32 = 48000;
pub(crate) const MAX_NUM_CHANNELS: u8 = 2;

pub(crate) const fn calc_sr(val:u16, org_sr: u32, dest_sr: u32) -> u16 {
    ((val as u32 * dest_sr) /org_sr) as u16
}
pub(crate) const fn calc_sr_u64(val:u64, org_sr: u32, dest_sr: u32) -> u64 {
    (val * dest_sr as u64) /(org_sr as u64)
}

pub(crate) const fn s_ps_to_audiopus(s_ps: u32) -> Result<SampleRate, Error> { 
    match s_ps {
        8000 => Ok(SampleRate::Hz8000),
        12000 => Ok(SampleRate::Hz12000),
        16000 => Ok(SampleRate::Hz16000),
        24000 => Ok(SampleRate::Hz24000),
        48000 => Ok(SampleRate::Hz48000),
        _ => return Err(Error::InvalidSps)
    }
}