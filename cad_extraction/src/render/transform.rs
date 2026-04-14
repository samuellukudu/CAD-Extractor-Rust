use crate::extraction::models::Point2;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine2 {
    pub m11: f64,
    pub m12: f64,
    pub m21: f64,
    pub m22: f64,
    pub tx: f64,
    pub ty: f64,
}

impl Affine2 {
    pub const IDENTITY: Self = Self {
        m11: 1.0,
        m12: 0.0,
        m21: 0.0,
        m22: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    pub fn from_trs(translate_x: f64, translate_y: f64, scale_x: f64, scale_y: f64, rotation: f64) -> Self {
        let c = rotation.cos();
        let s = rotation.sin();
        Self {
            m11: scale_x * c,
            m12: -scale_y * s,
            m21: scale_x * s,
            m22: scale_y * c,
            tx: translate_x,
            ty: translate_y,
        }
    }

    pub fn compose(self, rhs: Self) -> Self {
        Self {
            m11: self.m11 * rhs.m11 + self.m12 * rhs.m21,
            m12: self.m11 * rhs.m12 + self.m12 * rhs.m22,
            m21: self.m21 * rhs.m11 + self.m22 * rhs.m21,
            m22: self.m21 * rhs.m12 + self.m22 * rhs.m22,
            tx: self.m11 * rhs.tx + self.m12 * rhs.ty + self.tx,
            ty: self.m21 * rhs.tx + self.m22 * rhs.ty + self.ty,
        }
    }

    pub fn transform_point(&self, point: Point2) -> Point2 {
        Point2::new(
            self.m11 * point.x + self.m12 * point.y + self.tx,
            self.m21 * point.x + self.m22 * point.y + self.ty,
        )
    }

    pub fn scale_hint(&self) -> f64 {
        let sx = (self.m11 * self.m11 + self.m21 * self.m21).sqrt();
        let sy = (self.m12 * self.m12 + self.m22 * self.m22).sqrt();
        ((sx + sy) * 0.5).max(1e-6)
    }

    pub fn rotation_hint(&self) -> f64 {
        self.m21.atan2(self.m11)
    }
}
