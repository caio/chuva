use std::fmt::Write;

use askama::Template;
use jiff::{Span, Timestamp, civil::DateTime, tz::TimeZone};

use crate::{
    Result,
    chuva::{Chuva, Prediction},
    interpreter::{Expr, Lexer},
};

pub struct Renderer<'a> {
    lenient: bool,
    plain_text: bool,
    chuva: &'a Chuva,
    tz: &'a TimeZone,
}

impl<'a> Renderer<'a> {
    pub fn new(chuva: &'a Chuva, tz: &'a TimeZone) -> Self {
        Self {
            lenient: false,
            plain_text: true,
            chuva,
            tz,
        }
    }

    pub fn lenient(mut self, lenient: bool) -> Self {
        self.lenient = lenient;
        self
    }

    pub fn plain_text(mut self, plain_text: bool) -> Self {
        self.plain_text = plain_text;
        self
    }

    pub fn render_into<W: std::fmt::Write>(&self, preds: Prediction, mut writer: W) -> Result<()> {
        let mut now = Timestamp::now();

        let slot = match self.chuva.get_time_slot(now) {
            Ok(slot) => slot,
            Err(err) if self.lenient => {
                eprintln!("WARNING: {err}: Using the datafile epoch as current time");
                now = self.chuva.created_at();
                0
            }
            Err(err) => return Err(err),
        };

        let no_rain = preds.iter().all(|&mmhr| mmhr == 0f64);
        if no_rain && self.plain_text {
            write!(
                writer,
                "It's {}\nNo rain in sight\n",
                self.tz.to_datetime(now).strftime("%H:%M")
            )?;
            return Ok(());
        }
        if no_rain {
            let tmpl = NoRain {
                now: self.tz.to_datetime(now),
            };
            tmpl.render_into(&mut writer)?;
            return Ok(());
        }

        if self.plain_text {
            let tmpl = PredictionTxt::new(
                self.tz.to_datetime(self.chuva.created_at()),
                self.tz.to_datetime(now),
                slot,
                preds,
            );
            tmpl.render_into(&mut writer)?;
            return Ok(());
        }

        let tmpl = PredictionHtml::new(
            self.tz.to_datetime(self.chuva.created_at()),
            self.tz.to_datetime(now),
            slot,
            preds,
            self.lenient,
        );

        tmpl.render_into(&mut writer)?;
        Ok(())
    }
}

#[derive(Template)]
#[template(path = "norain.html.jinja")]
pub struct NoRain {
    now: DateTime,
}

#[derive(Template)]
#[template(path = "index.html.jinja")]
pub struct Index;

impl Index {
    // just so I don't have to `use askama::Template` outside of this mod
    pub fn render_into<W: std::fmt::Write>(&self, mut writer: W) -> Result<()> {
        Template::render_into(self, &mut writer)?;
        Ok(())
    }
}

#[derive(Template)]
#[template(path = "prediction.txt.jinja")]
pub struct PredictionTxt<'a> {
    now: DateTime,
    spark: Sparker<'a>,
    marker: Marker,
    events: Events<'a>,
}

impl<'a> PredictionTxt<'a> {
    pub fn new(created_at: DateTime, now: DateTime, slot: usize, preds: Prediction<'a>) -> Self {
        Self {
            now,
            spark: Sparker(preds),
            marker: Marker(slot),
            events: Events::new(created_at, slot, preds),
        }
    }

    fn minutes_relative(&self, date: &DateTime) -> Minutes {
        minutes_relative(*date, self.now)
    }
}

#[derive(Template)]
#[template(path = "prediction.html.jinja")]
pub struct PredictionHtml<'a> {
    now: DateTime,
    plot: Plot<'a>,
    events: Events<'a>,
    demo: bool,
}

// XXX Could impl Display and gen the whole plot at once
#[derive(Clone, Copy)]
struct Plot<'a> {
    preds: Prediction<'a>,
    cursor: usize,
    x: usize,
    marker: PlotMarker,
    created_at: DateTime,
}

#[derive(Clone, Copy)]
struct PlotMarker {
    left: usize,
    top: usize,
    bottom: usize,
    right: usize,
}

impl PlotMarker {
    fn new(left: usize, bottom: usize) -> Self {
        let right = left + Plot::RECT_WIDTH;
        let top = left + Plot::MARKER_HEIGHT;
        Self {
            left,
            top,
            bottom,
            right,
        }
    }
}

impl std::fmt::Display for PlotMarker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self {
            left,
            top,
            bottom,
            right,
        } = &self;
        let height = bottom - Plot::MARKER_HEIGHT;
        f.write_fmt(format_args!(
            r#"<polyline points="{left},{bottom} {top},{height}, {right},{bottom}"><title>We're here</title></polyline>"#
        ))
    }
}

struct Rect {
    x: usize,
    y: usize,
    height: usize,
    width: usize,
    value: Value,
    at: DateTime,
}

struct Value(f64);

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{:.2}", self.0))
    }
}

impl<'a> Plot<'a> {
    const HEIGHT: usize = 56;
    const WIDTH: usize = Self::RECT_WIDTH * 25;

    const RECT_WIDTH: usize = 12;

    const MARKER_HEIGHT: usize = 6;

    fn new(preds: Prediction<'a>, slot: usize, created_at: DateTime) -> Self {
        Self {
            preds,
            x: 0,
            cursor: 0,
            created_at,
            marker: PlotMarker::new(
                slot * Self::RECT_WIDTH,
                Self::HEIGHT + Self::MARKER_HEIGHT + 1,
            ),
        }
    }

    // Called by the template
    const fn height(&self) -> usize {
        Self::HEIGHT + Self::MARKER_HEIGHT + 1
    }

    // Called by the template
    const fn width(&self) -> usize {
        Self::WIDTH
    }

    fn next(&mut self) -> Option<Rect> {
        let pred = self.preds.get(self.cursor)?;
        let height = scale_height(*pred);
        let at = self.created_at + jiff::Span::new().minutes((self.cursor * 5) as i64);

        let rect = Rect {
            x: self.x,
            y: Self::HEIGHT.saturating_sub(height),
            height,
            width: Self::RECT_WIDTH,
            value: Value(*pred),
            at,
        };

        self.x += Self::RECT_WIDTH;
        self.cursor += 1;
        Some(rect)
    }
}

// XXX might be nice to keep these buckets in line with spark()
const fn scale_height(mmhr: f64) -> usize {
    if mmhr < 0f64.next_up() {
        0
    } else if mmhr < 0.13 {
        7
    } else if mmhr < 0.25 {
        14
    } else if mmhr < 0.5 {
        21
    } else if mmhr < 2.0 {
        28
    } else if mmhr < 4.0 {
        35
    } else if mmhr < 6.0 {
        42
    } else if mmhr < 8.0 {
        49
    } else {
        Plot::HEIGHT
    }
}

impl<'a> Iterator for Plot<'a> {
    type Item = Rect;

    fn next(&mut self) -> Option<Self::Item> {
        Self::next(self)
    }
}

impl<'a> PredictionHtml<'a> {
    pub fn new(
        created_at: DateTime,
        now: DateTime,
        slot: usize,
        preds: Prediction<'a>,
        demo: bool,
    ) -> Self {
        Self {
            now,
            events: Events::new(created_at, slot, preds),
            plot: Plot::new(preds, slot, created_at),
            demo,
        }
    }

    fn minutes_relative(&self, date: &DateTime) -> Minutes {
        minutes_relative(*date, self.now)
    }
}

fn minutes_relative(ends_at: DateTime, now: DateTime) -> Minutes {
    debug_assert!(ends_at > now);
    Minutes(
        (ends_at - now)
            .total(jiff::Unit::Minute)
            // XXX is there a valid case where date > now?
            //     the callsite is always the very first
            //     event and events start from slot which
            //     is derived from Timestamp::now()
            .unwrap_or(0f64),
    )
}

struct Minutes(f64);

impl std::fmt::Display for Minutes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let rounded = self.0.round() as usize;
        if rounded <= 1 {
            f.write_str("1 minute")
        } else {
            f.write_fmt(format_args!("{} minutes", rounded))
        }
    }
}

struct Sparker<'a>(Prediction<'a>);

impl<'a> std::fmt::Display for Sparker<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for &item in self.0 {
            f.write_char(spark(item))?;
        }
        Ok(())
    }
}

struct Marker(usize);

impl std::fmt::Display for Marker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for i in 0..dataset::STEPS {
            if i == self.0 {
                f.write_char('^')?;
            } else {
                f.write_char(' ')?;
            }
        }
        Ok(())
    }
}

// I hate this but always end up with something similar
// when dealing with templates...
struct Event {
    starts_at: DateTime,
    ends_at: DateTime,
    is_rain: bool,
    is_showers: bool,
}

#[derive(Clone, Copy)]
/// Token -> Expr -> Event
struct Events<'a> {
    src: Lexer<'a>,
    created_at: DateTime,
}

impl<'a> Events<'a> {
    fn new(created_at: DateTime, slot: usize, src: Prediction<'a>) -> Self {
        Self {
            src: Lexer::new(slot, &src[..]),
            created_at,
        }
    }

    fn expr_to_event(&self, expr: Expr) -> Event {
        let (range, is_showers, is_rain) = match expr {
            Expr::Showers { range, gaps: _ } => (range, true, true),
            Expr::Rain(range) => (range, false, true),
            Expr::Dry(range) => (range, false, false),
        };

        let starts_at = self
            .created_at
            .saturating_add(Span::new().minutes((range.start * 5) as i32));
        let ends_at = self
            .created_at
            .saturating_add(Span::new().minutes((range.end * 5) as i32));

        Event {
            starts_at,
            ends_at,
            is_rain,
            is_showers,
        }
    }
}

impl<'a> Iterator for Events<'a> {
    type Item = Event;

    fn next(&mut self) -> Option<Self::Item> {
        self.src.next().map(|expr| self.expr_to_event(expr))
    }
}

const fn spark(mmhr: f64) -> char {
    // TODO figure out good buckets? this is pure yolo
    //      so maybe look at yearly stats and slice
    //      according to the distribution?
    if mmhr < 0f64.next_up() {
        ' '
    } else if mmhr < 0.13 {
        '▁'
    } else if mmhr < 0.25 {
        '▂'
    } else if mmhr < 0.5 {
        '▃'
    } else if mmhr < 2.0 {
        '▄'
    } else if mmhr < 4.0 {
        '▅'
    } else if mmhr < 6.0 {
        '▆'
    } else if mmhr < 8.0 {
        '▇'
    } else {
        '█'
    }
}
