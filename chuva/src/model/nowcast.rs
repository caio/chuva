use super::{Kind, Model, NLGeoPoint, Prediction};
use crate::Result;

const HEIGHT: usize = 765;
const WIDTH: usize = 700;
const STEPS: usize = 25;
const MAX_OFFSET: usize = HEIGHT * WIDTH * STEPS - STEPS;

pub struct Nowcast {
    proj: Projector,
    name: String,
    data: Box<[f32; STEPS * HEIGHT * WIDTH]>,
}

impl Model for Nowcast {
    fn predict(&self, point: NLGeoPoint) -> Option<Prediction<'_>> {
        self.by_lat_lon(point.latitude(), point.longitude())
    }

    fn filename(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> Kind {
        Kind::Nowcast
    }
}

impl Nowcast {
    pub fn new<P: AsRef<std::path::Path>>(model: P) -> Result<Self> {
        let mut data = vec![0f32; STEPS * HEIGHT * WIDTH];

        // metadata docs:
        // https://www.knmi.nl/kennis-en-datacentrum/publicatie/knmi-hdf5-data-format-specification-v3-5
        let file = netcdf::open(model.as_ref())?;

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

        let name: String = model
            .as_ref()
            .file_name()
            .expect("netcdf::open worked, this is obviously a file")
            .to_string_lossy()
            .into();
        let data = data
            .into_boxed_slice()
            .try_into()
            .expect("Dimensions are exact");
        let proj = Projector::new();
        Ok(Self { data, proj, name })
    }

    pub fn by_lat_lon(&self, lat: f64, lon: f64) -> Option<Prediction<'_>> {
        let offset = self.proj.to_offset(lat, lon)?;
        assert!(offset.is_multiple_of(STEPS) && offset <= MAX_OFFSET);
        self.data[offset..(offset + STEPS)].try_into().ok()
    }
}

struct Projector {
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
    use super::*;

    #[test]
    fn corners() {
        // hdf5 geo_product_corners
        // lon,lat counter-clockwise from upper left (UL)
        let corners = [
            (0.0, 49.362064361572266),
            (0.0, 55.973602294921875),
            (10.856452941894531, 55.388973236083984),
            (9.009300231933594, 48.895301818847656),
        ];
        let wanted = [
            (0, HEIGHT),     // UL
            (0, 0),          // LL
            (WIDTH, 0),      // LR
            (WIDTH, HEIGHT), // UR
        ];

        let proj = Projector::new();
        for ((lon, lat), wanted) in corners.into_iter().zip(wanted) {
            let projected = proj.to_x_y(lat, lon);
            assert_eq!(
                projected,
                Some(wanted),
                "Bad projection for ({lat},{lon}): Wanted {wanted:?} got {projected:?}"
            );
        }
    }
}
