use glam::Vec3;

pub struct Camera {
    pub position: Vec3,
    pub forward: Vec3,
    pub up: Vec3,
    pub aspect: f32,
    pub fovy: f32,
    pub znear: f32,
    pub zfar: f32,
}

impl Camera {
    pub fn look_at(from: Vec3, to: Vec3) -> Self {
        Self { 
            position: from, 
            forward: Vec3::normalize_or(to - from, Vec3::Z), 
            up: Vec3::Y, 
            aspect: 16.0 / 9.0,
            fovy: 45.0, 
            znear: 0.05, 
            zfar: 1000.0
        }
    }

    pub fn compute_view_projection(&self) -> (glam::Mat4, glam::Mat4) {
        let view = glam::Mat4::look_at_rh(self.position, self.position + self.forward, self.up);
        let perspective = glam::Mat4::perspective_rh(self.fovy, self.aspect, self.znear, self.zfar);

        (view, perspective)
    }
}