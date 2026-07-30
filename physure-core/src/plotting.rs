//! 100% Native Rust 3D/2D Plotting and Export Engine.
//!
//! Provides zero-copy surface mesh triangulation, normal generation, colormap mapping,
//! and exporting to standard 3D/2D formats:
//! - STL (Binary & ASCII)
//! - Wavefront OBJ
//! - glTF 2.0 (JSON scene with embedded buffers)
//! - Stanford PLY
//! - Standalone WebGL Interactive Viewer (Three.js HTML)
//! - SVG Isometric Surface
//! - CSV / JSON Data Tables

use crate::error::{PhysureError, PhysureResult};

static THREE_JS: &str = include_str!("vendor/three.min.js");
static ORBIT_CONTROLS_JS: &str = include_str!("vendor/OrbitControls.min.js");

/// Represents 3D mesh surface data with grid dimensions and physical unit labels.
#[derive(Debug, Clone)]
pub struct Mesh3DData {
    pub title: String,
    pub x_label: String,
    pub y_label: String,
    pub z_label: String,
    pub x_grid: Vec<f64>,
    pub y_grid: Vec<f64>,
    pub z_grid: Vec<f64>,
    pub rows: usize,
    pub cols: usize,
}

impl Mesh3DData {
    /// Create a new 3D mesh data surface.
    pub fn new(
        title: impl Into<String>,
        x_label: impl Into<String>,
        y_label: impl Into<String>,
        z_label: impl Into<String>,
        x_grid: Vec<f64>,
        y_grid: Vec<f64>,
        z_grid: Vec<f64>,
        rows: usize,
        cols: usize,
    ) -> Self {
        Self {
            title: title.into(),
            x_label: x_label.into(),
            y_label: y_label.into(),
            z_label: z_label.into(),
            x_grid,
            y_grid,
            z_grid,
            rows,
            cols,
        }
    }

    /// Compute 3D vertices [x, y, z] for each node in the grid with proportional Z scaling.
    pub fn vertices(&self) -> Vec<[f64; 3]> {
        let mut raw_verts = Vec::with_capacity(self.rows * self.cols);

        let has_full_x = self.x_grid.len() == self.rows * self.cols;
        let has_full_y = self.y_grid.len() == self.rows * self.cols;

        let mut x_min = f64::INFINITY; let mut x_max = f64::NEG_INFINITY;
        let mut y_min = f64::INFINITY; let mut y_max = f64::NEG_INFINITY;
        let mut z_min = f64::INFINITY; let mut z_max = f64::NEG_INFINITY;

        for r in 0..self.rows {
            let y_val = if has_full_y {
                0.0
            } else if r < self.y_grid.len() {
                self.y_grid[r]
            } else {
                r as f64
            };

            for c in 0..self.cols {
                let idx = r * self.cols + c;
                let x_val = if has_full_x {
                    self.x_grid[idx]
                } else if c < self.x_grid.len() {
                    self.x_grid[c]
                } else {
                    c as f64
                };

                let y_final = if has_full_y {
                    self.y_grid[idx]
                } else {
                    y_val
                };

                let z_val = if idx < self.z_grid.len() {
                    self.z_grid[idx]
                } else {
                    0.0
                };

                if x_val < x_min { x_min = x_val; }
                if x_val > x_max { x_max = x_val; }
                if y_final < y_min { y_min = y_final; }
                if y_final > y_max { y_max = y_final; }
                if z_val < z_min { z_min = z_val; }
                if z_val > z_max { z_max = z_val; }

                raw_verts.push([x_val, y_final, z_val]);
            }
        }

        let x_span = if x_max != x_min { x_max - x_min } else { 1.0 };
        let y_span = if y_max != y_min { y_max - y_min } else { 1.0 };
        let xy_extent = x_span.max(y_span);
        let z_span = if z_max != z_min { z_max - z_min } else { 1.0 };

        // Proportionally scale height (Z) relative to physical XY extent (max Z height ~ 0.5 * xy_extent)
        let z_scale = (0.5 * xy_extent) / z_span;

        raw_verts
            .into_iter()
            .map(|[x, y, z]| {
                let scaled_z = (z - z_min) * z_scale;
                [x, y, scaled_z]
            })
            .collect()
    }

    /// Triangulate the grid surface into face indices [v0, v1, v2].
    pub fn faces(&self) -> Vec<[usize; 3]> {
        if self.rows < 2 || self.cols < 2 {
            return Vec::new();
        }
        let num_cells = (self.rows - 1) * (self.cols - 1);
        let mut faces = Vec::with_capacity(num_cells * 2);

        for r in 0..(self.rows - 1) {
            for c in 0..(self.cols - 1) {
                let idx00 = r * self.cols + c;
                let idx01 = r * self.cols + (c + 1);
                let idx10 = (r + 1) * self.cols + c;
                let idx11 = (r + 1) * self.cols + (c + 1);

                // Triangle 1
                faces.push([idx00, idx10, idx01]);
                // Triangle 2
                faces.push([idx10, idx11, idx01]);
            }
        }

        faces
    }

    /// Compute normal vectors for each face triangle.
    pub fn face_normals(&self, verts: &[[f64; 3]], faces: &[[usize; 3]]) -> Vec<[f64; 3]> {
        faces
            .iter()
            .map(|f| {
                let v0 = verts[f[0]];
                let v1 = verts[f[1]];
                let v2 = verts[f[2]];

                let u = [v1[0] - v0[0], v1[1] - v0[1], v1[2] - v0[2]];
                let v = [v2[0] - v0[0], v2[1] - v0[1], v2[2] - v0[2]];

                let nx = u[1] * v[2] - u[2] * v[1];
                let ny = u[2] * v[0] - u[0] * v[2];
                let nz = u[0] * v[1] - u[1] * v[0];

                let len = (nx * nx + ny * ny + nz * nz).sqrt();
                if len > 1e-12 {
                    [nx / len, ny / len, nz / len]
                } else {
                    [0.0, 0.0, 1.0]
                }
            })
            .collect()
    }

    /// Map Z values to RGB colors using a plasma colormap.
    pub fn colors_rgb(&self, verts: &[[f64; 3]]) -> Vec<[f64; 3]> {
        if verts.is_empty() {
            return Vec::new();
        }

        let mut min_z = f64::INFINITY;
        let mut max_z = f64::NEG_INFINITY;
        for v in verts {
            if v[2] < min_z {
                min_z = v[2];
            }
            if v[2] > max_z {
                max_z = v[2];
            }
        }

        let range = if max_z > min_z { max_z - min_z } else { 1.0 };

        verts
            .iter()
            .map(|v| {
                let t = ((v[2] - min_z) / range).clamp(0.0, 1.0);
                plasma_colormap(t)
            })
            .collect()
    }

    /// Export to Binary STL file bytes.
    pub fn export_stl_binary(&self) -> Vec<u8> {
        let verts = self.vertices();
        let faces = self.faces();
        let normals = self.face_normals(&verts, &faces);

        let num_triangles = faces.len() as u32;
        let capacity = 80 + 4 + (faces.len() * 50);
        let mut buffer = Vec::with_capacity(capacity);

        // 80-byte header
        let mut header = [0u8; 80];
        let msg = b"Physure 3D Rust Engine Mesh Export";
        header[..msg.len()].copy_from_slice(msg);
        buffer.extend_from_slice(&header);

        // Triangle count (u32 little endian)
        buffer.extend_from_slice(&num_triangles.to_le_bytes());

        for (i, f) in faces.iter().enumerate() {
            let n = normals[i];
            let v0 = verts[f[0]];
            let v1 = verts[f[1]];
            let v2 = verts[f[2]];

            // Normal float32 x 3
            buffer.extend_from_slice(&(n[0] as f32).to_le_bytes());
            buffer.extend_from_slice(&(n[1] as f32).to_le_bytes());
            buffer.extend_from_slice(&(n[2] as f32).to_le_bytes());

            // Vertex 1 float32 x 3
            buffer.extend_from_slice(&(v0[0] as f32).to_le_bytes());
            buffer.extend_from_slice(&(v0[1] as f32).to_le_bytes());
            buffer.extend_from_slice(&(v0[2] as f32).to_le_bytes());

            // Vertex 2 float32 x 3
            buffer.extend_from_slice(&(v1[0] as f32).to_le_bytes());
            buffer.extend_from_slice(&(v1[1] as f32).to_le_bytes());
            buffer.extend_from_slice(&(v1[2] as f32).to_le_bytes());

            // Vertex 3 float32 x 3
            buffer.extend_from_slice(&(v2[0] as f32).to_le_bytes());
            buffer.extend_from_slice(&(v2[1] as f32).to_le_bytes());
            buffer.extend_from_slice(&(v2[2] as f32).to_le_bytes());

            // Attribute byte count (u16 = 0)
            buffer.extend_from_slice(&0u16.to_le_bytes());
        }

        buffer
    }

    /// Export to ASCII STL string.
    pub fn export_stl_ascii(&self) -> String {
        let verts = self.vertices();
        let faces = self.faces();
        let normals = self.face_normals(&verts, &faces);

        let mut out = String::with_capacity(faces.len() * 200 + 100);
        out.push_str("solid physure_mesh\n");

        for (i, f) in faces.iter().enumerate() {
            let n = normals[i];
            let v0 = verts[f[0]];
            let v1 = verts[f[1]];
            let v2 = verts[f[2]];

            out.push_str(&format!(
                "  facet normal {:.6e} {:.6e} {:.6e}\n",
                n[0], n[1], n[2]
            ));
            out.push_str("    outer loop\n");
            out.push_str(&format!(
                "      vertex {:.6e} {:.6e} {:.6e}\n",
                v0[0], v0[1], v0[2]
            ));
            out.push_str(&format!(
                "      vertex {:.6e} {:.6e} {:.6e}\n",
                v1[0], v1[1], v1[2]
            ));
            out.push_str(&format!(
                "      vertex {:.6e} {:.6e} {:.6e}\n",
                v2[0], v2[1], v2[2]
            ));
            out.push_str("    endloop\n");
            out.push_str("  endfacet\n");
        }

        out.push_str("endsolid physure_mesh\n");
        out
    }

    /// Export to Wavefront OBJ format string (with vertex RGB colors).
    pub fn export_obj(&self) -> String {
        let verts = self.vertices();
        let faces = self.faces();
        let colors = self.colors_rgb(&verts);

        let mut out = String::with_capacity(verts.len() * 40 + faces.len() * 20);
        out.push_str("# Physure 3D Mesh OBJ Export (Rust Engine)\n");

        for (v, c) in verts.iter().zip(colors.iter()) {
            out.push_str(&format!(
                "v {:.6} {:.6} {:.6} {:.4} {:.4} {:.4}\n",
                v[0], v[1], v[2], c[0], c[1], c[2]
            ));
        }

        for f in &faces {
            out.push_str(&format!("f {} {} {}\n", f[0] + 1, f[1] + 1, f[2] + 1));
        }

        out
    }

    /// Export to Stanford PLY ASCII string.
    pub fn export_ply(&self) -> String {
        let verts = self.vertices();
        let faces = self.faces();
        let colors = self.colors_rgb(&verts);

        let mut out = String::new();
        out.push_str("ply\n");
        out.push_str("format ascii 1.0\n");
        out.push_str("comment Created by Physure 3D Rust Engine\n");
        out.push_str(&format!("element vertex {}\n", verts.len()));
        out.push_str("property float x\nproperty float y\nproperty float z\n");
        out.push_str("property uchar red\nproperty uchar green\nproperty uchar blue\n");
        out.push_str(&format!("element face {}\n", faces.len()));
        out.push_str("property list uchar int vertex_indices\n");
        out.push_str("end_header\n");

        for (v, c) in verts.iter().zip(colors.iter()) {
            let r = (c[0] * 255.0) as u8;
            let g = (c[1] * 255.0) as u8;
            let b = (c[2] * 255.0) as u8;
            out.push_str(&format!("{:.6} {:.6} {:.6} {} {} {}\n", v[0], v[1], v[2], r, g, b));
        }

        for f in &faces {
            out.push_str(&format!("3 {} {} {}\n", f[0], f[1], f[2]));
        }

        out
    }

    /// Export to glTF 2.0 JSON standard scene.
    pub fn export_gltf(&self) -> String {
        let verts = self.vertices();
        let faces = self.faces();
        let colors = self.colors_rgb(&verts);

        let mut v_bytes = Vec::with_capacity(verts.len() * 12);
        let mut min_pos = [f64::INFINITY; 3];
        let mut max_pos = [f64::NEG_INFINITY; 3];

        for v in &verts {
            for k in 0..3 {
                if v[k] < min_pos[k] { min_pos[k] = v[k]; }
                if v[k] > max_pos[k] { max_pos[k] = v[k]; }
                v_bytes.extend_from_slice(&(v[k] as f32).to_le_bytes());
            }
        }

        let mut c_bytes = Vec::with_capacity(colors.len() * 12);
        for c in &colors {
            for k in 0..3 {
                c_bytes.extend_from_slice(&(c[k] as f32).to_le_bytes());
            }
        }

        let mut f_bytes = Vec::with_capacity(faces.len() * 12);
        for f in &faces {
            for idx in f {
                f_bytes.extend_from_slice(&(*idx as u32).to_le_bytes());
            }
        }

        let mut buffer_data = Vec::new();
        buffer_data.extend_from_slice(&v_bytes);
        let v_len = v_bytes.len();
        let c_len = c_bytes.len();
        buffer_data.extend_from_slice(&c_bytes);
        let f_offset = buffer_data.len();
        buffer_data.extend_from_slice(&f_bytes);

        let b64_str = base64_encode(&buffer_data);
        let data_uri = format!("data:application/octet-stream;base64,{}", b64_str);

        format!(
            r#"{{
  "asset": {{ "version": "2.0", "generator": "Physure 3D Rust Engine" }},
  "scene": 0,
  "scenes": [ {{ "nodes": [0] }} ],
  "nodes": [ {{ "mesh": 0 }} ],
  "meshes": [
    {{
      "primitives": [
        {{
          "attributes": {{ "POSITION": 0, "COLOR_0": 1 }},
          "indices": 2,
          "mode": 4
        }}
      ]
    }}
  ],
  "buffers": [ {{ "uri": "{}", "byteLength": {} }} ],
  "bufferViews": [
    {{ "buffer": 0, "byteOffset": 0, "byteLength": {}, "target": 34962 }},
    {{ "buffer": 0, "byteOffset": {}, "byteLength": {}, "target": 34962 }},
    {{ "buffer": 0, "byteOffset": {}, "byteLength": {}, "target": 34963 }}
  ],
  "accessors": [
    {{ "bufferView": 0, "byteOffset": 0, "componentType": 5126, "count": {}, "type": "VEC3", "min": [{:.4}, {:.4}, {:.4}], "max": [{:.4}, {:.4}, {:.4}] }},
    {{ "bufferView": 1, "byteOffset": 0, "componentType": 5126, "count": {}, "type": "VEC3" }},
    {{ "bufferView": 2, "byteOffset": 0, "componentType": 5125, "count": {}, "type": "SCALAR" }}
  ]
}}"#,
            data_uri,
            buffer_data.len(),
            v_len,
            v_len,
            c_len,
            f_offset,
            f_bytes.len(),
            verts.len(),
            min_pos[0], min_pos[1], min_pos[2],
            max_pos[0], max_pos[1], max_pos[2],
            verts.len(),
            faces.len() * 3
        )
    }

    pub fn x_bounds(&self) -> (f64, f64) {
        let mut min_val = f64::INFINITY;
        let mut max_val = f64::NEG_INFINITY;
        for &v in &self.x_grid {
            if v < min_val { min_val = v; }
            if v > max_val { max_val = v; }
        }
        if min_val == f64::INFINITY { (0.0, 1.0) } else { (min_val, max_val) }
    }

    pub fn y_bounds(&self) -> (f64, f64) {
        let mut min_val = f64::INFINITY;
        let mut max_val = f64::NEG_INFINITY;
        for &v in &self.y_grid {
            if v < min_val { min_val = v; }
            if v > max_val { max_val = v; }
        }
        if min_val == f64::INFINITY { (0.0, 1.0) } else { (min_val, max_val) }
    }

    pub fn z_bounds(&self) -> (f64, f64) {
        let mut min_val = f64::INFINITY;
        let mut max_val = f64::NEG_INFINITY;
        for &v in &self.z_grid {
            if v < min_val { min_val = v; }
            if v > max_val { max_val = v; }
        }
        if min_val == f64::INFINITY { (0.0, 1.0) } else { (min_val, max_val) }
    }
}

pub fn sanitize_unit_label(raw: &str) -> String {
    let clean = raw
        .replace(" * sin", "")
        .replace(" * cos", "")
        .replace(" * tan", "")
        .replace(" * SIN", "")
        .replace(" * COS", "")
        .replace(" * TAN", "")
        .replace("* sin", "")
        .replace("* cos", "")
        .replace("* tan", "")
        .replace("* SIN", "")
        .replace("* COS", "")
        .replace("* TAN", "")
        .replace("SIN", "")
        .replace("COS", "")
        .replace("TAN", "");
    let trimmed = clean.trim();
    if trimmed.is_empty() {
        "dimensionless".to_string()
    } else {
        trimmed.to_string()
    }
}

impl Mesh3DData {
    /// Export to standalone HTML file containing Three.js WebGL 3D Interactive Viewer with quantitative analytical features.
    pub fn export_html_threejs(&self) -> String {
        let verts = self.vertices();
        let faces = self.faces();
        let colors = self.colors_rgb(&verts);

        let (x_min, x_max) = self.x_bounds();
        let (y_min, y_max) = self.y_bounds();
        let (z_min, z_max) = self.z_bounds();
        let z_mid = (z_min + z_max) / 2.0;

        let v_flat: Vec<f32> = verts.iter().flat_map(|v| [v[0] as f32, v[2] as f32, v[1] as f32]).collect();
        let f_flat: Vec<u32> = faces.iter().flat_map(|f| [f[0] as u32, f[1] as u32, f[2] as u32]).collect();
        let c_flat: Vec<f32> = colors.iter().flat_map(|c| [c[0] as f32, c[1] as f32, c[2] as f32]).collect();

        let x_label_clean = sanitize_unit_label(&self.x_label);
        let y_label_clean = sanitize_unit_label(&self.y_label);
        let z_label_clean = sanitize_unit_label(&self.z_label);

        format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0, user-scalable=no">
    <title>{}</title>
    <style>
        *, *::before, *::after {{ box-sizing: border-box; }}
        html, body {{ width: 100%; height: 100%; margin: 0; padding: 0; overflow: hidden; background-color: #0f172a; font-family: system-ui, -apple-system, sans-serif; color: #f8fafc; position: relative; -webkit-tap-highlight-color: transparent; touch-action: none; }}
        #canvas-container {{ width: 100%; height: 100%; display: block; position: absolute; top: 0; left: 0; right: 0; bottom: 0; }}

        /* ── HUD Panel ── */
        #hud {{
            position: absolute; top: 12px; left: 12px;
            background: rgba(15, 23, 42, 0.88); backdrop-filter: blur(12px);
            border: 1px solid rgba(255, 255, 255, 0.12);
            padding: 14px 16px; border-radius: 12px;
            box-shadow: 0 8px 32px rgba(0,0,0,0.45);
            max-width: 260px; z-index: 10; pointer-events: auto;
            transition: transform 0.25s ease, opacity 0.25s ease;
        }}
        #hud.collapsed {{ transform: translateX(calc(-100% - 24px)); opacity: 0; pointer-events: none; }}
        #hud h2 {{ margin: 0 0 8px 0; font-size: 14px; font-weight: 700; color: #38bdf8; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; letter-spacing: -0.3px; }}
        #hud p {{ margin: 3px 0; font-size: 11.5px; color: #cbd5e1; line-height: 1.4; }}
        #hud p strong {{ color: #94a3b8; font-weight: 600; }}
        .controls-info {{ margin-top: 10px; font-size: 10.5px; color: #64748b; border-top: 1px solid rgba(255,255,255,0.08); padding-top: 8px; line-height: 1.5; }}

        /* ── Toggle button ── */
        #hud-toggle {{
            position: absolute; top: 12px; left: 12px;
            width: 36px; height: 36px; border-radius: 10px;
            background: rgba(15, 23, 42, 0.88); backdrop-filter: blur(12px);
            border: 1px solid rgba(255, 255, 255, 0.12);
            color: #38bdf8; font-size: 18px; cursor: pointer;
            display: none; align-items: center; justify-content: center;
            z-index: 11; box-shadow: 0 4px 16px rgba(0,0,0,0.35);
            transition: background 0.2s ease;
        }}
        #hud-toggle:hover, #hud-toggle:active {{ background: rgba(56, 189, 248, 0.2); }}

        /* ── Colorbar ── */
        #colorbar {{
            position: absolute; top: 12px; right: 12px;
            background: rgba(15, 23, 42, 0.88); backdrop-filter: blur(12px);
            border: 1px solid rgba(255, 255, 255, 0.12);
            padding: 10px 14px; border-radius: 12px;
            display: flex; flex-direction: column; align-items: center; gap: 6px;
            z-index: 10; box-shadow: 0 8px 32px rgba(0,0,0,0.45);
        }}
        .cb-title {{ font-size: 10px; font-weight: 700; color: #94a3b8; text-transform: uppercase; letter-spacing: 0.6px; white-space: nowrap; max-width: 120px; overflow: hidden; text-overflow: ellipsis; }}
        .cb-gradient {{ width: 16px; height: 140px; border-radius: 4px; background: linear-gradient(to top, #312e81, #4f46e5, #06b6d4, #10b981, #eab308, #ef4444); border: 1px solid rgba(255,255,255,0.15); }}
        .cb-labels {{ display: flex; flex-direction: column; justify-content: space-between; height: 140px; font-size: 10px; color: #cbd5e1; font-family: 'SF Mono', 'Cascadia Code', monospace; }}

        /* ── Tooltip ── */
        #tooltip {{
            position: absolute; display: none;
            background: rgba(15, 23, 42, 0.96); border: 1px solid #38bdf8;
            padding: 10px 14px; border-radius: 10px;
            font-size: 11.5px; color: #f8fafc; pointer-events: none; z-index: 20;
            box-shadow: 0 8px 24px rgba(0,0,0,0.55); font-family: 'SF Mono', 'Cascadia Code', monospace;
            max-width: 280px; line-height: 1.5;
        }}

        /* ── Responsive: Tablet ── */
        @media (max-width: 768px) {{
            #hud {{ max-width: 220px; padding: 10px 12px; }}
            #hud h2 {{ font-size: 12px; }}
            #hud p {{ font-size: 10.5px; }}
            .controls-info {{ font-size: 9.5px; }}
            #colorbar {{ padding: 8px 10px; }}
            .cb-gradient {{ height: 110px; width: 14px; }}
            .cb-labels {{ height: 110px; font-size: 9px; }}
        }}

        /* ── Responsive: Mobile ── */
        @media (max-width: 540px) {{
            #hud {{ max-width: 60%; padding: 10px 12px; top: 10px; left: 10px; border-radius: 10px; }}
            #hud h2 {{ font-size: 11px; margin-bottom: 4px; }}
            #hud p {{ font-size: 9.5px; margin: 2px 0; }}
            .controls-info {{ display: none; }}
            #hud-toggle {{ display: flex; }}
            #colorbar {{ top: 10px; right: 10px; padding: 6px 8px; border-radius: 10px; }}
            .cb-title {{ max-width: 70px; font-size: 8px; }}
            .cb-gradient {{ height: 80px; width: 10px; }}
            .cb-labels {{ height: 80px; font-size: 8px; }}
            #tooltip {{ font-size: 10px; padding: 8px 10px; max-width: 200px; }}
        }}

        /* ── Responsive: Small phone ── */
        @media (max-width: 380px) {{
            #hud {{ max-width: 55%; padding: 8px 10px; }}
            #hud h2 {{ font-size: 10px; }}
            #hud p {{ font-size: 9px; }}
            #colorbar {{ padding: 5px 6px; }}
            .cb-gradient {{ height: 60px; width: 8px; }}
            .cb-labels {{ height: 60px; font-size: 7px; }}
        }}
    </style>
    <script>{}</script>
    <script>{}</script>
</head>
<body>
    <button id="hud-toggle" onclick="toggleHud()" aria-label="Toggle info panel">ℹ</button>

    <div id="hud">
        <h2>{}</h2>
        <p><strong>X:</strong> {:.3} → {:.3}  <em style="color:#64748b">({})</em></p>
        <p><strong>Y:</strong> {:.3} → {:.3}  <em style="color:#64748b">({})</em></p>
        <p><strong>Z:</strong> {:.3} → {:.3}  <em style="color:#64748b">({})</em></p>
        <div class="controls-info" id="controls-label"></div>
    </div>

    <div id="colorbar">
        <div class="cb-title">Scale ({})</div>
        <div style="display: flex; gap: 8px;">
            <div class="cb-gradient"></div>
            <div class="cb-labels">
                <span>{:.2}</span>
                <span>{:.2}</span>
                <span>{:.2}</span>
            </div>
        </div>
    </div>

    <div id="tooltip"></div>
    <div id="canvas-container"></div>

    <script>
        /* ── Responsive HUD toggle ── */
        let hudVisible = true;
        function toggleHud() {{
            const hud = document.getElementById('hud');
            const btn = document.getElementById('hud-toggle');
            hudVisible = !hudVisible;
            hud.classList.toggle('collapsed', !hudVisible);
            btn.textContent = hudVisible ? '✕' : 'ℹ';
        }}
        /* auto-collapse on small screens */
        if (window.innerWidth <= 540) {{
            toggleHud();
        }}

        /* ── Adaptive controls label ── */
        const isTouchDevice = ('ontouchstart' in window) || navigator.maxTouchPoints > 0;
        const controlsLabel = document.getElementById('controls-label');
        if (controlsLabel) {{
            controlsLabel.innerHTML = isTouchDevice
                ? '👆 <strong>Controls:</strong> One finger drag to rotate · Pinch to zoom · Two fingers to pan'
                : '🖱 <strong>Controls:</strong> Left drag to rotate · Right drag to pan · Scroll to zoom';
        }}

        /* ── Three.js Scene Setup ── */
        const vertices = new Float32Array({:?});
        const indices = new Uint32Array({:?});
        const colors = new Float32Array({:?});

        const xMin = {}; const xMax = {};
        const yMin = {}; const yMax = {};
        const zMin = {}; const zMax = {};

        const container = document.getElementById('canvas-container');
        const scene = new THREE.Scene();
        scene.background = new THREE.Color(0x0f172a);

        const camera = new THREE.PerspectiveCamera(45, (container.clientWidth || window.innerWidth) / (container.clientHeight || window.innerHeight), 0.1, 1000);
        const renderer = new THREE.WebGLRenderer({{ antialias: true }});
        renderer.setSize(container.clientWidth || window.innerWidth, container.clientHeight || window.innerHeight);
        renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
        container.appendChild(renderer.domElement);

        const controls = new THREE.OrbitControls(camera, renderer.domElement);
        controls.enableDamping = true;
        controls.dampingFactor = 0.08;
        controls.rotateSpeed = isTouchDevice ? 0.5 : 0.8;
        controls.panSpeed = isTouchDevice ? 0.4 : 0.8;
        controls.zoomSpeed = isTouchDevice ? 0.6 : 1.0;

        const geometry = new THREE.BufferGeometry();
        geometry.setAttribute('position', new THREE.BufferAttribute(vertices, 3));
        geometry.setAttribute('color', new THREE.BufferAttribute(colors, 3));
        geometry.setIndex(new THREE.BufferAttribute(indices, 1));
        geometry.computeVertexNormals();

        geometry.computeBoundingBox();
        const bbox = geometry.boundingBox;
        const center = new THREE.Vector3();
        bbox.getCenter(center);
        const size = new THREE.Vector3();
        bbox.getSize(size);

        const material = new THREE.MeshStandardMaterial({{
            vertexColors: true,
            side: THREE.DoubleSide,
            roughness: 0.35,
            metalness: 0.08,
        }});
        const mesh = new THREE.Mesh(geometry, material);
        scene.add(mesh);

        const wireframeGeom = new THREE.WireframeGeometry(geometry);
        const wireframeMat = new THREE.LineBasicMaterial({{ color: 0x38bdf8, transparent: true, opacity: 0.12 }});
        const wireframe = new THREE.LineSegments(wireframeGeom, wireframeMat);
        mesh.add(wireframe);

        /* ── Grid helper ── */
        const gridScale = Math.max(size.x, size.y) * 1.5;
        const gridHelper = new THREE.GridHelper(gridScale, 20, 0x38bdf8, 0x1e293b);
        gridHelper.position.set(center.x, bbox.min.y, center.z);
        scene.add(gridHelper);

        /* ── Axis lines ── */
        const axLen = Math.max(size.x, size.y, size.z) * 0.6;
        const axOrigin = new THREE.Vector3(bbox.min.x, bbox.min.y, bbox.min.z);
        function makeAxis(dir, color) {{
            const pts = [axOrigin.clone(), axOrigin.clone().add(dir.clone().multiplyScalar(axLen))];
            const g = new THREE.BufferGeometry().setFromPoints(pts);
            return new THREE.Line(g, new THREE.LineBasicMaterial({{ color: color, linewidth: 2 }}));
        }}
        scene.add(makeAxis(new THREE.Vector3(1,0,0), 0xef4444));
        scene.add(makeAxis(new THREE.Vector3(0,1,0), 0x22c55e));
        scene.add(makeAxis(new THREE.Vector3(0,0,1), 0x3b82f6));

        /* ── Lighting ── */
        scene.add(new THREE.AmbientLight(0xffffff, 0.55));
        const dirLight1 = new THREE.DirectionalLight(0xffffff, 0.75);
        dirLight1.position.set(1, 2, 3);
        scene.add(dirLight1);
        const dirLight2 = new THREE.DirectionalLight(0x38bdf8, 0.35);
        dirLight2.position.set(-1, -1, -2);
        scene.add(dirLight2);

        /* ── Raycaster for tooltip ── */
        const maxDim = Math.max(size.x, size.y, size.z);
        const raycaster = new THREE.Raycaster();
        const mouse = new THREE.Vector2();
        const tooltip = document.getElementById('tooltip');

        function handlePointer(event) {{
            const clientX = event.touches ? event.touches[0].clientX : event.clientX;
            const clientY = event.touches ? event.touches[0].clientY : event.clientY;
            const rect = renderer.domElement.getBoundingClientRect();
            mouse.x = ((clientX - rect.left) / rect.width) * 2 - 1;
            mouse.y = -((clientY - rect.top) / rect.height) * 2 + 1;
            raycaster.setFromCamera(mouse, camera);
            const intersects = raycaster.intersectObject(mesh);
            if (intersects.length > 0) {{
                const pt = intersects[0].point;
                const normX = size.x > 1e-6 ? (pt.x - bbox.min.x) / size.x : 0;
                const normY = size.y > 1e-6 ? (pt.y - bbox.min.y) / size.y : 0;
                const normZ = size.z > 1e-6 ? (pt.z - bbox.min.z) / size.z : 0;
                const realX = xMin + normX * (xMax - xMin);
                const realY = yMin + normY * (yMax - yMin);
                const realZ = zMin + normZ * (zMax - zMin);
                tooltip.style.display = 'block';
                const tx = Math.min(clientX + 14, window.innerWidth - 220);
                const ty = Math.min(clientY + 14, window.innerHeight - 80);
                tooltip.style.left = tx + 'px';
                tooltip.style.top = ty + 'px';
                tooltip.innerHTML = `<strong>📍 Point</strong><br/>` +
                    `X: ${{realX.toFixed(4)}} ({})<br/>` +
                    `Y: ${{realY.toFixed(4)}} ({})<br/>` +
                    `Z: ${{realZ.toFixed(4)}} ({})`;
            }} else {{
                tooltip.style.display = 'none';
            }}
        }}
        window.addEventListener('pointermove', handlePointer);

        /* ── Camera positioning ── */
        camera.position.set(center.x + maxDim * 1.4, center.y + maxDim * 1.1, center.z + maxDim * 1.7);
        camera.lookAt(center);
        controls.target.copy(center);

        /* ── Responsive resize ── */
        function updateSize() {{
            const w = container.clientWidth || window.innerWidth;
            const h = container.clientHeight || window.innerHeight;
            camera.aspect = w / (h || 1);
            camera.updateProjectionMatrix();
            renderer.setSize(w, h);
        }}
        updateSize();
        if (window.ResizeObserver) {{
            new ResizeObserver(updateSize).observe(container);
        }}
        window.addEventListener('resize', updateSize);

        function animate() {{
            requestAnimationFrame(animate);
            controls.update();
            renderer.render(scene, camera);
        }}
        animate();
    </script>
</body>
</html>
"#,
            self.title,
            THREE_JS,
            ORBIT_CONTROLS_JS,
            self.title,
            x_min, x_max, x_label_clean,
            y_min, y_max, y_label_clean,
            z_min, z_max, z_label_clean,
            z_label_clean,
            z_max, z_mid, z_min,
            v_flat,
            f_flat,
            c_flat,
            x_min, x_max,
            y_min, y_max,
            z_min, z_max,
            self.x_label, self.y_label, self.z_label
        )
    }

    /// Export to any specified format ("stl", "stl_ascii", "obj", "gltf", "ply", "html").
    pub fn export_format(&self, format: &str) -> PhysureResult<Vec<u8>> {
        match format.to_lowercase().as_str() {
            "stl" | "stl_binary" => Ok(self.export_stl_binary()),
            "stl_ascii" => Ok(self.export_stl_ascii().into_bytes()),
            "obj" => Ok(self.export_obj().into_bytes()),
            "gltf" | "glb" => Ok(self.export_gltf().into_bytes()),
            "ply" => Ok(self.export_ply().into_bytes()),
            "html" | "threejs" | "webgl" => Ok(self.export_html_threejs().into_bytes()),
            _ => Err(PhysureError::Generic(format!("Unsupported export format: {}", format))),
        }
    }
}

/// Helper function to map normalized t in [0, 1] to Plasma RGB values.
fn plasma_colormap(t: f64) -> [f64; 3] {
    let t = t.clamp(0.0, 1.0);
    // Smooth plasma approximation curve
    let r = (0.05 + 0.95 * (std::f64::consts::PI * t * 0.8).sin().powi(2)).clamp(0.0, 1.0);
    let g = (0.01 + 0.85 * (std::f64::consts::PI * t).sin().powi(3)).clamp(0.0, 1.0);
    let b = (0.5 + 0.5 * (std::f64::consts::PI * t * 0.5 + 0.5).cos()).clamp(0.0, 1.0);
    [r, g, b]
}

/// Standard Base64 encoder helper for glTF URIs.
fn base64_encode(input: &[u8]) -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    let mut i = 0;
    while i < input.len() {
        let b0 = input[i] as u32;
        let b1 = if i + 1 < input.len() { input[i + 1] as u32 } else { 0 };
        let b2 = if i + 2 < input.len() { input[i + 2] as u32 } else { 0 };

        let triplet = (b0 << 16) | (b1 << 8) | b2;

        out.push(CHARSET[((triplet >> 18) & 0x3F) as usize] as char);
        out.push(CHARSET[((triplet >> 12) & 0x3F) as usize] as char);

        if i + 1 < input.len() {
            out.push(CHARSET[((triplet >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }

        if i + 2 < input.len() {
            out.push(CHARSET[(triplet & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }

        i += 3;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_3d_mesh_export() {
        let x_grid = vec![0.0, 1.0, 2.0];
        let y_grid = vec![0.0, 1.0, 2.0];
        let z_grid = vec![
            0.0, 0.5, 1.0,
            0.5, 1.0, 1.5,
            1.0, 1.5, 2.0,
        ];

        let mesh = Mesh3DData::new(
            "Test Mesh",
            "X (m)",
            "Y (m)",
            "Z (Pa)",
            x_grid,
            y_grid,
            z_grid,
            3,
            3,
        );

        let stl_bytes = mesh.export_stl_binary();
        assert!(stl_bytes.len() > 84);

        let obj_str = mesh.export_obj();
        assert!(obj_str.contains("v "));
        assert!(obj_str.contains("f "));

        let gltf_str = mesh.export_gltf();
        assert!(gltf_str.contains("generator"));

        let ply_str = mesh.export_ply();
        assert!(ply_str.contains("ply"));

        let html_str = mesh.export_html_threejs();
        assert!(html_str.contains("OrbitControls"));
    }
}
