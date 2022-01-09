//! Frame container

use eframe::egui::{self, epaint};
use egui::{layers::ShapeIdx, *};
use epaint::*;

/// Put an image background behind some UI.
#[derive(Clone, Copy, Debug)]
#[must_use = "You should call .show()"]
pub struct ImageBg {
    texture_id: TextureId,
    uv: Rect,
    size: Vec2,
    offset: Vec2,
    bg_fill: Color32,
    tint: Color32,
}

impl ImageBg {
    pub fn new(texture_id: TextureId, size: impl Into<Vec2>) -> Self {
        Self {
            texture_id,
            uv: Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            size: size.into(),
            offset: Vec2::ZERO,
            bg_fill: Default::default(),
            tint: Color32::WHITE,
        }
    }

    /// Select UV range. Default is (0,0) in top-left, (1,1) bottom right.
    #[allow(dead_code)]
    pub fn uv(mut self, uv: impl Into<Rect>) -> Self {
        self.uv = uv.into();
        self
    }

    /// A solid color to put behind the image. Useful for transparent images.
    #[allow(dead_code)]
    pub fn bg_fill(mut self, bg_fill: impl Into<Color32>) -> Self {
        self.bg_fill = bg_fill.into();
        self
    }

    /// Multiply image color with this. Default is WHITE (no tint).
    pub fn tint(mut self, tint: impl Into<Color32>) -> Self {
        self.tint = tint.into();
        self
    }

    pub fn offset(mut self, offset: impl Into<Vec2>) -> Self {
        self.offset = offset.into();
        self
    }
}

pub struct Prepared {
    pub frame: ImageBg,
    where_to_put_background: ShapeIdx,
    pub content_ui: Ui,
}

impl ImageBg {
    pub fn begin(self, ui: &mut Ui) -> Prepared {
        let where_to_put_background = ui.painter().add(Shape::Noop);
        let outer_rect_bounds = ui.available_rect_before_wrap();
        let mut inner_rect = outer_rect_bounds;

        // Make sure we don't shrink to the negative:
        inner_rect.max.x = inner_rect.max.x.max(inner_rect.min.x);
        inner_rect.max.y = inner_rect.max.y.max(inner_rect.min.y);

        let content_ui = ui.child_ui(inner_rect, *ui.layout());

        Prepared {
            frame: self,
            where_to_put_background,
            content_ui,
        }
    }

    pub fn show<R>(self, ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
        self.show_dyn(ui, Box::new(add_contents))
    }

    fn show_dyn<'c, R>(self, ui: &mut Ui, add_contents: Box<dyn FnOnce(&mut Ui) -> R + 'c>) -> InnerResponse<R> {
        let mut prepared = self.begin(ui);
        let ret = add_contents(&mut prepared.content_ui);
        let response = prepared.end(ui);
        InnerResponse::new(ret, response)
    }

    pub fn paint(&self, mut rect: Rect) -> Shape {
        let Self {
            texture_id,
            uv,
            size,
            bg_fill,
            tint,
            offset,
        } = self;

        rect.set_width(size.x);
        rect.set_height(size.y);

        rect = rect.translate(*offset);

        let image_mesh = {
            // TODO: builder pattern for Mesh
            let mut mesh = Mesh::with_texture(*texture_id);
            mesh.add_rect_with_uv(rect, *uv, *tint);
            Shape::mesh(mesh)
        };

        if *bg_fill != Default::default() {
            let mut mesh = Mesh::default();
            mesh.add_colored_rect(rect, *bg_fill);
            Shape::Vec(vec![Shape::mesh(mesh), image_mesh])
        } else {
            image_mesh
        }
    }
}

impl Prepared {
    pub fn outer_rect(&self) -> Rect {
        self.content_ui.min_rect()
    }

    pub fn end(self, ui: &mut Ui) -> Response {
        let outer_rect = self.outer_rect();

        let Prepared {
            frame,
            where_to_put_background,
            ..
        } = self;

        if ui.is_rect_visible(outer_rect) {
            let shape = frame.paint(outer_rect);
            ui.painter().set(where_to_put_background, shape);
        }

        ui.allocate_rect(outer_rect, Sense::hover())
    }
}
