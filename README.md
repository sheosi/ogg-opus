# Ogg Opus
A decoder/encoder for Ogg Opus in [Rust](https://www.rust-lang.org/)

# Usage
Add to your `cargo.toml`

```toml
ogg-opus = "^0.1"
```

## Minimum Rust version

Since we use `const generics`, the minimum version of [Rust](https://www.rust-lang.org/) is [1.51](https://blog.rust-lang.org/2021/03/25/Rust-1.51.0.html)

# Example

## Encode

This example makes use of `wav` crate, you can use it adding to your `cargo.toml` file:

```toml
wav = "^1.0"
```

```rust
let mut f = File::open("my_file.wav").unwrap();
let (_, b) = wav::read(&mut f).unwrap();
let audio = b.try_into_sixteen().unwrap();
let opus = ogg_opus::encode::<16000, 1>(&audio).unwrap();
```

## Decode

### Read from file

```rust
let mut f = File::open("my_file.ogg").unwrap();
let (raw, header) = ogg_opus::decode::<_,16000>(f).unwrap();
```

### Read from Vec
```rust
use std::io::Cursor;

// Let's say this vec contains Ogg Opus data
let opus: Vec<u8> = Vec::new();
let (raw, header) = ogg_opus::decode::<_,16000>(Cursor::new(opus)).unwrap();
```

# What works and what not

* Only supports `i16` (integer of 16 bits) for the raw part.
* Both mono and stereo are supported but only mono is tested.
* More channels than stereo are untested and will probably break it.
* Supports decoding and encoding any sample rate supported by Opus (8k Hz, 12k Hz, 24k Hz and 64k Hz) but only 16k Hz has been tested
* Encoding is set to a bitrate of 24k (because of Lily's constraints)
* There's still some inaccuracies around start and end of audio (can't tell if it's due to the encoder or the decoder)
* Advanced decode and encoding features (repairables streams, fec and others)