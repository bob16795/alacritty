use std::mem;

use crossfont::Metrics;

use alacritty_terminal::term::color::Rgb;

use crate::display::SizeInfo;
use crate::gl;
use crate::gl::types::*;
use crate::renderer::shader::{ShaderError, ShaderProgram, ShaderVersion};
use crate::renderer::{self, cstr};

#[derive(Debug, Copy, Clone, Default, PartialEq)]
pub struct QuadPoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Copy, Clone)]
pub struct RenderQuad {
    pub points: [QuadPoint; 4],
    pub color: Rgb,
    pub alpha: f32,
}

impl RenderQuad {
    pub fn new(points: [QuadPoint; 4], color: Rgb, alpha: f32) -> Self {
        RenderQuad { points, color, alpha }
    }
}

/// Shader sources for rect rendering program.
static RECT_SHADER_F: &str = include_str!("../../res/rect.f.glsl");
static RECT_SHADER_V: &str = include_str!("../../res/rect.v.glsl");

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct Vertex {
    // Normalized screen coordinates.
    x: f32,
    y: f32,

    // Color.
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

/// Rectangle drawing program.
#[derive(Debug)]
pub struct QuadShaderProgram {
    /// Shader program.
    program: ShaderProgram,

    /// Cell width.
    u_cell_width: Option<GLint>,

    /// Cell height.
    u_cell_height: Option<GLint>,

    /// Terminal padding.
    u_padding_x: Option<GLint>,

    /// A padding from the bottom of the screen to viewport.
    u_padding_y: Option<GLint>,

    /// Underline position.
    u_underline_position: Option<GLint>,

    /// Underline thickness.
    u_underline_thickness: Option<GLint>,

    /// Undercurl position.
    u_undercurl_position: Option<GLint>,
}

impl QuadShaderProgram {
    pub fn new(shader_version: ShaderVersion) -> Result<Self, ShaderError> {
        // XXX: This must be in-sync with fragment shader defines.
        let program = ShaderProgram::new(shader_version, None, RECT_SHADER_V, RECT_SHADER_F)?;

        Ok(Self {
            u_cell_width: program.get_uniform_location(cstr!("cellWidth")).ok(),
            u_cell_height: program.get_uniform_location(cstr!("cellHeight")).ok(),
            u_padding_x: program.get_uniform_location(cstr!("paddingX")).ok(),
            u_padding_y: program.get_uniform_location(cstr!("paddingY")).ok(),
            u_underline_position: program.get_uniform_location(cstr!("underlinePosition")).ok(),
            u_underline_thickness: program.get_uniform_location(cstr!("underlineThickness")).ok(),
            u_undercurl_position: program.get_uniform_location(cstr!("undercurlPosition")).ok(),
            program,
        })
    }

    fn id(&self) -> GLuint {
        self.program.id()
    }

    pub fn update_uniforms(&self, size_info: &SizeInfo, metrics: &Metrics) {
        let position = (0.5 * metrics.descent).abs();
        let underline_position = metrics.descent.abs() - metrics.underline_position.abs();

        let viewport_height = size_info.height() - size_info.padding_y();
        let padding_y = viewport_height
            - (viewport_height / size_info.cell_height()).floor() * size_info.cell_height();

        unsafe {
            if let Some(u_cell_width) = self.u_cell_width {
                gl::Uniform1f(u_cell_width, size_info.cell_width());
            }
            if let Some(u_cell_height) = self.u_cell_height {
                gl::Uniform1f(u_cell_height, size_info.cell_height());
            }
            if let Some(u_padding_y) = self.u_padding_y {
                gl::Uniform1f(u_padding_y, padding_y);
            }
            if let Some(u_padding_x) = self.u_padding_x {
                gl::Uniform1f(u_padding_x, size_info.padding_x());
            }
            if let Some(u_underline_position) = self.u_underline_position {
                gl::Uniform1f(u_underline_position, underline_position);
            }
            if let Some(u_underline_thickness) = self.u_underline_thickness {
                gl::Uniform1f(u_underline_thickness, metrics.underline_thickness);
            }
            if let Some(u_undercurl_position) = self.u_undercurl_position {
                gl::Uniform1f(u_undercurl_position, position);
            }
        }
    }
}

#[derive(Debug)]
pub struct QuadRenderer {
    // GL buffer objects.
    vao: GLuint,
    vbo: GLuint,

    programs: QuadShaderProgram,
    vertices: Vec<Vertex>,
}

impl QuadRenderer {
    pub fn new(shader_version: ShaderVersion) -> Result<Self, renderer::Error> {
        let mut vao: GLuint = 0;
        let mut vbo: GLuint = 0;

        let rect_program = QuadShaderProgram::new(shader_version)?;

        unsafe {
            // Allocate buffers.
            gl::GenVertexArrays(1, &mut vao);
            gl::GenBuffers(1, &mut vbo);

            gl::BindVertexArray(vao);

            // VBO binding is not part of VAO itself, but VBO binding is stored in attributes.
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);

            let mut attribute_offset = 0;

            // Position.
            gl::VertexAttribPointer(
                0,
                2,
                gl::FLOAT,
                gl::FALSE,
                mem::size_of::<Vertex>() as i32,
                attribute_offset as *const _,
            );
            gl::EnableVertexAttribArray(0);
            attribute_offset += mem::size_of::<f32>() * 2;

            // Color.
            gl::VertexAttribPointer(
                1,
                4,
                gl::UNSIGNED_BYTE,
                gl::TRUE,
                mem::size_of::<Vertex>() as i32,
                attribute_offset as *const _,
            );
            gl::EnableVertexAttribArray(1);

            // Reset buffer bindings.
            gl::BindVertexArray(0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
        }

        let programs = rect_program;
        Ok(Self { vao, vbo, programs, vertices: Default::default() })
    }

    pub fn draw(&mut self, size_info: &SizeInfo, metrics: &Metrics, quads: Vec<RenderQuad>) {
        unsafe {
            // Bind VAO to enable vertex attribute slots.
            gl::BindVertexArray(self.vao);

            // Bind VBO only once for buffer data upload only.
            gl::BindBuffer(gl::ARRAY_BUFFER, self.vbo);
        }

        let half_width = size_info.width() / 2.;
        let half_height = size_info.height() / 2.;

        // Build rect vertices vector.
        self.vertices.clear();
        for quad in &quads {
            Self::add_quad(&mut self.vertices, half_width, half_height, quad);
        }

        unsafe {
            // We iterate in reverse order to draw plain rects at the end, since we want visual
            // bell or damage rects be above the lines.
            let vertices = &mut self.vertices;
            if !vertices.is_empty() {
                let program = &self.programs;
                gl::UseProgram(program.id());
                program.update_uniforms(size_info, metrics);

                // Upload accumulated undercurl vertices.
                gl::BufferData(
                    gl::ARRAY_BUFFER,
                    (vertices.len() * mem::size_of::<Vertex>()) as isize,
                    vertices.as_ptr() as *const _,
                    gl::STREAM_DRAW,
                );

                // Draw all vertices as list of triangles.
                gl::DrawArrays(gl::TRIANGLES, 0, vertices.len() as i32);

                // Disable program.
                gl::UseProgram(0);

                // Reset buffer bindings to nothing.
                gl::BindBuffer(gl::ARRAY_BUFFER, 0);
                gl::BindVertexArray(0);
            }
        }
    }

    fn add_quad(vertices: &mut Vec<Vertex>, half_width: f32, half_height: f32, quad: &RenderQuad) {
        // Calculate rectangle vertices positions in normalized device coordinates.
        // NDC range from -1 to +1, with Y pointing up.
        for i in [0, 1, 2, 0, 3, 2] {
            let p = quad.points[i];

            let x = p.x / half_width - 1.0;
            let y = -p.y / half_height + 1.0;
            let (r, g, b) = quad.color.as_tuple();
            let a = (quad.alpha * 255.) as u8;

            // Append the vertices to form two triangles.
            vertices.push(Vertex { x, y, r, g, b, a });
        }
    }
}
