// We use this to check whether a file is ogg opus or not inside the client
pub(crate) const OPUS_MAGIC_HEADER:[u8;8] = [b'O', b'p', b'u', b's', b'H', b'e', b'a', b'd'];
pub(crate) const OGG_OPUS_SPS: u32 = 48000;
pub(crate) const MAX_NUM_CHANNELS: u8 = 2;

pub(crate) const fn calc_sr(val:u16, org_sr: u32, dest_sr: u32) -> u16 {
    ((val as u32 * dest_sr) /org_sr) as u16
}
pub(crate) const fn calc_sr_u64(val:u64, org_sr: u32, dest_sr: u32) -> u64 {
    (val * dest_sr as u64) /(org_sr as u64)
}