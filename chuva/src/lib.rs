use std::path::{Path, PathBuf};

use jiff::Timestamp;

pub const HEIGHT: usize = 765;
pub const WIDTH: usize = 700;
pub const STEPS: usize = 25;
pub const MAX_OFFSET: usize = HEIGHT * WIDTH * STEPS - STEPS;

pub type Dataset = Box<[f32; STEPS * HEIGHT * WIDTH]>;

pub type Prediction<'a> = &'a [f32; STEPS];

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Sync + Send>>;

pub struct Projector {
    knmi: proj4rs::Proj,
    longlat: proj4rs::Proj,
}

impl Projector {
    pub fn new() -> Self {
        let longlat = proj4rs::Proj::from_user_string("WGS84").expect("valid user string");
        let knmi = proj4rs::Proj::from_proj_string(
            // From the dataset metadata:
            "+proj=stere +lat_0=90 +lon_0=0 +lat_ts=60 +a=6378137 +b=6356752 +x_0=0 +y_0=0 +units=km",
            // hdf5 /geographic/map_projection ("same" as above, but units=default=meters):
            // "+proj=stere +lat_0=90 +lon_0=0 +lat_ts=60 +a=6378.14 +b=6356.75 +x_0=0 y_0=0",
        )
        .expect("valid proj string");
        Self { knmi, longlat }
    }

    pub fn to_offset(&self, lat: f64, lon: f64) -> Option<usize> {
        if !coords_within_bounds(lat, lon) {
            return None;
        }

        let (x, y) = self.to_x_y(lat, lon)?;
        if x < WIDTH && y < HEIGHT {
            let offset = (x * WIDTH + y) * STEPS;
            assert!(offset <= MAX_OFFSET);
            Some(offset)
        } else {
            None
        }
    }

    pub(crate) fn to_x_y(&self, lat: f64, lon: f64) -> Option<(usize, usize)> {
        let mut coord = (lon.to_radians(), lat.to_radians(), 0f64);
        proj4rs::transform::transform(&self.longlat, &self.knmi, &mut coord).ok()?;

        // hdf5 /geographic/geo_pixel_size_x
        let size_x = 1.000003457069397f64;
        // hdf5 /geographic/geo_pixel_size_y
        let size_y = -1.000004768371582f64;
        // hdf5 /geographic/geo_row_offset
        let row_offset = 3649.98193359375f64;

        let x = coord.0 * size_x + size_x / 2.0;
        let y = (row_offset + coord.1) * size_y + size_y / 2.0;

        Some((x as usize, y.round() as usize))
    }
}

impl Default for Projector {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Chuva {
    pub kind: ModelKind,
    pub created_at: Timestamp,
    pub filename: String,
    pub data: Dataset,
    pub proj: crate::Projector,
}

impl std::fmt::Debug for Chuva {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Model")
            .field("kind", &self.kind)
            .field("created_at", &self.created_at)
            .field("filename", &self.filename)
            .finish_non_exhaustive()
    }
}

impl Chuva {
    pub fn load_kind<P: AsRef<Path>>(file: P, kind: ModelKind) -> Result<Self> {
        let filename = file
            .as_ref()
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .ok_or("No filename")?;
        let created_at = jiff::fmt::strtime::parse(kind.timestamp_mask(), &filename)?
            .to_datetime()?
            .in_tz("UTC")?
            .timestamp();
        let data = kind.load_predictions(file)?;

        Ok(Self {
            kind,
            filename,
            created_at,
            data,
            proj: crate::Projector::new(),
        })
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
        let offset = self.proj.to_offset(lat, lon)?;
        self.by_offset(offset)
    }

    #[inline]
    pub fn by_offset(&self, offset: usize) -> Option<Prediction<'_>> {
        assert!(offset.is_multiple_of(STEPS) && offset <= MAX_OFFSET);
        Some(self.data[offset..(offset + STEPS)].try_into().unwrap())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ModelKind {
    Simple,
    #[cfg(feature = "debug")]
    SimpleNdarray,
    Ensemble,
    #[cfg(feature = "debug")]
    EnsembleNdarray,
}

impl std::fmt::Display for ModelKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelKind::Simple => f.write_str("Simple"),
            #[cfg(feature = "debug")]
            ModelKind::SimpleNdarray => f.write_str("SimpleNdarray"),
            ModelKind::Ensemble => f.write_str("Ensemble"),
            #[cfg(feature = "debug")]
            ModelKind::EnsembleNdarray => f.write_str("EnsembleNdarray"),
        }
    }
}

impl ModelKind {
    fn timestamp_mask(&self) -> &'static str {
        match self {
            ModelKind::Simple => "RAD_NL25_RAC_FM_%Y%m%d%H%M.h5",
            #[cfg(feature = "debug")]
            ModelKind::SimpleNdarray => "RAD_NL25_RAC_FM_%Y%m%d%H%M.h5",
            ModelKind::Ensemble => "KNMI_PYSTEPS_BLEND_ENS_%Y%m%d%H%M.nc",
            #[cfg(feature = "debug")]
            ModelKind::EnsembleNdarray => "KNMI_PYSTEPS_BLEND_ENS_%Y%m%d%H%M.nc",
        }
    }

    fn guess<P: AsRef<Path>>(file: P) -> Option<Self> {
        let extension = file.as_ref().extension()?;
        let name = file.as_ref().file_name().map(|n| n.as_encoded_bytes())?;

        if extension == "h5" && name.starts_with(b"RAD_NL25_RAC_FM_") {
            Some(Self::Simple)
        } else if extension == "nc" && name.starts_with(b"KNMI_PYSTEPS_BLEND_ENS_") {
            Some(Self::Ensemble)
        } else {
            None
        }
    }

    pub fn load_from_dir<P: AsRef<Path>>(&self, dir: P) -> Result<Chuva> {
        let file = most_recent_data_file(dir, Some(*self))?;
        Chuva::load(file)
    }

    fn load_predictions<P: AsRef<Path>>(&self, file: P) -> Result<Dataset> {
        match self {
            ModelKind::Simple => load(file),
            #[cfg(feature = "debug")]
            ModelKind::SimpleNdarray => load_with_ndarray(file),
            ModelKind::Ensemble => load_ensemble_dataset(file),
            #[cfg(feature = "debug")]
            ModelKind::EnsembleNdarray => load_ensemble_with_ndarray(file),
        }
    }
}

fn most_recent_data_file<P: AsRef<Path>>(
    dir: P,
    kind: Option<ModelKind>,
) -> std::io::Result<PathBuf> {
    // data files always have the same name shape with
    // a timestamp at the end, so lexi sort is enough
    // XXX max() on the entry doesn't really work if
    //     there are different model kinds in the same path
    std::fs::read_dir(dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|e| ModelKind::guess(e).is_some_and(|k| kind.is_none_or(|kind| kind == k)))
        .max()
        .ok_or(std::io::Error::other("No data file found in given path"))
}

fn load<P: AsRef<std::path::Path>>(path: P) -> Result<Dataset> {
    let mut data = vec![0f32; STEPS * HEIGHT * WIDTH];

    // metadata docs:
    // https://www.knmi.nl/kennis-en-datacentrum/publicatie/knmi-hdf5-data-format-specification-v3-5
    let file = netcdf::open(path.as_ref())?;

    // hdf5 /imageK/image_bytes_per_pixel is 2
    let mut buf = vec![0u16; HEIGHT * WIDTH];
    let mut load = |name, z: usize| -> netcdf::Result<()> {
        let group = file
            .group(name)?
            .ok_or_else(|| netcdf::Error::from(format!("{name} not found")))?;
        let image = group.variable("image_data").ok_or_else(|| {
            netcdf::Error::from(format!("group {name} doesn't contain `image_data` var"))
        })?;
        image.get_values_into(&mut buf, ..)?;

        for (idx, value) in buf.iter().copied().enumerate() {
            // `* 0.01` hdf5 /imageX/calibration/calibration_formula
            // `* 12` to convert from 5min to 1h
            let mmhr = f32::from(value) * 0.01 * 12f32;
            let offset = (idx * STEPS) + z;
            data[offset] = mmhr;
        }

        Ok(())
    };

    load("image1", 0)?;
    load("image2", 1)?;
    load("image3", 2)?;
    load("image4", 3)?;
    load("image5", 4)?;
    load("image6", 5)?;
    load("image7", 6)?;
    load("image8", 7)?;
    load("image9", 8)?;
    load("image10", 9)?;
    load("image11", 10)?;
    load("image12", 11)?;
    load("image13", 12)?;
    load("image14", 13)?;
    load("image15", 14)?;
    load("image16", 15)?;
    load("image17", 16)?;
    load("image18", 17)?;
    load("image19", 18)?;
    load("image20", 19)?;
    load("image21", 20)?;
    load("image22", 21)?;
    load("image23", 22)?;
    load("image24", 23)?;
    load("image25", 24)?;

    Ok(data
        .into_boxed_slice()
        .try_into()
        .expect("exact dimensions"))
}

#[cfg(feature = "debug")]
fn load_with_ndarray<P: AsRef<std::path::Path>>(path: P) -> Result<Dataset> {
    let mut data = vec![0f32; STEPS * HEIGHT * WIDTH];

    // metadata docs:
    // https://www.knmi.nl/kennis-en-datacentrum/publicatie/knmi-hdf5-data-format-specification-v3-5
    let file = netcdf::open(path.as_ref())?;

    // hdf5 /imageK/image_bytes_per_pixel is 2
    use ndarray::Array2;
    let mut buf = Array2::<u16>::zeros((HEIGHT, WIDTH));
    let mut load = |name, z: usize| -> netcdf::Result<()> {
        let group = file
            .group(name)?
            .ok_or_else(|| netcdf::Error::from(format!("{name} not found")))?;
        let image = group.variable("image_data").ok_or_else(|| {
            netcdf::Error::from(format!("group {name} doesn't contain `image_data` var"))
        })?;
        image.get_into(buf.view_mut(), ..)?;

        for (idx, value) in buf.iter().copied().enumerate() {
            // `* 0.01` hdf5 /imageX/calibration/calibration_formula
            // `* 12` to convert from 5min to 1h
            let mmhr = f32::from(value) * 0.01 * 12f32;
            let offset = (idx * STEPS) + z;
            data[offset] = mmhr;
        }

        Ok(())
    };

    load("image1", 0)?;
    load("image2", 1)?;
    load("image3", 2)?;
    load("image4", 3)?;
    load("image5", 4)?;
    load("image6", 5)?;
    load("image7", 6)?;
    load("image8", 7)?;
    load("image9", 8)?;
    load("image10", 9)?;
    load("image11", 10)?;
    load("image12", 11)?;
    load("image13", 12)?;
    load("image14", 13)?;
    load("image15", 14)?;
    load("image16", 15)?;
    load("image17", 16)?;
    load("image18", 17)?;
    load("image19", 18)?;
    load("image20", 19)?;
    load("image21", 20)?;
    load("image22", 21)?;
    load("image23", 22)?;
    load("image24", 23)?;
    load("image25", 24)?;

    Ok(data
        .into_boxed_slice()
        .try_into()
        .expect("exact dimensions"))
}

#[cfg(feature = "debug")]
fn load_ensemble_with_ndarray<P: AsRef<std::path::Path>>(path: P) -> Result<Dataset> {
    let file = netcdf::open(path.as_ref())?;
    let mut data = vec![0f32; STEPS * HEIGHT * WIDTH];

    let precip = file
        .variable("precip_intensity")
        .ok_or("Variable precip_intensity doesn't exist")?;
    assert_eq!(4, precip.dimensions().len());

    const ENS_SIZE: usize = 20;
    use ndarray::Array3;
    let mut buf = Array3::<u16>::zeros((ENS_SIZE, HEIGHT, WIDTH));

    // XXX This dataset gives predictions for up to 6h ahead, but
    //     I'm mostly interested in the next 2h (what the nowcast
    for time in 0..STEPS {
        let selector: netcdf::Extents = (
            ..,   // every model output
            time, // for this specific time slot
            ..,   // whole height
            ..,   // whole width
        )
            .try_into()
            .expect("valid extents spec");
        precip.get_into(buf.view_mut(), selector)?;

        // FIXME getfattr zomgwtfbbq
        //       https://github.com/Unidata/netcdf-c/blob/6038ed2c4b8f53fbe38792d65cfca983c6c08907/libdispatch/dinfermodel.c#L1619
        let mut ens_members = [0u16; ENS_SIZE];
        for x in 0..WIDTH {
            for y in 0..HEIGHT {
                for z in 0..ENS_SIZE {
                    ens_members[z] = buf[[z, y, x]];
                    buf.get([z, y, x]);
                }
                ens_members.sort_unstable();
                let offset = (x * WIDTH + y) * STEPS + time;
                data[offset] = f32::from(ens_members[13]) * 0.01;
            }
        }
    }

    Ok(data
        .into_boxed_slice()
        .try_into()
        .expect("exact dimensions"))
}

fn load_ensemble_dataset<P: AsRef<std::path::Path>>(path: P) -> Result<Dataset> {
    let file = netcdf::open(path.as_ref())?;
    let mut data = vec![0f32; STEPS * HEIGHT * WIDTH];

    let precip = file
        .variable("precip_intensity")
        .ok_or("Variable precip_intensity doesn't exist")?;
    assert_eq!(4, precip.dimensions().len());

    const ENS_SIZE: usize = 20;
    let mut buf = vec![0u16; ENS_SIZE * HEIGHT * WIDTH];

    // XXX This dataset gives predictions for up to 6h ahead, but
    //     I'm mostly interested in the next 2h (what the nowcast
    //     dataset provides)
    for time in 0..STEPS {
        let selector: netcdf::Extents = (
            ..,   // every model output
            time, // for this specific time slot
            ..,   // whole height
            ..,   // whole width
        )
            .try_into()
            .expect("valid extents spec");
        precip.get_values_into(&mut buf, selector)?;

        // FIXME getfattr zomgwtfbbq
        //       https://github.com/Unidata/netcdf-c/blob/6038ed2c4b8f53fbe38792d65cfca983c6c08907/libdispatch/dinfermodel.c#L1619
        let mut ens_members = [0u16; ENS_SIZE];
        for x in 0..WIDTH {
            for y in 0..HEIGHT {
                // Passing this lint makes the code look worse. z is
                // used to compute the offset, not just for indexing
                #[expect(clippy::needless_range_loop)]
                for z in 0..ENS_SIZE {
                    let offset = z * WIDTH * HEIGHT + y * WIDTH + x;
                    ens_members[z] = buf[offset];
                }
                ens_members.sort_unstable();
                let offset = (x * WIDTH + y) * STEPS + time;
                data[offset] = f32::from(ens_members[13]) * 0.01;
            }
        }
    }

    Ok(data
        .into_boxed_slice()
        .try_into()
        .expect("exact dimensions"))
}

// hdf5 geo_product_corners
// lon,lat counter-clockwise from upper left (UL)
#[cfg(test)]
static CORNERS: [(f64, f64); 4] = [
    (0.0, 49.362064361572266),
    (0.0, 55.973602294921875),
    (10.856452941894531, 55.388973236083984),
    (9.009300231933594, 48.895301818847656),
];

// no std::ops::Range for floats
// no min()/max() for float iters
// so numbers are hardcoded instead of going through CORNERS
// notice that corners is not actually a square
const fn coords_within_bounds(lat: f64, lon: f64) -> bool {
    lon.is_finite()
        && lon >= 0.0
        && lon < 10.856452941894531
        && lat.is_finite()
        && lat >= 48.895301818847656
        && lat < 55.973602294921875
}

#[cfg(test)]
mod tests {
    use super::{CORNERS, HEIGHT, Projector, WIDTH};

    #[test]
    fn corners_match() {
        let expected = [
            Some((0, HEIGHT)),     // UL
            Some((0, 0)),          // LL
            Some((WIDTH, 0)),      // LR
            Some((WIDTH, HEIGHT)), // UR
        ];

        let proj = Projector::new();
        for (idx, &(lon, lat)) in CORNERS.iter().enumerate() {
            assert_eq!(
                proj.to_x_y(lat, lon),
                expected[idx],
                "incorrect corner-to-xy on index={idx}"
            );
        }
    }
}
