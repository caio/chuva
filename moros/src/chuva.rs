use std::path::{Path, PathBuf};

use fst::{IntoStreamer, Streamer};
use jiff::Timestamp;

use dataset::{Dataset, MAX_OFFSET, Projector, STEPS};

type Result<T> = crate::Result<T>;

pub type Prediction<'a> = &'a [f64; STEPS];

pub struct Chuva {
    data: Dataset,
    filename: String,
    proj: Projector,
    fst: fst::Map<&'static [u8]>,
    created_at: Timestamp,
}

impl Chuva {
    pub fn load_from_dir<P: AsRef<Path>>(dir: P) -> Result<Self> {
        let dataset_path = most_recent_data_file(dir)?;
        Self::load(dataset_path)
    }

    pub fn load<P: AsRef<Path>>(dataset_path: P) -> Result<Self> {
        let created_at = parse_ts(&dataset_path)?;
        let data = dataset::load(&dataset_path)?;
        let filename = dataset_path
            .as_ref()
            .file_name()
            .expect("it's a file, since load() worked")
            .to_string_lossy()
            .into_owned();
        let fst = fst::Map::new(FST_STATE)?;
        Ok(Self {
            data,
            proj: Projector::new(),
            fst,
            created_at,
            filename,
        })
    }

    pub fn by_lat_lon(&self, lat: f64, lon: f64) -> Option<Prediction<'_>> {
        let offset = self.proj.to_offset(lat, lon)?;
        self.by_offset(offset)
    }

    pub fn by_postcode(&self, code: &str) -> Option<Prediction<'_>> {
        let offset = self.fst.get(code)? as usize;
        self.by_offset(offset)
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
        Some(self.data[offset..(offset + STEPS)].try_into().unwrap())
    }

    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }

    pub fn filename(&self) -> &str {
        &self.filename
    }

    pub fn get_time_slot(&self, now: Timestamp) -> Result<usize> {
        get_time_slot(self.created_at, now).map_err(|_| "Dataset too old".into())
    }
}

static FST_STATE: &[u8] = include_bytes!("../asset/postcodes.fst").as_slice();

fn parse_ts<P: AsRef<Path>>(name: P) -> std::result::Result<Timestamp, jiff::Error> {
    let name = name
        .as_ref()
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let ts = jiff::fmt::strtime::parse("RAD_NL25_RAC_FM_%Y%m%d%H%M.h5", name)?
        .to_datetime()?
        .in_tz("UTC")?
        .timestamp();
    Ok(ts)
}

fn most_recent_data_file<P: AsRef<Path>>(dir: P) -> Result<PathBuf> {
    // data files always have the same name shape with
    // a timestamp at the end, so lexi sort is enough
    std::fs::read_dir(dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|e| e.extension().is_some_and(|ext| ext == "h5"))
        .filter(|e| {
            e.file_name().is_some_and(|name| {
                // just to avoid yet another awkward is_some dance
                name.as_encoded_bytes().starts_with(b"RAD_NL25_RAC_FM_")
            })
        })
        .max()
        .ok_or("No data file found in given path".into())
}

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

#[cfg(test)]
mod tests {
    use super::get_time_slot;
    use jiff::{Timestamp, ToSpan};

    #[test]
    fn slot_works() {
        let now = Timestamp::now();

        assert_eq!(Err(-1), get_time_slot(now, now - 1.minute()));
        assert_eq!(Ok(0), get_time_slot(now, now));
        assert_eq!(Ok(24), get_time_slot(now, now + 2.hours()));
        assert_eq!(Err(121), get_time_slot(now, now + 121.minutes()));
    }
}
