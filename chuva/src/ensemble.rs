use super::{
    FloatRange, Model, ModelKind, NLGeoPoint, Prediction, Result, STEPS, floats::LinearScaler,
};

pub struct Ensemble {
    name: String,
    data: Box<[f32; STEPS * HEIGHT * WIDTH]>,
    created_at: jiff::Timestamp,
}

impl Model for Ensemble {
    fn predict(&self, point: NLGeoPoint) -> Option<Prediction<'_>> {
        self.by_lat_lon(point.latitude(), point.longitude())
    }

    fn filename(&self) -> &str {
        &self.name
    }

    fn kind(&self) -> ModelKind {
        ModelKind::Ensemble
    }

    fn timestamp(&self) -> jiff::Timestamp {
        self.created_at
    }
}

const WIDTH: usize = 780;
const HEIGHT: usize = 780;
const MEMBERS: usize = 20;

const LAT_MIN: f64 = 48.995;
const LAT_BUCKET_SIZE: f64 = 0.0090;
const LAT_MAX: f64 = 56.0065 + LAT_BUCKET_SIZE;

const LON_MIN: f64 = -0.00725;
const LON_BUCKET_SIZE: f64 = 0.0145;
const LON_MAX: f64 = 11.28825 + LON_BUCKET_SIZE;

const VALID_INDEX_RANGE: FloatRange = FloatRange::new(0.0f64, 779.0f64);

const LON_SCALER: LinearScaler =
    LinearScaler::new(FloatRange::new(LON_MIN, LON_MAX), VALID_INDEX_RANGE);

const LAT_SCALER: LinearScaler =
    LinearScaler::new(FloatRange::new(LAT_MIN, LAT_MAX), VALID_INDEX_RANGE);

const fn latlon_to_xy(lat: f64, lon: f64) -> Option<(usize, usize)> {
    // No point in being const here, but why not
    if let Some(x) = LON_SCALER.scale(lon)
        && let Some(y) = LAT_SCALER.scale(lat)
    {
        Some((x.round() as usize, y.round() as usize))
    } else {
        None
    }
}

impl Ensemble {
    pub fn by_lat_lon(&self, lat: f64, lon: f64) -> Option<Prediction<'_>> {
        // XXX conversion to model projection skipped. fixmemaybe
        let (x, y) = latlon_to_xy(lat, lon)?;
        let offset = (x * WIDTH + y) * STEPS;
        self.data[offset..(offset + STEPS)].try_into().ok()
    }

    pub fn new<P: AsRef<std::path::Path>>(model: P) -> Result<Ensemble> {
        let file = netcdf::open(model.as_ref())?;

        let precip = file
            .variable("precip_intensity")
            .ok_or("Variable precip_intensity doesn't exist")?;
        assert_eq!(4, precip.dimensions().len());

        let mut data = vec![0f32; STEPS * HEIGHT * WIDTH];
        let mut buf = vec![0u16; MEMBERS * WIDTH * HEIGHT];

        for time in 0..STEPS {
            let selector: netcdf::Extents = (
                ..,   // every model output
                time, // for this specific time slot
                ..,   // whole height
                ..,   // whole width
            )
                .try_into()
                .expect("valid extents spec");
            precip
                .get_values_into(&mut buf, selector)
                .map_err(|err| format!("get_values_into failed for time={time}: {err}"))?;

            // FIXME getfattr zomgwtfbbq
            //       https://github.com/Unidata/netcdf-c/blob/6038ed2c4b8f53fbe38792d65cfca983c6c08907/libdispatch/dinfermodel.c#L1619
            let mut ens_members = [0u16; MEMBERS];
            for x in 0..WIDTH {
                for y in 0..HEIGHT {
                    for (z, member) in ens_members.iter_mut().enumerate() {
                        let offset = z * WIDTH * HEIGHT + y * WIDTH + x;
                        *member = buf[offset];
                    }
                    ens_members.sort_unstable();
                    let offset = (x * WIDTH + y) * STEPS + time;
                    data[offset] = f32::from(ens_members[13]) * 0.01;
                }
            }
        }

        let name: String = model
            .as_ref()
            .file_name()
            .expect("netcdf::open worked, this is obviously a file")
            .to_string_lossy()
            .into();

        let created_at = ModelKind::Ensemble
            .extract_ts(&name)
            .ok_or("Unable to guess timestamp from filename")?;

        Ok(Self {
            name,
            data: data
                .into_boxed_slice()
                .try_into()
                .expect("Dimensions are exact"),
            created_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corners() {
        let corners = [
            (LAT_MAX, LON_MIN), // UL
            (LAT_MIN, LON_MIN), // LL
            (LAT_MIN, LON_MAX), // LR
            (LAT_MAX, LON_MAX), // UR
        ];
        let wanted = [(0, 779), (0, 0), (779, 0), (779, 779)];

        for ((lat, lon), wanted) in corners.into_iter().zip(wanted) {
            let scaled = latlon_to_xy(lat, lon);
            assert_eq!(
                scaled,
                Some(wanted),
                "Bad scaling for ({lat},{lon}): Wanted {wanted:?} got {scaled:?}"
            );
        }
    }
}
