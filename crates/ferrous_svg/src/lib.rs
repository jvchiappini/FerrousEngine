//! SVG Tessellation system for Ferrous Engine.
//!
//! Converts SVG path data to GPU-friendly triangle meshes using Lyon. The
//! parser understands `<path d="...">` elements, raw SVG path `d` data, and the
//! basic shape elements (`<rect>`, `<circle>`, `<ellipse>`, `<polygon>`,
//! `<polyline>`) by reducing them to path commands first. Anything else is a
//! typed error instead of a guessed placeholder shape.

use std::fmt::Write as _;

use lyon_svg::parser::path::{Token, Tokenizer};
use lyon_svg::parser::xmlparser::FromSpan;
use lyon_tessellation::path::builder::SvgPathBuilder;
use lyon_tessellation::path::math::{point, vector, Angle, Point, Vector};
use lyon_tessellation::path::{ArcFlags, Path};
use lyon_tessellation::{
    FillOptions, FillRule, FillTessellator, FillVertex, FillVertexConstructor, VertexBuffers,
};

// Minimal Rect to avoid dependency cycle with ferrous_ui_core
#[derive(Debug, Clone, Copy, Default)]
pub struct SvgRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Represents a GPU-ready triangle mesh generated from an SVG.
///
/// `indices` are `u32` because the UI renderer concatenates several meshes
/// into one arena before a single indexed draw; the arena offset can exceed
/// `u16::MAX` once a frame carries more than 65 536 vertices. A u16 path would
/// therefore need a capacity guard in the renderer, not only in this crate.
#[derive(Debug, Clone, Default)]
pub struct SvgMesh {
    pub vertices: Vec<SvgVertex>,
    pub indices: Vec<u32>,
    pub bounding_box: SvgRect,
}

/// One vertex of a tessellated SVG: position, straight RGBA colour and depth.
///
/// Memory layout (28 bytes total, 4-byte aligned, no padding):
/// - offset  0: position [f32; 2]
/// - offset  8: color    [f32; 4]
/// - offset 24: `z_order`  f32
///
/// The UI path overwrites `color` and `z_order` per draw command
/// (`gui_renderer.rs` prepares each upload from the command), but both fields
/// remain part of the vertex layout because that is the interface consumed by
/// `svg.wgsl` and the GUI pipeline declares all three attributes.
#[derive(Copy, Clone, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct SvgVertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
    pub z_order: f32,
}

// The GUI uploads this struct verbatim and the SVG pipeline derives its stride
// from `size_of::<SvgVertex>()`; a layout drift would silently shear meshes.
const _: () = {
    assert!(std::mem::size_of::<SvgVertex>() == 28);
    assert!(std::mem::align_of::<SvgVertex>() == 4);
    assert!(std::mem::offset_of!(SvgVertex, color) == 8);
    assert!(std::mem::offset_of!(SvgVertex, z_order) == 24);
};

impl SvgMesh {
    /// Parses SVG path data and tessellates it into a triangle mesh.
    ///
    /// Accepted inputs:
    ///
    /// - one or more `<path d="...">` elements (other attributes are ignored),
    /// - raw SVG path `d` data such as `"M10 10 H90 V90 H10 Z"`,
    /// - `<rect>`, `<circle>`, `<ellipse>`, `<polygon>` and `<polyline>`
    ///   elements, which are converted to path commands before parsing.
    ///
    /// The name mirrors the existing public API; it is not the
    /// `std::str::FromStr` trait implementation.
    ///
    /// # Errors
    ///
    /// Returns a message when no supported geometry is found, when the path
    /// data is malformed, or when the resulting path covers no area.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(svg_content: &str) -> Result<Self, String> {
        let paths = collect_path_data(svg_content)?;
        tessellate(&paths)
    }
}

/// Extracts every supported path `d` string from `svg_content`.
fn collect_path_data(svg_content: &str) -> Result<Vec<String>, String> {
    let mut paths = Vec::new();

    for element in find_elements(svg_content, "path") {
        if let Some(data) = attribute(element, "d") {
            let data = data.trim();
            if !data.is_empty() {
                paths.push(data.to_string());
            }
        }
    }
    if !paths.is_empty() {
        return Ok(paths);
    }

    for element in find_elements(svg_content, "rect") {
        if let Some(data) = rect_path_data(element) {
            paths.push(data);
        }
    }
    for element in find_elements(svg_content, "circle") {
        if let Some(data) = ellipse_path_data(element, true) {
            paths.push(data);
        }
    }
    for element in find_elements(svg_content, "ellipse") {
        if let Some(data) = ellipse_path_data(element, false) {
            paths.push(data);
        }
    }
    for element in find_elements(svg_content, "polygon") {
        if let Some(data) = polygon_path_data(element, true) {
            paths.push(data);
        }
    }
    for element in find_elements(svg_content, "polyline") {
        if let Some(data) = polygon_path_data(element, false) {
            paths.push(data);
        }
    }
    if !paths.is_empty() {
        return Ok(paths);
    }

    // Raw path data: the first non-space byte is an SVG path command.
    let trimmed = svg_content.trim_start();
    if starts_with_path_command(trimmed) {
        return Ok(vec![trimmed.to_string()]);
    }

    Err("no supported SVG geometry found (expected a path, rect, circle, ellipse, polygon or polyline)".to_string())
}

fn starts_with_path_command(text: &str) -> bool {
    matches!(
        text.as_bytes().first().copied(),
        Some(
            b'M' | b'm'
                | b'L'
                | b'l'
                | b'H'
                | b'h'
                | b'V'
                | b'v'
                | b'C'
                | b'c'
                | b'S'
                | b's'
                | b'Q'
                | b'q'
                | b'T'
                | b't'
                | b'A'
                | b'a'
                | b'Z'
                | b'z'
        )
    )
}

/// Returns the contents of every `<tag ...>` element (without the angle
/// brackets). Matching is case-insensitive and requires a tag boundary, so
/// `<path>` does not match `<paths>`.
fn find_elements<'a>(source: &'a str, tag: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut rest = source;
    while let Some(open) = rest.find('<') {
        let after = &rest[open + 1..];
        let name_len = tag.len();
        if after.len() >= name_len && after[..name_len].eq_ignore_ascii_case(tag) {
            let boundary = after.as_bytes().get(name_len).copied();
            if matches!(
                boundary,
                None | Some(b' ' | b'\t' | b'\r' | b'\n' | b'/' | b'>')
            ) {
                if let Some(close) = after.find('>') {
                    out.push(&after[..close]);
                    rest = &after[close + 1..];
                    continue;
                }
            }
        }
        rest = &rest[open + 1..];
    }
    out
}

/// Attribute lookup on an element body (e.g. `rect x="1" width="2"`).
fn attribute<'a>(element: &'a str, name: &str) -> Option<&'a str> {
    let bytes = element.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        // Skip to the next name start.
        while index < bytes.len() && !is_name_start(bytes[index]) {
            index += 1;
        }
        let start = index;
        while index < bytes.len() && is_name_char(bytes[index]) {
            index += 1;
        }
        if start == index {
            index += 1;
            continue;
        }
        let candidate = &element[start..index];
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index < bytes.len() && bytes[index] == b'=' {
            index += 1;
            while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            if index < bytes.len() && (bytes[index] == b'"' || bytes[index] == b'\'') {
                let quote = bytes[index];
                index += 1;
                let value_start = index;
                while index < bytes.len() && bytes[index] != quote {
                    index += 1;
                }
                let value = &element[value_start..index];
                if candidate.eq_ignore_ascii_case(name) {
                    return Some(value);
                }
                if index < bytes.len() {
                    index += 1;
                }
            }
        }
    }
    None
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b':'
}

fn is_name_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b':' | b'.')
}

fn number(element: &str, name: &str) -> Option<f32> {
    attribute(element, name).and_then(|value| value.trim().parse::<f32>().ok())
}

fn rect_path_data(element: &str) -> Option<String> {
    let x = number(element, "x").unwrap_or(0.0);
    let y = number(element, "y").unwrap_or(0.0);
    let width = number(element, "width")?;
    let height = number(element, "height")?;
    if !(width > 0.0 && height > 0.0) {
        return None;
    }
    let rx = number(element, "rx")
        .or_else(|| number(element, "ry"))
        .unwrap_or(0.0)
        .clamp(0.0, width * 0.5);
    let ry = number(element, "ry")
        .or_else(|| number(element, "rx"))
        .unwrap_or(0.0)
        .clamp(0.0, height * 0.5);
    let right = x + width;
    let bottom = y + height;
    if rx <= 0.0 || ry <= 0.0 {
        return Some(format!("M {x} {y} H {right} V {bottom} H {x} Z"));
    }
    Some(format!(
        "M {x_start} {y} H {x_end} A {rx} {ry} 0 0 1 {right} {y_end} V {y_bottom} A {rx} {ry} 0 0 1 {x_end} {bottom} H {x_start} A {rx} {ry} 0 0 1 {x} {y_bottom} V {y_end} A {rx} {ry} 0 0 1 {x_start} {y} Z",
        x_start = x + rx,
        x_end = right - rx,
        y_end = y + ry,
        y_bottom = bottom - ry,
    ))
}

fn ellipse_path_data(element: &str, circle: bool) -> Option<String> {
    let cx = number(element, "cx").unwrap_or(0.0);
    let cy = number(element, "cy").unwrap_or(0.0);
    let rx = if circle {
        number(element, "r")?
    } else {
        number(element, "rx")?
    };
    let ry = if circle { rx } else { number(element, "ry")? };
    if !(rx > 0.0 && ry > 0.0) {
        return None;
    }
    let left = cx - rx;
    let right = cx + rx;
    Some(format!(
        "M {right} {cy} A {rx} {ry} 0 1 0 {left} {cy} A {rx} {ry} 0 1 0 {right} {cy} Z"
    ))
}

fn polygon_path_data(element: &str, close: bool) -> Option<String> {
    let points = attribute(element, "points")?;
    let mut coordinates = points.split(|c: char| c.is_ascii_whitespace() || c == ',');
    let mut data = String::new();
    let mut first = true;
    while let Some(x) = coordinates.next() {
        let Some(y) = coordinates.next() else { break };
        let Ok(x) = x.trim().parse::<f32>() else {
            continue;
        };
        let Ok(y) = y.trim().parse::<f32>() else {
            continue;
        };
        if first {
            let _ = write!(data, "M {x} {y}");
            first = false;
        } else {
            let _ = write!(data, " L {x} {y}");
        }
    }
    if first {
        return None;
    }
    if close {
        data.push_str(" Z");
    }
    Some(data)
}

/// Parses SVG path `d` data into a Lyon path.
///
/// `svgparser` exposes the commands as `f64`, while the renderer geometry is
/// `f32`; the cast down to `f32` is the documented geometry precision, not an
/// accidental truncation.
#[allow(clippy::cast_possible_truncation)]
fn parse_path(data: &str) -> Path {
    fn p(x: f64, y: f64) -> Point {
        point(x as f32, y as f32)
    }
    fn v(x: f64, y: f64) -> Vector {
        vector(x as f32, y as f32)
    }

    let mut builder = Path::builder().with_svg();
    for token in Tokenizer::from_str(data) {
        match token {
            Token::MoveTo { abs, x, y } => {
                if abs {
                    builder.move_to(p(x, y));
                } else {
                    builder.relative_move_to(v(x, y));
                }
            }
            Token::LineTo { abs, x, y } => {
                if abs {
                    builder.line_to(p(x, y));
                } else {
                    builder.relative_line_to(v(x, y));
                }
            }
            Token::HorizontalLineTo { abs, x } => {
                if abs {
                    builder.horizontal_line_to(x as f32);
                } else {
                    builder.relative_horizontal_line_to(x as f32);
                }
            }
            Token::VerticalLineTo { abs, y } => {
                if abs {
                    builder.vertical_line_to(y as f32);
                } else {
                    builder.relative_vertical_line_to(y as f32);
                }
            }
            Token::CurveTo {
                abs,
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                if abs {
                    builder.cubic_bezier_to(p(x1, y1), p(x2, y2), p(x, y));
                } else {
                    builder.relative_cubic_bezier_to(v(x1, y1), v(x2, y2), v(x, y));
                }
            }
            Token::SmoothCurveTo { abs, x2, y2, x, y } => {
                if abs {
                    builder.smooth_cubic_bezier_to(p(x2, y2), p(x, y));
                } else {
                    builder.smooth_relative_cubic_bezier_to(v(x2, y2), v(x, y));
                }
            }
            Token::Quadratic { abs, x1, y1, x, y } => {
                if abs {
                    builder.quadratic_bezier_to(p(x1, y1), p(x, y));
                } else {
                    builder.relative_quadratic_bezier_to(v(x1, y1), v(x, y));
                }
            }
            Token::SmoothQuadratic { abs, x, y } => {
                if abs {
                    builder.smooth_quadratic_bezier_to(p(x, y));
                } else {
                    builder.smooth_relative_quadratic_bezier_to(v(x, y));
                }
            }
            Token::EllipticalArc {
                abs,
                rx,
                ry,
                x_axis_rotation,
                large_arc,
                sweep,
                x,
                y,
            } => {
                let radii = v(rx, ry);
                let rotation = Angle::degrees(x_axis_rotation as f32);
                let flags = ArcFlags { large_arc, sweep };
                if abs {
                    builder.arc_to(radii, rotation, flags, p(x, y));
                } else {
                    builder.relative_arc_to(radii, rotation, flags, v(x, y));
                }
            }
            Token::ClosePath { .. } => builder.close(),
        }
    }
    builder.build()
}

fn tessellate(paths: &[String]) -> Result<SvgMesh, String> {
    let mut buffers: VertexBuffers<SvgVertex, u32> = VertexBuffers::new();
    let mut fill_tessellator = FillTessellator::new();
    let options = FillOptions::default().with_fill_rule(FillRule::NonZero);

    for data in paths {
        let path = parse_path(data);
        fill_tessellator
            .tessellate_path(
                &path,
                &options,
                &mut lyon_tessellation::BuffersBuilder::new(&mut buffers, VertexCtor),
            )
            .map_err(|error| format!("tessellation failed: {error:?}"))?;
    }

    if buffers.vertices.is_empty() {
        return Err("the SVG path covers no area".to_string());
    }

    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for vertex in &buffers.vertices {
        min_x = min_x.min(vertex.position[0]);
        min_y = min_y.min(vertex.position[1]);
        max_x = max_x.max(vertex.position[0]);
        max_y = max_y.max(vertex.position[1]);
    }

    Ok(SvgMesh {
        vertices: buffers.vertices,
        indices: buffers.indices,
        bounding_box: SvgRect {
            x: min_x,
            y: min_y,
            width: (max_x - min_x).max(0.0),
            height: (max_y - min_y).max(0.0),
        },
    })
}

struct VertexCtor;
impl FillVertexConstructor<SvgVertex> for VertexCtor {
    fn new_vertex(&mut self, vertex: FillVertex) -> SvgVertex {
        SvgVertex {
            position: [vertex.position().x, vertex.position().y],
            color: [1.0, 1.0, 1.0, 1.0],
            z_order: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Strictly-inside test for a non-overlapping triangulation: a point is
    /// covered when it lies inside at least one triangle, excluding shared
    /// triangle edges so a boundary point is not reported twice.
    fn inside(mesh: &SvgMesh, px: f32, py: f32) -> bool {
        mesh.indices.chunks_exact(3).any(|triangle| {
            let [first, second, third] = [
                mesh.vertices[triangle[0] as usize].position,
                mesh.vertices[triangle[1] as usize].position,
                mesh.vertices[triangle[2] as usize].position,
            ];
            let area = (second[0] - first[0]) * (third[1] - first[1])
                - (third[0] - first[0]) * (second[1] - first[1]);
            if area.abs() < f32::EPSILON {
                return false;
            }
            let w0 =
                ((second[0] - px) * (third[1] - py) - (third[0] - px) * (second[1] - py)) / area;
            let w1 = ((third[0] - px) * (first[1] - py) - (first[0] - px) * (third[1] - py)) / area;
            let w2 = 1.0 - w0 - w1;
            w0 >= -1e-4 && w1 >= -1e-4 && w2 >= -1e-4
        })
    }

    #[test]
    fn raw_path_data_is_tessellated_as_the_real_shape() {
        // A diamond. The old placeholder triangle covered x in [30, 70] at
        // y = 50, so (15, 50) proves the parsed path is used, not a fallback.
        let mesh = SvgMesh::from_str("M 10 50 L 50 10 L 90 50 L 50 90 Z").expect("diamond");
        assert!(!mesh.indices.is_empty());
        assert!(inside(&mesh, 48.0, 52.0), "centre is filled");
        assert!(inside(&mesh, 20.0, 50.0), "left half is filled");
        assert!(!inside(&mesh, 5.0, 5.0), "outside is empty");
    }

    #[test]
    fn a_path_element_attribute_is_parsed() {
        let mesh = SvgMesh::from_str(
            r#"<svg width="100" height="100"><path d="M10 10 H90 V90 H10 Z" fill="red"/></svg>"#,
        )
        .expect("path element");
        assert!(inside(&mesh, 55.0, 45.0));
        assert!(!inside(&mesh, 2.0, 2.0));
        assert!((mesh.bounding_box.width - 80.0).abs() < 1e-3);
    }

    #[test]
    fn basic_shapes_are_real_geometry() {
        let rect =
            SvgMesh::from_str(r#"<rect x="10" y="10" width="80" height="80"/>"#).expect("rect");
        assert!(inside(&rect, 55.0, 45.0));
        assert!(!inside(&rect, 5.0, 5.0));

        let circle = SvgMesh::from_str(r#"<circle cx="50" cy="50" r="40"/>"#).expect("circle");
        assert!(inside(&circle, 60.0, 50.0));
        assert!(
            !inside(&circle, 5.0, 5.0),
            "circle does not fill its corner"
        );
    }

    #[test]
    fn multiple_paths_are_combined_into_one_mesh() {
        let mesh = SvgMesh::from_str(
            r#"<svg><path d="M0 0 H20 V20 H0 Z"/><path d="M40 40 H60 V60 H40 Z"/></svg>"#,
        )
        .expect("two paths");
        assert!(inside(&mesh, 15.0, 15.0));
        assert!(inside(&mesh, 55.0, 55.0));
        assert!(!inside(&mesh, 30.0, 30.0));
    }

    #[test]
    fn unsupported_input_is_an_error_not_a_placeholder() {
        assert!(SvgMesh::from_str("<svg><g/></svg>").is_err());
        assert!(SvgMesh::from_str("not a path").is_err());
    }
}
