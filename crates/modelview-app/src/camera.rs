use egui::{Pos2, Rect, Vec2};
use glam::{Mat4, Vec3};
use modelview_core::Bounds;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewPreset {
    Home,
    Top,
    Bottom,
    Front,
    Back,
    Left,
    Right,
}

impl ViewPreset {
    pub fn label(self) -> &'static str {
        match self {
            ViewPreset::Home => "Home",
            ViewPreset::Top => "Top",
            ViewPreset::Bottom => "Bottom",
            ViewPreset::Front => "Front",
            ViewPreset::Back => "Back",
            ViewPreset::Left => "Left",
            ViewPreset::Right => "Right",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub target: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub fov_y: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            yaw: -0.96,
            pitch: 0.49,
            distance: 220.0,
            fov_y: 40.0_f32.to_radians(),
        }
    }
}

impl Camera {
    pub fn fit(&mut self, bounds: &Bounds) {
        self.target = bounds.center();
        let radius = bounds.radius().max(0.5);
        self.distance = (radius / (self.fov_y * 0.5).sin() * 1.18).clamp(radius * 1.05, 100_000.0);
    }

    pub fn framed(bounds: &Bounds) -> Self {
        let mut camera = Camera::default();
        camera.fit(bounds);
        camera
    }

    pub fn eye(&self) -> Vec3 {
        self.target + self.view_dir() * self.distance
    }

    pub fn view_dir(&self) -> Vec3 {
        Vec3::new(
            self.pitch.cos() * self.yaw.sin(),
            -self.pitch.cos() * self.yaw.cos(),
            self.pitch.sin(),
        )
    }

    pub fn basis(&self) -> (Vec3, Vec3, Vec3) {
        let forward = -self.view_dir();
        let mut right = forward.cross(Vec3::Z);
        if right.length_squared() < 1e-8 {
            right = Vec3::X;
        } else {
            right = right.normalize();
        }
        let up = right.cross(forward).normalize_or_zero();
        (right, up, forward)
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let eye = self.eye();
        let near = (self.distance * 0.004).clamp(0.01, 4.0);
        let far = (self.distance * 24.0 + 2000.0).max(200.0);
        let projection = perspective_rh(self.fov_y, aspect.max(0.01), near, far);
        let view = look_at_rh(eye, self.target, Vec3::Z);
        projection * view
    }

    pub fn orbit(&mut self, delta: Vec2) {
        self.yaw -= delta.x * 0.0072;
        self.pitch = (self.pitch + delta.y * 0.0072).clamp(-1.5533, 1.5533);
    }

    pub fn pan(&mut self, delta: Vec2, viewport: Vec2) {
        let height = viewport.y.max(1.0);
        let units = 2.0 * self.distance * (self.fov_y * 0.5).tan() / height;
        let (right, up, _) = self.basis();
        self.target += -right * (delta.x * units) + up * (delta.y * units);
    }

    pub fn zoom(&mut self, factor: f32, cursor: Option<Vec2>, aspect: f32) {
        let old = self.distance;
        let new = (old * factor).clamp(0.05, 100_000.0);
        let effective = new / old;
        if let Some(ndc) = cursor {
            let (right, up, _) = self.basis();
            let half_h = old * (self.fov_y * 0.5).tan();
            let half_w = half_h * aspect.max(0.01);
            let cursor_point = self.target + right * (ndc.x * half_w) + up * (ndc.y * half_h);
            self.target += (cursor_point - self.target) * (1.0 - effective);
        }
        self.distance = new;
    }

    pub fn preset(&mut self, preset: ViewPreset) {
        match preset {
            ViewPreset::Home => {
                self.yaw = -0.96;
                self.pitch = 0.49;
            }
            ViewPreset::Top => {
                self.yaw = 0.0;
                self.pitch = 1.5533;
            }
            ViewPreset::Bottom => {
                self.yaw = 0.0;
                self.pitch = -1.5533;
            }
            ViewPreset::Front => {
                self.yaw = 0.0;
                self.pitch = 0.0;
            }
            ViewPreset::Back => {
                self.yaw = std::f32::consts::PI;
                self.pitch = 0.0;
            }
            ViewPreset::Left => {
                self.yaw = -std::f32::consts::FRAC_PI_2;
                self.pitch = 0.0;
            }
            ViewPreset::Right => {
                self.yaw = std::f32::consts::FRAC_PI_2;
                self.pitch = 0.0;
            }
        }
    }

    /// Camera pose for a defect theater: target the marker, move to its outward
    /// side, and close in enough to read the local topology.
    pub fn focus(bounds: &Bounds, position: Vec3, current: &Camera) -> Self {
        let mut camera = *current;
        camera.target = position;
        let radius = bounds.radius().max(0.5);
        camera.distance = (radius * 1.8).clamp(2.0, 100_000.0);
        let radial = position - bounds.center();
        if radial.length() > radius * 0.03 {
            let dir = radial.normalize();
            camera.pitch = dir.z.clamp(-0.98, 0.98).asin().clamp(-1.48, 1.48);
            camera.yaw = dir.x.atan2(-dir.y);
        }
        camera
    }

    pub fn lerp(from: &Camera, to: &Camera, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let yaw_delta = shortest_angle(to.yaw - from.yaw);
        Self {
            target: from.target.lerp(to.target, t),
            yaw: from.yaw + yaw_delta * t,
            pitch: from.pitch + (to.pitch - from.pitch) * t,
            distance: from.distance + (to.distance - from.distance) * t,
            fov_y: from.fov_y,
        }
    }

    pub fn project(&self, point: Vec3, rect: Rect, aspect: f32) -> Option<Pos2> {
        let clip = self.view_proj(aspect) * point.extend(1.0);
        if clip.w <= 0.0 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        if ndc.z < -0.2 || ndc.z > 1.2 {
            return None;
        }
        Some(Pos2::new(
            rect.center().x + ndc.x * rect.width() * 0.5,
            rect.center().y - ndc.y * rect.height() * 0.5,
        ))
    }
}

fn shortest_angle(angle: f32) -> f32 {
    (angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

fn perspective_rh(fov_y: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fov_y * 0.5).tan();
    Mat4::from_cols_array(&[
        f / aspect,
        0.0,
        0.0,
        0.0,
        0.0,
        f,
        0.0,
        0.0,
        0.0,
        0.0,
        far / (near - far),
        -1.0,
        0.0,
        0.0,
        far * near / (near - far),
        0.0,
    ])
}

fn look_at_rh(eye: Vec3, center: Vec3, up: Vec3) -> Mat4 {
    let f = (center - eye).normalize();
    let s = f.cross(up).normalize();
    let u = s.cross(f);
    Mat4::from_cols_array(&[
        s.x,
        u.x,
        -f.x,
        0.0,
        s.y,
        u.y,
        -f.y,
        0.0,
        s.z,
        u.z,
        -f.z,
        0.0,
        -s.dot(eye),
        -u.dot(eye),
        f.dot(eye),
        1.0,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube_bounds() -> Bounds {
        let mut bounds = Bounds::empty();
        bounds.add_point(Vec3::splat(-10.0));
        bounds.add_point(Vec3::splat(10.0));
        bounds
    }

    #[test]
    fn focus_targets_the_marker_from_its_outward_side() {
        let marker = Vec3::new(10.0, 0.0, 2.0);
        let camera = Camera::focus(&cube_bounds(), marker, &Camera::default());
        assert!((camera.target - marker).length() < 1e-5);
        assert!(camera.eye().x > marker.x);
        assert!(camera.distance > 10.0 && camera.distance < 40.0);
    }

    #[test]
    fn lerp_takes_the_short_yaw_route() {
        let from = Camera {
            yaw: 3.0,
            ..Camera::default()
        };
        let to = Camera {
            yaw: -3.0,
            ..Camera::default()
        };
        let mid = Camera::lerp(&from, &to, 0.5);
        assert!(mid.yaw.abs() > 3.0);
    }
}
