use std::path::{Path, PathBuf};

use jiff::Timestamp;

use crate::{HEIGHT, STEPS, WIDTH};

pub type Dataset = Box<[f32; STEPS * HEIGHT * WIDTH]>;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Sync + Send>>;

pub struct Model {
    pub kind: ModelKind,
    pub created_at: Timestamp,
    pub filename: String,
    pub data: Dataset,
}

impl std::fmt::Debug for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Model")
            .field("kind", &self.kind)
            .field("created_at", &self.created_at)
            .field("filename", &self.filename)
            .finish_non_exhaustive()
    }
}

impl Model {
    pub fn load<P: AsRef<Path>>(file: P) -> Result<Self> {
        let kind = ModelKind::guess(&file).ok_or("Model kind not recognized")?;
        let filename = file
            .as_ref()
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .expect("guess() would fail if this wasn't an actual file");
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
        })
    }

    pub fn load_from_dir<P: AsRef<Path>>(dir: P) -> Result<Self> {
        let file = most_recent_data_file(dir, None)?;
        Self::load(file)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ModelKind {
    Simple,
    Ensemble,
}

impl std::fmt::Display for ModelKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelKind::Simple => f.write_str("Simple"),
            ModelKind::Ensemble => f.write_str("Ensemble"),
        }
    }
}

impl ModelKind {
    fn timestamp_mask(&self) -> &'static str {
        match self {
            ModelKind::Simple => "RAD_NL25_RAC_FM_%Y%m%d%H%M.h5",
            ModelKind::Ensemble => "KNMI_PYSTEPS_BLEND_ENS_%Y%m%d%H%M.nc",
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

    pub fn load_from_dir<P: AsRef<Path>>(&self, dir: P) -> Result<Model> {
        let file = most_recent_data_file(dir, Some(*self))?;
        Model::load(file)
    }

    fn load_predictions<P: AsRef<Path>>(&self, file: P) -> Result<Dataset> {
        match self {
            ModelKind::Simple => load(file),
            ModelKind::Ensemble => load_ensemble_dataset(file),
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

        // FIXME Is this not laid out how I think it is?
        //       I'm finding it difficult to compare with the
        //       smaller model
        // FIXME getfattr zomgwtfbbq
        //       https://github.com/Unidata/netcdf-c/blob/6038ed2c4b8f53fbe38792d65cfca983c6c08907/libdispatch/dinfermodel.c#L1619
        //
        // Each chunk contains the 20 different model predictions for
        // the current `time` slice
        let (chunks, remainder) = buf[..].as_chunks_mut::<ENS_SIZE>();
        assert!(remainder.is_empty());
        for (idx, chunk) in chunks.iter_mut().enumerate() {
            // The simplest way to choose which of the outputs to use
            // is getting the median value. My VUA decided to bias
            // for rain (i.e.: I'd rather it tells me that it's gonna
            // rain but it doesn't instead of the other way around),
            // so I'm using the 70th percentile
            chunk.sort_unstable();
            let offset = (idx * STEPS) + time;
            data[offset] = f32::from(chunk[13]) * 0.01;
        }
    }

    Ok(data
        .into_boxed_slice()
        .try_into()
        .expect("exact dimensions"))
}
