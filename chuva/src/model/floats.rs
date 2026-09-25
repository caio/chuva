pub struct FloatRange(f64, f64);

impl FloatRange {
    pub const fn new(min: f64, max: f64) -> Self {
        if min.is_finite() && max.is_finite() && min < max {
            Self(min, max)
        } else {
            panic!("Bad float range");
        }
    }

    pub const fn contains(&self, point: f64) -> bool {
        point >= self.0 && point < self.1
    }

    pub const fn min(&self) -> f64 {
        self.0
    }

    pub const fn max(&self) -> f64 {
        self.1
    }

    pub const fn delta(&self) -> f64 {
        self.1 - self.0
    }
}

pub struct LinearScaler {
    from: FloatRange,
    to: FloatRange,
}

impl LinearScaler {
    pub const fn new(from: FloatRange, to: FloatRange) -> Self {
        Self { from, to }
    }

    pub const fn scale(&self, x: f64) -> Option<f64> {
        // contains() is [a,b[. this checks [a,b]
        if x < self.from.min() || x > self.from.max() {
            None
        } else {
            let scaled =
                (self.to.delta()) * (x - self.from.min()) / self.from.delta() + self.to.min();
            // thanks floating point math
            if scaled < self.to.min() {
                Some(self.to.min())
            } else if scaled > self.to.max() {
                Some(self.to.max())
            } else {
                Some(scaled)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FloatRange, LinearScaler};

    #[test]
    fn longitude_scaling() {
        let width = 0.0145f64; // the x "size" of a rectangle in the grid
        let min_lon = -0.00725f64;
        let max_lon = 11.28825f64 + width;

        let geo = FloatRange::new(min_lon, max_lon);
        let line = FloatRange::new(0.0f64, 780.0f64);
        let scaler = LinearScaler::new(geo, line);

        let mut acc = min_lon;
        for i in 0..780 {
            let scaled = scaler.scale(acc).unwrap();
            assert_eq!(i, scaled.round() as usize);
            acc += width;
        }
    }
}
