use std::path::Path;

use jiff::Timestamp;

use chuva::{Chuva, ModelKind, Prediction, STEPS};
use postcode::Postcodes;

type Result<T> = crate::Result<T>;

pub struct Moros {
    chuva: Chuva,
    postcodes: Postcodes,
}

impl Moros {
    pub fn load_from_dir<P: AsRef<Path>>(dir: P) -> Result<Self> {
        let chuva = Chuva::load_from_dir(dir)?;
        let postcodes = Postcodes::new();

        Ok(Self { postcodes, chuva })
    }

    pub fn by_postcode(&self, code: &str) -> Option<Prediction<'_>> {
        let (lat, lon) = self.postcodes.find(code)?;
        self.chuva.by_lat_lon(lat, lon)
    }

    pub fn by_postcode4(&self, code: &str) -> Option<Prediction<'_>> {
        let (lat, lon) = self.postcodes.find(code)?;
        self.chuva.by_lat_lon(lat, lon)
    }

    pub fn by_lat_lon(&self, lat: f64, lon: f64) -> Option<Prediction<'_>> {
        self.chuva.by_lat_lon(lat, lon)
    }

    pub fn created_at(&self) -> Timestamp {
        self.chuva.created_at()
    }

    pub fn filename(&self) -> &str {
        self.chuva.filename()
    }

    pub fn kind(&self) -> ModelKind {
        self.chuva.kind()
    }

    pub fn get_time_slot(&self, now: Timestamp) -> Result<usize> {
        get_time_slot(self.chuva.created_at(), now).map_err(|_| "Dataset too old".into())
    }
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
