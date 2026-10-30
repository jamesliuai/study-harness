use std::io::Cursor;
pub const MAX_SECONDS: f64 = 120.;
pub fn validate_wav(bytes: &[u8]) -> Result<f64, String> {
    if bytes.len() > 3_840_044 || bytes.len() < 44 {
        return Err("Recording must be a nonempty PCM WAV under 120 seconds.".into());
    }
    let mut reader = hound::WavReader::new(Cursor::new(bytes))
        .map_err(|_| "Recording could not be decoded as WAV.")?;
    let s = reader.spec();
    if s.channels != 1
        || s.sample_rate != 16000
        || s.bits_per_sample != 16
        || s.sample_format != hound::SampleFormat::Int
    {
        return Err("Recording must be mono 16 kHz 16-bit PCM WAV.".into());
    }
    let duration = reader.duration() as f64 / s.sample_rate as f64;
    if !(0.2..=MAX_SECONDS).contains(&duration) {
        return Err("Recording must be 0.2–120 seconds long.".into());
    }
    let mut count = 0;
    for sample in reader.samples::<i16>() {
        sample.map_err(|_| "Recording data is incomplete.")?;
        count += 1;
    }
    if count != reader.duration() {
        return Err("Recording duration does not match its data.".into());
    }
    Ok(duration)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_actual_pcm_duration() {
        let mut c = Cursor::new(Vec::new());
        {
            let mut w = hound::WavWriter::new(
                &mut c,
                hound::WavSpec {
                    channels: 1,
                    sample_rate: 16000,
                    bits_per_sample: 16,
                    sample_format: hound::SampleFormat::Int,
                },
            )
            .unwrap();
            for _ in 0..16000 {
                w.write_sample(0i16).unwrap();
            }
            w.finalize().unwrap();
        }
        assert_eq!(validate_wav(c.get_ref()).unwrap(), 1.);
        assert!(validate_wav(&c.get_ref()[..100]).is_err());
        assert!(validate_wav(&[0; 100]).is_err());
    }
}
