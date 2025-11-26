use std::path::Path;

use fst::{Automaton, IntoStreamer, Streamer};
use jiff::Timestamp;

use chuva::{MAX_OFFSET, Model, Projector, STEPS};

type Result<T> = crate::Result<T>;

pub type Prediction<'a> = &'a [f32; STEPS];

pub struct Chuva {
    model: Model,
    proj: Projector,
    fst: fst::Map<&'static [u8]>,
}

impl Chuva {
    pub fn load_from_dir<P: AsRef<Path>>(dir: P) -> Result<Self> {
        let model = Model::load_from_dir(dir)?;
        let fst = fst::Map::new(FST_STATE)?;

        Ok(Self {
            proj: Projector::new(),
            fst,
            model,
        })
    }

    pub fn by_lat_lon(&self, lat: f64, lon: f64) -> Option<Prediction<'_>> {
        let offset = self.proj.to_offset(lat, lon)?;
        self.by_offset(offset)
    }

    pub fn by_postcode(&self, code: &str) -> Option<Prediction<'_>> {
        let mut stream = self
            .fst
            .search(AsciiUpperCase::new(code).starts_with())
            .into_stream();
        let (_, offset) = stream.next()?;
        self.by_offset(offset as usize)
    }

    pub fn by_postcode4(&self, code: &str) -> Option<Prediction<'_>> {
        let mut stream = self.fst.range().gt(code).into_stream();
        let (key, offset) = stream.next()?;
        assert_eq!(6, key.len(), "key is pc6");
        if &key[..4] == code.as_bytes() {
            self.by_offset(offset as usize)
        } else {
            None
        }
    }

    #[inline]
    pub(crate) fn by_offset(&self, offset: usize) -> Option<Prediction<'_>> {
        assert!(offset <= MAX_OFFSET);
        Some(
            self.model.data[offset..(offset + STEPS)]
                .try_into()
                .unwrap(),
        )
    }

    pub fn created_at(&self) -> Timestamp {
        self.model.created_at
    }

    pub fn filename(&self) -> &str {
        &self.model.filename
    }

    pub fn kind(&self) -> chuva::ModelKind {
        self.model.kind
    }

    pub fn get_time_slot(&self, now: Timestamp) -> Result<usize> {
        get_time_slot(self.model.created_at, now).map_err(|_| "Dataset too old".into())
    }
}

static FST_STATE: &[u8] = include_bytes!("../asset/postcodes.fst").as_slice();

fn get_time_slot(created_at: Timestamp, now: Timestamp) -> std::result::Result<usize, i64> {
    let age = (now - created_at)
        .total(jiff::Unit::Minute)
        .map_err(|_| 420)?; // irrelevant

    if !(0.0..=120.0).contains(&age) {
        Err(age as i64)
    } else {
        let slot = (age / 5.0) as usize;
        assert!(slot < STEPS);
        Ok(slot)
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
    use super::{AsciiUpperCase, FST_STATE, get_time_slot};

    use fst::{Automaton, IntoStreamer, Streamer};
    use jiff::{Timestamp, ToSpan};

    #[test]
    fn slot_works() {
        let now = Timestamp::now();

        assert_eq!(Err(-1), get_time_slot(now, now - 1.minute()));
        assert_eq!(Ok(0), get_time_slot(now, now));
        assert_eq!(Ok(24), get_time_slot(now, now + 2.hours()));
        assert_eq!(Err(121), get_time_slot(now, now + 121.minutes()));
    }

    #[test]
    fn case_insensitive_postcode_search() {
        let fst = fst::Map::new(FST_STATE).expect("valid fst state");

        let _ = fst.get("1017CE").expect("Key 1017CE exists in the fst");

        let mut stream = fst
            .search(AsciiUpperCase::new("1017ce").starts_with())
            .into_stream();
        let (key, _) = stream.next().expect("lower case search matches");

        assert_eq!(
            b"1017CE", key,
            "lower case search should match upper case key"
        );
    }
}
