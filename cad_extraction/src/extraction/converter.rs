//! Converts an `acadrust::CadDocument` into the output JSON types.

use std::collections::BTreeMap;

use acadrust::{CadDocument, EntityType};
use acadrust::entities::EntityCommon;

use super::output::*;

// ────────────────────────── Public entry point ──────────────────────────

pub fn convert_document(doc: &CadDocument) -> CadOutput {
    CadOutput {
        header: convert_header(doc),
        tables: convert_tables(doc),
        blocks: convert_blocks(doc),
    }
}

// ────────────────────────── Header ──────────────────────────

fn convert_header(doc: &CadDocument) -> HeaderOutput {
    let h = &doc.header;
    let mut vars = BTreeMap::new();

    vars.insert("$ACADVER".into(), HeaderValue::String(doc.version.to_string()));
    vars.insert("$INSBASE".into(), HeaderValue::Point3D(v3_to_p3(&h.model_space_insertion_base)));
    vars.insert("$EXTMIN".into(), HeaderValue::Point3D(v3_to_p3(&h.model_space_extents_min)));
    vars.insert("$EXTMAX".into(), HeaderValue::Point3D(v3_to_p3(&h.model_space_extents_max)));
    vars.insert("$LIMMIN".into(), HeaderValue::Point2D(v2_to_p2(&h.model_space_limits_min)));
    vars.insert("$LIMMAX".into(), HeaderValue::Point2D(v2_to_p2(&h.model_space_limits_max)));
    vars.insert("$ORTHOMODE".into(), HeaderValue::Bool(h.ortho_mode));
    vars.insert("$FILLMODE".into(), HeaderValue::Bool(h.fill_mode));
    vars.insert("$QTEXTMODE".into(), HeaderValue::Bool(h.quick_text_mode));
    vars.insert("$MIRRTEXT".into(), HeaderValue::Bool(h.mirror_text));
    vars.insert("$LTSCALE".into(), HeaderValue::Float(h.linetype_scale));
    vars.insert("$ATTMODE".into(), HeaderValue::Int(h.attribute_visibility as i64));
    vars.insert("$TEXTSIZE".into(), HeaderValue::Float(h.text_height));
    vars.insert("$TEXTSTYLE".into(), HeaderValue::String(h.current_text_style_name.clone()));
    vars.insert("$CLAYER".into(), HeaderValue::String(h.current_layer_name.clone()));
    vars.insert("$CELTYPE".into(), HeaderValue::String(h.current_linetype_name.clone()));
    vars.insert("$CELTSCALE".into(), HeaderValue::Float(h.current_entity_linetype_scale));
    vars.insert("$DIMSCALE".into(), HeaderValue::Float(h.dim_scale));
    vars.insert("$DIMASZ".into(), HeaderValue::Float(h.dim_arrow_size));
    vars.insert("$DIMTXT".into(), HeaderValue::Float(h.dim_text_height));
    vars.insert("$DIMSTYLE".into(), HeaderValue::String(h.current_dimstyle_name.clone()));
    vars.insert("$LUNITS".into(), HeaderValue::Int(h.linear_unit_format as i64));
    vars.insert("$LUPREC".into(), HeaderValue::Int(h.linear_unit_precision as i64));
    vars.insert("$AUNITS".into(), HeaderValue::Int(h.angular_unit_format as i64));
    vars.insert("$AUPREC".into(), HeaderValue::Int(h.angular_unit_precision as i64));
    vars.insert("$INSUNITS".into(), HeaderValue::Int(h.insertion_units as i64));
    vars.insert("$ANGBASE".into(), HeaderValue::Float(h.angle_base));
    vars.insert("$ANGDIR".into(), HeaderValue::Int(h.angle_direction as i64));
    vars.insert("$MEASUREMENT".into(), HeaderValue::Int(h.measurement as i64));

    HeaderOutput { vars }
}

// ────────────────────────── Tables ──────────────────────────

fn convert_tables(doc: &CadDocument) -> TablesOutput {
    TablesOutput {
        viewport: convert_viewports(doc),
        line_type: convert_line_types(doc),
        layer: convert_layers(doc),
        style: convert_text_styles(doc),
        dimstyle: convert_dim_styles(doc),
    }
}

fn convert_viewports(doc: &CadDocument) -> ViewPortTableOutput {
    let viewports = doc.vports.iter().map(|vp| {
        ViewPortEntryOutput {
            owner_handle: format!("{:X}", doc.vports.handle().value()),
            name: vp.name.clone(),
            lower_left_corner: v2_to_p2(&vp.lower_left),
            upper_right_corner: v2_to_p2(&vp.upper_right),
            center: v2_to_p2(&vp.view_center),
            snap_base_point: v2_to_p2(&vp.snap_base),
            snap_spacing: v2_to_p2(&vp.snap_spacing),
            grid_spacing: v2_to_p2(&vp.grid_spacing),
            view_direction_from_target: v3_to_p3(&vp.view_direction),
            view_target: v3_to_p3(&vp.view_target),
            view_twist_angle: vp.view_twist,
            render_mode: 0,
        }
    }).collect();

    ViewPortTableOutput {
        handle: format!("{:X}", doc.vports.handle().value()),
        owner_handle: "0".into(),
        viewports,
    }
}

fn convert_line_types(doc: &CadDocument) -> LineTypeTableOutput {
    let line_types = doc.line_types.iter().map(|lt| {
        let pattern: Vec<f64> = lt.elements.iter().map(|el| el.length).collect();
        (lt.name.clone(), LineTypeEntryOutput {
            name: lt.name.clone(),
            description: lt.description.clone(),
            pattern,
            pattern_length: lt.pattern_length,
        })
    }).collect();

    LineTypeTableOutput {
        handle: format!("{:X}", doc.line_types.handle().value()),
        owner_handle: "0".into(),
        line_types,
    }
}

fn convert_layers(doc: &CadDocument) -> LayerTableOutput {
    let layers = doc.layers.iter().map(|layer| {
        let ci = color_to_layer_index(&layer.color);
        let rgb = color_to_layer_rgb(&layer.color);
        (layer.name.clone(), LayerEntryOutput {
            name: layer.name.clone(),
            frozen: layer.is_frozen(),
            visible: layer.is_visible(),
            color_index: ci,
            color: rgb,
            line_type: layer.line_type.clone(),
            line_weight: line_weight_to_output(&layer.line_weight),
            display_name: layer.name.clone(),
        })
    }).collect();

    LayerTableOutput {
        handle: format!("{:X}", doc.layers.handle().value()),
        owner_handle: "0".into(),
        layers,
    }
}

fn convert_text_styles(doc: &CadDocument) -> StyleTableOutput {
    let styles = doc.text_styles.iter().map(|style| {
        (style.name.clone(), StyleEntryOutput {
            style_name: style.name.clone(),
            fixed_text_height: style.height,
            width_factor: style.width_factor,
            oblique_angle: style.oblique_angle,
            last_height: style.last_height,
            font: style.font_file.clone(),
            big_font: style.big_font_file.clone(),
        })
    }).collect();

    StyleTableOutput {
        handle: format!("{:X}", doc.text_styles.handle().value()),
        owner_handle: "0".into(),
        styles,
    }
}

fn convert_dim_styles(doc: &CadDocument) -> DimStyleTableOutput {
    let dim_styles = doc.dim_styles.iter().map(|ds| {
        (ds.name.clone(), DimStyleEntryOutput {
            name: ds.name.clone(),
            DIMSCALE: ds.dimscale,
            DIMASZ: ds.dimasz,
            DIMEXO: ds.dimexo,
            DIMEXE: ds.dimexe,
            DIMTXT: ds.dimtxt,
            DIMGAP: ds.dimgap,
            DIMCLRT: ds.dimclrt,
            DIMDEC: ds.dimdec,
            DIMDLE: ds.dimdle,
            DIMDLI: ds.dimdli,
            DIMLFAC: ds.dimlfac,
            DIMRND: ds.dimrnd,
            DIMTAD: ds.dimtad,
            DIMTIH: ds.dimtih,
            DIMTOH: ds.dimtoh,
            DIMTIX: ds.dimtix,
            DIMTOFL: ds.dimtofl,
        })
    }).collect();

    DimStyleTableOutput {
        handle: format!("{:X}", doc.dim_styles.handle().value()),
        owner_handle: "0".into(),
        dim_styles,
    }
}

// ────────────────────────── Blocks & Entities ──────────────────────────

fn convert_blocks(doc: &CadDocument) -> BTreeMap<String, BlockOutput> {
    use acadrust::Handle;

    let block_name_by_handle: BTreeMap<Handle, String> = doc
        .block_records
        .iter()
        .map(|br| (br.handle, br.name.clone()))
        .collect();

    let mut entities_by_block: BTreeMap<String, Vec<EntityOutput>> = BTreeMap::new();

    for entity in doc.entities() {
        let common = entity.common();
        let block_name = block_name_by_handle
            .get(&common.owner_handle)
            .cloned()
            .unwrap_or_else(|| "*Model_Space".to_string());

        let entity_out = convert_entity(entity, common);
        entities_by_block.entry(block_name).or_default().push(entity_out);
    }

    let mut blocks = BTreeMap::new();

    for br in doc.block_records.iter() {
        let entities = entities_by_block.remove(&br.name).unwrap_or_default();
        blocks.insert(br.name.clone(), BlockOutput {
            handle: format!("{:X}", br.handle.value()),
            owner_handle: "0".into(),
            layer: "0".into(),
            name: br.name.clone(),
            position: p3(0.0, 0.0, 0.0),
            name2: br.name.clone(),
            xref_path: String::new(),
            entities,
        });
    }

    for (name, entities) in entities_by_block {
        blocks.entry(name.clone()).or_insert_with(|| BlockOutput {
            handle: "0".into(),
            owner_handle: "0".into(),
            layer: "0".into(),
            name: name.clone(),
            position: p3(0.0, 0.0, 0.0),
            name2: name,
            xref_path: String::new(),
            entities,
        });
    }

    blocks
}

fn convert_entity(entity: &EntityType, common: &EntityCommon) -> EntityOutput {
    let (color_index, color_decimal) = color_to_output(&common.color);
    let line_type = if common.linetype.is_empty() {
        None
    } else {
        Some(common.linetype.clone())
    };

    EntityOutput {
        entity_type: entity.as_entity().entity_type().to_owned(),
        handle: format!("{:X}", common.handle.value()),
        owner_handle: format!("{:X}", common.owner_handle.value()),
        layer: common.layer.clone(),
        line_type,
        color_index,
        color: color_decimal,
        geometry: convert_geometry(entity),
    }
}

fn convert_geometry(entity: &EntityType) -> EntityGeometryOutput {
    match entity {
        EntityType::Line(line) => EntityGeometryOutput::Line {
            vertices: vec![
                p3(line.start.x, line.start.y, line.start.z),
                p3(line.end.x, line.end.y, line.end.z),
            ],
        },
        EntityType::Circle(c) => EntityGeometryOutput::Circle {
            center: p3(c.center.x, c.center.y, c.center.z),
            radius: c.radius,
        },
        EntityType::Arc(a) => EntityGeometryOutput::Arc {
            center: p3(a.center.x, a.center.y, a.center.z),
            radius: a.radius,
            start_angle: a.start_angle,
            end_angle: a.end_angle,
        },
        EntityType::Ellipse(e) => EntityGeometryOutput::Ellipse {
            center: p3(e.center.x, e.center.y, e.center.z),
            major_axis_end_point: p3(e.major_axis.x, e.major_axis.y, e.major_axis.z),
            axis_ratio: e.minor_axis_ratio,
            start_angle: e.start_parameter,
            end_angle: e.end_parameter,
        },
        EntityType::LwPolyline(pl) => EntityGeometryOutput::LwPolyline {
            vertices: pl.vertices.iter().map(|v| p2(v.location.x, v.location.y)).collect(),
            shape: pl.is_closed,
            has_continuous_linetype_pattern: false,
            width: if pl.constant_width != 0.0 { Some(pl.constant_width) } else { None },
        },
        EntityType::Polyline(pl) => EntityGeometryOutput::Polyline {
            vertices: pl.vertices.iter().map(|v| p3(v.location.x, v.location.y, v.location.z)).collect(),
            shape: pl.is_closed(),
        },
        EntityType::Polyline2D(pl) => EntityGeometryOutput::LwPolyline {
            vertices: pl.vertices.iter().map(|v| p2(v.location.x, v.location.y)).collect(),
            shape: pl.is_closed(),
            has_continuous_linetype_pattern: false,
            width: None,
        },
        EntityType::Spline(s) => EntityGeometryOutput::Spline {
            degree: s.degree,
            knots: s.knots.clone(),
            control_points: s.control_points.iter().map(|p| p3(p.x, p.y, p.z)).collect(),
            fit_points: s.fit_points.iter().map(|p| p3(p.x, p.y, p.z)).collect(),
        },
        EntityType::Text(t) => {
            let halign = t.horizontal_alignment as i16;
            let end_point = t.alignment_point.as_ref().map(|ap| p3(ap.x, ap.y, ap.z));
            EntityGeometryOutput::Text {
                start_point: p3(t.insertion_point.x, t.insertion_point.y, t.insertion_point.z),
                text_height: t.height,
                text: t.value.clone(),
                rotation: if t.rotation != 0.0 { Some(t.rotation) } else { None },
                x_scale: if t.width_factor != 1.0 { Some(t.width_factor) } else { None },
                style_name: if t.style.is_empty() { None } else { Some(t.style.clone()) },
                halign: if halign != 0 { Some(halign) } else { None },
                end_point: if halign != 0 { end_point } else { None },
            }
        }
        EntityType::MText(t) => EntityGeometryOutput::MText {
            insertion_point: p3(t.insertion_point.x, t.insertion_point.y, t.insertion_point.z),
            text_height: t.height,
            text: t.value.clone(),
            rotation: if t.rotation != 0.0 { Some(t.rotation) } else { None },
            width: if t.rectangle_width != 0.0 { Some(t.rectangle_width) } else { None },
            attachment_point: Some(t.attachment_point as i16),
        },
        EntityType::Insert(ins) => EntityGeometryOutput::Insert {
            name: ins.block_name.clone(),
            position: p3(ins.insert_point.x, ins.insert_point.y, ins.insert_point.z),
            x_scale: if ins.x_scale() != 1.0 { Some(ins.x_scale()) } else { None },
            y_scale: if ins.y_scale() != 1.0 { Some(ins.y_scale()) } else { None },
            z_scale: if ins.z_scale() != 1.0 { Some(ins.z_scale()) } else { None },
            rotation: if ins.rotation != 0.0 { Some(ins.rotation) } else { None },
        },
        EntityType::Dimension(dim) => EntityGeometryOutput::Dimension {
            block_name: dim.base().block_name.clone(),
            insertion_point: p3(
                dim.base().insertion_point.x,
                dim.base().insertion_point.y,
                dim.base().insertion_point.z,
            ),
        },
        EntityType::Solid(s) => EntityGeometryOutput::Solid {
            points: vec![
                p3(s.first_corner.x, s.first_corner.y, s.first_corner.z),
                p3(s.second_corner.x, s.second_corner.y, s.second_corner.z),
                p3(s.third_corner.x, s.third_corner.y, s.third_corner.z),
                p3(s.fourth_corner.x, s.fourth_corner.y, s.fourth_corner.z),
            ],
        },
        EntityType::Hatch(h) => EntityGeometryOutput::Hatch {
            pattern_name: format!("{:?}", h.pattern_type),
            is_solid: h.is_solid,
            boundary_loops: h.paths.iter().map(convert_hatch_boundary).collect(),
        },
        _ => EntityGeometryOutput::Unsupported {
            original_type: entity.as_entity().entity_type().to_owned(),
        },
    }
}

fn convert_hatch_boundary(path: &acadrust::entities::hatch::BoundaryPath) -> HatchBoundaryLoopOutput {
    use acadrust::entities::hatch::BoundaryEdge;

    let mut polyline_out = None;
    let mut edges_out = None;

    for edge in &path.edges {
        match edge {
            BoundaryEdge::Polyline(pl) => {
                polyline_out = Some(HatchPolylineOutput {
                    vertices: pl.vertices.iter().map(|v| p3(v.x, v.y, v.z)).collect(),
                    is_closed: pl.is_closed,
                });
            }
            _ => {
                let edge_vec = edges_out.get_or_insert_with(Vec::new);
                match edge {
                    BoundaryEdge::Line(l) => edge_vec.push(HatchEdgeOutput::Line {
                        start: p2(l.start.x, l.start.y),
                        end: p2(l.end.x, l.end.y),
                    }),
                    BoundaryEdge::CircularArc(a) => edge_vec.push(HatchEdgeOutput::CircularArc {
                        center: p2(a.center.x, a.center.y),
                        radius: a.radius,
                        start_angle: a.start_angle,
                        end_angle: a.end_angle,
                        counter_clockwise: a.counter_clockwise,
                    }),
                    BoundaryEdge::EllipticArc(e) => edge_vec.push(HatchEdgeOutput::EllipticArc {
                        center: p2(e.center.x, e.center.y),
                        major_axis_end_point: p2(e.major_axis_endpoint.x, e.major_axis_endpoint.y),
                        minor_axis_ratio: e.minor_axis_ratio,
                        start_angle: e.start_angle,
                        end_angle: e.end_angle,
                        counter_clockwise: e.counter_clockwise,
                    }),
                    BoundaryEdge::Spline(s) => edge_vec.push(HatchEdgeOutput::Spline {
                        degree: s.degree,
                        knots: s.knots.clone(),
                        control_points: s.control_points.iter().map(|p| p2(p.x, p.y)).collect(),
                    }),
                    BoundaryEdge::Polyline(_) => unreachable!(),
                }
            }
        }
    }

    HatchBoundaryLoopOutput {
        loop_type: path.flags.bits(),
        polyline: polyline_out,
        edges: edges_out,
    }
}

// ────────────────────────── Point helpers ──────────────────────────

fn p2(x: f64, y: f64) -> Point2DOutput {
    Point2DOutput { x, y }
}

fn p3(x: f64, y: f64, z: f64) -> Point3DOutput {
    Point3DOutput { x, y, z }
}

fn v2_to_p2(v: &acadrust::Vector2) -> Point2DOutput {
    Point2DOutput { x: v.x, y: v.y }
}

fn v3_to_p3(v: &acadrust::Vector3) -> Point3DOutput {
    Point3DOutput { x: v.x, y: v.y, z: v.z }
}
