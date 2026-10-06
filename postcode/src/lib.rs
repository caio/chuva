use fst::{Automaton, IntoStreamer, Streamer};

pub struct Postcodes(fst::Map<&'static [u8]>);

static FST_STATE: &[u8] = include_bytes!("../asset/postcodes.fst").as_slice();

impl Postcodes {
    #[allow(
        clippy::new_without_default,
        reason = "ceremonial boilerplate for what"
    )]
    pub fn new() -> Self {
        let fst = fst::Map::new(FST_STATE).expect("static fst is valid");
        Self(fst)
    }

    pub fn find(&self, code: &str) -> Option<(f64, f64)> {
        match code.len() {
            4 => self.get_pc4(code),
            6 => self.get_pc6(code),
            _ => None,
        }
    }

    fn get_pc6(&self, code: &str) -> Option<(f64, f64)> {
        let mut stream = self
            .0
            .search(AsciiUpperCase::new(code).starts_with())
            .into_stream();
        let (_, packed) = stream.next()?;
        Some(LossyNLCoordPacker::unpack(packed))
    }

    fn get_pc4(&self, code: &str) -> Option<(f64, f64)> {
        let mut stream = self.0.range().gt(code).into_stream();
        let (key, packed) = stream.next()?;
        debug_assert_eq!(6, key.len(), "key is always pc6");
        if &key[..4] == code.as_bytes() {
            Some(LossyNLCoordPacker::unpack(packed))
        } else {
            None
        }
    }

    pub fn for_each_entry<F: FnMut(&str, (f64, f64)) -> bool>(&self, mut func: F) {
        let mut stream = self.0.stream();
        while let Some((key, value)) = stream.next() {
            let key = str::from_utf8(key).expect("keys are valid utf-8");
            let (lat, lon) = LossyNLCoordPacker::unpack(value);
            if !func(key, (lat, lon)) {
                break;
            }
        }
    }

    // Either a callback thinger or a plain Vec if I ever do
    // autocomplete
}

pub struct LossyNLCoordPacker;

impl LossyNLCoordPacker {
    // Scaling to a representation that uses more bits than f32's
    // mantissa gives me better precision than simply converting
    // f64 to f32 and packing them together
    //
    // This likely doesn't matter at all since the models map to a
    // grid of 1x1 km lol
    //
    // I just really didn't want to mess around with floats
    const SCALE: f64 = 2u32.pow(26) as f64;

    pub const fn pack(lat: f64, lon: f64) -> u64 {
        if lat < 0.0 || lon < 0.0 {
            panic!("Only positive coords!");
        }

        let scaled_lat = lat * Self::SCALE;
        let scaled_lon = lon * Self::SCALE;

        debug_assert!(scaled_lat <= u32::MAX as f64);
        debug_assert!(scaled_lon <= u32::MAX as f64);

        let int_lat = scaled_lat.trunc() as u64;
        let int_lon = scaled_lon.trunc() as u64;

        int_lat << 32 | int_lon
    }

    pub const fn unpack(packed: u64) -> (f64, f64) {
        let int_lat = (packed >> 32) & 0xFFFFFFFF;
        let int_lon = packed & 0xFFFFFFFF;

        let lat = (int_lat as f64) / Self::SCALE;
        let lon = (int_lon as f64) / Self::SCALE;
        (lat, lon)
    }
}

struct AsciiUpperCase<'a> {
    input: &'a [u8],
}

impl<'a> AsciiUpperCase<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input: input.as_bytes(),
        }
    }
}

impl<'a> fst::Automaton for AsciiUpperCase<'a> {
    type State = Option<usize>;

    #[inline]
    fn start(&self) -> Self::State {
        Some(0)
    }

    #[inline]
    fn is_match(&self, state: &Self::State) -> bool {
        *state == Some(self.input.len())
    }

    #[inline]
    fn can_match(&self, pos: &Self::State) -> bool {
        pos.is_some()
    }

    #[inline]
    fn accept(&self, state: &Self::State, byte: u8) -> Self::State {
        let pos = (*state)?;
        // The keys in the FST are in upper case.
        // We want a case-insensitive match and to_ascii_uppercase
        // does the right thing for bytes outside the lower case range
        let current = self.input.get(pos).map(|b| b.to_ascii_uppercase())?;
        if current == byte { Some(pos + 1) } else { None }
    }
}

#[cfg(test)]
mod tests {
    use super::Postcodes;

    #[test]
    fn case_insensitive_search() {
        let pcs = Postcodes::new();

        let uppercase = pcs.find("1017CE").expect("Key 1017CE exists in the fst");
        for key in ["1017ce", "1017cE", "1017Ce"] {
            let found = pcs.find(key).expect("Can find 1017CE regardless of case");
            assert_eq!(uppercase, found);
        }
    }

    #[test]
    fn shortcode_pc4_search() {
        let pcs = Postcodes::new();

        assert!(pcs.find("1017").is_some());
        assert!(pcs.find("8011").is_some());
    }
}
