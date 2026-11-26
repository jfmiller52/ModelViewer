struct Globals {
    view_proj: mat4x4<f32>,
    camera: vec4<f32>,
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> globals: Globals;

struct VsIn {
    @location(0) position: vec3<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = globals.view_proj * vec4<f32>(in.position, 1.0);
    out.world = in.position;
    return out;
}

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let cutoff = c < vec3<f32>(0.0031308);
    let lower = c * 12.92;
    let higher = 1.055 * pow(max(c, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(higher, lower, cutoff);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    var n = normalize(cross(dpdx(in.world), dpdy(in.world)));
    let view = normalize(globals.camera.xyz - in.world);
    if (dot(n, view) < 0.0) {
        n = -n;
    }
    let key = normalize(vec3<f32>(0.38, 0.55, 0.74));
    let fill = normalize(vec3<f32>(-0.62, -0.18, 0.36));
    let base = vec3<f32>(0.18, 0.22, 0.27);
    let ndl = max(dot(n, key), 0.0);
    let ndf = max(dot(n, fill), 0.0);
    let hemi = 0.5 + 0.5 * n.z;
    var col = base * (0.18 + 0.64 * ndl + 0.14 * ndf + 0.16 * hemi);
    let rim = pow(1.0 - max(dot(n, view), 0.0), 3.2) * 0.11;
    col += base * rim;
    return vec4<f32>(linear_to_srgb(col), 1.0);
}
