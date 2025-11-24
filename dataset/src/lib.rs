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

#[cfg(feature = "load")]
mod load;

#[cfg(feature = "load")]
pub use load::*;

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
