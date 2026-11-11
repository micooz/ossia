use crate::{AppError, SourceMap};
use quick_xml::{events::Event, Reader};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::OnceLock};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BBox {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl BBox {
    fn points(points: &[(f64, f64)]) -> Result<Self, AppError> {
        if points.is_empty() || points.iter().any(|(x, y)| !x.is_finite() || !y.is_finite()) {
            return Err(AppError::invalid("invalid SVG coordinates"));
        }
        Ok(Self {
            x0: points.iter().map(|p| p.0).fold(f64::INFINITY, f64::min),
            y0: points.iter().map(|p| p.1).fold(f64::INFINITY, f64::min),
            x1: points.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max),
            y1: points.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max),
        })
    }

    pub(crate) fn center(self) -> (f64, f64) {
        ((self.x0 + self.x1) / 2., (self.y0 + self.y1) / 2.)
    }

    pub(crate) fn contains(self, x: f64, y: f64) -> bool {
        x >= self.x0 && x <= self.x1 && y >= self.y0 && y <= self.y1
    }
}

#[derive(Clone, Copy)]
struct Matrix([f64; 6]);

impl Matrix {
    fn identity() -> Self {
        Self([1., 0., 0., 1., 0., 0.])
    }

    fn point(self, x: f64, y: f64) -> (f64, f64) {
        let [a, b, c, d, e, f] = self.0;
        (a * x + c * y + e, b * x + d * y + f)
    }

    fn then(self, r: Self) -> Self {
        let [a, b, c, d, e, f] = self.0;
        let [g, h, i, j, k, l] = r.0;
        Self([
            a * g + c * h,
            b * g + d * h,
            a * i + c * j,
            b * i + d * j,
            a * k + c * l + e,
            b * k + d * l + f,
        ])
    }

    fn bbox(self, b: BBox) -> Result<BBox, AppError> {
        BBox::points(&[
            self.point(b.x0, b.y0),
            self.point(b.x1, b.y0),
            self.point(b.x0, b.y1),
            self.point(b.x1, b.y1),
        ])
    }
}

fn number_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^[+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?").unwrap())
}

fn numbers(mut s: &str) -> Result<Vec<f64>, AppError> {
    let mut out = Vec::new();
    loop {
        s = s.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == ',');
        if s.is_empty() {
            return Ok(out);
        }
        let m = number_re()
            .find(s)
            .ok_or_else(|| AppError::invalid("invalid SVG number list"))?;
        let n = m
            .as_str()
            .parse::<f64>()
            .map_err(|_| AppError::invalid("invalid SVG number"))?;
        if !n.is_finite() {
            return Err(AppError::invalid("non-finite SVG number"));
        }
        out.push(n);
        s = &s[m.end()..];
    }
}

fn transform(mut s: &str) -> Result<Matrix, AppError> {
    let mut result = Matrix::identity();
    while !s.trim().is_empty() {
        s = s.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == ',');
        if s.is_empty() {
            break;
        }
        let open = s
            .find('(')
            .ok_or_else(|| AppError::invalid("bad SVG transform"))?;
        let close = s
            .find(')')
            .ok_or_else(|| AppError::invalid("unclosed SVG transform"))?;
        if close < open {
            return Err(AppError::invalid("bad SVG transform parentheses"));
        }
        let n = numbers(&s[open + 1..close])?;
        let mat = match (s[..open].trim(), n.as_slice()) {
            ("translate", [x]) => Matrix([1., 0., 0., 1., *x, 0.]),
            ("translate", [x, y]) => Matrix([1., 0., 0., 1., *x, *y]),
            ("scale", [x]) => Matrix([*x, 0., 0., *x, 0., 0.]),
            ("scale", [x, y]) => Matrix([*x, 0., 0., *y, 0., 0.]),
            ("matrix", [a, b, c, d, e, f]) => Matrix([*a, *b, *c, *d, *e, *f]),
            _ => return Err(AppError::unsupported("unsupported SVG transform")),
        };
        result = result.then(mat);
        s = &s[close + 1..];
    }
    Ok(result)
}

/// Conservative path bounds include Bézier control points. Unsupported commands never produce a guessed box.
pub fn path_bbox(mut d: &str) -> Result<BBox, AppError> {
    let mut points = Vec::new();
    let mut xy = (0., 0.);
    let mut start = xy;
    let mut cmd = None;
    let mut cubic_control: Option<(f64, f64)> = None;
    let mut quadratic_control: Option<(f64, f64)> = None;
    while !d.trim().is_empty() {
        d = d.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == ',');
        if d.is_empty() {
            break;
        }
        let first = d.as_bytes()[0];
        if first.is_ascii_alphabetic() {
            cmd = Some(first as char);
            d = &d[1..];
            if first.eq_ignore_ascii_case(&b'z') {
                xy = start;
                points.push(xy);
                cmd = None;
                cubic_control = None;
                quadratic_control = None;
                continue;
            }
        }
        let c = cmd.ok_or_else(|| AppError::invalid("path coordinates without a command"))?;
        let count = match c.to_ascii_uppercase() {
            'M' | 'L' | 'T' => 2,
            'H' | 'V' => 1,
            'C' => 6,
            'S' | 'Q' => 4,
            _ => return Err(AppError::unsupported("unsupported SVG path command")),
        };
        let mut n = Vec::new();
        for _ in 0..count {
            d = d.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == ',');
            let m = number_re()
                .find(d)
                .ok_or_else(|| AppError::invalid("incomplete SVG path coordinates"))?;
            let value = m
                .as_str()
                .parse::<f64>()
                .map_err(|_| AppError::invalid("bad SVG path number"))?;
            if !value.is_finite() {
                return Err(AppError::invalid("non-finite SVG path"));
            }
            n.push(value);
            d = &d[m.end()..];
        }
        let rel = c.is_ascii_lowercase();
        let base = if rel { xy } else { (0., 0.) };
        match c.to_ascii_uppercase() {
            'H' => {
                xy.0 = n[0] + if rel { xy.0 } else { 0. };
                points.push(xy);
            }
            'V' => {
                xy.1 = n[0] + if rel { xy.1 } else { 0. };
                points.push(xy);
            }
            'S' => {
                let control = cubic_control.map_or(xy, |p| (2. * xy.0 - p.0, 2. * xy.1 - p.1));
                points.push(control);
                for pair in n.as_chunks::<2>().0 {
                    points.push((pair[0] + base.0, pair[1] + base.1));
                }
                xy = *points.last().unwrap();
            }
            'T' => {
                let control = quadratic_control.map_or(xy, |p| (2. * xy.0 - p.0, 2. * xy.1 - p.1));
                points.push(control);
                points.push((n[0] + base.0, n[1] + base.1));
                xy = *points.last().unwrap();
                quadratic_control = Some(control);
            }
            _ => {
                for pair in n.as_chunks::<2>().0 {
                    points.push((pair[0] + base.0, pair[1] + base.1));
                }
                xy = *points.last().unwrap();
            }
        }
        cubic_control = match c.to_ascii_uppercase() {
            'C' => Some((n[2] + base.0, n[3] + base.1)),
            'S' => Some((n[0] + base.0, n[1] + base.1)),
            _ => None,
        };
        quadratic_control = match c.to_ascii_uppercase() {
            'Q' => Some((n[0] + base.0, n[1] + base.1)),
            'T' => quadratic_control,
            _ => None,
        };
        if c.eq_ignore_ascii_case(&'M') {
            start = xy;
            cmd = Some(if rel { 'l' } else { 'L' });
        }
    }
    BBox::points(&points)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceAnchor {
    pub href: String,
    pub line: usize,
    pub char_offset: usize,
    pub column: usize,
    pub byte: Option<usize>,
}

fn anchor(href: &str, map: &SourceMap<'_>) -> Option<SourceAnchor> {
    if !href.starts_with("textedit:") {
        return None;
    }
    let mut parts = href.rsplitn(4, ':');
    let column = parts.next()?.parse().ok()?;
    let char_offset = parts.next()?.parse().ok()?;
    let line = parts.next()?.parse().ok()?;
    // Only the compiled source can establish a position. Included standard-library files cannot.
    let file = parts.next()?;
    let source_file = file.rsplit('/').next()?;
    let byte = if source_file == "score.ly" {
        map.anchor_byte(line, char_offset)
    } else {
        None
    };
    Some(SourceAnchor {
        href: href.into(),
        line,
        char_offset,
        column,
        byte,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SvgElement {
    pub kind: String,
    pub bbox: BBox,
    pub anchor: Option<SourceAnchor>,
}

#[derive(Clone)]
struct Frame {
    matrix: Matrix,
    anchor: Option<SourceAnchor>,
    hidden: bool,
}

pub(crate) fn read_svg(
    svg: &str,
    map: &SourceMap<'_>,
) -> Result<(BBox, Vec<SvgElement>), AppError> {
    let mut reader = Reader::from_str(svg);
    let mut stack = vec![Frame {
        matrix: Matrix::identity(),
        anchor: None,
        hidden: false,
    }];
    let mut elements = Vec::new();
    let mut view_box = None;
    let mut seen_root = false;
    let mut closed_root = false;
    loop {
        let event = reader
            .read_event()
            .map_err(|_| AppError::invalid("malformed SVG XML"))?;
        let empty = matches!(event, Event::Empty(_));
        match event {
            Event::Start(e) | Event::Empty(e) => {
                let tag = e.local_name().as_ref().to_string();
                let mut attrs = HashMap::new();
                for a in e.attributes() {
                    let a = a.map_err(|_| AppError::invalid("malformed SVG attribute"))?;
                    let key = a.key.local_name().as_ref().to_string();
                    let value = a
                        .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                        .map_err(|_| AppError::invalid("invalid SVG attribute entity"))?
                        .into_owned();
                    attrs.insert(key, value);
                }
                if !seen_root {
                    if tag != "svg" {
                        return Err(AppError::invalid("SVG root is missing"));
                    }
                    seen_root = true;
                    let n = numbers(
                        attrs
                            .get("viewBox")
                            .ok_or_else(|| AppError::unsupported("SVG viewBox is required"))?,
                    )?;
                    if n.len() != 4 || n[2] <= 0. || n[3] <= 0. {
                        return Err(AppError::invalid("invalid SVG viewBox"));
                    }
                    view_box = Some(BBox::points(&[(n[0], n[1]), (n[0] + n[2], n[1] + n[3])])?);
                } else if closed_root || tag == "svg" {
                    return Err(AppError::unsupported("multiple or nested SVG roots"));
                }
                let mut frame = stack.last().unwrap().clone();
                frame.matrix = frame.matrix.then(transform(
                    attrs.get("transform").map_or("", String::as_str),
                )?);
                frame.hidden |= matches!(tag.as_str(), "defs" | "clipPath" | "symbol")
                    || attrs.get("display").is_some_and(|v| v == "none");
                if tag == "a" {
                    frame.anchor = attrs.get("href").and_then(|s| anchor(s, map));
                }
                let num = |key: &str, default: f64| -> Result<f64, AppError> {
                    match attrs.get(key) {
                        Some(s) => {
                            let n = s
                                .parse::<f64>()
                                .map_err(|_| AppError::invalid("bad SVG coordinate"))?;
                            if !n.is_finite() {
                                return Err(AppError::invalid("non-finite SVG coordinate"));
                            }
                            Ok(n)
                        }
                        None => Ok(default),
                    }
                };
                let bbox = match tag.as_str() {
                    "line" => {
                        let p = frame.matrix.point(num("x1", 0.)?, num("y1", 0.)?);
                        let q = frame.matrix.point(num("x2", 0.)?, num("y2", 0.)?);
                        Some(BBox::points(&[p, q])?)
                    }
                    "rect" => {
                        let x = num("x", 0.)?;
                        let y = num("y", 0.)?;
                        let w = num("width", 0.)?;
                        let h = num("height", 0.)?;
                        if w < 0. || h < 0. {
                            return Err(AppError::invalid("negative SVG dimensions"));
                        }
                        Some(frame.matrix.bbox(BBox {
                            x0: x,
                            y0: y,
                            x1: x + w,
                            y1: y + h,
                        })?)
                    }
                    "polygon" | "polyline" => {
                        let points = numbers(
                            attrs
                                .get("points")
                                .ok_or_else(|| AppError::invalid("SVG polygon has no points"))?,
                        )?;
                        if points.len() < 4 || points.len() % 2 != 0 {
                            return Err(AppError::invalid("invalid SVG polygon points"));
                        }
                        let transformed: Vec<_> = points
                            .as_chunks::<2>()
                            .0
                            .iter()
                            .map(|p| frame.matrix.point(p[0], p[1]))
                            .collect();
                        Some(BBox::points(&transformed)?)
                    }
                    "path" => Some(
                        frame.matrix.bbox(path_bbox(
                            attrs
                                .get("d")
                                .ok_or_else(|| AppError::invalid("SVG path has no data"))?,
                        )?)?,
                    ),
                    "text" => {
                        let p = frame.matrix.point(num("x", 0.)?, num("y", 0.)?);
                        Some(BBox::points(&[p])?)
                    }
                    "svg" | "g" | "a" | "style" | "tspan" | "defs" | "clipPath" | "symbol"
                    | "title" | "desc" | "metadata" => None,
                    _ => {
                        return Err(AppError::unsupported(format!(
                            "unsupported SVG element: {tag}"
                        )))
                    }
                };
                if !frame.hidden {
                    if let Some(bbox) = bbox {
                        elements.push(SvgElement {
                            kind: tag.clone(),
                            bbox,
                            anchor: frame.anchor.clone(),
                        });
                    }
                }
                if !empty {
                    stack.push(frame);
                } else if tag == "svg" {
                    closed_root = true;
                }
            }
            Event::End(_) => {
                if stack.len() <= 1 {
                    return Err(AppError::invalid("unmatched SVG close"));
                }
                stack.pop();
                if stack.len() == 1 {
                    closed_root = true;
                }
            }
            Event::DocType(_) => return Err(AppError::unsupported("SVG DTD is not supported")),
            Event::Eof => break,
            _ => {}
        }
    }
    if !closed_root || stack.len() != 1 {
        return Err(AppError::invalid("truncated SVG"));
    }
    Ok((
        view_box.ok_or_else(|| AppError::invalid("empty SVG"))?,
        elements,
    ))
}
