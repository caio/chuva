use std::{collections::HashMap, fs, io};

use fst::MapBuilder;
use tinyjson::JsonValue;

use dataset::Projector;

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

    let proj = Projector::default();
    let mut values = Vec::new();

    for pc2 in fs::read_dir(dir)?.flatten() {
        for pc6 in fs::read_dir(pc2.path())?.flatten() {
            let data = fs::read_to_string(pc6.path())?;
            let parsed: JsonValue = data.parse()?;
            let geoj: &HashMap<_, _> = parsed.get().ok_or("input not json object")?;
            let name = read_pc6(&geoj["properties"])?;
            let (lat, lon) = read_point_from_geom(&geoj["geometry"])?;
            let offset = proj.to_offset(lat, lon).expect("valid NL lat/lon");

            values.push((name.clone(), offset));
        }
        println!("Done with {pc2:?}");
    }

    values.sort_by(|a, b| a.0.cmp(&b.0));
    let wtr = io::BufWriter::new(fs::File::create("postcodes.fst")?);
    let mut build = MapBuilder::new(wtr)?;

    for (name, offset) in values {
        build.insert(name, offset as u64)?;
    }

    build.finish()?;
    println!("Done");

    Ok(())
}
