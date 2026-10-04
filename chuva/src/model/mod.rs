use crate::Prediction;

mod floats;
use floats::FloatRange;

mod ensemble;

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Nowcast,
    Ensemble,
}

trait Model {
    fn predict(&self, point: NLGeoPoint) -> Option<Prediction<'_>>;
    // fn timestamp(&self) -> jiff::Timestamp;
    fn filename(&self) -> &str;
    fn kind(&self) -> Kind;
}

struct Nowcast;

pub struct NLGeoPoint(f64, f64);

impl NLGeoPoint {
    const LAT: FloatRange = FloatRange::new(48.895301818847656, 55.973602294921875);
    const LON: FloatRange = FloatRange::new(0.0, 10.856452941894531);

    pub const fn new(lat: f64, lon: f64) -> Option<Self> {
        if Self::LAT.contains(lat) && Self::LON.contains(lon) {
            Some(Self(lat, lon))
        } else {
            None
        }
    }

    pub const fn latitude(&self) -> f64 {
        self.0
    }

    pub const fn longitude(&self) -> f64 {
        self.1
    }
}
