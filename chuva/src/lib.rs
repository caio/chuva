use jiff::Timestamp;
use std::path::{Path, PathBuf};

mod ensemble;
mod floats;
mod nowcast;

pub use ensemble::Ensemble;
pub use nowcast::Nowcast;

use floats::FloatRange;

pub trait Model {
    fn predict(&self, point: NLGeoPoint) -> Option<Prediction<'_>>;
    fn timestamp(&self) -> jiff::Timestamp;
    fn filename(&self) -> &str;
    fn kind(&self) -> ModelKind;
}

pub const STEPS: usize = 25;
pub type Prediction<'a> = &'a [f32; STEPS];

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Sync + Send>>;

pub struct Chuva {
    model: Box<dyn Model + Send + Sync>,
}

impl std::fmt::Debug for Chuva {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Model")
            .field("kind", &self.kind())
            .field("created_at", &self.created_at())
            .field("filename", &self.filename())
            .finish_non_exhaustive()
    }
}

impl Chuva {
    pub fn load_kind<P: AsRef<Path>>(file: P, kind: ModelKind) -> Result<Self> {
        let model = kind.load_model(file)?;

        Ok(Self { model })
    }

    pub fn load<P: AsRef<Path>>(file: P) -> Result<Self> {
        let kind = ModelKind::guess(&file).ok_or("Model kind not recognized")?;
        Self::load_kind(file, kind)
    }

    pub fn load_from_dir<P: AsRef<Path>>(dir: P) -> Result<Self> {
        let file = most_recent_data_file(dir, None)?;
        Self::load(file)
    }

    pub fn by_lat_lon(&self, lat: f64, lon: f64) -> Option<Prediction<'_>> {
        self.model.predict(NLGeoPoint::new(lat, lon)?)
    }

    pub fn created_at(&self) -> Timestamp {
        self.model.timestamp()
    }

    pub fn filename(&self) -> &str {
        self.model.filename()
    }

    pub fn kind(&self) -> ModelKind {
        self.model.kind()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelKind {
    Nowcast,
    Ensemble,
}

impl std::fmt::Display for ModelKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelKind::Nowcast => f.write_str("Nowcast"),
            ModelKind::Ensemble => f.write_str("Ensemble"),
        }
    }
}

impl std::cmp::Ord for ModelKind {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use ModelKind::*;
        use std::cmp::Ordering;
        match (self, other) {
            (Nowcast, Nowcast) => Ordering::Equal,
            (Nowcast, Ensemble) => Ordering::Less,
            (Ensemble, Nowcast) => Ordering::Greater,
            (Ensemble, Ensemble) => Ordering::Equal,
        }
    }
}

impl std::cmp::PartialOrd for ModelKind {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl ModelKind {
    fn timestamp_mask(&self) -> &'static str {
        match self {
            ModelKind::Nowcast => "RAD_NL25_RAC_FM_%Y%m%d%H%M.h5",
            ModelKind::Ensemble => "KNMI_PYSTEPS_BLEND_ENS_%Y%m%d%H%M.nc",
        }
    }

    fn guess<P: AsRef<Path>>(file: P) -> Option<Self> {
        let extension = file.as_ref().extension()?;
        let name = file.as_ref().file_name().map(|n| n.as_encoded_bytes())?;

        if extension == "h5" && name.starts_with(b"RAD_NL25_RAC_FM_") {
            Some(Self::Nowcast)
        } else if extension == "nc" && name.starts_with(b"KNMI_PYSTEPS_BLEND_ENS_") {
            Some(Self::Ensemble)
        } else {
            None
        }
    }

    pub fn extract_ts(&self, filename: &str) -> Option<Timestamp> {
        jiff::fmt::strtime::parse(self.timestamp_mask(), filename)
            .ok()?
            .to_datetime()
            .ok()?
            .in_tz("UTC")
            .ok()
            .map(|zoned| zoned.timestamp())
    }

    fn guess_with_timestamp<P: AsRef<Path>>(file: P) -> Option<(Timestamp, ModelKind)> {
        let kind = Self::guess(file.as_ref())?;
        // FIXME not portable eh
        let name = file.as_ref().file_name()?.to_str().unwrap_or("");
        let created_at = kind.extract_ts(name)?;
        Some((created_at, kind))
    }

    fn load_model<P: AsRef<Path>>(&self, file: P) -> Result<Box<dyn Model + Send + Sync>> {
        Ok(match self {
            ModelKind::Nowcast => Box::new(Nowcast::new(file)?),
            ModelKind::Ensemble => Box::new(Ensemble::new(file)?),
        })
    }
}

fn most_recent_data_file<P: AsRef<Path>>(
    dir: P,
    kind: Option<ModelKind>,
) -> std::io::Result<PathBuf> {
    // data files always have the same name shape with
    // a timestamp at the end, so lexi sort is enough
    std::fs::read_dir(dir)?
        .flatten()
        .map(|e| e.path())
        // Only files we can guess the model kind and timestamp
        .flat_map(|e| ModelKind::guess_with_timestamp(e.as_path()).map(|(ts, k)| (ts, k, e)))
        // Only the selected kind
        .filter(|(_ts, k, _e)| kind.is_none_or(|kind| &kind == k))
        .max()
        .map(|(_ts, _kind, file)| file)
        .ok_or(std::io::Error::other("No data file found in given path"))
}

pub struct NLGeoPoint {
    lat: f64,
    lon: f64,
}

impl NLGeoPoint {
    const LAT: FloatRange = FloatRange::new(48.895301818847656, 55.973602294921875);
    const LON: FloatRange = FloatRange::new(0.0, 10.856452941894531);

    pub const fn new(lat: f64, lon: f64) -> Option<Self> {
        if Self::LAT.contains(lat) && Self::LON.contains(lon) {
            Some(Self { lat, lon })
        } else {
            None
        }
    }

    pub const fn latitude(&self) -> f64 {
        self.lat
    }

    pub const fn longitude(&self) -> f64 {
        self.lon
    }
}
