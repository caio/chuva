// Very dirty "script" that generates the fst used by this crate
//
// It crawls the `pc6` directory from https://github.com/caio/netherlands-postcode-geojson
// and generates what's essentially a `Map<String, (lat,lon)>` with great prefix-search
// support that's used to access the precipitation predictions
//
// The resulting postcodes.fst file is directly included in the binary via include_bytes!()
use std::{collections::HashMap, fs, io};

use fst::MapBuilder;
use tinyjson::JsonValue;

fn read_pc6(parsed: &JsonValue) -> Result<&String, &'static str> {
    let properties: &HashMap<_, _> = parsed.get().ok_or("properties not an object")?;
    properties["name"]
        .get::<String>()
        .ok_or("name not a string")
}

fn read_point_from_geom(parsed: &JsonValue) -> Result<(f64, f64), &'static str> {
    let geometry: &HashMap<_, _> = parsed.get().ok_or("geometry not an object")?;
    let coords: &Vec<_> = geometry["coordinates"]
        .get()
        .ok_or("coordinates not an array")?;
    // multipolygon: js array of array of array
    let first_poly: &Vec<_> = coords[0].get().ok_or("first poly not array")?;
    let points: &Vec<_> = first_poly[0].get().ok_or("points not array")?;
    let first_point: &Vec<_> = points[0].get().ok_or("first point not array")?;
    let lon: f64 = *first_point[0].get().ok_or("lon not number")?;
    let lat: f64 = *first_point[1].get().ok_or("lat not number")?;

    Ok((lat, lon))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args();
    let dir = args.nth(1).expect("dir path first arg");

    let mut values = Vec::new();

    for pc2 in fs::read_dir(dir)?.flatten() {
        for pc6 in fs::read_dir(pc2.path())?.flatten() {
            let data = fs::read_to_string(pc6.path())?;
            let parsed: JsonValue = data.parse()?;
            let geoj: &HashMap<_, _> = parsed.get().ok_or("input not json object")?;
            let name = read_pc6(&geoj["properties"])?;
            let (lat, lon) = read_point_from_geom(&geoj["geometry"])?;

            let packed = postcode::LossyNLCoordPacker::pack(lat, lon);
            if cfg!(debug_assertions) {
                const EPS: f64 = 0.0000001;
                let (ulat, ulon) = postcode::LossyNLCoordPacker::unpack(packed);
                assert!((ulat - lat).abs() < EPS, "bad lat round trip");
                assert!((ulon - lon).abs() < EPS, "bad lon round trip");
            }

            values.push((name.to_ascii_uppercase(), packed));
        }
        println!("Done with {pc2:?}");
    }

    values.sort_by(|a, b| a.0.cmp(&b.0));
    let wtr = io::BufWriter::new(fs::File::create("postcodes.fst")?);
    let mut build = MapBuilder::new(wtr)?;

    for (name, packed_coords) in values {
        build.insert(name, packed_coords)?;
    }

    build.finish()?;
    println!("Done");

    Ok(())
}
