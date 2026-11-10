use crate::{resolve_locator, ScoreIndex, SourceMap};
use ossia_types::{AppError, ContentHash, ErrorCode, LocatedRegion, LocationCapability, Locator};
use quick_xml::{events::Event, Reader};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

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
    fn center(self) -> (f64, f64) {
        ((self.x0 + self.x1) / 2., (self.y0 + self.y1) / 2.)
    }
    fn contains(self, x: f64, y: f64) -> bool {
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
                for pair in n.chunks_exact(2) {
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
                for pair in n.chunks_exact(2) {
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
fn read_svg(svg: &str, map: &SourceMap<'_>) -> Result<(BBox, Vec<SvgElement>), AppError> {
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
                            .chunks_exact(2)
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StaffGeometry {
    pub top: f64,
    pub bot: f64,
    pub x0: f64,
    pub x1: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemGeometry {
    pub top: f64,
    pub bot: f64,
    pub staves: Vec<StaffGeometry>,
    pub bar_xs: Vec<f64>,
}
fn rounded(n: f64, d: f64) -> f64 {
    (n * d).round() / d
}
fn sorted_unique(mut n: Vec<f64>) -> Vec<f64> {
    n.sort_by(f64::total_cmp);
    n.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    n
}
fn systems(
    elements: &[SvgElement],
    staff_count: usize,
) -> Result<(f64, Vec<SystemGeometry>), AppError> {
    let lines: Vec<_> = elements
        .iter()
        .filter(|e| {
            e.kind == "line" && (e.bbox.y1 - e.bbox.y0).abs() < 1e-6 && e.bbox.x1 - e.bbox.x0 > 5.
        })
        .collect();
    let ys = sorted_unique(lines.iter().map(|e| rounded(e.bbox.y0, 1000.)).collect());
    if ys.len() < 5 {
        return Err(AppError::unsupported("no five-line staff geometry"));
    }
    let gaps: Vec<_> = ys
        .windows(2)
        .map(|w| rounded(w[1] - w[0], 1000.))
        .filter(|n| *n > 0.)
        .collect();
    let unique = sorted_unique(gaps.clone());
    let staff_space = *unique
        .iter()
        .max_by(|a, b| {
            let ac = gaps.iter().filter(|g| (**g - **a).abs() < 1e-6).count();
            let bc = gaps.iter().filter(|g| (**g - **b).abs() < 1e-6).count();
            ac.cmp(&bc).then_with(|| b.total_cmp(a))
        })
        .ok_or_else(|| AppError::unsupported("staff spacing unavailable"))?;
    let mut runs: Vec<Vec<f64>> = vec![vec![ys[0]]];
    for &y in &ys[1..] {
        if y - runs.last().unwrap().last().unwrap() >= staff_space * 1.6 {
            runs.push(Vec::new());
        }
        runs.last_mut().unwrap().push(y);
    }
    let mut staves = Vec::new();
    for run in runs {
        if run.len() != 5 {
            continue;
        }
        let top = run[0];
        let bot = run[4];
        let ls: Vec<_> = lines
            .iter()
            .filter(|l| (l.bbox.y0 - top).abs() < 1e-3 || (l.bbox.y0 - bot).abs() < 1e-3)
            .collect();
        staves.push(StaffGeometry {
            top,
            bot,
            x0: ls.iter().map(|l| l.bbox.x0).fold(f64::INFINITY, f64::min),
            x1: ls
                .iter()
                .map(|l| l.bbox.x1)
                .fold(f64::NEG_INFINITY, f64::max),
        });
    }
    if staff_count == 0 || staves.is_empty() || staves.len() % staff_count != 0 {
        return Err(AppError::unsupported(
            "source staff mapping does not match SVG",
        ));
    }
    let mut result = Vec::new();
    // Source contexts determine staves per system, removing the ambiguous one-gap/two-staff case.
    for group in staves.chunks(staff_count) {
        let mut system = SystemGeometry {
            top: group[0].top,
            bot: group.last().unwrap().bot,
            staves: group.to_vec(),
            bar_xs: Vec::new(),
        };
        system.bar_xs = barlines(elements, &system);
        let left = group.iter().map(|s| s.x0).fold(f64::INFINITY, f64::min);
        if let Some(first) = system.bar_xs.first().copied() {
            if first - left > staff_space
                && elements.iter().any(|e| {
                    let (x, y) = e.bbox.center();
                    e.anchor.as_ref().is_some_and(|a| a.byte.is_some())
                        && x >= left
                        && x < first
                        && y >= system.top - staff_space * 2.
                        && y <= system.bot + staff_space * 2.
                })
            {
                system.bar_xs.insert(0, left);
            }
        }
        result.push(system);
    }
    Ok((staff_space, result))
}
fn barlines(elements: &[SvgElement], system: &SystemGeometry) -> Vec<f64> {
    let left = system
        .staves
        .iter()
        .map(|s| s.x0)
        .fold(f64::INFINITY, f64::min);
    let right = system
        .staves
        .iter()
        .map(|s| s.x1)
        .fold(f64::NEG_INFINITY, f64::max);
    let mut rects: Vec<_> = elements
        .iter()
        .filter(|e| {
            e.kind == "rect"
                && e.bbox.x1 - e.bbox.x0 <= 0.45
                && e.bbox.x0 >= left - 1.
                && e.bbox.x0 <= right + 1.
        })
        .collect();
    rects.sort_by(|a, b| a.bbox.x0.total_cmp(&b.bbox.x0));
    let mut clusters: Vec<(f64, Vec<BBox>)> = Vec::new();
    for rect in rects {
        if let Some((x, boxes)) = clusters
            .iter_mut()
            .find(|(x, _)| (*x - rect.bbox.x0).abs() <= 0.25)
        {
            boxes.push(rect.bbox);
            *x = (*x * (boxes.len() - 1) as f64 + rect.bbox.x0) / boxes.len() as f64;
        } else {
            clusters.push((rect.bbox.x0, vec![rect.bbox]));
        }
    }
    clusters
        .into_iter()
        .filter(|(_, boxes)| {
            system.staves.iter().all(|s| {
                let mut intervals: Vec<_> = boxes
                    .iter()
                    .map(|b| (b.y0.max(s.top - 0.5), b.y1.min(s.bot + 0.5)))
                    .filter(|(a, b)| b > a)
                    .collect();
                intervals.sort_by(|a, b| a.0.total_cmp(&b.0));
                let mut merged: Vec<(f64, f64)> = Vec::new();
                for (a, b) in intervals {
                    if let Some(last) = merged.last_mut().filter(|last| a <= last.1 + 0.5) {
                        last.1 = last.1.max(b);
                    } else {
                        merged.push((a, b));
                    }
                }
                merged.iter().map(|(a, b)| b - a).sum::<f64>() >= (s.bot - s.top) * 0.85
            })
        })
        .map(|(x, _)| rounded(x, 100.))
        .collect()
}

pub struct SvgPage<'a> {
    pub page: u32,
    pub svg: &'a str,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageLayout {
    pub page: u32,
    pub view_box: Option<BBox>,
    pub staff_space: f64,
    pub systems: Vec<SystemGeometry>,
    pub elements: Vec<SvgElement>,
    pub capability: LocationCapability,
    pub reason: Option<String>,
    pub error: Option<AppError>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeasureColumn {
    pub page: u32,
    pub system_index: usize,
    pub column_index: usize,
    pub x0: f64,
    pub x1: f64,
    pub measure_start: Option<u32>,
    pub measure_end: Option<u32>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoreLayout {
    pub source_hash: ContentHash,
    pub pages: Vec<PageLayout>,
    pub columns: Vec<MeasureColumn>,
    pub capability: LocationCapability,
    pub reason: Option<String>,
}

/// Join all pages to one source index. A failed page never silently renumbers subsequent pages.
pub fn build_layout(index: &ScoreIndex, pages: &[SvgPage<'_>]) -> Result<ScoreLayout, AppError> {
    let mut seen = HashSet::new();
    if pages.iter().any(|p| p.page == 0 || !seen.insert(p.page)) {
        return Err(AppError::new(
            ErrorCode::InvalidRequest,
            "page numbers must be positive and unique",
        ));
    }
    let mut input: Vec<_> = pages.iter().collect();
    input.sort_by_key(|p| p.page);
    let map = SourceMap::new(index.source());
    let mut result = Vec::new();
    let mut columns = Vec::new();
    let mut evidence = Vec::new();
    let mut occurrences = HashMap::<(u32, u32), usize>::new();
    for p in input {
        let mut page = PageLayout {
            page: p.page,
            view_box: None,
            staff_space: 0.,
            systems: Vec::new(),
            elements: Vec::new(),
            capability: LocationCapability::None,
            reason: None,
            error: None,
        };
        match read_svg(p.svg, &map) {
            Err(error) => {
                page.reason = Some(error.to_string());
                page.error = Some(error);
            }
            Ok((view_box, elements)) => {
                page.view_box = Some(view_box);
                page.elements = elements;
                match systems(&page.elements, index.staves.len()) {
                    Err(error) => {
                        page.capability = LocationCapability::AnchorOnly;
                        page.reason = Some(error.to_string());
                        page.error = Some(error);
                    }
                    Ok((space, systems)) => {
                        page.staff_space = space;
                        page.systems = systems;
                        page.capability = LocationCapability::Measure;
                        for (system_index, sys) in page.systems.iter().enumerate() {
                            if sys.bar_xs.len() < 2 {
                                page.capability = LocationCapability::AnchorOnly;
                                page.reason = Some("no reliable measure grid".into());
                            }
                            for (column_index, w) in sys.bar_xs.windows(2).enumerate() {
                                let candidates: Vec<_> = page
                                    .elements
                                    .iter()
                                    .filter(|e| {
                                        let (x, y) = e.bbox.center();
                                        x >= w[0]
                                            && x < w[1]
                                            && y >= sys.top - space * 2.
                                            && y <= sys.bot + space * 2.
                                    })
                                    .filter_map(|e| {
                                        e.anchor.as_ref()?.byte.and_then(|b| index.measure_at(b))
                                    })
                                    .collect();
                                let candidate = if candidates.is_empty() {
                                    None
                                } else {
                                    let start =
                                        candidates.iter().map(|m| m.measure_start).max().unwrap();
                                    let end =
                                        candidates.iter().map(|m| m.measure_end).min().unwrap();
                                    (start <= end).then_some((start, end))
                                };
                                if let Some(range) = candidate {
                                    *occurrences.entry(range).or_default() += 1;
                                }
                                evidence.push((candidate, !candidates.is_empty()));
                                columns.push(MeasureColumn {
                                    page: p.page,
                                    system_index,
                                    column_index,
                                    x0: w[0],
                                    x1: w[1],
                                    measure_start: None,
                                    measure_end: None,
                                });
                            }
                        }
                    }
                }
            }
        }
        result.push(page);
    }
    let mut used = HashMap::<(u32, u32), u32>::new();
    let mut next = Some(1u32);
    let mut previous_page = 0;
    let mut offset = 0;
    let mut partial = false;
    for page in &mut result {
        if page.page != previous_page + 1 {
            next = None;
            partial = true;
        }
        previous_page = page.page;
        if page.capability != LocationCapability::Measure {
            next = None;
            partial = true;
        }
        let mut previous_system = None;
        for column in columns.iter_mut().filter(|c| c.page == page.page) {
            if previous_system.map_or(column.system_index > 0, |s| column.system_index > s + 1) {
                next = None;
                partial = true;
            }
            previous_system = Some(column.system_index);
            let (candidate, conflicting) = evidence[offset];
            offset += 1;
            let range = match candidate {
                Some((start, end)) if start == end => Some((start, end)),
                Some(range @ (start, end)) => {
                    let count = occurrences[&range];
                    if count == 1 {
                        Some(range)
                    } else if count as u64 == u64::from(end) - u64::from(start) + 1 {
                        let n = used.entry(range).or_default();
                        let measure = start + *n;
                        *n += 1;
                        Some((measure, measure))
                    } else {
                        None
                    }
                }
                None if !conflicting => next.filter(|n| *n <= index.max_measures).map(|n| (n, n)),
                _ => None,
            };
            if let Some((start, end)) = range {
                column.measure_start = Some(start);
                column.measure_end = Some(end);
                next = end.checked_add(1);
            } else {
                next = None;
                page.capability = LocationCapability::AnchorOnly;
                page.reason
                    .get_or_insert("incomplete or ambiguous measure grid".into());
                partial = true;
            }
        }
    }
    // A stem can cover a whole single staff. Identical singleton source measures on both
    // sides prove that the apparent bar is inside a measure, not a new measure boundary.
    let mut merged: Vec<MeasureColumn> = Vec::new();
    for column in columns {
        if let Some(previous) = merged.last_mut().filter(|previous| {
            previous.page == column.page
                && previous.system_index == column.system_index
                && previous.measure_start.is_some()
                && previous.measure_start == previous.measure_end
                && previous.measure_start == column.measure_start
                && previous.measure_end == column.measure_end
        }) {
            previous.x1 = column.x1;
        } else {
            merged.push(column);
        }
    }
    let mut columns = merged;
    let mut group = None;
    let mut local = 0;
    for column in &mut columns {
        let current = (column.page, column.system_index);
        if group != Some(current) {
            group = Some(current);
            local = 0;
        }
        column.column_index = local;
        local += 1;
    }
    let mut expected = Some(1);
    for column in &columns {
        if expected != column.measure_start {
            partial = true;
        }
        expected = column.measure_end.and_then(|n| n.checked_add(1));
    }
    let capability = if columns.iter().any(|c| c.measure_start.is_some()) {
        LocationCapability::Measure
    } else if result.iter().any(|p| !p.elements.is_empty()) {
        LocationCapability::AnchorOnly
    } else {
        LocationCapability::None
    };
    Ok(ScoreLayout {
        source_hash: index.source_hash.clone(),
        pages: result,
        columns,
        capability,
        reason: partial
            .then(|| "partial layout capability; inspect individual pages and columns".into()),
    })
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum LocateQuery {
    Point { page: u32, x: f64, y: f64 },
    Region { locator: Locator },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationResult {
    pub capability: LocationCapability,
    pub reason: Option<String>,
    pub region: Option<LocatedRegion>,
    pub page: Option<u32>,
    pub system_index: Option<usize>,
    pub anchors: Vec<SourceAnchor>,
}
fn no_hit(reason: &str) -> LocationResult {
    LocationResult {
        capability: LocationCapability::None,
        reason: Some(reason.into()),
        region: None,
        page: None,
        system_index: None,
        anchors: Vec::new(),
    }
}

/// Point coordinates are SVG user units. No nearest-staff or first-page fallback is performed.
pub fn locate(
    index: &ScoreIndex,
    layout: &ScoreLayout,
    query: &LocateQuery,
) -> Result<LocationResult, AppError> {
    if index.source_hash != layout.source_hash {
        return Err(AppError::new(
            ErrorCode::StaleSelection,
            "layout belongs to another source snapshot",
        ));
    }
    if let LocateQuery::Region { locator } = query {
        let region = resolve_locator(index, locator)?;
        return Ok(LocationResult {
            capability: region.capability,
            reason: region.reason.clone(),
            region: Some(region),
            page: None,
            system_index: None,
            anchors: Vec::new(),
        });
    }
    let LocateQuery::Point { page, x, y } = *query else {
        unreachable!()
    };
    if !x.is_finite() || !y.is_finite() {
        return Err(AppError::new(
            ErrorCode::InvalidRequest,
            "point must be finite",
        ));
    }
    let Some(p) = layout.pages.iter().find(|p| p.page == page) else {
        return Ok(no_hit("no-hit"));
    };
    if !p.view_box.is_some_and(|b| b.contains(x, y)) {
        return Ok(no_hit(p.reason.as_deref().unwrap_or("no-hit")));
    }
    if p.systems.is_empty() {
        let anchors: Vec<_> = p
            .elements
            .iter()
            .filter(|e| e.bbox.contains(x, y))
            .filter_map(|e| e.anchor.clone())
            .collect();
        if !anchors.is_empty() {
            return Ok(LocationResult {
                capability: LocationCapability::AnchorOnly,
                reason: p.reason.clone(),
                region: None,
                page: Some(page),
                system_index: None,
                anchors,
            });
        }
    }
    for (system_index, sys) in p.systems.iter().enumerate() {
        let Some((staff_index, staff)) = sys.staves.iter().enumerate().find(|(_, s)| {
            y >= s.top - p.staff_space * 1.2
                && y <= s.bot + p.staff_space * 1.2
                && x >= s.x0
                && x <= s.x1
        }) else {
            continue;
        };
        let anchors: Vec<_> = p
            .elements
            .iter()
            .filter(|e| e.bbox.contains(x, y))
            .filter_map(|e| e.anchor.clone())
            .collect();
        if let Some(col) = layout
            .columns
            .iter()
            .find(|c| c.page == page && c.system_index == system_index && x >= c.x0 && x < c.x1)
        {
            if let (Some(start), Some(end)) = (col.measure_start, col.measure_end) {
                let region = resolve_locator(
                    index,
                    &Locator {
                        staff_index,
                        measure_start: start,
                        measure_end: end,
                        voice: None,
                    },
                )?;
                if region.capability == LocationCapability::Measure {
                    return Ok(LocationResult {
                        capability: region.capability,
                        reason: region.reason.clone(),
                        region: Some(region),
                        page: Some(page),
                        system_index: Some(system_index),
                        anchors,
                    });
                }
            }
        }
        let _ = staff;
        if !anchors.is_empty() {
            return Ok(LocationResult {
                capability: LocationCapability::AnchorOnly,
                reason: Some("no reliable measure grid".into()),
                region: None,
                page: Some(page),
                system_index: Some(system_index),
                anchors,
            });
        }
    }
    Ok(no_hit("no-hit"))
}
