pub const HEIGHT: usize = 765;
pub const WIDTH: usize = 700;
pub const STEPS: usize = 25;
pub const MAX_OFFSET: usize = HEIGHT * WIDTH * STEPS - STEPS;

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
        // hdf5 /geographic/geo_row_offset
        let row_offset = 3649.98193359375f64;

        let mut coord = (lon.to_radians(), lat.to_radians(), 0f64);
        proj4rs::transform::transform(&self.longlat, &self.knmi, &mut coord).ok()?;

        // XXX wtf am I doing here for y?
        //     just adding the offset doesn't make lat positive
        //     but if I disregard the sign the boundaries
        //     of the bbox in the datafile match correctly...
        //
        //     My current coords also map to reasonble x/y in
        //     the image... unsure how to test this more
        //     thoroughly than the test on `corners_match`
        //
        //     Why don't I see the offset mentioned in other
        //     datasets that use the same grid/projection?
        //     (e.g. the ensenble dataset) Is it me not holding
        //     netcdf proper?
        //     It's likely the offset is embeded in the netcdf
        //     data directly:
        //
        //     https://github.com/KNMI/adaguc-server/blob/58d498e6f292edc3200228cda850cae03dbb0237/CCDFDataModel/CCDFHDF5IO.cpp#L1228
        let x = coord.0.round() as usize;
        let y = (coord.1 + row_offset).round().abs() as usize;
        Some((x, y))
    }
}

impl Default for Projector {
    fn default() -> Self {
        Self::new()
    }
}

// TODO Check out the ensemble dataset
//      https://dataplatform.knmi.nl/dataset/seamless-precipitation-ensemble-forecast-members-1-0
//      Should be able to give better output during
//      flaky ass windy af octobers
//
// TODO learn some netcdf eh?
//      https://pro.arcgis.com/en/pro-app/latest/help/data/multidimensional/fundamentals-of-netcdf-data-storage.htm
//      (developers developers developers developers):
//      https://cfconventions.org/Data/cf-conventions/cf-conventions-1.7/cf-conventions.html
//
//      Load times are gonna suck with it tho
//      So, cronjob => dump floats to a file
//      Then https://docs.rs/bytemuck/latest/bytemuck/fn.try_cast_slice.html
//      Maybe mmap? CGI so I don't need the caveman this time?

#[cfg(feature = "load")]
pub type Dataset = Box<[f32; STEPS * HEIGHT * WIDTH]>;

#[cfg(feature = "load")]
pub fn load<P: AsRef<std::path::Path>>(
    path: P,
) -> Result<Dataset, Box<dyn std::error::Error + Send + Sync>> {
    let mut data = vec![0f32; STEPS * HEIGHT * WIDTH];

    // metadata docs:
    // https://www.knmi.nl/kennis-en-datacentrum/publicatie/knmi-hdf5-data-format-specification-v3-5
    let file = hdf5_metno::File::open(path.as_ref())?;

    let mut load = |name, z: usize| -> hdf5_metno::Result<()> {
        let group = file.group(name)?;
        let image = group.dataset("image_data")?;
        // hdf5 /imageK/image_bytes_per_pixel is 2
        let vals = image.read_2d::<u16>()?;
        for x in 0..HEIGHT {
            for y in 0..WIDTH {
                let offset = (x * WIDTH + y) * STEPS + z;
                // `* 0.01` hdf5 /imageX/calibration/calibration_formula
                // `* 12` to convert from 5min to 1h
                let mmhr = f32::from(vals[[x, y]]) * 0.01 * 12f32;
                // if mmhr >= 8.0 {
                //     dbg!(offset - z);
                // }
                data[offset] = mmhr
            }
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
