use std::{convert::Infallible, sync::Arc, time::SystemTime};

use jiff::tz::TimeZone;
use tokio::net::TcpListener;

use caveman::{
    BodyBytes, BytesMut, Request,
    http::{
        Method, Response, StatusCode,
        header::{CACHE_CONTROL, CONTENT_TYPE},
    },
    service_fn,
};

mod interpreter;
mod ui;
mod util;

mod moros;
use moros::{Moros, Prediction};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Debug)]
enum View<'a> {
    Index,
    Info,
    Demo,
    App,
    Manifest,
    Logo(Logo),
    Postcode(&'a str, Prediction<'a>),
    BadPostcode,
    Coords(f64, f64, Prediction<'a>),
    BadCoords,
    NotFound,
}

#[derive(Debug)]
enum Logo {
    X16,
    X32,
    X192,
    X512,
}

impl Logo {
    fn as_bytes(&self) -> &'static [u8] {
        match self {
            Logo::X16 => include_bytes!("../asset/logo16.png"),
            Logo::X32 => include_bytes!("../asset/logo32.png"),
            Logo::X192 => include_bytes!("../asset/logo192.png"),
            Logo::X512 => include_bytes!("../asset/logo512.png"),
        }
    }
}

fn route<'a>(req: &'a Request, moros: &'a Moros) -> View<'a> {
    if req.method() != Method::GET {
        return View::NotFound;
    }

    let path = util::normalize(req.uri().path());
    match path {
        "/" => View::Index,
        "/info" => View::Info,
        "/demo" => View::Demo,
        "/app" => View::App,
        "/manifest.json" => View::Manifest,
        "/static/logo16.png" => View::Logo(Logo::X16),
        "/static/logo32.png" => View::Logo(Logo::X32),
        "/static/logo192.png" => View::Logo(Logo::X192),
        "/static/logo512.png" => View::Logo(Logo::X512),
        // /@lat,lon (ex: @52.363137,4.889856)
        path if path.starts_with("/@") => {
            let (_, coords) = path.split_at(2);
            util::latlon_from_path(coords)
                .and_then(|(lat, lon)| {
                    moros
                        .by_lat_lon(lat, lon)
                        .map(|preds| View::Coords(lat, lon, preds))
                })
                .unwrap_or(View::BadCoords)
        }
        // /<6-digit-postcode>
        path if path.len() == 7 => {
            let (_, code) = path.split_at(1);
            moros
                .by_postcode(code)
                .map(|preds| View::Postcode(code, preds))
                .unwrap_or(View::BadPostcode)
        }
        // /<4-digit-postcode>
        path if path.len() == 5 => {
            let (_, code) = path.split_at(1);
            moros
                .by_postcode4(code)
                .map(|preds| View::Postcode(code, preds))
                .unwrap_or(View::BadPostcode)
        }
        _ => View::NotFound,
    }
}

fn render(req: Request, state: &State) -> Result<Response<BodyBytes>> {
    let (preds, lenient) = match route(&req, &state.moros) {
        View::Index => {
            let mut body = BytesMut::new();
            ui::Index::render_into(&mut body)?;
            return Ok(Response::new(body.into()));
        }
        View::Info => {
            let mut body = BytesMut::new();
            ui::Info::new(&state.moros).render_into(&mut body)?;
            return Ok(Response::new(body.into()));
        }
        View::Demo => {
            let preds: Prediction<'static> = &[
                0.48, 0.84, 0.0, 1.92, 4.32, 5.52, 2.76, 0.12, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                0.0, 0.0, 0.0, 0.12, 1.56, 3.24, 1.92, 0.24, 0.0, 0.0,
            ];
            (preds, true)
        }
        View::App => {
            let now = state.tz.to_datetime(jiff::Timestamp::now());
            let mut body = BytesMut::new();
            ui::App::new(now).render_into(&mut body)?;
            return Ok(Response::new(body.into()));
        }
        View::Manifest => {
            let data = include_bytes!("../asset/manifest.json").as_slice();
            let response = Response::builder()
                .header(CONTENT_TYPE, "application/manifest+json")
                .body(data.into())?;
            return Ok(response);
        }
        View::Logo(logo) => {
            let response = Response::builder()
                .header(CONTENT_TYPE, "image/png")
                .header(CACHE_CONTROL, "max-age:86400")
                .body(logo.as_bytes().into())?;
            return Ok(response);
        }
        View::Postcode(_code, preds) => (preds, false),
        View::Coords(_lat, _lon, preds) => (preds, false),
        View::BadPostcode => {
            let response = Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body("Invalid postcode\n".into())?;
            return Ok(response);
        }
        View::BadCoords => {
            let response = Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body("Invalid coordinates\n".into())?;
            return Ok(response);
        }
        View::NotFound => {
            let response = Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body("Page not found\n".into())?;
            return Ok(response);
        }
    };

    let renderer = ui::Renderer::new(&state.moros, &state.tz)
        .plain_text(util::wants_plaintext(&req))
        .lenient(lenient);

    let mut body = BytesMut::new();
    renderer.render_into(preds, &mut body)?;

    // TODO cache headers?
    //      Prediction won't change until created_at+5min
    //      Presentation will after <60s since it prints current HH:MM
    Ok(Response::new(body.into()))
}

struct State {
    moros: Moros,
    tz: TimeZone,
}

fn async_main(moros: Moros) -> Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()?;

    let tz = TimeZone::get("Europe/Amsterdam")?;
    let state = Arc::new(State { moros, tz });

    let service = service_fn(move |req: Request| {
        let state = Arc::clone(&state);
        let response = render(req, &state).unwrap_or_else(|err| {
            // TODO proper log eh
            eprintln!("error500: {err:?}");
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body("Internal Server Error".into())
                .expect("valid error500 input")
        });
        async move {
            // sleep(Duration::from_secs(4)).await;
            Ok::<_, Infallible>(response)
        }
    });

    rt.block_on(async move {
        let listener = listener_from_env_or("127.0.0.1:42069")?;
        caveman::serve(listener, service).await;

        Ok(())
    })
}

fn listener_from_env_or(fallback: &str) -> Result<TcpListener> {
    let mut listenfd = listenfd::ListenFd::from_env();

    let listener = if let Some(from_env) = listenfd.take_tcp_listener(0)? {
        from_env
    } else {
        eprintln!("WARNING: no tcp listener from env, using {fallback}");
        std::net::TcpListener::bind(fallback)?
    };
    listener.set_nonblocking(true)?;

    let listener = TcpListener::from_std(listener)?;
    Ok(listener)
}

fn main() -> Result<()> {
    let mut args = std::env::args();

    let prog = args.next().expect("argv[0] is program name");
    let usage = || format!("Usage: {prog} <serve|cli> /path/to/data/dir/");

    let is_server = match args.next().ok_or_else(usage)?.as_str() {
        "serve" => true,
        "cli" => false,
        _ => {
            return Err(usage().into());
        }
    };

    let dir = args.next().expect("dir path first arg");
    let start = SystemTime::now();
    let moros = Moros::load_from_dir(dir)?;
    eprintln!("load in {}s", start.elapsed()?.as_secs_f32());

    if is_server {
        return async_main(moros);
    }

    let preds = if let Some(code) = args.next() {
        if args.len() > 0 {
            let lat: f64 = code.parse()?;
            let lon: f64 = args.next().unwrap().parse()?;
            moros.by_lat_lon(lat, lon)
        } else {
            moros.by_postcode(&code).or_else(|| {
                code.parse::<usize>()
                    .ok()
                    .and_then(|offset| moros.by_offset(offset))
            })
        }
    } else {
        moros.by_lat_lon(52.325, 4.873)
    };

    if let Some(preds) = preds {
        let tz = TimeZone::get("Europe/Amsterdam")?;
        let renderer = ui::Renderer::new(&moros, &tz)
            .plain_text(true)
            .lenient(true);
        renderer.render_into(preds, util::FmtStdout::new())?;
    } else {
        println!("invalid input");
    }

    Ok(())
}
